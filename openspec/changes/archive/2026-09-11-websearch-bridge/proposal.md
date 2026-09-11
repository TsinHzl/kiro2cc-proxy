# 变更提案：websearch-bridge

## 背景

Claude Code CLI 的 WebSearch 是 Anthropic 官方 API 的 server tool（`type: "web_search_20250305"`），仅官方 API 可执行。通过本反代使用 CC 时，该工具从未生效——现有 `websearch.rs` 的拦截条件 `has_web_search_tool()` 要求请求**有且只有一个** `web_search` 工具，而 CC 总是携带全套工具（Bash/Read/Edit…）+ WebSearch，永远命中不了。

Kiro 上游存在可用的 `web_search` MCP 工具（`https://q.{region}.amazonaws.com/mcp`，JSON-RPC `tools/call`），`provider.call_mcp()` 链路已打通（重试/RPM 门控/sticky 路由）。

## 目标范围

**在范围内：**
- CC 使用本反代时，WebSearch 工具真实可用：模型自主决定搜索、解读结果、支持多轮迭代
- 对 CC 客户端呈现为官方 server tool 行为（流内 `server_tool_use` + `web_search_tool_result` 块）
- 复用 `provider.call_mcp` / `call_api_stream` 全链路（多账号、RPM、超时）
- converter 剔除 server tool、handlers 桥接状态机、websearch.rs 可见性提升（已完成部分）

**不在范围内：**
- 不支持 WebFetch 等其他 server tool
- 不改动 OpenAI 兼容层
- 不改动 `stream.rs` 状态机、`provider.rs`
- 不修改系统提示：桥接不向 system prompt 追加任何搜索说明（保持 history[0] 冻结不变量零风险）

## 技术方案

方案 B：客户端工具桥接——桥接环在"反代 ↔ Kiro"一段服务端内部闭环，CC 全程无感知：

1. **分派条件**：tools 中命中 web_search server tool → 走新桥接路径（流式与非流式均适用，差异仅在响应呈现形式）；未命中 → 维持现有行为（单工具纯搜索请求仍走旧拦截式兜底）
2. `converter.rs` 识别 web_search server tool → 从 Kiro `context.tools` 剔除（Kiro 不识别 server tool 格式），历史工具名收集中排除 web_search 占位符生成
3. Kiro 流中出现 `toolUseEvent(name="web_search")` → handlers 层截获，不透传为 tool_use SSE；聚合 input 分片得到 query
4. 反代调 `call_mcp_api()` 真实搜索
5. 构建**续请求**：复用首次转换的 `ConversationState`（clone），`conversationId`/`agentContinuationId` 不变（保 prompt cache），`context.tool_results` 回填 `ToolResult::success(tool_use_id, results)`，流式经 `call_api_stream` 续流（复用同一 StreamContext，块索引单调延续，不重发 message_start），事件链入同一客户端 SSE；非流式经 `call_api` 再次调用，基于最终响应组装同步 JSON（content 数组含 `server_tool_use` + `web_search_tool_result` 块）
6. 已流出的 assistant 文本/thinking **不回填**续请求，依赖 Kiro 同 conversationId 的会话续接（实测验证点，失败降级见风险表）
7. 多轮搜索上限 `min(max_uses, 5)`；MCP 失败回填 error ToolResult；Kiro 拒绝续请求则主流补发 error 事件后正常收尾
8. 客户端可见性：截获后发送 `server_tool_use` + `web_search_tool_result` SSE 块（复用现有事件格式）

**关键陷阱（已识别）**：`validate_tool_pairing` 会把 tool_use_id 不在历史中的 tool_result 当孤立结果过滤——桥接续请求回填的 web_search ToolResult 的 toolUseId 是 Kiro 流中新生成的，不在原请求历史里。因此续请求**不能走完整 convert_request 重转换**，必须基于已转换的 `ConversationState` 手工构建。

## Capabilities

### 新增 Capabilities
- `websearch`：WebSearch server tool 桥接到 Kiro MCP 的行为契约（识别/剔除/截获/续请求/多轮上限/失败降级）

## 预期影响

- `converter.rs`：+~60 行（识别/剔除函数 + 占位符排除），历史消息处理不变
- `handlers.rs`：+~300 行（流式 BridgeState 状态机嵌入 create_sse_stream 的 unfold + 非流式事件循环截获/续请求 + 桥接上下文传参），非桥接请求路径零行为变化
- `websearch.rs`：可见性提升（已完成），拦截式路径保留兜底
- prompt cache：系统提示零改动；续请求复用同一 conversationId/agentContinuationId，history[0] 冻结不变量不受影响
- 用量统计：MCP 调用不计入 token 计费（沿用现有 websearch.rs 拦截式口径）

## 风险

| 风险 | 应对 |
|---|---|
| Kiro 对「同 conversationId 续请求携带 toolResults」组合的接受度未实测 | 实现后以真实请求验证；续请求被拒 → 主流补发 error 事件（说明搜索不可用）后正常收尾，不挂起 |
| 续请求丢失已流出文本的上下文 | 依赖 conversationId 会话续接；实测若发现上下文丢失，降级为将已流出文本追加进续请求 history（design D6） |
| 续请求被 validate_tool_pairing 过滤 | 已识别，续请求手工构建 ConversationState，绕过该验证 |
| 多轮桥接导致单请求耗时拉长 | 多轮硬上限 5；沿用 stream deadline 全局超时（/cc/v1 的 300s deadline 作用于桥接总时长）；非流式沿用普通上游超时分档（普通 180s / compact 1000s）作用于每次上游调用 |
| MCP 失败 | ToolResult::error 回填，模型自行告知用户，流不中断 |
| MCP 调用 / 续请求构建期间客户端收不到 ping（unfold await 窗口内 select! 的 ping/deadline 分支不触发） | 搜索通常秒级完成，风险可接受；若实测客户端超时断连，在桥接 await 处补发 ping（design 风险表） |
