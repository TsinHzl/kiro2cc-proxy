# 变更提案：classify-refresh-token-failures

## 背景

`src/kiro/token_manager.rs` 的 `acquire_context` 当前对 Token 刷新失败不做任何分类：无论服务端返回 `400 invalid_grant`（refreshToken 已被永久撤销）还是 `500`/`429`/网络超时等瞬态错误，都统一走"切换到下一优先级账号，不计入失败次数"（见 `acquire_context` 注释："Token 刷新失败，切换到下一个优先级的账号（不计入失败次数）"）。

这带来两个问题：
1. refreshToken 一旦被服务端撤销（`invalid_grant`），该账号会在每次被轮到时反复发起注定失败的刷新请求，产生噪音日志和无意义的网络往返，且没有机制促使运维介入更换凭证。
2. 其他类型的刷新失败永远不会被计数或自动禁用，长期处于故障状态的账号会持续拖慢故障转移路径（每轮请求都要先尝试它再切换）。

参考仓库 `kiro.rs` 中已有对应实现（`RefreshTokenInvalidError` 立即禁用 + `refresh_failure_count` 计数禁用），本次是该实现思路的移植与适配。

## 目标范围

**在范围内：**
- 检测 `refresh_social_token` / `refresh_idc_token` / `refresh_external_idp_token`（三条刷新路径全部覆盖）返回 `400` + 响应体含 `"invalid_grant"` + `"Invalid refresh token provided"` 的情况，返回专用错误类型（区别于其他刷新失败）
- 新增 `DisabledReason::InvalidRefreshToken` 变体：遇到该错误立即禁用对应账号，不计数、不重试
- 新增 `DisabledReason::TooManyRefreshFailures` 变体 + `CredentialEntry.refresh_failure_count` 字段：普通刷新失败计数，达到 `MAX_FAILURES_PER_CREDENTIAL`（3）阈值后禁用
- `acquire_context` / `acquire_context_filtered` / `acquire_context_sticky` 三个方法各自的 `try_ensure_token` 失败分支均根据错误类型分流调用不同的上报方法（`acquire_context_sticky` 是 `provider.rs` 生产请求路径实际使用的入口，若遗漏会导致分类逻辑在真实流量上不生效）
- 现有"全部账号被禁用后自愈重置"逻辑扩展到同时覆盖 `TooManyFailures` 和 `TooManyRefreshFailures`（两者都是瞬态故障，适合自愈）；`InvalidRefreshToken` **不**参与自愈（refreshToken 已被服务端撤销，重置重试只会立即再次失败）
- `save_stats()` 的持久化白名单同步加入 `InvalidRefreshToken` / `TooManyRefreshFailures`，确保新禁用状态能像现有 `QuotaExceeded`/`TooManyFailures` 一样跨重启存活（否则重启后会静默恢复启用，直接违背本提案目标）
- 刷新成功路径（`try_ensure_token`）清零 `refresh_failure_count`，避免长期运行下孤立的偶发失败无限累积导致误禁用
- Admin API 的 `set_disabled(id, false)` / `reset_and_enable` 手动重新启用时同步重置 `refresh_failure_count`（与现有对 `failure_count` 的重置保持一致），确保手动恢复路径生效
- `CredentialEntrySnapshot` 新增 `refresh_failure_count` 字段，对称于现有 `failure_count`，供 Admin API 读取
- 单元测试覆盖：invalid_grant 检测（social + idc + external_idp 三条路径）、立即禁用行为、普通失败计数到阈值禁用、`TooManyRefreshFailures` 参与自愈、`InvalidRefreshToken` 不参与自愈、新禁用原因跨重启存活、刷新成功清零计数器、手动重新启用清零计数器

**不在范围内：**
- Admin UI 前端展示改动（后端字段就位即可，前端如需展示由后续任务处理）
- API Key 凭据的刷新逻辑（本身不支持刷新，不受影响）
- 现有 `failure_count`（API 调用失败计数）/ `QuotaExceeded` 逻辑的行为改动

