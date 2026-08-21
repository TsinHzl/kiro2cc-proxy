# 任务清单：add-openai-compatible-endpoints

## 状态：ARCHIVED

## 任务

### 阶段一：Chat Completions（打通链路）

- [x] T1 建立模块骨架：新增 `src/openai/mod.rs`，在 `src/main.rs` 声明 `mod openai;`。空模块须通过 `cargo check`
      （spec.md 引用：无直接场景，为后续任务前置）
- [x] T2 实现模型名映射 `src/openai/model_map.rs`：纯函数 `map_model(client_model: &str) -> &str`。规则优先级：codex 系精确映射 → `gpt-5.6-` **前缀**透传（覆盖 `sol` / `terra` / `luna` 及上游未来新增型号）→ `claude-` 前缀透传 → 其余 `gpt-*` 兜底 `gpt-5.6-terra` 并 `WARN`。附单元测试穷举全部规则 + 显式断言 `gpt-5.6-sol` 不被改写 + 空串 + 非 gpt 非 claude 名
      （spec.md 引用：需求「模型名映射」全部场景）
- [x] T3 实现 Anthropic SSE 增量解析器 `src/openai/sse.rs`：输入字节流片段，输出 `(event_name, serde_json::Value)` 序列，正确处理跨 chunk 半帧缓冲与 `data: [DONE]`；`ping` 事件归入静默白名单（丢弃且不 WARN）。附单元测试：单帧、跨 chunk 断帧、连续多帧、空行噪声、ping 帧被静默丢弃
      （spec.md 引用：需求「Chat Completions 端点」流式场景、需求「Responses 端点」流式场景的公共前置；「ping 保活事件静默跳过」）
- [x] T4 实现 Chat Completions 请求转换 `src/openai/chat_request.rs`：OpenAI 请求 JSON → Anthropic 请求 JSON。附单元测试：system/developer 提取、tool 声明转换、`role:"tool"` → `tool_result`、assistant `tool_calls` → `tool_use`、`max_completion_tokens` 兼容、`reasoning_effort` → `thinking`、`tool_choice` 非 `auto` 时 WARN 留痕
      （spec.md 引用：「system 消息转为 Anthropic system」「工具声明转换」「工具结果回传」「assistant 历史中的工具调用回传」「tool_choice 的既有限制显式化」）
- [x] T5 实现 Chat Completions 非流式响应转换 `src/openai/chat_response.rs`（非流式部分）：Anthropic 响应 JSON → `chat.completion` JSON，含 usage 换算、`finish_reason` 全 4 种 `stop_reason` 映射（`end_turn`→`stop`、`tool_use`→`tool_calls`、`max_tokens`/`model_context_window_exceeded`→`length`，未枚举值回退 `stop` 并 WARN）、`tool_calls` 组装、model 回写客户端原始名。附单元测试逐场景断言
      （spec.md 引用：「非流式对话」「非流式 usage 与上游一致」「模型请求调用工具（非流式）」「响应回写客户端原始模型名」「finish_reason 覆盖上游全部 stop_reason」）
- [x] T6 实现 Chat Completions 流式响应转换 `src/openai/chat_response.rs`（流式部分）：Anthropic SSE → `chat.completion.chunk` SSE，含 `delta.role` 首帧、`delta.content` 增量、`tool_calls[].index` 稳定分配、`delta.reasoning_content`、`finish_reason` 帧、`stream_options.include_usage` 的 usage 帧、`[DONE]`。**增量必须按 `delta.type` 白名单过滤**：只认 `text_delta` / `thinking_delta` / `input_json_delta`，显式丢弃 `signature_delta`。附单元测试：纯文本流、单工具流、双并发工具流（断言 index 不重复）、thinking 流、含 `signature_delta` 的 thinking 流（断言签名串不出现在任何输出帧中）
      （spec.md 引用：「流式对话的首帧与末帧」「流式含 usage 上报」「模型请求调用工具（流式）」「reasoning 增量映射」「signature_delta 不得进入可见文本」）
- [x] T7 实现错误映射 `src/openai/error.rs`：Anthropic 错误 JSON → OpenAI 错误结构（保留 HTTP 状态码）；流式中途出错的 error 事件构造；未识别事件的 `WARN` 跳过策略（`ping` 除外，见 T3）。附单元测试：429、400、500 三种状态码映射
      （spec.md 引用：需求「认证与错误」的「非流式上游错误透传」「流式过程中上游出错」「未识别的上游事件」）
- [x] T8 实现 handler `src/openai/handlers.rs::post_chat_completions`：调用 T4 转换 → 调用 `anthropic::handlers::post_messages` → 按 `stream` 分流走 T5 或 T6 包裹响应体。不修改 `post_messages` 任何签名或实现
      （spec.md 引用：需求「Chat Completions 端点」全部场景的集成点）
- [x] T9 挂载路由：在 `src/anthropic/router.rs` 的 `v1_routes` 追加 `.route("/chat/completions", post(...))`，仅新增 import 与一行 route。在 `src/main.rs` 启动日志追加一行端点提示
      （spec.md 引用：需求「认证与错误」的「缺少 API Key」「Bearer 认证」）
