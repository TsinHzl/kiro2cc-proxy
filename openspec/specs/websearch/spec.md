# websearch 能力规范

## Purpose

将 Anthropic server tool 协议的 WebSearch（web_search_20250305）桥接到 Kiro 上游的真实 MCP web_search 能力，使 Claude Code 等客户端通过本反代使用模型时，WebSearch 工具真实可用：模型自主触发搜索、反代服务端闭环执行、结果回填续请求，客户端全程以官方 server tool 格式接收。流式与非流式请求均支持桥接；未携带 web_search 的请求行为保持现状不变。

## ADDED Requirements

### Requirement: web_search server tool 识别与剔除

系统 SHALL 在协议转换时识别 Anthropic server tool 格式的 web_search 工具（`tool_type` 含 `web_search` 或 `name == "web_search"`），并将其从发送给 Kiro 的 `context.tools` 中剔除，同时保留其 `max_uses` 供桥接层使用。

#### Scenario: 混合工具列表剔除 server tool
- **WHEN** 客户端请求 tools 同时包含普通工具（Bash/Read）与 `{"type": "web_search_20250305", "name": "web_search", "max_uses": 3}`
- **THEN** 转换后的 Kiro context.tools 仅含普通工具，web_search 不出现
- **AND** 桥接层获得 max_uses = 3

#### Scenario: 无 server tool 时零影响
- **WHEN** 客户端请求 tools 不含 web_search
- **THEN** 转换结果与现有行为逐字节一致

### Requirement: web_search 占位符排除

系统 SHALL 在为历史工具名生成占位符定义时排除 `web_search` 名称——历史中的 web_search toolUse 不生成占位符。

#### Scenario: 历史含桥接产生的 web_search toolUse
- **WHEN** 历史消息的 assistant tool_uses 包含 `web_search`
- **THEN** context.tools 中不出现 web_search 的占位符定义

### Requirement: web_search 调用截获与服务端执行

系统 SHALL 在 Kiro 流（流式或非流式）中出现 `toolUseEvent(name="web_search")` 时截获该事件（不透传为普通 tool_use），聚合其 input 分片得到 query，调用 Kiro MCP `tools/call` 真实执行搜索。流式请求以 `server_tool_use` + `web_search_tool_result` 格式的 SSE content block 呈现（不重发 message_start）；非流式请求在同步 JSON 响应的 content 数组中以相同格式的块呈现。

#### Scenario: 模型触发搜索
- **WHEN** Kiro 流输出 `toolUseEvent(name="web_search", input={"query": "..."})` 完整结束
- **THEN** 反代调用 Kiro MCP `tools/call` 执行搜索，不向客户端发送普通 tool_use 块
- **AND** 向客户端发送 `server_tool_use` + `web_search_tool_result` 格式的 content block 序列

#### Scenario: 非目标工具不受影响
- **WHEN** Kiro 流输出 `toolUseEvent(name="Bash")`
- **THEN** 该事件按现有普通 tool_use 逻辑透传，桥接状态机不介入

#### Scenario: 非流式请求桥接
- **WHEN** 客户端发送 `stream=false` 且 tools 含 web_search server tool 的请求，Kiro 非流式响应中出现完整 `toolUseEvent(name="web_search", input={"query": "..."})`
- **THEN** 反代调用 Kiro MCP `tools/call` 执行搜索，构建续请求（回填搜索结果）并再次调用上游，基于最终响应组装同步 JSON 返回客户端
- **AND** 客户端响应 content 数组中包含 `server_tool_use` + `web_search_tool_result` 格式的块，不出现裸 tool_use 块

### Requirement: 搜索结果回填续请求

系统 SHALL 基于首次转换的 ConversationState 手工构建续请求（不经 convert_request 重转换）：conversationId、agentContinuationId、history 保持不变，`context.tool_results` 回填搜索结果，并通过 `call_api_stream` 续流，续流事件链入同一客户端 SSE 流（不重发 message_start，message_stop 由桥接收尾统一补发）。已流出的 assistant 文本/thinking 不回填续请求（依赖 Kiro 同 conversationId 会话续接）。

#### Scenario: 续请求保持缓存不变量
- **WHEN** 桥接构建续请求
- **THEN** 续请求的 conversationId 与 agentContinuationId 与首次请求完全一致
- **AND** history 与首次请求逐字节一致

#### Scenario: 多轮桥接状态演进
- **WHEN** 单请求内发生第 2 轮及后续轮次桥接
- **THEN** 每轮续请求基于上一轮续请求所用的 ConversationState 演进（`current_message.tool_results` 替换为本轮结果）
- **AND** 所有轮次的 history 与首次请求逐字节一致（中间轮 toolResults 不沉入 history）

