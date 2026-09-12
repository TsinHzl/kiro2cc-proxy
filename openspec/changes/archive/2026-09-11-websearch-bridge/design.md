# 变更技术方案：websearch-bridge

## 上下文

Anthropic server tool（web_search_20250305）由官方 API 服务端执行；Kiro 侧等价能力是 `q.{region}.amazonaws.com/mcp` 的 JSON-RPC `tools/call`（工具名 `web_search`，参数 `{query}`）。本反代需在服务端把两者接起来，对 CC 客户端仍呈现官方 server tool SSE 格式。

## 目标 / 非目标

**目标**
- CC 请求携带 web_search server tool 时，模型可自主触发真实搜索并基于结果继续生成
- conversationId / agentContinuationId 跨续请求稳定，不破坏 prompt cache
- MCP 失败降级不中断主流

**非目标**
- WebFetch 等其他 server tool
- OpenAI 兼容层、stream.rs、provider.rs 改动
- 系统提示修改（不追加搜索说明，保持 history[0] 零风险）

## 决策

### D1：server tool 识别依据（converter.rs）
`Tool.tool_type` 字段含 `web_search`（如 `web_search_20250305`）**或** `name == "web_search"` 即判定为 server tool。不依赖单一条件，兼容 CC 版本差异（部分版本只发 name + max_uses）。

### D2：剔除位置
- `convert_request` 第 6 步前用 `split_web_search_tool` 拆出普通工具与 max_uses，`convert_tools` 只转换普通工具
- `collect_history_tool_names` 返回后过滤 `web_search` 名称——历史中桥接产生的 web_search toolUse 不生成占位符定义（Kiro 不需要，且避免污染 context.tools）

### D3：续请求手工构建（绕过 validate_tool_pairing）
桥接回填的 ToolResult 的 toolUseId 是 Kiro 流中新生成的，不在原请求历史中，走 `convert_request` 会被当孤立结果过滤。因此：
- handlers 层保存首次转换的 `ConversationState`（clone）
- 续请求时：取其 `current_message.user_input_message.user_input_message_context`，替换/追加 `tool_results = [ToolResult::success(tool_use_id, 摘要)]`；`conversation_id` / `agent_continuation_id` / `history` / `agent_task_type` / `chat_trigger_type` 全部原样保留
- `serde_json::to_string(KiroRequest)` 后直接 `provider.call_api_stream`

**多轮桥接的状态累积语义（第 2 轮审查补充）**：每轮续请求都**基于当前 `BridgeContext.conversation_state` 演进**，而非从原始 clone 重新出发——构建第 N+1 轮续请求时，clone 第 N 轮续请求所用的 ConversationState，仅替换 `current_message` 的 `tool_results` 为本轮新结果（`current_message` 始终承载最新一轮的 toolResults），`history` 不追加中间轮的 toolResults。这样多轮搜索的中间结果不丢失（都在同一 conversationId 会话续接上下文中），且 history 逐字节不变量跨所有轮次保持。若实测发现 Kiro 要求中间轮 toolResults 沉入 history，按 D6 降级模式处理（追加进 history 尾部，接受一次 cache 失效）。

### D4：桥接状态机（handlers.rs）
**流式**（嵌入 create_sse_stream 的 unfold 状态）：
```
PassThrough   正常透传；遇 Event::ToolUse(name="web_search") → 记录 tool_use_id → Collecting
Collecting    聚合 input JSON 分片直到 tool_use.stop == true；不向客户端发 tool_use SSE
Resume        流结束（上游 None / 错误）时：call_mcp_api → 构建续请求 → call_api_stream → 新流事件链入同一客户端 SSE → 回 PassThrough
```
- 多轮上限：`min(max_uses.unwrap_or(5), 5)`
- MCP 失败：`ToolResult::error(tool_use_id, 错误信息)` 回填，流不中断
- 客户端可见性：截获后发送 `server_tool_use`（含 input_json_delta）+ `web_search_tool_result` content block 序列（复用 `generate_websearch_events` 的块格式，**但不重发 message_start**——主响应的 message_start 已在 initial_events 发出；块索引由 StreamContext state_manager 的 `next_block_index()` 分配）
- 上游流错误/空响应兜底逻辑在桥接态同样生效（Collecting 中断 → 按现有 error 事件收尾，不发起续请求）

**非流式**（handle_non_stream_request 事件循环）：状态为轻量的累计结构（被截获的 `Vec<(tool_use_id, query)>` + 已执行轮数），事件循环内截获聚合、循环结束后统一执行 MCP → 续请求 → 第二次事件循环 → JSON 组装，多轮在同一循环演进（详见 D5 非流式段）。

### D5：桥接所需上下文传入 create_sse_stream（传参方案）
`handle_stream_request` 目前只接收 `request_body: &str`，**不持有** payload 与 conversion（二者在 `post_messages` 中已被 move 消费）。传参方案：

