# 变更提案：streamify-cc-v1-endpoint

## 背景

`/cc/v1/messages` 目前为**完全缓冲模式**：`create_buffered_sse_stream` 收下上游 Kiro
的全部帧后一个字节都不转发，等流结束才一次性吐出所有 SSE 事件，等待期间仅发 25s 一次 ping。

该设计的唯一目的是拿到精确的 `input_tokens` —— 早期实现需要等流末尾的 `contextUsageEvent`
才能反算出来，所以必须全部攒住、算完再改写 `message_start`。

**这个目的在此前「contextUsage 本地化」改动后已不复存在：**

- `src/anthropic/stream.rs:814-816` 明确注明不再用 `percentage × window` 反算 `input_tokens`
- `src/anthropic/stream.rs:1428` 注明 `context_input_tokens` 已弃用、**始终为 `None`**
- 于是 `BufferedStreamContext::finish_and_get_all_events`（stream.rs:1645-1648）中
  `raw_final_input_tokens = context_input_tokens.unwrap_or(estimated_input_tokens)`
  **恒等于 `estimated_input_tokens`** —— 请求阶段就已知的本地估算值

「更正 message_start」确实仍会改变数值，但改变源于 cache_read 拆分口径不同
（`prefix_estimated_tokens` 优先于 `scale_to` 模拟），而该口径的三个输入
（`estimated_input_tokens` / `prefix_estimated_tokens` / `prompt_cache_usage`）
全部在请求阶段确定，无一需要等流结束。唯一真需等待的
`metering_cache_read_tokens` / `metering_cache_creation_tokens`，代码注释
（stream.rs:1450）自述「实测当前版本不透传，保留兜底」；且这两个字段挂在
`StreamContext` 上，流式路径的 `generate_final_events`（stream.rs:1454-1458）
同样会读取并用于 `message_delta` 与 usage 入库 —— 即便上游哪天开始透传，
流式路径也不会丢失它们，差别仅在于生效位置是 `message_delta` 而非 `message_start`。

**结论：当前为零精度收益支付全部缓冲代价。**

代价具体为：客户端在整个生成期间零进展显示；单个 turn 内的多个 `tool_use`
无法与后续生成重叠执行（Claude Code 的 `streamingToolExecutor` 在
`Tr.type==="assistant"` 时才 `addTool`，缓冲把该时刻推到流末）；Esc 打断时
缓冲内容尚未发出、全部作废；超过 300s deadline 时已生成内容一并作废。

**影响面（已核实，与初稿判断不同）：** `/cc/v1` 并非默认路径。README 的
「接入 Claude Code」一节（README.md:549-610）引导配置的是
`ANTHROPIC_BASE_URL="http://127.0.0.1:5678"`，Claude Code 据此拼出的是
`/v1/messages`；验证示例（README.md:602）命中的同样是 `/v1`。
要命中 `/cc/v1` 需用户显式把 base URL 配成 `.../cc`，
`docs/源码全景解析.md:1900,1923` 也把它描述为「用户需额外指定 `/cc/v1` 路径」的
规避手段。README.md:624-631 只是端点参考表格。

因此实际受影响人群是显式配置了 `/cc` 的用户，占比未知。这降低了变更的紧迫性，
但不改变判断：对这部分用户而言，缓冲代价是全额支付而收益为零。

## 目标范围

**在范围内：**

- 将 `/cc/v1/messages` 的流式分派由 `handle_stream_request_buffered` 切换为
  `handle_stream_request`（handlers.rs:1721-1736；两函数参数列表已完全一致）
- 为 `create_sse_stream` 增加可选全局 deadline 支持，`/cc/v1` 传 300s 以保住
  缓冲模式现有的上游挂起保护；`/v1` 传 `None` 保持现状不变
- 删除随之失效的死代码：`BufferedStreamContext`(stream.rs:1533-1695)、
  `handle_stream_request_buffered`(handlers.rs:1758-1799)、
  `create_buffered_sse_stream`(handlers.rs:1800-1937)，合计约 340 行
- 补充 deadline 行为的单元测试（现有 `BufferedStreamContext` 为零测试覆盖）
- 更新描述缓冲行为的文档，实测共 5 个文件 20 处：
  - `src/anthropic/mod.rs:14`
  - `CLAUDE.md:42`、`CLAUDE.md:79`（`/cc/v1 vs /v1` 一节）
  - `README.md:628`、`README.md:631`
  - `docs/代码速查表.md:55`、`:354`、`:375`、`:376`
  - `docs/源码全景解析.md:201`、`:246`、`:649`、`:1065-1085`（整节「解析二：/cc/v1
    缓冲模式」）、`:1361`、`:1628`、`:1896-1900`（决策二）、`:1923`
- 顺带修正 `docs/源码全景解析.md:1085` 的过期描述：文中所称
  `infer_cache_read_tokens()` 在当前代码中已不存在（grep 零命中）

**不在范围内：**

- **不改 `message_start` 的 cache_read 拆分口径。** `create_message_start_event`
  用 `prompt_cache_usage` 直接取值，与末尾 `message_delta` 的 `prefix` 优先口径
  本就不同，该差异在 `/v1` 上长期存在且无相关问题报告；末尾 `message_delta` 会
  校准终值（已验证 Claude Code 两条 usage 合并路径都吃
  `message_delta.usage.input_tokens`）。改它会连带变更 `/v1` 行为，超出本次范围。
- 不加 config 开关。缓冲路径直接删除，不保留双路径（用户明确决定）。
- 不改 `/v1/messages` 的任何现有行为。
- 不改后端 usage 记账逻辑（`usage_tracker` 在流结束上报 `final_input_tokens`，与
  流式/缓冲无关）。