## 技术方案

- 复用 `kiro.rs` 的 `RefreshTokenInvalidError` 模式：定义一个实现 `std::error::Error` 的错误类型，在 `acquire_context` 循环中用 `e.downcast_ref::<RefreshTokenInvalidError>()` 分流
- invalid_grant 检测逻辑提取为纯函数 `is_invalid_grant_response(status, body)`，在 `refresh_social_token` / `refresh_idc_token` / `refresh_external_idp_token` 三处错误分支统一调用，避免引入 HTTP mock 测试依赖，同时保证可单元测试
- `refresh_failure_count` 复用现有 `MAX_FAILURES_PER_CREDENTIAL` 常量（值为 3），不新增独立阈值常量，与 API 失败计数阈值保持一致
- 自愈逻辑：现有 `e.disabled_reason == Some(DisabledReason::TooManyFailures)` 条件扩展为同时匹配 `TooManyFailures` 或 `TooManyRefreshFailures`
- `try_ensure_token` 有 3 个调用点（`acquire_context`/`acquire_context_filtered`/`acquire_context_sticky`），三处的 `Err(e)` 分支均需接入相同的分类/上报逻辑；`acquire_context_sticky` 命中 `RefreshTokenInvalidError` 或达到计数阈值时，除禁用账号外仍保留其现有的"驱逐 sticky cache 并 fallback 到 acquire_context_filtered"行为不变
- `refresh_failure_count` 不持久化到 `StatsEntry`（与现有 `failure_count` 一致，仅终态 `disabled_reason` 持久化，计数器本身只在内存中，重启后从 0 开始）
- `save_stats()` 现有的 `disabled_reason` 持久化白名单（`matches!(r, DisabledReason::QuotaExceeded | DisabledReason::TooManyFailures)`）必须同步扩展为包含 `InvalidRefreshToken` / `TooManyRefreshFailures`，否则新禁用状态无法跨重启存活，这是本次改动能否达成目标的关键前提，而非可选项
- `try_ensure_token` 刷新成功分支（更新 `entry.credentials` / `entry.last_refreshed_at` 的同一处）追加 `entry.refresh_failure_count = 0`
- `set_disabled(id, false)` / `reset_and_enable` 在重置 `entry.failure_count = 0` 的同一处追加 `entry.refresh_failure_count = 0`，保持两个计数器的重置时机一致

## 预期影响

- 行为变化：普通刷新失败账号从"永不禁用"变为"3 次后禁用"；已撤销 refreshToken 的账号会更快被排除出选择池，减少无效刷新请求
- 无 API/协议兼容性影响（纯内部状态机与容错逻辑变化）
- 现有测试（如 `test_too_many_failures_disabled_reason_survives_restart`）作为参考模式，新测试遵循相同结构

## 风险

- 若 Kiro 服务端返回的 `invalid_grant` 响应体格式与假设的字符串匹配条件不完全一致，该分支可能不会触发，退化为普通失败计数——不构成回归，只是优化未生效；该匹配模式已在 `kiro.rs` 中验证过，风险较低
- `refresh_failure_count` 与 `failure_count` 是两个独立计数器，需注意 `report_failure`（API 调用失败上报）不能误清空/误累加 `refresh_failure_count`，反之亦然
- 若遗漏 `save_stats()` 白名单扩展，新禁用状态会在进程重启后静默恢复启用——已在设计中显式列为独立任务并附对称的"跨重启存活"测试兜底
- 若只在 `acquire_context` 接入分类逻辑，遗漏 `acquire_context_filtered`/`acquire_context_sticky`，会导致改动在生产请求路径（`provider.rs` 实际调用的是 `acquire_context_sticky`）上完全不生效——已在 tasks.md 中为三个调用点分别列出独立任务并附生产路径的专项测试