- `post_messages` 在构建 `KiroRequest` 前调用 `converter::split_web_search_tool`（对原始 payload.tools），命中 server tool（流式/非流式均适用，见 D7）时，构造 `BridgeContext` 结构体：
  ```rust
  struct BridgeContext {
      conversation_state: ConversationState,  // 首次转换的 clone（KiroRequest 构建前 clone），多轮桥接的演进基底（D3）
      profile_arn: Option<String>,            // 续请求需要同参序列化
      additional_model_request_fields: Option<serde_json::Value>,
      max_uses: Option<i32>,                  // server tool 声明的次数上限
      bound_ids: Vec<u64>,                    // call_api_stream / call_mcp_api 均需要；unfold 闭包作用域内不可得，必须随 BridgeContext 携带
      is_compact_request: bool,               // 决定续请求的上游超时分档（普通 180s / compact 1000s），同上必须随 BridgeContext 携带
  }
  ```
- `handle_stream_request` 新增参数 `bridge_ctx: Option<BridgeContext>`（13 个参数已 `#[allow(clippy::too_many_arguments)]`，追加 1 个即可）；`handle_non_stream_request` 同样追加该参数（14 个参数，同样已有 allow）
- `/v1`（894-911 行）与 `/cc/v1`（1850-1868 行）两个调用点各传 `bridge_ctx`——两个调用点都已在 post_messages/post_cc_messages 作用域内持有 payload 与 conversion，改动仅为多传一个实参；非流式调用点同样传入（分派条件 D7 对流式/非流式一致）
- `create_sse_stream` 的 unfold 状态元组追加 `bridge: Option<BridgeState>` 字段；`None` 时全部桥接分支短路，非桥接请求零行为变化

**非流式路径（`handle_non_stream_request`）**：同样接收 `bridge_ctx: Option<BridgeContext>` 参数（第 14 个参数，沿用现有 `#[allow(clippy::too_many_arguments)]`）。其桥接在现有事件循环（`EventStreamDecoder.decode_iter()`）内实现：
- 事件循环中截获 `Event::ToolUse(name="web_search")` 且轮次未达上限 → 按 tool_use_id 聚合 input 分片（复用/参照现有 `tool_json_buffers` 模式），`stop == true` 时视为本轮截获完成，**不**将其解析为普通 tool_use 块
- 全部事件读取完毕后：若有被截获的搜索 → `call_mcp_api` → 基于当前 `BridgeContext.conversation_state` 按 D3 语义构建续请求 → `provider.call_api` 再次调用 → 用**新的** `EventStreamDecoder` 对续请求响应做第二次事件循环收集 → 基于最终响应组装同步 JSON
- 客户端可见性：最终 content 数组在 thinking/text 块之外，追加 `server_tool_use` + `web_search_tool_result` 两个块（与流式 SSE 的块格式一致），不出现裸 tool_use 块
- 非流式路径复用同一套 BridgeContext 与 D3 多轮演进语义（第 2 轮及后续轮次在第二次事件循环内继续截获），MCP 失败/续请求失败降级语义与流式一致（见 D4 流式段与 spec Req6）
- block index 说明：非流式无 SSE 块索引概念，content 数组顺序为 server_tool_use → web_search_tool_result → thinking/text/tool_use 按实际输出顺序排列

### D6：已流出 assistant 文本的处理
Collecting 期间模型可能已透传 thinking/text 分片。续请求对这些文本的处理决策：

- **主方案：不回填，依赖 Kiro 会话续接**。续请求复用同一 conversationId（agentContinuationId 由其派生），Kiro 侧按会话上下文续接，已流出的文本无需在 history 中重放。此方案保证续请求的 history 与首次请求**逐字节一致**（prompt cache 命中条件最强）
- **实测验证点**：实现后以真实请求验证续流的输出是否延续首次流上下文（不重复、不答非所问）
- **降级方案（实测失败时启用）**：将首次流已累计的 assistant 文本以 `Message::assistant` 追加进续请求 history 尾部（此时 history 不再逐字节一致，prompt cache 该请求失效一次，可接受）。降级切换仅改构建续请求处 ~10 行，不影响任务拆分

### D7：桥接路径 vs 旧拦截式路径的分派条件
两条路径互斥，判定顺序（post_messages 778 行处）：

1. `has_web_search_tool`（请求**有且只有一个** web_search 工具，**不限流式/非流式**）→ 维持旧拦截式兜底（行为不变，避免回归）
2. `split_web_search_tool` 命中（混合工具列表中存在 web_search server tool，流式/非流式均适用）→ **新桥接路径**；`stream == false` 时非流式路径 `handle_non_stream_request` 同样接收 `bridge_ctx` 并走桥接（见 D4 非流式段）
3. 未命中 web_search → 现有行为不变

旧拦截式处理的是"客户端直接要一次搜索结果"的场景；新桥接处理"模型在 agentic 循环中自主搜索"的场景，二者语义不同故共存。流式/非流式共享同一套分派条件与 BridgeContext，差异仅在响应呈现形式（SSE vs 同步 JSON）。