- 不修 issue #29。#29 根因在 Claude Code 客户端本地排队机制，代理侧不可修复；
  本变更只缩短 turn 时长，属间接改善。

## 同分支携带的独立变更（issue #25）

本分支的工作区**另含 issue #25 的错误文案修复**（约 77 行）：
`handlers.rs` 的 `format_prompt_too_long()` / `CLIENT_ASSUMED_CONTEXT_WINDOW`、
`map_provider_error_with_context` 的超窗错误文案改为对齐 Anthropic 官方格式
`prompt is too long: N tokens > M maximum`，加 2 个单元测试。

**这不属于本变更的 In Scope**，与流式化无技术关联。用户在阶段 0 明确决策
「带入流式化分支一起提交」，计划为**同分支两个独立 commit**（#25 一个、
streamify 一个），便于分别回滚与追溯。本提案的 spec.md / tasks.md 均不覆盖该部分；
其正确性由前序独立验证保证（`n.max(CLIENT_ASSUMED_CONTEXT_WINDOW + 1)` 兜底、
2 个测试通过）。此处留痕仅为保持 diff 可追溯，不将其纳入本变更的验收范围。

## 技术方案

`handle_stream_request` 与 `handle_stream_request_buffered` 的参数列表
（provider / request_body / model / input_tokens / prefix_estimated_tokens /
thinking_enabled / usage_tracker / api_key_id / prompt_cache_usage / bound_ids /
client_ip）顺序与类型完全一致，故分派切换为单点替换。

`create_sse_stream` 现有 `unfold` state 为 5 元组
`(body_stream, ctx, decoder, finished, ping_interval)`，已含 25s ping。
增加 `deadline: Option<Instant>` 参数并纳入 state，在 `tokio::select!` 中新增一个
deadline 分支，超时发 `overloaded_error` 后终止。

**与初稿不同：不加 `biased`**（详见 design.md D4）。初稿计划「沿用
`create_buffered_sse_stream` 的 `biased` 前置分支」，实施时判定该写法会改变 `/v1`
现有的分支优先级、违反「`/v1` 行为零变化」目标；且 `select!` 默认的随机分支选择
已足够避免 ping / deadline 被饿死 —— 缓冲版需要 `biased` 是因为它在单次 `poll`
内用显式 `loop` 反复 select 且 chunk 分支不返回，`create_sse_stream` 无此结构。

`None` 时该分支等价于不存在（`wait_deadline` 走 `std::future::pending()` 永不就绪），
`/v1` 行为不变。

`BufferedStreamContext` 是对 `StreamContext` 的独立包装（`inner: StreamContext`），
所有方法均为委托或缓冲专用，无其他模块引用、无测试引用，可整体删除。

不新增外部 crate；deadline 复用已在用的 `tokio::time::{Instant, sleep_until}`。

## 预期影响

**行为变化（`/cc/v1` 流式请求）：**

| 项 | 变更前 | 变更后 |
|---|---|---|
| 首字延迟 | = 上游全部生成时长 | 上游首帧到达即转发 |
| turn 内工具执行 | 须等整条 assistant 消息生成完 | 收到 `tool_use` 块即可开始 |
| Esc 打断 | 缓冲内容全部作废 | 已流出内容保留 |
| 撞 300s deadline | 整体失败，已生成内容作废 | 已流出内容有效，尾部补 error |
| 中断语义 | all-or-nothing（丢弃缓冲、只发单个 error） | 部分内容 + `stream_interrupted_error_event` |
| `input_tokens` 精度 | — | 不变（末尾 `message_delta` 给终值） |
| 25s ping 保活 | 有 | 有（`create_sse_stream` 已具备） |

**不变：** `/v1` 全部行为；非流式 `/cc/v1`（`handle_non_stream_request` 不经改动）；
后端 credits/cost 记账；账号故障转移（发生在取得响应头之前）。

**兼容性：** 对客户端仍是合法 Anthropic SSE 序列，事件类型与顺序不变，只是时间分布不同。

## 风险

| 风险 | 等级 | 应对 |
|---|---|---|
| 无 config 开关可回退，`/cc/v1` 出问题只能整体回退版本 | 中（初稿评为高，因误判 `/cc/v1` 为默认端点；实测受影响者仅显式配置 `/cc` 的用户） | 用户已知悉并明确选择不加开关；发版前真机验证；release note 显著标注行为变更 |
| all-or-nothing 契约丢失，中断时客户端会看到残缺回答 + error | 中 | 与 `/v1` 现状一致，`stream_interrupted_error_event` 已明确不把中断伪装成正常完成 |
| auto-compact 时机 —— 已确证 Claude Code 两条 usage 合并路径都吃末尾 `message_delta.usage.input_tokens`（SDK 累积层 + 自有 `T$e`，条件为非 null 且 > 0），但未追到 auto-compact 判定点直接引用累积结果的那行 | 中 | 真机验证：长上下文会话比对 `/context` 显示百分比与后端 `[input_tokens] 本地化: estimated=N final=M` 日志是否一致 |
| `message_start` 与末尾 `message_delta` 的 input_tokens 口径差异在流式下变为「先估后校准」，turn 进行中显示值会跳变 | 低 | 与 `/v1` 长期现状一致；turn 结束即校准；列为观察项而非本次修改项 |
| 新增 deadline 分支引入 `select!` 分支竞争，可能影响 ping 或数据转发 | 低 | 不加 `biased`，依赖 `select!` 默认的随机分支选择（任一分支被连续跳过的概率指数衰减）；`None` 时该分支等价于不存在；补单元测试覆盖 `wait_deadline` 两种语义 |
