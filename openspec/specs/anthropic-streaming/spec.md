# 规范增量：anthropic-streaming — /cc/v1 流式化

## 修改需求

### 需求：/cc/v1/messages 流式响应实时转发

原行为：`/cc/v1/messages` 在 `stream: true` 时缓冲上游全部事件，直到上游流结束才
一次性发送。变更为与 `/v1/messages` 相同的实时转发。

#### 场景：上游首帧到达即转发

- **WHEN** 客户端向 `/cc/v1/messages` 发起 `stream: true` 请求，上游 Kiro 返回
  第一个含内容的二进制帧（CRC32 校验通过、可解码为 `Event`）
- **THEN** 代理立即将该帧转换出的 SSE 事件写入响应体，不等待后续帧
- **AND** `message_start` 事件在上游首帧之前即已发出（由 `generate_initial_events()`
  在建立流之前产生）

#### 场景：turn 内 tool_use 块可提前执行

- **WHEN** 上游在同一条 assistant 消息中先产出 `tool_use` 块、其后仍有更多内容
- **THEN** 该 `tool_use` 的 `content_block_start` / `input_json_delta` /
  `content_block_stop` 在上游后续内容仍在生成时就已送达客户端

#### 场景：input_tokens 由末尾 message_delta 给出终值

- **WHEN** 上游流正常结束
- **THEN** 末尾 `message_delta` 事件的 `usage.input_tokens` 为
  `scale_for_client(report_input, model)`，其中 `report_input` 是
  `generate_final_events()`（stream.rs:1449-1473）三级优先链算出的**非缓存部分**
  （metering 透传 → 前缀估算 → `scale_to` 模拟），即
  `final_input_tokens - cache_read - cache_creation`，**不等于** `final_input_tokens`
  本身（仅当两个 cache 项均为 0 时才相等）
- **AND** 同一 `usage` 中并列给出 `cache_read_input_tokens` 与
  `cache_creation_input_tokens`，三者之和才是总输入量（Anthropic 标准语义）
- **AND** 该 `input_tokens` 非 null 且大于 0
- **AND** `message_start` 中的 `usage` 为请求阶段的估算口径，与上述终值可能不同；
  客户端以 `message_delta` 为准（Anthropic SDK 累积层与 Claude Code 自有的
  usage 合并逻辑均在 `input_tokens` 非 null 且 > 0 时用它覆盖 `message_start` 的值）
- **AND** 上述三级优先链逻辑本次变更不修改 —— 它位于 `StreamContext` 中，
  流式与原缓冲路径共用

#### 场景：连接中断时已发内容不作废

- **WHEN** 上游流已产出部分内容后读取失败（`Some(Err(_))`）
- **THEN** 已发送给客户端的事件保持有效，代理追加 `stream_interrupted_error_event()`
  的 `error` 事件后终止，不伪装成正常完成
- **AND** 与原缓冲模式的 all-or-nothing 契约不同：原模式丢弃全部缓冲内容、
  只发单个 error 事件

#### 场景：上游返回空响应仍按现有分支处理

- **WHEN** 上游流结束但未产出任何内容事件（`is_empty_response()` 为真）
- **THEN** 若 `empty_response_is_oversized_context()` 为真，发送
  `generate_final_events()`（含超长上下文提示）
- **AND** 否则发送 `empty_response_error_event(false)`
- **AND** 此分支行为与 `/v1` 完全一致，本次变更不修改它

### 需求：/v1/messages 行为不变

#### 场景：/v1 无全局 deadline

- **WHEN** 客户端向 `/v1/messages` 发起 `stream: true` 请求，上游长时间不返回数据
- **THEN** 代理不因任何全局超时主动终止，仅每 25s 发送 `ping`（与变更前一致）

#### 场景：/v1 的 ping 与数据竞争顺序不变

- **WHEN** 上游 chunk 密集到达且 ping 定时器同时就绪
- **THEN** `tokio::select!` 不使用 `biased`，两分支保持随机公平选择（与变更前一致）

## 新增需求

### 需求：流式路径可选全局 deadline

`create_sse_stream` 接受 `deadline: Option<tokio::time::Instant>` 参数，用于保留
原缓冲模式的上游挂起保护能力。

#### 场景：/cc/v1 撞 300s deadline

- **WHEN** `/cc/v1/messages` 流式请求自建流起 300 秒后上游流仍未结束
- **THEN** 代理发送 `type: "error"`、`error.type: "overloaded_error"` 的 SSE 事件后
  终止该流
- **AND** 已发送给客户端的内容事件保持有效（与原缓冲模式「整体作废」不同）
- **AND** 记录 `tracing::error!` 日志

#### 场景：deadline 为 None 时不注册超时分支

- **WHEN** `create_sse_stream` 的 `deadline` 参数为 `None`（`/v1` 的传值）
- **THEN** 该 `select!` 分支等价于 `std::future::pending()`，永不就绪，
  不影响数据转发与 ping

#### 场景：deadline 未到时不干扰正常转发

- **WHEN** `deadline` 为 `Some(d)` 且当前时刻早于 `d`
- **THEN** 数据分支与 ping 分支的行为与 `deadline` 为 `None` 时完全相同

## 移除需求

### 需求：移除 /cc/v1 缓冲模式

#### 场景：缓冲实现整体删除

- **WHEN** 代码库中检索 `BufferedStreamContext` /
  `handle_stream_request_buffered` / `create_buffered_sse_stream`
- **THEN** 三者均不存在，且 `cargo build --release` 无 dead_code 警告

#### 场景：非流式 /cc/v1 不受影响

- **WHEN** 客户端向 `/cc/v1/messages` 发起 `stream: false` 请求
- **THEN** 仍走 `handle_non_stream_request`，行为与变更前完全一致
  （非流式本身即等待上游完成，不涉及缓冲流实现）