- [x] T10 阶段一验证：`cargo fmt` + `cargo clippy`（不新增 warning）+ `cargo test` 全绿；启动本地服务，用 `curl` 验证非流式与流式两条路径，并验证无 API Key 时返回 401
      （spec.md 引用：需求「现有行为不变（零回归）」的「现有测试全绿」）

### 阶段二：Responses API（Codex 全功能）

- [x] T11 实现 Responses 请求转换 `src/openai/responses_request.rs`：`instructions` → system；`input` 字符串与数组两种形态；`message` / `function_call` / `function_call_output` / `custom_tool_call` / `custom_tool_call_output` 五类 item；`tools` 的 `function` 与 `custom` 两类；`max_output_tokens`、`reasoning.effort`；`tool_choice` 非 `auto` 时 WARN 留痕；`previous_response_id` 非空则返回 400；`include` 的 `reasoning.encrypted_content` 忽略。附单元测试逐场景断言
      （spec.md 引用：「instructions 转为 system」「input 为纯字符串」「input 含 function_call 与 function_call_output」「custom 工具声明降级」「custom 工具调用与结果」「拒绝有状态请求」「忽略 encrypted_content include」「max_output_tokens 映射」「tool_choice 的既有限制显式化」）
- [x] T12 实现 Responses 非流式响应转换 `src/openai/responses_response.rs`（非流式部分）：Anthropic 响应 → `response` 对象（含 `output[]` 结构、`usage` 三字段、model 回写）；`stop_reason` 为 `max_tokens` / `model_context_window_exceeded` 时 `status` 为 `incomplete` 且 `incomplete_details.reason` 为 `max_output_tokens`。附单元测试含截断分支
      （spec.md 引用：需求「Responses 端点」的「非流式响应」「截断状态映射」）
- [x] T13 实现 Responses 流式响应转换 `src/openai/responses_response.rs`（流式部分）：产出完整事件序列，维护 `sequence_number` 严格递增与 `output_index` / `content_index` 分配；增量按 `delta.type` 白名单过滤（丢弃 `signature_delta`）；截断时以 `response.incomplete` 收尾而非 `response.completed`。文件头注明事件序列参考 CLIProxyAPI（MIT）。附单元测试：纯文本序列完整性、工具调用序列、reasoning 序列（断言签名串不出现）、sequence_number 单调性、截断收尾分支
      （spec.md 引用：「流式事件序列」「流式工具调用事件」「流式 reasoning 事件」「signature_delta 不得进入可见文本」「截断状态映射」）
- [x] T14 实现 handler `src/openai/handlers.rs::post_responses` 并挂载 `.route("/responses", post(...))`（同 T9 方式，仅追加）
      （spec.md 引用：需求「Responses 端点」全部场景的集成点）
- [x] T15 阶段二验证：`cargo fmt` + `cargo clippy` + `cargo test` 全绿；用真实 Codex CLI 配置 `wire_api = "responses"` 端到端跑一次含工具调用的任务（读文件 + 改文件）
      （spec.md 引用：需求「Responses 端点」全部场景）

### 收尾

- [x] T16 零回归回归验证：`cargo test` 确认变更前已存在的测试全部通过且断言未被修改；手工冒烟 `POST /v1/messages`（流式 + 非流式）、`POST /cc/v1/messages`、`GET /v1/models`、Admin UI 登录，确认行为无变化
      （spec.md 引用：需求「现有行为不变（零回归）」全部 4 个场景）
- [x] T17 文档更新：`README.md` 补 Codex CLI 与 OpenAI SDK 接入说明（含 `~/.codex/config.toml` 示例、`wire_api` 两种取值、`previous_response_id` 不支持的说明）；`docs/代码速查表.md` 补 `src/openai/` 模块条目
      （spec.md 引用：无，交付完整性要求）

## 验收标准

- [x] `cargo fmt --check` 无差异；`cargo clippy` 不新增 warning（仓库既存 warning 不计）
- [x] `cargo test` 全绿，且变更前已存在的测试用例数量不减少、断言未被修改
- [x] spec.md 中每个 WHEN/THEN 场景均有对应单元测试或已记录的手工验证结果
- [x] `POST /v1/chat/completions` 非流式与流式均可返回正确内容，含工具调用往返
- [x] `POST /v1/responses` 非流式与流式均可返回正确内容，含工具调用往返
- [x] 真实 Codex CLI（`wire_api = "responses"`）可完成一次含读写文件工具调用的任务
- [x] 两个新端点在无 API Key 时返回 401
- [x] `previous_response_id` 非空时返回 400 且未产生上游请求
- [x] 含 thinking 的流式响应中，`signature_delta` 的签名串不出现在任何下发帧内（有单元测试断言）
- [x] 长连接流式会话不产生周期性 `ping` 相关 WARN 日志
- [x] `stop_reason` 的 4 种取值均有明确的 `finish_reason` / `status` 映射且有测试覆盖
- [x] `POST /v1/messages`、`POST /cc/v1/messages`、`GET /v1/models`、Admin UI 手工冒烟通过，行为与变更前一致
- [x] `Cargo.toml` 的 `[dependencies]` 未新增任何条目
- [x] `src/anthropic/handlers.rs` 的 diff 为空（零改动）