### D8：桥接状态机的事件处理与流终止
- Collecting 期间收到 `Event::AssistantResponse`（模型调工具前的说明文字）→ 正常透传为 text_delta，**不记录**进桥接状态（D6 主方案无需回填）
- 截获 web_search toolUse 的完整周期：`tool_use.stop == true` 时聚合完成 → 立即发 `server_tool_use` + `web_search_tool_result` SSE 块（客户端可见性）→ **Kiro 流继续读取到自然结束**（message_stop / 上游 None）→ 才发起 MCP 调用与续请求
- **续流复用同一个 `StreamContext`（第 2 轮审查修正，替代原"新建 StreamContext"方案）**：续请求 `call_api_stream` 返回的响应流接入后，产生的 Kiro 事件继续送入**首次流的同一个 StreamContext**（`ctx.process_kiro_event`），而非创建新上下文。理由：
  - **块索引单调性**：`SseStateManager::next_block_index()` 是单调递增计数器。若续流新建 StreamContext，index 从 0 重启，与首次流已发出的块 index 冲突，客户端收到重复 `content_block_start(index=N)` 的非法 SSE。复用同一 ctx 后 index 自然延续，协议合法性由现有计数器保证
  - **message_stop 门控统一**：`generate_final_events` 的 message_stop 由 `message_ended` 门控发出。复用同一 ctx 时，首次流结束时**不调用** `generate_final_events`（桥接态下 unfold 的上游 None 分支改为把控制权交给桥接续流逻辑），`message_ended` 保持 false；仅当桥接全部轮次结束后，由桥接收尾逻辑调用一次 `ctx.generate_final_events()` 补发 final events（含 message_stop）。全流程恰好一次 message_stop，无需在 handlers 侧过滤首次流的 final events，也无需修改 stream.rs
  - **弃用备选方案说明（index 偏移重写）**：曾考虑"续流新建 StreamContext + 对续流事件按首次流已用块数做 index 偏移重写"。弃用原因：偏移重写需要在 handlers 层逐事件改写 SseEvent 内容，侵入 stream.rs 的事件结构假设（thinking 块、signature_delta 等内部 index 引用），实现面远大于复用 ctx，且与"不改 stream.rs"的非目标冲突
  - **使用代价与适配点**：复用同一 ctx 意味着续流事件走同一套 thinking 状态机——续流若再次输出 `</thinking>` 类标签会被现有 thinking 状态机按续流位置处理，属可接受行为（模型通常在工具结果后直接输出正文）；`is_empty_response` 等判定均以首次流累计为准，续流为空时不会被误判为空响应
- 续流的接入方式：unfold 状态元组中的 `body_stream` 被替换为续请求的响应 `bytes_stream`，`decoder` 重置为 `EventStreamDecoder::new()`，`ctx` 与 `bridge` 状态原样携带——对 unfold 结构而言，续流只是"换了一个上游 body 继续 unfold"，实现侵入面最小
- MCP 调用与续请求 `call_api_stream` 均为 async，在 unfold 的 async move 闭包内直接 `.await`（该闭包本就是 async，tokio 运行时可用）
- 桥接轮次耗尽（达到 `min(max_uses, 5)`）后模型仍触发 web_search → 不再截获，该 toolUse 按**现有普通 tool_use 逻辑透传**（客户端会收到 tool_use 块；CC 侧无对应客户端工具，tool_result 缺失由 CC 自身的 agentic 循环处理——与现网行为一致，不新增处理）

## 风险 / 权衡

- Kiro 对续请求携带 toolResults 的接受度需实测（官方 CLI 未公开此用法）——拒绝时主流补发 error 事件后正常收尾，不挂起
- 已流出文本依赖 conversationId 会话续接——实测失败则切换 D6 降级方案
- Collecting 期间 thinking/文本分片仍正常透传（模型可能在调工具前输出说明文字）
- 续请求会再次消耗 Kiro 配额，多轮桥接最多 5 次，可接受
- **MCP 调用 / 续请求构建期间的 keep-alive 窗口（第 2 轮审查补充）**：unfold 数据分支内 `.await` MCP 与续请求时，`select!` 的 ping 与 deadline 分支在该次 await 期间不触发，客户端在此窗口（MCP 搜索可达数秒）收不到 ping。搜索通常秒级完成，风险可接受；若实测出现客户端超时断连，可在桥接 await 处套一层带超时（沿用 stream_deadline）并在返回前补发一个 ping 事件，不影响任务拆分
- /cc/v1 端点的 300s 全局 deadline 作用于**桥接总时长**（含所有轮次的 MCP 调用与续流）——deadline 分支在轮次间 await 窗口内同样不触发，实际约束略宽松于 300s；/v1 端点无 deadline，多轮桥接不受限
- 桥接收尾兜底：若续请求阶段失败/超时，桥接收尾逻辑必须保证 `generate_final_events`（含 message_stop）或 error 事件最终发出，防止客户端流悬挂

## 迁移方案

无数据迁移。旧的拦截式 `has_web_search_tool` 路径保留为兜底（单工具纯搜索流式/非流式请求仍走原逻辑，见 D7）。

## 待决问题

无——实现后以真实请求验证 D3/D6 的核心假设（Kiro 续请求接受度与会话续接），失败走已定义的降级路径。
