# 任务清单：websearch-bridge

## 状态：ARCHIVED

## 任务

### 1. converter.rs：web_search server tool 识别与剔除
- [x] 新增 `pub(crate) fn is_web_search_server_tool(t: &super::types::Tool) -> bool`：`tool_type` 含 `web_search` 或 `name == "web_search"` 即命中
- [x] 新增 `pub(crate) fn split_web_search_tool(req: &MessagesRequest) -> Option<(Option<i32>, Vec<super::types::Tool>)>`：命中 server tool 时返回 `Some((max_uses, 剔除后的普通工具列表))`，未命中返回 `None`
- [x] `convert_request` 第 6 步改用拆分后的工具列表转换（未命中时行为与现状逐字节一致）
- [x] `collect_history_tool_names` 循环处跳过 `web_search` 名称，排除占位符生成
- [x] **单测**（converter::tests）：混合列表剔除 / 无 server tool 透传 / 占位符排除
- 验证：`cargo test web_search` + `cargo check`

### 2. handlers.rs：桥接上下文传参（D5）
- [x] 定义 `BridgeContext` 结构体（conversation_state clone / profile_arn / additional_model_request_fields / max_uses / **bound_ids** / **is_compact_request**）
- [x] `post_messages` 在构建 KiroRequest 前：`split_web_search_tool` 命中 → 构造 `Option<BridgeContext>`（流式/非流式均构造，见 D7）；同时按 D7 分派条件保留旧拦截式优先级
- [x] `handle_stream_request` 签名追加 `bridge_ctx: Option<BridgeContext>` 参数；`/v1` 与 `/cc/v1` 两个调用点各传实参
- [x] `handle_non_stream_request` 签名同样追加 `bridge_ctx: Option<BridgeContext>` 参数；非流式调用点传实参（D5 非流式段）
- [x] **单测**：BridgeContext 构造条件（流式命中/非流式命中/未命中不构造/混合列表命中）
- 验证：`cargo check` + 现有 `cargo test` 无回归（613 passed；新增 4 个 build_bridge_context 单测全绿）

### 3. handlers.rs：桥接状态机——截获与聚合
- [x] 定义 `BridgeState` 枚举（PassThrough / Collecting），嵌入 `create_sse_stream` 的 unfold 状态元组（`Option<BridgeState>`，None 时全部分支短路）
- [x] PassThrough → 截获 `Event::ToolUse(name="web_search")`（且轮次未达上限）→ Collecting：聚合 input 分片、记录 tool_use_id，不透传为 tool_use SSE
- [x] 截获后向客户端发送 `server_tool_use` + `web_search_tool_result` SSE 块（复用 websearch.rs 事件格式，不重发 message_start；块索引经 state_manager 分配）
- [x] Collecting 期间 AssistantResponse（说明文字）正常透传
- [x] **单测**：input 分片聚合 / 非目标工具透传 / 截获不发 tool_use SSE
- 验证：`cargo test`（617 passed，含 4 个新桥接单测）+ `cargo check` + `cargo clippy` clean（本次改动文件零警告）

### 4. handlers.rs：桥接状态机——MCP 调用与续请求续流
- [x] 流结束后（上游 None / 错误兜底不触发时）`call_mcp_api` 真实搜索；MCP 失败 → `ToolResult::error` 降级
- [x] 构建续请求：**基于当前 `BridgeContext.conversation_state` 演进**（多轮时 clone 上一轮续请求所用状态，`current_message.tool_results` 替换为本轮结果，history 不追加中间轮 toolResults，见 D3 多轮语义），绕过 `validate_tool_pairing`（手工构建，不走 convert_request）；`conversation_id` / `agent_continuation_id` / `history` 逐字节不变
- [x] `call_api_stream` 续流链入同一客户端 SSE——**复用同一 StreamContext**（`ctx` 原样携带，body_stream/decoder 替换为续请求的响应流，见 D8），不重发 message_start；桥接收尾统一调用一次 `generate_final_events` 补发 final events（含 message_stop），含续请求失败时的 error 事件兜底（防客户端悬挂）
- [x] Kiro 拒绝续请求 → 主流补发 error 事件（说明搜索不可用）+ 桥接收尾 final events 后正常收尾
- [x] **单测**：续请求体构建（conversationId/agentContinuationId/history 不变 + tool_results 回填）/ 多轮演进语义（第 2 轮续请求基于第 1 轮状态、history 仍逐字节不变）/ MCP 失败 → error ToolResult / 续请求失败 → error 事件收尾
- 验证：`cargo test`（621 passed，含 4 个新续请求单测）+ `cargo check` + `cargo fmt` clean + 本次改动文件 `cargo clippy` 零警告

