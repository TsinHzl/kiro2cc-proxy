# 任务清单：classify-refresh-token-failures

## 状态：ARCHIVED

## 任务

- [x] T1：新增 `RefreshTokenInvalidError` 错误类型 + `is_invalid_grant_response(status, body)` 纯函数（`src/kiro/token_manager.rs`）
- [x] T2：`refresh_social_token` 错误分支调用 `is_invalid_grant_response`，命中时返回 `RefreshTokenInvalidError`
- [x] T3：`refresh_idc_token` 错误分支调用 `is_invalid_grant_response`，命中时返回 `RefreshTokenInvalidError`
- [x] T4：`refresh_external_idp_token` 错误分支（约 410-420 行 `!status.is_success()` 处）调用 `is_invalid_grant_response`，命中时返回 `RefreshTokenInvalidError`（第三条刷新路径，与 T2/T3 保持一致，避免三种认证方式行为不统一）
- [x] T5：`DisabledReason` 新增 `InvalidRefreshToken` + `TooManyRefreshFailures` 两个变体，更新 `describe()` 匹配分支
- [x] T6：`CredentialEntry` 新增 `refresh_failure_count: u32` 字段，所有构造点（`new()`/`load_credentials`/测试 fixture）同步初始化为 0
- [x] T7：新增 `report_refresh_token_invalid(&self, id: u64) -> bool` 方法：立即禁用，`disabled_reason = InvalidRefreshToken`，不计数
- [x] T8：新增 `report_refresh_failure(&self, id: u64) -> bool` 方法：`refresh_failure_count += 1`，达到 `MAX_FAILURES_PER_CREDENTIAL` 后禁用并设 `disabled_reason = TooManyRefreshFailures`（参考现有 `report_failure` 结构）
- [x] T9：`acquire_context`（约 1168 行）循环内替换现有"Token 刷新失败，切换到下一个优先级的账号（不计入失败次数）"分支：用 `downcast_ref::<RefreshTokenInvalidError>()` 判断后分别调用 T7/T8 的方法
- [x] T10：`acquire_context_filtered`（约 1222 行）的 `Err(e)` 分支接入与 T9 相同的分类逻辑（判断后调用 T7/T8）
- [x] T11：`acquire_context_sticky`（约 1273 行）的 `Err(e)` 分支接入与 T9 相同的分类逻辑（判断后调用 T7/T8），同时保留其现有的"驱逐 sticky cache 条目"行为不变——此方法是 `provider.rs`（450/698 行）生产请求路径实际调用的入口，遗漏会导致本次改动在真实流量上不生效
- [x] T12：`acquire_context` 内"全灭自愈"分支的匹配条件从 `TooManyFailures` 扩展为 `TooManyFailures | TooManyRefreshFailures`（重置时按各自类型清零 `failure_count` 或 `refresh_failure_count`）
- [x] T13：`save_stats()`（约 1604 行）的 `disabled_reason` 持久化白名单 `matches!(r, DisabledReason::QuotaExceeded | DisabledReason::TooManyFailures)` 扩展为同时包含 `InvalidRefreshToken` / `TooManyRefreshFailures`（关键任务：遗漏此项会导致新禁用状态在重启后静默失效，违背提案核心目标）
- [x] T14：`try_ensure_token`（约 1430-1437 行）刷新成功、更新 `entry.credentials`/`entry.last_refreshed_at` 的同一处，追加 `entry.refresh_failure_count = 0`
- [x] T15：`set_disabled(id, false)`（约 2136 行）和 `reset_and_enable`（约 2181 行）在重置 `entry.failure_count = 0` 的同一处，追加 `entry.refresh_failure_count = 0`
- [x] T16：`CredentialEntrySnapshot` 新增 `refresh_failure_count` 字段，snapshot 构造处填充
- [x] T17：单元测试 —— `is_invalid_grant_response` 命中/不命中场景（400+invalid_grant+匹配文案 / 400+其他文案 / 非400状态码）
- [x] T18：单元测试 —— `refresh_external_idp_token` 的 400+invalid_grant 场景同样返回 `RefreshTokenInvalidError`（覆盖 T4）
- [x] T19：单元测试 —— `report_refresh_token_invalid` 立即禁用且不计入 `refresh_failure_count`
- [x] T20：单元测试 —— `report_refresh_failure` 计数到阈值前不禁用、达到阈值后禁用并设置正确的 `disabled_reason`
- [x] T21：单元测试 —— `acquire_context_filtered` 与 `acquire_context_sticky` 路径下，刷新失败同样触发正确的分类/计数/禁用（覆盖 T10/T11，验证生产路径 `acquire_context_sticky` 确实生效，不是只有 `acquire_context` 生效）
- [x] T22：单元测试 —— 全灭自愈场景下 `TooManyRefreshFailures` 账号被重置，`InvalidRefreshToken` 账号保持禁用不变
- [x] T23：单元测试 —— `InvalidRefreshToken` 与 `TooManyRefreshFailures` 两个新禁用原因经 `save_stats()` 落盘后重新加载仍保持禁用（对称于现有 `test_quota_disabled_reason_survives_restart` / `test_too_many_failures_disabled_reason_survives_restart`）
- [x] T24：单元测试 —— 刷新成功后 `refresh_failure_count` 被清零；`set_disabled(id, false)` / `reset_and_enable` 后 `refresh_failure_count` 被清零
- [x] T25：`cargo check` + `cargo test` + `cargo clippy` 全部通过

## 验收标准

- [ ] `refresh_social_token` / `refresh_idc_token` / `refresh_external_idp_token` 三条路径均在 400+invalid_grant+"Invalid refresh token provided" 时返回 `RefreshTokenInvalidError`，其他失败仍返回原有错误类型
- [ ] `acquire_context` / `acquire_context_filtered` / `acquire_context_sticky` 三个调用点均对 `RefreshTokenInvalidError` 立即禁用账号（`disabled_reason = InvalidRefreshToken`），对其他刷新失败计数，达到 3 次后禁用（`disabled_reason = TooManyRefreshFailures`）——尤其验证生产路径 `acquire_context_sticky` 生效
- [ ] 全灭自愈逻辑覆盖 `TooManyFailures` 与 `TooManyRefreshFailures`，不覆盖 `InvalidRefreshToken`
- [ ] `InvalidRefreshToken` / `TooManyRefreshFailures` 两个新禁用原因能够跨进程重启存活（`save_stats()` 白名单已同步扩展）
- [ ] 刷新成功和手动重新启用（`set_disabled`/`reset_and_enable`）均会清零 `refresh_failure_count`
- [ ] `CredentialEntrySnapshot` 暴露 `refresh_failure_count` 字段
- [ ] 新增测试全部通过，`cargo clippy` 无新增警告
