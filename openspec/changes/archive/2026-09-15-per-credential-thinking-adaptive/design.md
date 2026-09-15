# 技术方案：per-credential-thinking-adaptive

## 上下文

v3.3.0（commit 917dcb8）起 `build_additional_model_request_fields`（`src/anthropic/converter.rs` L1595-1645）结构性不发 `thinking` 字段——发该字段会让 Kiro 后端走额外 thinking 调度路径，TTFB 增加 2–4 秒。现在需要按账号粒度选择性恢复注入。

**核心难点：账号确定时机。** `additional_model_request_fields` 在 converter 阶段（`convert_request`）构建，而账号在 provider `call_api_with_retry` 内 `acquire_context_sticky` 才确定。converter 无法预知账号，provider 才能判定。

## 目标 / 非目标

**目标：**
- 账号级持久化开关 + Admin UI 切换入口
- 开关开启 + 客户端 adaptive + 模型支持 → 注入 `additionalModelRequestFields.thinking`
- 默认行为与 v3.3.0 逐字节一致

**非目标：**
- 不恢复 `<thinking_mode>` system 标签（用户已确认仅结构化字段）
- 不处理 `thinking: enabled`（用户已确认仅 adaptive）
- 不修改编辑对话框表单（仅操作列图标切换）

## 决策

### D1：注入点选在 provider 的 body 改写区（复用 rewrite_profile_arn 模式）

`rewrite_profile_arn`（`src/kiro/provider.rs` L1240-1264）已是"拿到 `CallContext` 后按凭据改写 request body"的既有先例，调用点两处：MCP 路径 L546、API 路径 L875。

方案：新增独立函数 `inject_thinking_adaptive(body: &str, credentials: &KiroCredentials, requested: bool) -> String`，与 `rewrite_profile_arn` 同模式（JSON 解析失败原样返回）。在两处调用点紧随 `rewrite_profile_arn` 之后调用：

```rust
let effective_body = Self::inject_thinking_adaptive(
    &Self::rewrite_profile_arn(request_body, &ctx.credentials),
    &ctx.credentials,
    thinking_adaptive_requested,
);
```

**备选被否决方案：**
- 在 converter 中预注入 → 账号未知，无法按账号判定
- 扩展 `rewrite_profile_arn` 签名合并两步 → 单一职责被破坏，且 MCP 路径当前不需要 thinking 语义差异（MCP 也统一处理，保持一致）
- 用 `CallContext` 携带标记字段 → CallContext 是账号绑定语义，塞请求级状态污染语义

### D2：标记经参数显式传递，不改 CallContext

`ConversionResult` 新增 `thinking_adaptive_requested: bool`。handlers 内部函数（流式 `handle_stream_internal` 签名 L1246 附近、非流式 L2098 附近、桥接 `BridgeContext`）各加一个 bool 参数（BridgeContext 加字段）。provider 公开方法 `call_api` / `call_api_stream` / `call_mcp` 增加尾部 bool 参数。

**为什么不复用 `additional_model_request_fields` 内嵌标记**：注入发生在 JSON 字符串改写层（body 为 `&str`），若在 fields 里塞哨兵值再在 provider 解出，会污染 `KiroRequest` 序列化路径，且序列化后再解析的开销与脆弱性都高于一个 bool 参数。

### D3：模型排除判定放在注入函数内部，从 body 读 modelId

注入函数解析 JSON 后从 `conversationState.currentMessage.userInputMessage.modelId` 读取模型 ID，复用 `is_gpt_model`（converter 已有 `pub(crate)`）与 `model_id.ends_with("4.5")` 判定，与 `build_additional_model_request_fields` 的跳过条件保持同一逻辑。不在调用方重复传模型名。

### D4：注入形态固定 `{"type": "adaptive"}`

与 v3.2.x 时代上游验证过的字段形态一致，不含 budget_tokens（adaptive 语义下上游自调度）。字段插入用 `serde_json::Value` 对象操作：`additionalModelRequestFields` 不存在时创建新对象并插入 `additionalModelRequestFields` 键；已存在时直接插入 `thinking` 键。

### D5：数据链复用 disabled 字段全套模式

- `KiroCredentials`：`#[serde(default)] pub thinking_adaptive: bool`，字段位置放在 `endpoint` 字段之后（proxy/disabled 同区）
- `UpdateCredentialRequest`：`pub thinking_adaptive: Option<bool>`
- `apply_update_fields`：`if let Some(v) = update.thinking_adaptive { cred.thinking_adaptive = v; }`
- `CredentialEntrySnapshot`：`pub thinking_adaptive: bool`（camelCase 序列化为 thinkingAdaptive）
- 前端复用 `useUpdateCredential`（PUT 部分更新语义已支持）

## 风险 / 权衡

- **流式 thinking 标签重建**：客户端传 adaptive 时 `resolve_thinking_enabled` 已返回 true，流式状态机本就开启 `<thinking>` 标签处理，与注入条件天然对齐，无需额外状态
- **开关与请求的竞态**：`ctx.credentials` 是获取 CallContext 时的克隆快照，单次请求内开关值稳定；并发修改开关只影响后续请求，可接受
- **每请求额外一次 JSON 解析/序列化**：仅在 `requested && 开关开` 时才执行改写（标记为 false 时直接短路返回原 body），默认路径零开销

## 迁移方案

无需迁移：serde default 保证旧 credentials.json 直接兼容；Admin API 字段可选，旧前端不传该字段不受影响。

## 待决问题

无——阶段 0 三问已由用户确认（仅结构化字段 / 操作列直接切换 / 仅 adaptive）。
