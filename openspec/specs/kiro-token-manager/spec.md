# kiro-token-manager 规范增量：refresh 失败分类

## 新增需求

### 需求：区分 invalid_grant 与普通刷新失败

#### 场景：refreshToken 已被服务端撤销（social）
- **WHEN** `refresh_social_token` 收到 HTTP 400，且响应体同时包含 `"invalid_grant"` 与 `"Invalid refresh token provided"`
- **THEN** 返回 `RefreshTokenInvalidError`，不落入通用的 `bail!` 错误分支

#### 场景：refreshToken 已被服务端撤销（IdC）
- **WHEN** `refresh_idc_token` 收到 HTTP 400，且响应体同时包含 `"invalid_grant"` 与 `"Invalid refresh token provided"`
- **THEN** 返回 `RefreshTokenInvalidError`，不落入通用的 `bail!` 错误分支

#### 场景：refreshToken 已被服务端撤销（external_idp）
- **WHEN** `refresh_external_idp_token` 收到 HTTP 400，且响应体同时包含 `"invalid_grant"` 与 `"Invalid refresh token provided"`
- **THEN** 返回 `RefreshTokenInvalidError`，不落入通用的 `bail!` 错误分支

#### 场景：invalid_grant 错误触发立即禁用（三个调用点均适用）
- **WHEN** `acquire_context` / `acquire_context_filtered` / `acquire_context_sticky` 任一方法在 `try_ensure_token` 失败后，通过 `downcast_ref::<RefreshTokenInvalidError>()` 识别出错误类型为 `RefreshTokenInvalidError`
- **THEN** 立即将该账号标记为 `disabled = true`、`disabled_reason = Some(DisabledReason::InvalidRefreshToken)`，不计入 `refresh_failure_count`，不重试该账号本轮请求，继续尝试下一账号（`acquire_context_sticky` 额外驱逐该 continuation_id 的 sticky cache 条目，与现有行为一致）

#### 场景：普通刷新失败计数未达阈值（三个调用点均适用）
- **WHEN** `acquire_context` / `acquire_context_filtered` / `acquire_context_sticky` 任一方法捕获到非 `RefreshTokenInvalidError` 的刷新失败，且该账号本次失败后的 `refresh_failure_count < MAX_FAILURES_PER_CREDENTIAL`
- **THEN** 账号保持启用状态，继续本轮请求的账号切换/重选流程（各方法维持各自现有的切换/重选行为不变）

#### 场景：普通刷新失败计数达到阈值（三个调用点均适用）
- **WHEN** `acquire_context` / `acquire_context_filtered` / `acquire_context_sticky` 任一方法捕获到非 `RefreshTokenInvalidError` 的刷新失败，且该账号本次失败后的 `refresh_failure_count >= MAX_FAILURES_PER_CREDENTIAL`
- **THEN** 将该账号标记为 `disabled = true`、`disabled_reason = Some(DisabledReason::TooManyRefreshFailures)`

#### 场景：生产请求路径（acquire_context_sticky）实际生效
- **WHEN** 通过 `provider.rs` 的生产请求入口发起请求，途经 `acquire_context_sticky` → （缓存未命中时）`acquire_context_filtered`，且被选中账号的刷新触发 `RefreshTokenInvalidError` 或计数达到阈值
- **THEN** 该账号被正确禁用（`InvalidRefreshToken` 或 `TooManyRefreshFailures`），而不是像改动前一样仅切换账号、不做任何计数或禁用

### 需求：自愈重置范围调整

#### 场景：TooManyRefreshFailures 参与全灭自愈
- **WHEN** 所有账号均处于禁用状态，且存在 `disabled_reason` 为 `TooManyFailures` 或 `TooManyRefreshFailures` 的账号
- **THEN** 这些账号被重置为 `disabled = false`、`disabled_reason = None`，对应的 `failure_count` 或 `refresh_failure_count` 清零

#### 场景：InvalidRefreshToken 不参与全灭自愈
- **WHEN** 所有账号均处于禁用状态，且触发自愈逻辑
- **THEN** `disabled_reason = Some(DisabledReason::InvalidRefreshToken)` 的账号保持禁用状态不变，需人工更换凭证后才能恢复

### 需求：新禁用原因的持久化与计数器重置一致性

#### 场景：InvalidRefreshToken 跨进程重启存活
- **WHEN** 账号因 `RefreshTokenInvalidError` 被禁用（`disabled_reason = InvalidRefreshToken`）后，调用 `save_stats()` 落盘，随后进程重启并重新加载统计缓存
- **THEN** 该账号仍保持 `disabled = true`、`disabled_reason = Some(DisabledReason::InvalidRefreshToken)`，不会因重启而恢复启用

#### 场景：TooManyRefreshFailures 跨进程重启存活
- **WHEN** 账号因连续刷新失败达到阈值被禁用（`disabled_reason = TooManyRefreshFailures`）后，调用 `save_stats()` 落盘，随后进程重启并重新加载统计缓存
- **THEN** 该账号仍保持 `disabled = true`、`disabled_reason = Some(DisabledReason::TooManyRefreshFailures)`，不会因重启而恢复启用

#### 场景：刷新成功清零 refresh_failure_count
- **WHEN** 账号的 `refresh_failure_count` 为非零值（未达禁用阈值），随后一次 `try_ensure_token` 触发的刷新成功完成
- **THEN** 该账号的 `refresh_failure_count` 被重置为 0

#### 场景：手动重新启用清零 refresh_failure_count
- **WHEN** 账号因 `TooManyRefreshFailures` 被禁用（`refresh_failure_count >= MAX_FAILURES_PER_CREDENTIAL`），运维通过 Admin API 调用 `set_disabled(id, false)` 或 `reset_and_enable(id)`
- **THEN** 该账号的 `refresh_failure_count` 被重置为 0，且 `disabled = false`、`disabled_reason = None`；此后再出现 1 次刷新失败不会立即重新触发禁用
