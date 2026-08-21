# 规范增量：OpenAI 协议兼容端点

## 新增需求

### 需求：模型名映射

#### 场景：Codex 模型名映射到 Kiro GPT 模型
- **WHEN** 请求 `model` 为 `gpt-5-codex` 或 `gpt-5.1-codex`
- **THEN** 发往 Kiro 的 `model` 为 `gpt-5.6-terra`

#### 场景：Codex max 变体映射到 luna
- **WHEN** 请求 `model` 为 `gpt-5.1-codex-max`
- **THEN** 发往 Kiro 的 `model` 为 `gpt-5.6-luna`

#### 场景：Kiro 原生 GPT 模型名原样透传
- **WHEN** 请求 `model` 以 `gpt-5.6-` 开头（当前上游含 `gpt-5.6-sol` / `gpt-5.6-terra` / `gpt-5.6-luna`，见 `src/anthropic/handlers.rs:623-649`）
- **THEN** 发往 Kiro 的 `model` 与请求值完全一致，不做改写，且不记录兜底 `WARN`
- **备注** 采用前缀规则而非枚举，确保上游未来新增 `gpt-5.6-*` 型号时自动透传，不会被兜底分支静默改写为 `gpt-5.6-terra`

#### 场景：Claude 系模型名原样透传
- **WHEN** 请求 `model` 以 `claude-` 开头
- **THEN** 发往 Kiro 的 `model` 与请求值完全一致，不做改写

#### 场景：未知 gpt 模型兜底
- **WHEN** 请求 `model` 以 `gpt-` 开头但不匹配上述任何规则（如 `gpt-4o`，注意 `gpt-5.6-*` 已被上一条前缀规则覆盖，不进入本分支）
- **THEN** 发往 Kiro 的 `model` 为 `gpt-5.6-terra`，并记录一条 `WARN` 级日志说明兜底

#### 场景：响应回写客户端原始模型名
- **WHEN** 客户端请求 `model` 为 `gpt-5-codex` 且上游成功返回
- **THEN** 响应体（非流式 JSON 与流式各 chunk）的 `model` 字段为 `gpt-5-codex`，不是 `gpt-5.6-terra`

---

### 需求：Chat Completions 端点

#### 场景：非流式对话
- **WHEN** `POST /v1/chat/completions` 携带有效 API Key、`stream` 缺省或为 `false`、`messages` 含一条 `role:"user"`
- **THEN** 返回 `200` 与 `Content-Type: application/json`，body 为 `{"id":"chatcmpl-<uuid>","object":"chat.completion","created":<unix秒>,"model":"<客户端原始模型名>","choices":[{"index":0,"message":{"role":"assistant","content":"<文本>"},"finish_reason":"stop"}],"usage":{"prompt_tokens":<n>,"completion_tokens":<n>,"total_tokens":<n>}}`

#### 场景：非流式 usage 与上游一致
- **WHEN** 上游 Anthropic 响应的 `usage` 为 `{"input_tokens":A,"output_tokens":B}`
- **THEN** 返回的 `usage.prompt_tokens` 等于 `A`、`usage.completion_tokens` 等于 `B`、`usage.total_tokens` 等于 `A+B`

#### 场景：流式对话的首帧与末帧
- **WHEN** `POST /v1/chat/completions` 且 `stream` 为 `true`
- **THEN** 返回 `200` 与 `Content-Type: text/event-stream`；首个 data 帧的 `choices[0].delta` 含 `{"role":"assistant"}`；文本增量以 `choices[0].delta.content` 逐帧下发；结束前发一帧 `choices[0].finish_reason` 非 `null` 且 `delta` 为空对象；最后一行为 `data: [DONE]`

#### 场景：流式含 usage 上报
- **WHEN** 流式请求携带 `stream_options.include_usage` 为 `true`
- **THEN** 在 `finish_reason` 帧之后、`[DONE]` 之前额外下发一帧，其 `choices` 为空数组且 `usage` 为完整 token 统计

