# 变更提案：per-credential-thinking-adaptive

## 背景

v3.3.0 起（commit 917dcb8），代理不再向 Kiro 上游发送 `additionalModelRequestFields.thinking` 字段，对齐 Kiro CLI 行为以消除 TTFB 劣化（发 thinking 字段会让 Kiro 后端走额外的 thinking 调度路径，首包延迟增加 2–4 秒）。

但该修复是全局一刀切：部分用户希望对某些账号启用 thinking 能力（接受更慢的响应换取更强推理），目前无法按账号粒度控制。本变更引入**账号级 thinking adaptive 开关**：管理员在 Admin 面板账号列表中切换某账号的开关后，当客户端请求携带 `thinking: {"type": "adaptive"}` 且该请求恰好路由到该账号时，代理向 Kiro 上游注入 `additionalModelRequestFields.thinking: {"type": "adaptive"}`。

默认值为关闭（不注入），与 v3.3.0 以来现状完全一致。

## 目标范围

**在范围内：**
- `KiroCredentials` 新增 `thinkingAdaptive: bool` 持久化字段（serde default=false，向后兼容旧 credentials.json）
- Admin API `PUT /credentials/{id}` 支持 `thinkingAdaptive: Option<bool>` 更新
- 账号快照（`CredentialEntrySnapshot`）透出该字段供前端读取
- 转换层在客户端 `req.thinking.thinking_type == "adaptive"` 时生成注入意图标记，随 `ConversionResult` 传递到 provider 调用链
- Provider 在拿到账号上下文后（`rewrite_profile_arn` 同位置），按 `账号开关开启 && 请求标记 && 模型支持` 向 `additionalModelRequestFields` 注入 thinking 字段
- Admin 前端账号列表操作区新增图标切换按钮（点击即切换），中英文 i18n 词条
- 单元测试覆盖：数据模型序列化、update 字段应用、注入逻辑（开/关/客户端非 adaptive/4.5 代/GPT 系边界）

**不在范围内：**
- 不恢复 `<thinking_mode>` system 文本标签注入（阶段 0 澄清：仅结构化字段）
- 客户端传 `thinking: {"type": "enabled"}` 或不传 thinking 时不注入任何内容（阶段 0 澄清：仅 adaptive 生效）
- 4.5 代际模型与 GPT 系模型不注入（这些模型整体跳过 additionalModelRequestFields，有 400 实测依据）
- Admin 编辑账号对话框表单不加入该字段（仅操作列图标切换）
- User UI 不暴露该开关
- OpenAI 兼容层（/v1/chat/completions、/v1/responses）不生效：`convert_reasoning_effort` 仅产出 `{"type": "enabled"}` 永不产生 adaptive，故该开关仅对 Anthropic 协议入口有意义

## 技术方案

**账号确定时机难点**：`additional_model_request_fields` 在 converter 阶段（`convert_request`）构建，此时账号未选择；账号在 provider `call_api_with_retry` 内 `acquire_context_sticky` 才确定。因此注入必须发生在 provider 拿到 `CallContext` 之后，复用 `rewrite_profile_arn` 的既有改写点（API 路径与 MCP 路径各一处）。

**标记透传**：converter 在构建 `ConversionResult` 时，若 `req.thinking` 存在且 `thinking_type == "adaptive"`（即 `is_enabled() && thinking_type == "adaptive"`），在 `additional_model_request_fields` 中不做任何变更（保持 v3.3.0 不发 thinking 的行为），改为新增 bool 字段 `thinking_adaptive_requested: bool` 随结果传递。handlers 三个上游调用路径（流式、非流式、web_search 桥接续请求）均把该标记传给 provider。

**Provider 注入**：在 `rewrite_profile_arn` 同位置扩展一个按凭据改写 body 的步骤：当 `ctx.credentials.thinking_adaptive && 请求标记` 且模型非 GPT 系/4.5 代（模型名可从 `conversationState.currentMessage.userInputMessage.modelId` 读出，或由调用方传入）时，向 `additionalModelRequestFields` 对象插入 `thinking: {"type": "adaptive"}`；字段不存在时先创建对象。故障转移到下一个账号后按新账号的开关值重新判定——同一请求在不同账号上可能注入或不注入，语义正确。

**数据/API 链**（全部复用既有模式）：
- `KiroCredentials` 加 `#[serde(default)] pub thinking_adaptive: bool`（参考 `disabled` 字段）
- `UpdateCredentialRequest` 加 `pub thinking_adaptive: Option<bool>`
- `apply_update_fields` 加对应应用逻辑
- `CredentialEntrySnapshot` 加 `pub thinking_adaptive: bool`
- 前端复用 `useUpdateCredential` mutation（PUT 语义已支持部分更新）

## 预期影响

- **默认行为零变化**：所有既有账号开关默认关闭，注入条件不满足时 provider 改写逻辑退化为原样返回，v3.3.0 修复的 TTFB 问题不会被回归
- **性能**：开启开关的账号在 adaptive 请求下会经历 TTFB 增加（这正是开关的目的，用户自担）
- **兼容性**：旧 credentials.json 无该字段 → serde default false；Admin API 未传该字段 → Option None → 不更新
- **多账号故障转移**：重试切换账号后按新账号开关重新判定注入，无需额外状态

## 风险

- **thinking 标签重建冲突**：流式状态机（`stream.rs`）在 `thinking_enabled` 时会重建 `<thinking>` 标签。若上游因收到 thinking 字段而输出思考内容但代理未开启 thinking 处理路径，可能出现标签透传异常。**缓解**：`resolve_thinking_enabled` 已按 `is_enabled()`（含 adaptive）判定，客户端传 adaptive 时流式路径本就开启，与注入条件天然对齐，无需额外处理
- **Kiro 后端 schema 变化**：未来某天上游对 thinking 字段加约束。**缓解**：注入值固定为 `{"type": "adaptive"}`（与 v3.2.x 时代验证过的形态一致），不改其他字段
- **GPT 系/4.5 代误注入**：这些模型的 `additional_model_request_fields` 本身为 None，注入逻辑需先创建对象——但实测这些模型拒绝该字段（400 REQUEST_BODY_INVALID）。**缓解**：注入逻辑显式排除这两个模型族（按 modelId 判定），不依赖"字段是否为 None"的间接信号