### 5. handlers.rs：多轮上限与收尾
- [x] 多轮上限 `min(max_uses, 5)`；达到上限后的 web_search toolUse：流式按普通 tool_use SSE 透传，非流式按普通 tool_use 块透传（D8 定义的行为）
- [x] 桥接整体收尾：全部轮次结束（或降级）后统一补发 message_stop
- [x] **单测**：上限计数 / 上限后透传（流式 + 非流式）/ 收尾 message_stop 补发
- 验证：`cargo test`（624 passed，含 3 个任务 5 新单测：上限计数/耗尽透传/message_stop 恰好一次）+ `cargo check` + `cargo fmt` clean + `cargo clippy`（3 条警告均在未改动文件，既有代码）

> 说明：流式上限（`BridgeState::new` 的 `clamp(0, 5)` + `has_remaining_rounds` + 轮次耗尽 PassThrough 透传）与收尾（`generate_final_events` 的 `message_ended` 门控恰好一次 message_stop）在任务 3/4 实现中已内置；本任务补齐单测验证。非流式路径的上限与透传随任务 5.5 在 `handle_non_stream_request` 事件循环中实现。

### 5.5 handlers.rs：非流式桥接（D4 非流式段 / D5 非流式段）
- [x] `handle_non_stream_request` 事件循环内截获 `Event::ToolUse(name="web_search")`（轮次未达上限）：按 tool_use_id 聚合 input 分片，stop 时截获完成，不解析为普通 tool_use 块
- [x] 事件循环结束后：`call_mcp_api` → 基于当前 BridgeContext.conversation_state 按 D3 语义构建续请求 → `provider.call_api` 再次调用 → 新 EventStreamDecoder 第二次事件循环收集 → 基于最终响应组装同步 JSON
- [x] 最终 content 数组追加 `server_tool_use` + `web_search_tool_result` 块，不出现裸 tool_use 块
- [x] 非流式 MCP 失败 → error ToolResult 回填续请求；续请求失败 → 按现有错误响应路径收尾（不悬挂）
- [x] **单测**：非流式截获聚合 / 非流式续请求体不变量（conversationId/agentContinuationId/history）/ 非流式 MCP 失败降级 / 非流式上限后透传普通 tool_use 块
- 验证：`cargo test`（629 passed，含 5 个新非流式桥接单测：截获聚合/query 解析失败兜底/上限与上限耗尽透传/非目标工具透传/结果块格式与 MCP 失败空数组）+ `cargo check` 零警告 + `cargo fmt` clean + `cargo clippy`（3 条警告均在未改动文件，既有代码）

> 说明：非流式续请求体不变量与 MCP 失败降级复用流式任务 4 已有的 `build_continuation_request` 单测（conversationId/agentContinuationId/history 逐字节不变 + error ToolResult），非流式单测聚焦截获状态机（`non_stream_bridge_step`）与可见性块格式（`build_web_search_result_block`），二者共用同一套构建函数。

### 6. 测试 + fmt + clippy 验收
- [x] `cargo test` 全绿（600+ 现有用例无回归；最终 630 passed，含 CR 修复后全量重跑）
- [x] `cargo fmt` 无 diff
- [x] `cargo clippy` clean（3 条警告均在未改动文件，既有代码；handlers.rs 零警告）
- [ ] 真实 CC 会话触发搜索验证（实测 D3/D6 假设：Kiro 对续请求携带 toolResults 的接受度、会话续接；失败按 design D6 降级 / 风险表收尾）；流式与非流式各验证一次

## 验收标准
- [ ] 单测覆盖：`split_web_search_tool` 剔除/透传、续请求体构建（不变量 + 回填）、多轮演进语义、上限计数、收尾 message_stop 恰好一次、非流式桥接（截获/续请求/降级/上限透传）、MCP 调用不计入 token 用量统计（`cargo test` 全绿）
- [ ] 集成验收（真实 CC 会话）：流式请求下 CC 使用本反代时 WebSearch 真实可用——模型触发 `toolUseEvent(name="web_search")` 后，客户端收到 `server_tool_use` + `web_search_tool_result` 块且模型基于结果继续输出
- [ ] 非流式请求（stream=false）携带 web_search 时同样真实桥接：同步 JSON 的 content 数组含 `server_tool_use` + `web_search_tool_result` 块，无裸 tool_use 块
- [ ] 多轮搜索受 `min(max_uses, 5)` 上限约束；超限后流式按普通 tool_use SSE 透传、非流式按普通 tool_use 块透传
- [ ] MCP 失败时客户端响应不中断（流式 error ToolResult 回填或 error 事件收尾；非流式 error ToolResult 回填续请求）
- [ ] 续请求 conversationId / agentContinuationId 与首次请求一致，且全部轮次 history 逐字节不变（prompt cache 不变量保持，由单测断言）
- [ ] 未携带 web_search 的请求行为与现状逐字节一致
- [ ] MCP 调用不产生 token 用量统计与计费（用量仅来自消息模型调用，沿用现有拦截式口径）