#### 场景：system 消息转为 Anthropic system
- **WHEN** `messages` 首条为 `role:"system"`（或 `role:"developer"`）
- **THEN** 其 `content` 进入 Anthropic 请求的 `system` 数组，且**不**出现在 Anthropic 请求的 `messages` 数组中

#### 场景：工具声明转换
- **WHEN** 请求含 `tools:[{"type":"function","function":{"name":"get_weather","description":"...","parameters":{...}}}]`
- **THEN** Anthropic 请求的 `tools[0]` 为 `{"name":"get_weather","description":"...","input_schema":{...}}`

#### 场景：模型请求调用工具（非流式）
- **WHEN** 上游返回含 `tool_use` block 且 `stop_reason` 为 `tool_use`
- **THEN** 返回的 `choices[0].message.tool_calls` 为 `[{"id":"<tool_use.id>","type":"function","function":{"name":"<tool_use.name>","arguments":"<input 的 JSON 字符串>"}}]`，`choices[0].message.content` 为 `null`，`finish_reason` 为 `tool_calls`

#### 场景：模型请求调用工具（流式）
- **WHEN** 上游流式返回 `content_block_start`（`tool_use`）后跟随多个 `input_json_delta`
- **THEN** 首帧下发 `choices[0].delta.tool_calls[0]` 含 `index`、`id`、`function.name`；后续帧仅下发 `choices[0].delta.tool_calls[0]` 的 `index` 与 `function.arguments` 增量；同一 `tool_use` 的 `index` 在整个流中保持稳定，多个并发 `tool_use` 的 `index` 互不重复

#### 场景：工具结果回传
- **WHEN** 请求 `messages` 含 `{"role":"tool","tool_call_id":"toolu_x","content":"18°C"}`
- **THEN** Anthropic 请求中对应位置为 `{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_x","content":"18°C"}]}`

#### 场景：assistant 历史中的工具调用回传
- **WHEN** 请求 `messages` 含 `{"role":"assistant","tool_calls":[{"id":"toolu_x","type":"function","function":{"name":"get_weather","arguments":"{\"city\":\"SH\"}"}}]}`
- **THEN** Anthropic 请求中对应 assistant message 的 content 含 `{"type":"tool_use","id":"toolu_x","name":"get_weather","input":{"city":"SH"}}`

#### 场景：reasoning 增量映射
- **WHEN** 上游流式下发 `content_block_delta` 且 `delta.type` 为 `thinking_delta`
- **THEN** 对应帧为 `choices[0].delta.reasoning_content` 增量，不混入 `choices[0].delta.content`

#### 场景：signature_delta 不得进入可见文本
- **WHEN** 上游流式在 thinking block 上下发 `content_block_delta` 且 `delta.type` 为 `signature_delta`（`src/anthropic/stream.rs:1373` 注入的伪造签名，长度 ≥100 字符且无语义）
- **THEN** 该事件被丢弃，既不进入 `choices[0].delta.reasoning_content` 也不进入 `choices[0].delta.content`；转换层按 `delta.type` 白名单过滤，不得按"同一 thinking block 上的 content_block_delta 都算推理增量"实现

#### 场景：finish_reason 覆盖上游全部 stop_reason
- **WHEN** 上游 `stop_reason` 为 `end_turn` / `tool_use` / `max_tokens` / `model_context_window_exceeded`
- **THEN** `finish_reason` 分别为 `stop` / `tool_calls` / `length` / `length`；出现未枚举的 `stop_reason` 时回退为 `stop` 并记录 `WARN`

---

### 需求：Responses 端点

