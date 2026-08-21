# 设计：add-openai-compatible-endpoints

## 上下文

现有链路是单协议单向的：Anthropic 请求 → `converter.rs` → `provider.rs`（多账号故障转移）→ Kiro 二进制帧 → `parser/` → `stream.rs` → Anthropic SSE。

`post_messages`（`src/anthropic/handlers.rs:723`，含其调用的 `handle_stream_request` / `handle_non_stream_request` 共约 900 行）是这条链路的唯一入口，且承载了大量正交关注点：认证上下文（`ApiKeyContext`）、客户端 IP 提取、RPM 计数、用量追踪、prompt cache 指纹、模型名派生的 thinking 覆盖、空流检测、`/cc/v1` 的 300s deadline。

新增 OpenAI 协议入口必须复用这全部能力，但用户提出的硬约束是**零回归**：现有功能一律不受影响。这决定了本设计的核心取舍。

## 目标 / 非目标

**目标**

- 让 Codex CLI 与 OpenAI SDK 客户端可直连本代理，用 Kiro 额度跑 `gpt-5.6-terra` / `gpt-5.6-luna`
- 新协议层与现有协议层物理隔离：新增文件为主，现有文件仅追加路由声明
- 完整继承故障转移 / 限流 / 用量统计 / 缓存能力，零重复实现
- 两个协议入口（`chat/completions`、`responses`）共享同一套请求/响应中间表示，避免两份平行实现

**非目标**

- 不追求 OpenAI 协议的完整覆盖（不做 Embeddings / Images / Audio / Assistants / 有状态 Responses）
- 不做性能极致优化（接受一层额外 SSE 解析开销）
- 不重构现有 handler 以求"更干净的分层"

## 决策

### 决策 1：外挂适配器，而非抽取共享内核

**选项 A（采纳）**：新 handler 自行完成 OpenAI → Anthropic 请求转换，然后**直接以普通函数方式调用** `anthropic::handlers::post_messages(State(state), identity, ConnectInfo(addr), headers, converted_body)`，再包裹其返回的 `Response`，把 body 流从 Anthropic SSE 转成 OpenAI SSE。

**选项 B（否决）**：把 `post_messages` 的主体抽取为 `dispatch_messages(state, ..., payload: MessagesRequest, variant: EndpointVariant) -> Response`，两个协议层共同调用。

选 A 的理由：

- `post_messages` 的参数全是可手工构造的具体包装类型（`State`、`Option<Extension<_>>`、`ConnectInfo`、`HeaderMap`、`Bytes`），axum 的 extractor 机制不阻碍直接调用 —— 这使得选项 A 在技术上无障碍
- 选项 B 要触碰故障转移重试、用量记账、deadline 三处最容易出回归的逻辑，与「零回归」硬约束直接冲突
- 选项 A 的代价是可量化且很小的：每个 SSE chunk 多一次 `\n\n` 分帧 + 一次 `serde_json` 解析，微秒级，相对上游网络 IO 可忽略

**代价承认**：Anthropic SSE 被序列化后立刻又被解析，属于"绕一圈"。这是为隔离性付的价，在注释中明确标注为有意选择，避免后人误以为是疏漏。

### 决策 2：请求侧统一收敠到 `MessagesRequest` 的 JSON 表示

两个入口都先把各自协议转成**Anthropic Messages 请求的 `serde_json::Value`**，序列化为 `Bytes` 后交给 `post_messages`。不直接构造 `MessagesRequest` 结构体，原因是该结构体只派生 `Deserialize` 而无 `Serialize` 实现，且 `system` 字段带自定义 `deserialize_with`（字符串/数组双形态）；走 JSON 表示可完全复用其现有反序列化与校验路径，同时保证与 `/v1/messages` 走完全相同的解析代码。

映射表（Chat Completions）：

| OpenAI | Anthropic |
|---|---|
| `model` | `model`（经模型映射） |
| `max_tokens` / `max_completion_tokens` | `max_tokens`（缺省沿用现有默认） |
| `messages[].role=="system"` / `"developer"` | `system[]` |
| `messages[].role=="user"` / `"assistant"` | `messages[]` |
| `messages[].role=="tool"` | `messages[]`（`role:"user"` + `tool_result` block） |
| `messages[].tool_calls[]` | assistant `tool_use` block |
| `tools[].function.parameters` | `tools[].input_schema` |
| `stream` | `stream` |
| `reasoning_effort` | `thinking` |

映射表（Responses）：

| OpenAI Responses | Anthropic |
|---|---|
| `instructions` | `system[]` |
| `input`（字符串形式） | 单条 user message |
| `input[].type=="message"` + `input_text` / `output_text` | user / assistant message |
| `input[].type=="function_call"` | assistant `tool_use` |
| `input[].type=="function_call_output"` | user `tool_result` |
| `input[].type=="custom_tool_call"` | assistant `tool_use`（入参包装为 `{"input": "<text>"}`） |
| `input[].type=="custom_tool_call_output"` | user `tool_result` |
| `tools[].parameters`（`type=="function"`） | `tools[].input_schema` |
| `tools[].type=="custom"` | `tools[].input_schema` = 固定 `{type:"object",properties:{input:{type:"string"}},required:["input"]}` |
| `max_output_tokens` | `max_tokens` |
| `reasoning.effort` | `thinking` |
| `previous_response_id` | 不支持 → 400 |
| `include[]` 含 `reasoning.encrypted_content` | 忽略该项（不报错） |

### 决策 3：响应侧按端点各自成流，共享 Anthropic SSE 解析器

