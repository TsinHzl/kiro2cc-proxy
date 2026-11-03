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
# 规范增量：模型规格对齐 Anthropic 官方

## 修改需求

### 需求：模型上下文窗口判断（`src/anthropic/stream.rs::context_window_for_model`）

#### 场景：opus-4.6 走 1M 窗口
- **WHEN** 调用 `context_window_for_model` 传入包含子串 `"opus-4-6"` 或 `"opus-4.6"` 的模型 ID
- **THEN** 返回 `1_000_000`

#### 场景：fable-5 走 1M 窗口
- **WHEN** 调用 `context_window_for_model` 传入包含子串 `"fable-5"` 或 `"fable_5"` 的模型 ID
- **THEN** 返回 `1_000_000`

#### 场景：opus-4.7 / opus-4.8 / sonnet-4.6 行为不变
- **WHEN** 调用 `context_window_for_model` 传入包含 `"opus-4-7"` / `"opus-4-8"` / `"sonnet-4-6"` 的模型 ID
- **THEN** 返回 `1_000_000`

#### 场景：haiku-4-5 / 旧版 / 未识别模型走默认 200K
- **WHEN** 调用 `context_window_for_model` 传入 `"claude-haiku-4-5-20251001"` 或不在 1M 列表的任何模型 ID
- **THEN** 返回 `200_000`

### 需求：缓存命中 token 反推（`src/anthropic/stream.rs::infer_cache_read_tokens`）

#### 场景：opus-4.6 使用与 opus-4.7/4.8 同档 k_ref
- **WHEN** 调用 `infer_cache_read_tokens` 传入 `model` 包含子串 `"opus-4-6"` 或 `"opus-4.6"`
- **THEN** 内部 `(k_ref, input_price, output_price)` 取 `(2.60, 15.0, 75.0)`，与 opus-4.7/4.8 同（依据：官方单价均 .00 / .00）

#### 场景：fable-5.1 映射到独立的上游模型 ID
- **WHEN** 调用 `map_model` 传入包含 `fable` 且版本号为 5.1 的模型名（`fable-5.1` / `fable-5-1` / `Fable 5.1`，可带 `-thinking` 或日期后缀）
- **THEN** 返回 `claude-fable-5.1`
- **AND** 其他 fable 名（`fable-5`、`fable-5-10` 等）仍返回 `claude-fable-5`
- **AND** `model_max_output_tokens` 对 fable 返回 128K，`min_cacheable_tokens("claude-fable-5.1")` 为 512
- **NOTE** Kiro 仅对特定账号 / 区域开放 Fable 5.1，能否使用由上游账号权限决定；静态模型表列出 `claude-fable-5-1` 与 `claude-fable-5-1-thinking`

#### 场景：fable-5 沿用 opus 顶端 k_ref（占位）
- **WHEN** 调用 `infer_cache_read_tokens` 传入 `model` 包含子串 `"fable"`
- **THEN** 内部 `(k_ref, input_price, output_price)` 取 `(2.60, 15.0, 75.0)` 作为占位值
- **AND** 函数返回 `Some(v)` 且 `v` 落在 `[0, total]` 区间，**不返回 None**

#### 场景：opus-4.5 及更早 / sonnet / haiku 行为不变
- **WHEN** 调用 `infer_cache_read_tokens` 传入 model 不含 4-6/4.6/4-7/4.7/4-8/4.8/fable 子串
- **THEN** 沿用本变更前的 k_ref 表（opus 4.5 → 2.40；sonnet → 7.06；haiku → None）

### 需求：模型路由（`src/anthropic/converter.rs::map_model`）

#### 场景：fable-5 路由到 Kiro fable-5 ID
- **WHEN** 调用 `map_model` 传入小写化后包含子串 `"fable"` 的模型 ID（如 `"claude-fable-5"`、`"claude-fable-5-thinking"`）
- **THEN** 返回 `Some("claude-fable-5".to_string())`

#### 场景：原有 sonnet / opus / haiku 路由不变
- **WHEN** 调用 `map_model` 传入不含 `"fable"` 的模型 ID
- **THEN** 返回值与本变更前一致（sonnet-4.6 → claude-sonnet-4.6、opus-4-6 默认 → claude-opus-4.6、opus-4-7 → claude-opus-4.7、opus-4-8 → claude-opus-4.8、haiku → claude-haiku-4.5）

### 需求：`/v1/models` 暴露的模型清单（`src/anthropic/handlers.rs::build_model_list`）

#### 场景：opus-4.6 暴露 128K max_tokens
- **WHEN** GET `/v1/models`
- **THEN** 响应 `data[]` 中存在 `id="claude-opus-4-6"` 与 `id="claude-opus-4-6-thinking"` 两条目
- **AND** 两者 `max_tokens` 字段值均为 `128000`

#### 场景：fable-5 出现在模型列表
- **WHEN** GET `/v1/models`
- **THEN** 响应 `data[]` 中存在 `id="claude-fable-5"` 条目
- **AND** 该条目 `max_tokens=128000`、`owned_by="anthropic"`、`object="model"`、`model_type="chat"`、`display_name="Claude Fable 5"`

#### 场景：fable-5-thinking 同步出现
- **WHEN** GET `/v1/models`
- **THEN** 响应 `data[]` 中存在 `id="claude-fable-5-thinking"` 条目
- **AND** 该条目 `max_tokens=128000`、`display_name="Claude Fable 5 (Thinking)"`

#### 场景：其他模型规格保持不变
- **WHEN** GET `/v1/models`
- **THEN** 以下条目的 `max_tokens` 字段与本变更前完全一致：
  - `claude-sonnet-4-6` → `64000`
  - `claude-sonnet-4-6-thinking` → `64000`
  - `claude-opus-4-7` → `128000`
  - `claude-opus-4-7-thinking` → `128000`
  - `claude-opus-4-8` → `128000`
  - `claude-opus-4-8-thinking` → `128000`
  - `claude-haiku-4-5-20251001` → `64000`
  - `claude-haiku-4-5-20251001-thinking` → `64000`
  - 旧版 3.x / 4 / 4.5 / auto / deepseek / glm 条目全部保持原值


## 修改需求（issue #59）

### 需求：会话中途内联 system 通知就地放置（`src/anthropic/converter/convert.rs` / `history.rs`）

#### 场景：被分流的内联通知挂回相邻 user 消息
- **WHEN** 会话已有 user/assistant 消息之后出现 `role:"system"` 内联消息，且它被判定为动态块（已知 hook/通知前缀，或相对会话基线是中途新增）
- **THEN** 它不进入 `history[0]`，也不再每轮置顶到当前消息开头，而是以 `<system-reminder>` 包裹挂到相邻 user 消息：优先挂到紧随其后的 user 消息开头，否则挂到紧邻的前一条 user 消息末尾，夹在两条 assistant 之间时就近取后面再取前面的 user 消息
- **AND** 同一通知在后续轮次里挂靠的 user 消息文本逐字节不变（前缀缓存稳定）

#### 场景：通知不得顶替当前消息
- **WHEN** 通知位于最后一条 user 消息（例如只含 tool_result）之后
- **THEN** 通知挂到该 user 消息末尾，该消息仍是当前消息并保留其 tool_result；正文为空时补 `(tool result above)` 占位

#### 场景：其余路径不变
- **WHEN** 动态块来自顶层 `system`，或是 `<total_tokens>` 预算提示，或内联 system 位于会话开头
- **THEN** 顶层动态块与预算提示仍放当前消息开头，会话开头的内联 system 仍并入系统区