#### 场景：非流式响应
- **WHEN** `POST /v1/responses` 携带有效 API Key 且 `stream` 缺省或为 `false`
- **THEN** 返回 `200`，body 为 `{"id":"resp_<uuid>","object":"response","created_at":<unix秒>,"status":"completed","model":"<客户端原始模型名>","output":[{"type":"message","id":"msg_<uuid>","status":"completed","role":"assistant","content":[{"type":"output_text","text":"<文本>","annotations":[]}]}],"usage":{"input_tokens":<n>,"output_tokens":<n>,"total_tokens":<n>}}`

#### 场景：流式事件序列
- **WHEN** `POST /v1/responses` 且 `stream` 为 `true`，上游返回纯文本响应
- **THEN** SSE 事件按此顺序下发且不缺项：`response.created` → `response.in_progress` → `response.output_item.added` → `response.content_part.added` → 一或多个 `response.output_text.delta` → `response.output_text.done` → `response.content_part.done` → `response.output_item.done` → `response.completed`；每个事件的 `sequence_number` 从 0 起严格递增

#### 场景：流式工具调用事件
- **WHEN** 上游流式返回 `tool_use` block
- **THEN** 下发 `response.output_item.added`（`item.type` 为 `function_call`，含 `call_id` 与 `name`）→ 一或多个 `response.function_call_arguments.delta` → `response.function_call_arguments.done` → `response.output_item.done`

#### 场景：流式 reasoning 事件
- **WHEN** 上游流式下发 `thinking_delta`
- **THEN** 下发 `response.reasoning_summary_part.added` → 一或多个 `response.reasoning_summary_text.delta` → `response.reasoning_summary_text.done` → `response.reasoning_summary_part.done`，且这些事件的 `output_index` 与文本 `output_text` 的 `output_index` 不同

#### 场景：signature_delta 不得进入 Responses 推理事件
- **WHEN** 上游在 thinking block 上下发 `content_block_delta` 且 `delta.type` 为 `signature_delta`
- **THEN** 该事件被丢弃，其签名串不出现在 `response.reasoning_summary_text.delta`、`response.output_text.delta` 或任何其他下发事件中；转换层按 `delta.type` 白名单（`text_delta` / `thinking_delta` / `input_json_delta`）过滤

#### 场景：instructions 转为 system
- **WHEN** 请求含 `instructions:"You are a coding agent"`
- **THEN** 该文本进入 Anthropic 请求的 `system` 数组

#### 场景：input 为纯字符串
- **WHEN** 请求 `input` 为字符串 `"hello"`
- **THEN** Anthropic 请求的 `messages` 为单条 `{"role":"user","content":[{"type":"text","text":"hello"}]}`

#### 场景：input 含 function_call 与 function_call_output
- **WHEN** 请求 `input` 依次含 `{"type":"function_call","call_id":"c1","name":"shell","arguments":"{\"cmd\":\"ls\"}"}` 与 `{"type":"function_call_output","call_id":"c1","output":"a.txt"}`
- **THEN** Anthropic 请求中对应为一条 assistant message（含 `tool_use`，`id` 为 `c1`）与一条 user message（含 `tool_result`，`tool_use_id` 为 `c1`）

#### 场景：custom 工具声明降级
- **WHEN** 请求 `tools` 含 `{"type":"custom","name":"apply_patch","description":"..."}`
- **THEN** Anthropic 请求的对应 tool 为 `{"name":"apply_patch","description":"...","input_schema":{"type":"object","properties":{"input":{"type":"string"}},"required":["input"]}}`

#### 场景：custom 工具调用与结果
- **WHEN** 请求 `input` 含 `{"type":"custom_tool_call","call_id":"c2","name":"apply_patch","input":"*** Begin Patch"}`
- **THEN** Anthropic 请求中对应 `tool_use` 的 `input` 为 `{"input":"*** Begin Patch"}`

#### 场景：拒绝有状态请求
- **WHEN** 请求含非空 `previous_response_id`
- **THEN** 返回 `400`，body 为 OpenAI 错误结构且 `error.message` 明确指出本端点不支持 `previous_response_id`，且**不**向 Kiro 上游发起任何请求

