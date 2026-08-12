# 设计文档：classify-refresh-token-failures

## 上下文

`acquire_context` 的刷新失败处理当前是"一刀切"：任何刷新失败都只是切换账号，不计数、不禁用。这在两类失败上都不是最优：
- `invalid_grant`（refreshToken 已被服务端撤销）：账号已经永久不可用，继续尝试只会浪费请求
- 瞬态失败（500/429/网络错误）：可能自行恢复，但长期故障的账号如果永不禁用，会一直拖慢故障转移路径

kiro.rs 仓库已有可参考的实现模式（`RefreshTokenInvalidError` + 计数禁用 + 分类自愈），本次设计基本沿用其思路，结合本仓库现有的 `DisabledReason` / 自愈 / 持久化机制做适配。

## 目标 / 非目标

**目标：**
- invalid_grant 立即禁用，不重试、不计数
- 普通刷新失败计数禁用，复用现有 `MAX_FAILURES_PER_CREDENTIAL` 阈值
- 瞬态性质的自动禁用（TooManyFailures / TooManyRefreshFailures）都参与"全灭自愈"；永久性质的禁用（InvalidRefreshToken）不参与

**非目标：**
- 不引入新的可配置阈值（不新增 `max_refresh_failures` 之类的配置项，直接复用现有常量，降低配置面）
- 不改动现有 API 调用失败（`failure_count` / `report_failure`）的行为
- 不在本次改动中处理 Admin UI 展示

## 决策

1. **错误类型复用 kiro.rs 的 `RefreshTokenInvalidError` 模式**：`anyhow::Error` + `downcast_ref` 分流，不改变 `refresh_token` / `refresh_social_token` / `refresh_idc_token` 的返回类型签名（仍是 `anyhow::Result<KiroCredentials>`），侵入面最小。

2. **`refresh_failure_count` 作为 `CredentialEntry` 新字段，独立于 `failure_count`**：两者语义不同（API 调用失败 vs Token 刷新失败），共享计数器会导致互相污染误判（例如一个账号 API 调用一直成功但刷新总失败，若共享计数器会被"成功调用"的重置逻辑意外清零）。

3. **阈值复用 `MAX_FAILURES_PER_CREDENTIAL`（3）而非新增常量**：保持配置面简单，且当前没有证据表明刷新失败需要不同的容忍度。

4. **`refresh_failure_count` 不持久化**：与现有 `failure_count` 处理方式一致——只有终态 `disabled_reason` 写入 `StatsEntry` 持久化，中间计数值不持久化，重启后计数器从 0 开始。这与"进程重启即重新给一次机会"的现有语义保持一致。

5. **自愈范围**：`TooManyFailures` 和 `TooManyRefreshFailures` 都视为"瞬态故障累积"，在"全部账号已禁用"这种极端情况下值得整体重试一次（等价于重启）。`InvalidRefreshToken` 是服务端确认的永久性失效，重置后立即再次失败是可预期的，因此排除在自愈范围外，避免自愈循环空转。

6. **invalid_grant 检测提取为纯函数以支持单元测试**：仓库当前没有 HTTP mock 测试基础设施（无 `wiremock`/`httpmock` 类 dev-dependency），且不打算为此新增外部 crate。将检测逻辑提取为 `fn is_invalid_grant_response(status: u16, body: &str) -> bool` 纯函数，在 `refresh_social_token` / `refresh_idc_token` / `refresh_external_idp_token` 的错误分支调用，使其可以直接传入构造好的 `(status, body)` 单元测试，无需真实发起 HTTP 请求或引入 mock server。

7. **`save_stats()` 持久化白名单必须同步扩展**：`disabled_reason` 能否跨重启存活完全由 `save_stats()` 内的硬编码 `matches!` 白名单决定（`persist_credentials()` 明确不写入自动禁用原因，见现有 `test_quota_disabled_reason_survives_restart` / `test_too_many_failures_disabled_reason_survives_restart` 两个回归测试）。若只新增枚举变体和判定逻辑而不同步扩展这个白名单，`InvalidRefreshToken` 和 `TooManyRefreshFailures` 会在进程重启后静默丢失，账号恢复为启用状态——对 `InvalidRefreshToken` 而言这是致命的（违反"需人工介入才能恢复"的设计目标）。因此本次改动把白名单扩展列为与新增枚举变体同等优先级的必需任务，并要求对称的存活测试覆盖新增的两个变体。