#### Scenario: 续流块索引单调
- **WHEN** 续流事件链入同一客户端 SSE 流
- **THEN** 全部 content block 的 index 沿首次流的块索引单调递增（复用同一 StreamContext 分配），无重复 index

#### Scenario: Kiro 拒绝续请求
- **WHEN** 续请求被 Kiro 拒绝（HTTP 错误 / 流立即异常）
- **THEN** 客户端主流收到一个 error 事件（说明搜索不可用）后正常收尾
- **AND** 客户端 SSE 流不悬挂、不中断连接

#### Scenario: 桥接收尾补发 message_stop
- **WHEN** 桥接全部轮次结束（正常完成、降级或续请求失败）
- **THEN** 客户端恰好收到一次 message_stop

### Requirement: 多轮搜索上限

系统 SHALL 将单次客户端请求内的桥接搜索轮数限制为 `min(max_uses, 5)`；达到上限后不再截获后续 web_search 调用，流式请求改按普通 tool_use SSE 透传，非流式请求按普通 tool_use 块透传。

#### Scenario: 超出上限
- **WHEN** 单请求内已桥接 5 轮（或 max_uses 次），模型再次触发 web_search
- **THEN** 不再执行真实搜索，该 toolUseEvent 在流式请求按普通 tool_use SSE 透传、在非流式请求按普通 tool_use 块透传给客户端

### Requirement: MCP 失败降级

系统 SHALL 在 MCP 调用失败时回填 `ToolResult::error(tool_use_id, 错误信息)` 并续请求，客户端响应（流式 SSE 或非流式 JSON）不中断。

#### Scenario: MCP 调用失败
- **WHEN** call_mcp_api 返回错误（网络/上游 4xx/解析失败）
- **THEN** 续请求回填 status="error" 的 ToolResult
- **AND** 客户端响应继续由续请求输出（流式为续流 SSE，非流式为续请求响应组装的 JSON），模型向用户说明搜索失败

### Requirement: 非桥接请求行为不变

系统 SHALL 保证未携带 web_search server tool 的请求，其处理路径与转换结果与本变更前逐字节一致。携带 web_search 的请求（流式/非流式）均走桥接；仅当桥接路径整体失败时按风险表降级，不静默回退到非桥接路径。

#### Scenario: 非流式请求不走桥接
- **WHEN** 客户端发送 `stream=false` 且 tools 不含 web_search server tool 的请求
- **THEN** 走现有普通转换路径，不发起桥接续请求，行为与现状逐字节一致

#### Scenario: 单工具纯搜索请求走旧拦截式
- **WHEN** 流式请求有且只有一个 web_search 工具（旧 `has_web_search_tool` 条件命中）
- **THEN** 仍走现有拦截式处理路径，行为不变

### Requirement: 端点差异（/v1 与 /cc/v1）

系统 SHALL 在 `/v1/messages` 与 `/cc/v1/messages` 两个端点提供一致的桥接行为，差异仅在全局超时约束：`/cc/v1` 的 300s 全局 deadline 作用于桥接总时长（含全部轮次的 MCP 调用与续请求），`/v1` 无全局 deadline（多轮桥接不受限）。

#### Scenario: /cc/v1 桥接受 300s deadline 约束
- **WHEN** 客户端通过 `/cc/v1/messages` 发起携带 web_search 的流式请求，桥接总时长（含多轮 MCP 与续流）超过 300s
- **THEN** 全局 deadline 到期时客户端收到 overloaded_error 事件（与现有 /cc/v1 超时行为一致）

#### Scenario: /v1 桥接无全局 deadline
- **WHEN** 客户端通过 `/v1/messages` 发起携带 web_search 的请求
- **THEN** 桥接不受全局 deadline 限制，行为与 /v1 现有转发一致（仅受各次上游调用自身的超时分档约束）

### Requirement: MCP 调用不产生 token 计费

系统 SHALL 保证桥接过程中的 Kiro MCP `tools/call` 调用不产生 token 用量统计与计费——用量仅来自消息模型调用（首次请求与各轮续请求），MCP 搜索调用不计入 token 计费（沿用现有 websearch.rs 拦截式路径的统计口径）。

#### Scenario: MCP 调用不进用量统计
- **WHEN** 桥接过程中发生 1 次或多轮 MCP web_search 调用
- **THEN** 这些调用的消耗不计入 UsageTracker / RPM 之外的任何 token 用量统计
- **AND** token 用量仅由消息模型调用（首次 + 续请求）的 metering/估算产生