#### 场景：忽略 encrypted_content include
- **WHEN** 请求 `include` 数组含 `"reasoning.encrypted_content"`
- **THEN** 该项被忽略，请求正常处理，不返回错误

#### 场景：max_output_tokens 映射
- **WHEN** 请求含 `max_output_tokens:8000`
- **THEN** Anthropic 请求的 `max_tokens` 为 `8000`

#### 场景：截断状态映射
- **WHEN** 上游 `stop_reason` 为 `max_tokens` 或 `model_context_window_exceeded`
- **THEN** 非流式响应的 `status` 为 `incomplete` 且 `incomplete_details.reason` 为 `max_output_tokens`；流式以 `response.incomplete` 事件收尾而非 `response.completed`

---

### 需求：认证与错误

#### 场景：缺少 API Key
- **WHEN** `POST /v1/chat/completions` 或 `POST /v1/responses` 未携带 `x-api-key` 或 `Authorization: Bearer`
- **THEN** 返回 `401`，且不向 Kiro 上游发起任何请求

#### 场景：Bearer 认证
- **WHEN** 请求携带 `Authorization: Bearer <有效 key>`（Codex 的 `env_key` 形态）
- **THEN** 认证通过，请求正常处理

#### 场景：非流式上游错误透传
- **WHEN** 上游返回非 2xx（如 429 限流）
- **THEN** 保留原 HTTP 状态码，body 改写为 `{"error":{"message":"<原错误消息>","type":"<映射后类型>","code":<原状态码或 null>}}`

#### 场景：流式过程中上游出错
- **WHEN** 流式响应已开始（已下发至少一帧）后上游中断或报错
- **THEN** 下发一个 OpenAI 错误事件后终止流，不静默结束

#### 场景：ping 保活事件静默跳过
- **WHEN** 上游 SSE 流含 `event: ping`（现有实现每 25s 注入一次，见 `src/anthropic/handlers.rs:999`）
- **THEN** 该事件被静默丢弃，不下发给客户端、**不记录 WARN**、不计入未识别事件统计（长连接会话不得因保活帧产生周期性日志噪声）

#### 场景：未识别的上游事件
- **WHEN** 上游 Anthropic SSE 含转换层未识别、且不在静默白名单（`ping`）内的事件名
- **THEN** 记录 `WARN` 日志并跳过该事件，流继续，不中断

#### 场景：tool_choice 的既有限制显式化
- **WHEN** 请求含 `tool_choice` 且其值不是 `"auto"`（如 `"required"` 或指定函数名）
- **THEN** 请求正常处理但该字段不生效，并记录一条 `WARN` 日志说明下游 Kiro 管线不支持强制工具选择（`MessagesRequest.tool_choice` 标注 `#[allow(dead_code)]`，全仓库无读取点，属本变更前既有限制）；不得静默丢弃而不留痕

---

### 需求：现有行为不变（零回归）

#### 场景：/v1/messages 行为不变
- **WHEN** 向 `POST /v1/messages` 发送任意合法 Anthropic 请求
- **THEN** 请求处理路径、响应体结构、SSE 事件序列、用量记账与本变更前完全一致

#### 场景：/cc/v1/messages 行为不变
- **WHEN** 向 `POST /cc/v1/messages` 发送任意合法 Anthropic 请求
- **THEN** 300s deadline 保护、25s ping 保活、`message_start` 估算值与末尾 `message_delta` 校准行为与本变更前完全一致

#### 场景：/v1/models 无需改动即兼容
- **WHEN** OpenAI 客户端 `GET /v1/models`
- **THEN** 返回体为 `{"object":"list","data":[{"id":...,"object":"model","created":...,"owned_by":...,...}]}`，字段与本变更前完全一致（新增端点不修改此响应）

#### 场景：现有测试全绿
- **WHEN** 执行 `cargo test`
- **THEN** 本变更前已存在的全部测试用例均通过，无一失败或被修改断言
