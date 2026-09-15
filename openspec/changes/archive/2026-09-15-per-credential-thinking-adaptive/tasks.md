# 任务清单：per-credential-thinking-adaptive

## 状态：ARCHIVED

## 任务

- [x] 任务 1：`KiroCredentials` 新增 `thinking_adaptive: bool` 字段（`#[serde(default)]`，含 doc 注释与 Debug 辅助），补序列化向后兼容单测（旧 JSON 无字段 → false）；验证：`cargo test -p kiro2cc-proxy credentials` 通过
- [x] 任务 2：Admin API 更新链——`UpdateCredentialRequest` 加 `thinking_adaptive: Option<bool>`，`apply_update_fields` 应用该字段，`CredentialEntrySnapshot` 透出；补 update 单测；验证：`cargo test update_credential` 通过
- [x] 任务 3：converter 标记透传——`ConversionResult` 新增 `thinking_adaptive_requested: bool`（`req.thinking` 为 adaptive 时为 true），流式/非流式/桥接三个 handlers 调用路径携带该标记；验证：`cargo test thinking` 通过
- [x] 任务 4：provider 注入逻辑——按 `CallContext.credentials.thinking_adaptive && 请求标记` 且模型非 GPT 系/4.5 代，向 `additionalModelRequestFields` 注入 `thinking: {"type": "adaptive"}`；单测覆盖开/关/非 adaptive/4.5 代/GPT 系五分支；验证：`cargo test rewrite` 通过
- [x] 任务 5：前端——`types/api.ts` 加 `thinkingAdaptive`，操作列新增切换按钮（复用 `useUpdateCredential`），i18n 中英词条；验证：`cd admin-ui && pnpm build` 成功 + 手工验证：点击切换后刷新页面，开关状态保持（呼应验收标准第 5 条持久化项）
- [x] 任务 6：集成验证——`cargo fmt && cargo clippy && cargo test` 全绿；端到端手工验证：开启某账号开关后用 adaptive 请求路由到该账号，抓 request_body 确认 thinking 字段存在

## 验收标准

- [x] 所有既有账号默认（开关关闭）行为与 v3.3.0 完全一致：任何请求的 Kiro request_body 均不含 additionalModelRequestFields.thinking
- [x] 开关开启的账号 + 客户端传 `thinking: {"type": "adaptive"}` + 模型非 GPT 系/4.5 代 → request_body 的 additionalModelRequestFields 含 `thinking: {"type": "adaptive"}`
- [x] 客户端传 `thinking: {"type": "enabled"}` 或不传 thinking 时，即使开关开启也不注入
- [x] GPT 系与 4.5 代际模型在任何开关状态下都不注入（保持整体跳过 additionalModelRequestFields 的现状）
- [x] 开关状态经 Admin API 更新后持久化到 credentials.json，重启后保留
- [x] 故障转移到另一账号时按新账号的开关值重新判定注入
- [x] Admin 前端账号列表可见切换按钮，状态与后端一致，中英文文案齐全