8. **刷新成功清零 `refresh_failure_count`，对称于 `report_success` 清零 `failure_count`**：`refresh_failure_count` 是"累计但无成功即清零"的计数器语义（与 `failure_count` 一致），否则跨越很长时间的多次孤立瞬时失败会不断累积并最终误禁用一个实际健康、只是偶发失败的账号。清零时机选在 `try_ensure_token` 刷新成功、更新 `entry.credentials`/`entry.last_refreshed_at` 的同一处，理由是"刷新成功"才是该计数器唯一关心的信号，不依赖 API 调用是否成功（`report_success` 语义不同，不能复用）。

9. **手动重新启用（Admin API）必须同步重置 `refresh_failure_count`**：`set_disabled(id, false)` 和 `reset_and_enable` 是运维对已禁用账号的标准恢复路径，两者目前都会重置 `failure_count`。若不同步重置 `refresh_failure_count`，因 `TooManyRefreshFailures` 被禁用的账号在运维手动重新启用后，只需再出现 1 次刷新失败就会立刻被重新禁用，使这条恢复路径形同虚设。

10. **分类逻辑必须接入全部 3 个 `try_ensure_token` 调用点，而非仅 `acquire_context`**：`acquire_context`（1168 行）/ `acquire_context_filtered`（1222 行）/ `acquire_context_sticky`（1273 行）各自持有独立的 `Err(e)` 分支。`provider.rs` 的两处生产请求路径（450/698 行）调用的是 `acquire_context_sticky`（内部在缓存未命中时 fallback 到 `acquire_context_filtered`），`acquire_context` 反而不是生产流量的实际入口。若只改造 `acquire_context`，新分类逻辑在真实流量上不会触发，等同于本次改动没有生效。三处均需接入相同的 `downcast_ref::<RefreshTokenInvalidError>()` 分流；`acquire_context_sticky` 命中失败时除按分类逻辑上报外，仍保留其现有的"驱逐 sticky cache 条目"行为（两者不冲突，禁用账号与清理 sticky 映射是独立的副作用）。

## 风险 / 权衡

- **字符串匹配脆弱性**：`invalid_grant` 检测依赖响应体包含固定子串。若 Kiro 服务端调整错误响应格式，该分支会静默退化为"普通刷新失败"（计数后禁用），不会引发功能性错误，只是分类不精确。可接受。
- **两个独立计数器的维护成本**：新增 `refresh_failure_count` 后，`CredentialEntry` 上会有 `failure_count` 和 `refresh_failure_count` 两个类似字段，未来需要小心保证两者的重置逻辑（成功回调、自愈、手动启用）各自独立且不遗漏。测试需覆盖两者不互相影响。
- **持久化白名单是隐藏的单点依赖**：`save_stats()` 的硬编码白名单是本仓库特有的持久化机制（kiro.rs 参考实现不存在这个约束），容易在后续新增 `DisabledReason` 变体时被再次遗漏。已通过 tasks.md 显式列为独立任务 + 对称测试缓解，但长期看可考虑将"是否持久化"作为 `DisabledReason` 自身的属性（如方法 `fn is_persistent(&self) -> bool`）而非外部白名单，避免枚举与白名单分离导致的遗漏风险——本次不做此重构，留作后续技术债务观察点。
- **3 个调用点重复实现分类分流逻辑**：`acquire_context`/`acquire_context_filtered`/`acquire_context_sticky` 各自的 `Err(e)` 分支都要写一遍 `downcast_ref` 判断，属于少量重复代码；未提取公共辅助函数是因为三处失败后的后续动作不同（切换优先级 / 记录 tried_ids / 驱逐 sticky cache），强行抽象会引入不必要的间接层，权衡后接受这点重复。
- **sticky 路径 fallback 时可能重复选中同一账号**：`acquire_context_sticky` 缓存未命中或刷新失败后会 fallback 到 `acquire_context_filtered`，若该账号在本次失败后仍未达禁用阈值（未被禁用），`acquire_context_filtered` 内的 `select_next_credential` 存在按优先级重新选中同一账号的可能，导致同一请求内对同一账号发起第二次刷新尝试。这是现有代码结构的既有行为（非本次改动引入），本次不做修复，仅在分类计数语义上保持与"每次实际发起的刷新尝试都计一次失败"一致。