新增一个内部 Anthropic SSE 增量解析器（`src/openai/sse.rs`），把字节流切成 `(event_name, json_value)` 序列。两个响应转换器（`chat_response.rs` / `responses_response.rs`）各自消费这个序列产出目标协议帧。这样 SSE 分帧、跨 chunk 半帧缓冲、`[DONE]` 处理只实现一次。

非流式路径更简单：`post_messages` 返回完整 JSON body，直接解析后重组。

### 决策 4：错误绝不静默吞掉

- 上游返回非 2xx（Anthropic 错误 JSON）时：非流式按 OpenAI 错误结构重写（`{"error":{"message","type","code"}}`）并保留原 HTTP 状态码；流式则发一个 OpenAI `error` 事件后终止流
- 遇到无法识别的 Anthropic 事件名：记 `tracing::warn!` 并跳过该事件，不中断流（前向兼容上游新增事件）
- 请求转换阶段的校验失败（如 `previous_response_id` 存在）：直接返回 400，不进入下游，避免浪费上游配额
- **静默白名单只含 `ping`** —— 现有实现每 25s 注入一次保活帧（`handlers.rs:999`），它是常规行为而非异常，必须静默丢弃；若归入上一条的 WARN 分支，长连接会话会每 25s 打一条噪声日志
- **`tool_choice` 非 `auto` 时不静默丢弃** —— 下游 `MessagesRequest.tool_choice` 标注 `#[allow(dead_code)]` 且全仓库无读取点，强制工具选择本就不生效。这是本变更前的既有限制，但 OpenAI 入口必须把它显式化（WARN 留痕），否则客户端会误以为 `"required"` 已生效

### 决策 4.1：thinking 增量必须按 `delta.type` 白名单过滤

现有 `stream.rs` 会在 thinking block 的 `content_block_stop` 之前注入一条 `delta.type == "signature_delta"` 的 `content_block_delta`（`stream.rs:1345-1373`），内容是为通过下游检测而伪造的 ≥100 字符签名串，无语义。

因此响应转换器**不能**按"落在 thinking block 上的 `content_block_delta` 都算推理增量"来实现，必须显式只认 `delta.type == "thinking_delta"`。否则这串签名会被拼进 `delta.reasoning_content` / `response.reasoning_summary_text.delta`，用户在 Codex 里会看到一段乱码。同理，文本增量只认 `text_delta`，工具入参只认 `input_json_delta`。

### 决策 5：模型名的请求/响应不对称处理

请求侧把客户端模型名映射为 Kiro 模型名；响应侧 `model` 字段回写**客户端原始请求的模型名**。原因：Codex 会校验响应 model 与请求 model 一致，若回写 `gpt-5.6-terra` 而客户端请求的是 `gpt-5-codex`，可能触发客户端侧告警或重试。映射函数是纯函数，单独文件 `model_map.rs`，便于单测穷举 6 条规则。

### 决策 6：路由挂载点

两条新 route 追加进 `src/anthropic/router.rs` 的 `v1_routes`（`:58-66`），而非新建独立 Router 后 merge。理由：

- `v1_routes` 已 `.layer(auth_middleware)`，追加 route 自动获得认证 —— 反之若新建 Router 忘记加 layer，就形成未授权入口
- `/v1` 已被 `nest`，路径拼接后正好是 `/v1/chat/completions` 与 `/v1/responses`，与 Codex 的 `base_url = ".../v1"` 契合
- 对现有文件的改动因此收敠为纯新增的两行 `.route()` + import，diff 最小

### 决策 7：不新增外部 crate

OpenAI 类型用 `serde_json::Value` 做路径操作（参考实现 CLIProxyAPI 也是纯 gjson/sjson 路径操作，不定义结构体），仅对请求入口定义少量 `Deserialize` 结构体做必要校验。SSE 分帧手写。符合 `config.yaml` 的 `rules.design`「不新增外部 crate」。

### 决策 8：参考实现的许可边界

CLIProxyAPI（MIT，`internal/translator/claude/openai/responses/`）提供了 Responses SSE 完整事件序列的事实标准。做法：把它当**行为规范**读，用 Rust 重写；在 `responses_response.rs` 头部注明参考来源与其 MIT 许可，不复制任何代码行。

## 风险 / 权衡

| 权衡 | 取向 | 理由 |
|---|---|---|
| 分层洁净度 vs 零回归 | 选零回归 | 用户明确的硬约束；洁净度可在未来独立重构变更中偿还 |
| 性能（多一层解析）vs 隔离性 | 选隔离性 | 解析开销相对上游 IO 可忽略，而回归成本不可逆 |
| Responses 有状态支持 vs 交付速度 | 先不做，显式 400 | Codex CLI 默认 `store=false` 会带完整 input，无状态足够；静默降级会产生难排查的上下文丢失 |
| `custom` 工具的 freeform 语义损失 | 包装为 `{input: string}` | Kiro 的 schema 规范化会拒绝复杂/空 schema；包装后语义等价且能通过校验 |
| 两阶段交付 vs 一次性交付 | 两阶段 | `chat/completions` 结构简单，先打通可验证「请求转换 → 下游复用 → 响应转换」整条脉络，再上 Responses 的 40+ 事件 |

## 已知限制（本次不修正，仅标注）

- `gpt-5.6-luna` 上游恒返回 `thinking=0`，`reasoning.effort` 对其无实际效果
- `context_window_for_model` 对所有模型硬返回 1M，`gpt-5.6-*` 的真实窗口未标定
- 客户端 token 显示缩放（`CLIENT_TOKEN_DISPLAY_SCALE`）会同样作用于新端点的 usage 上报
- `gpt-5.6` 系列不支持 `additionalModelRequestFields`，现有 converter 已整体跳过该字段
