# Spec: tools 字段位置 (converter)

## 修改的需求

### 需求：tools 定义在 Kiro 请求中的位置

代理 → Kiro 上游的请求体（`KiroRequest.conversationState.currentMessage`）中，tools 完整定义（含 name / description / inputSchema.json）必须放置于 `currentMessage.userInputMessage.userInputMessageContext.tools` 字段，且 history 中不得包含任何 `<tools>...</tools>` 文本块。

#### 场景：含工具的请求 — tools 出现在 currentMessage

- **WHEN** Anthropic `MessagesRequest.tools` 非空（包含 ≥1 个工具定义）
- **THEN** 转换后的 `KiroRequest.conversationState.currentMessage.userInputMessage.userInputMessageContext.tools` 数组长度 ≥ 客户端提供的工具数（含 placeholder 补齐的工具）
- **AND** 数组每个元素的 `toolSpecification.inputSchema.json` 字段为完整规范化后的 JSON Schema（不再是空对象骨架）
- **AND** `KiroRequest.conversationState.history` 中不存在 content 以 `<tools>` 开头或包含 `</tools>` 的 user message

#### 场景：无工具的请求 — context.tools 为空数组

- **WHEN** Anthropic `MessagesRequest.tools` 为空或不存在，且 history 中也未引用任何工具
- **THEN** `currentMessage.userInputMessage.userInputMessageContext.tools` 序列化为空数组（默认值，按 conversation.rs:153 行为）
- **AND** history 中不存在 `<tools>` 注入

#### 场景：history 引用了 currentMessage.tools 中未声明的工具 — placeholder 补齐

- **WHEN** history 消息中存在某个 `tool_use` 引用了名为 `X` 的工具，但客户端在 `MessagesRequest.tools` 中未声明 `X`
- **THEN** 代理生成名为 `X` 的 placeholder ToolSpecification（description = "Tool used in conversation history"，inputSchema = 默认 object schema）并加入 `userInputMessageContext.tools`
- **AND** 工具名匹配按 lowercase 比对，避免重复添加

#### 场景：history hash 稳定性 — 非 tools 相关请求间保持稳定

- **WHEN** 同一 conversation_id 的两次连续请求，system prompt / tool 列表 / history 内容均未变化
- **THEN** `[cache-check] history[0]` 与 `[cache-check] history[1]` 输出的哈希在两次请求中保持一致（PREV_H0 冻结仍生效）
- **AND** 不再出现带 `(tools)` 标签的 history 行

## 移除的需求

### 需求：history[2..3] tools 文本注入

> 该需求被本变更移除。

历史行为：当 tools 非空时，代理在 `history[2]` 插入 user message `<tools>{json}</tools>` 与 `history[3]` 插入 assistant message `OK`，并在 `userInputMessageContext.tools` 仅放精简骨架（仅 name 与 description 首字符）。

移除原因：Kiro 官方 CLI 抓包确认上游原生支持 `userInputMessageContext.tools` 字段承载完整 schema，无需注入 history。
