// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic API Handler 函数

use crate::anthropic::stream::{SseEvent, StreamContext};

use std::convert::Infallible;

use crate::kiro::model::events::Event;
use crate::kiro::parser::decoder::EventStreamDecoder;
use axum::{
    body::Body,
    http::{StatusCode, header},
    response::Response,
};
use bytes::Bytes;
use futures::{Stream, StreamExt, stream};
use std::time::Duration;
use tokio::time::{Instant, interval_at};

use super::bridge::{
    BridgeContext, BridgeRoundOutcome, BridgeState, InFlightRound, bridge_execute_round_owned,
    bridge_handle_event, build_web_search_result_events, harvest_bridge_round,
};
use super::error::map_provider_error_with_context;

/// 处理流式请求
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_stream_request(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    input_tokens: i32,
    prefix_estimated_tokens: i32,
    thinking_enabled: bool,
    usage_tracker: Option<std::sync::Arc<crate::model::usage::UsageTracker>>,
    api_key_id: Option<u32>,
    prompt_cache_usage: crate::cache::PromptCacheUsage,
    bound_ids: Vec<u64>,
    client_ip: Option<String>,
    // 上游流的全局超时；None 表示不限时（/v1 的现有行为）
    stream_deadline: Option<Duration>,
    // 是否为 Claude Code /compact 压缩请求（决定上游超时：普通 180s / 压缩 1000s）
    is_compact_request: bool,
    // 客户端是否请求了 thinking adaptive（与账号级开关在 provider 侧共同决定注入）
    thinking_adaptive_requested: bool,
    // web_search server tool 桥接上下文（None = 非桥接请求，零行为变化）
    bridge_ctx: Option<BridgeContext>,
    // 请求的 effort 级别（output_config 存在时取值，否则 None），随 usage 记录入库
    effort: Option<String>,
) -> Response {
    // 调用 Kiro API（支持多账号故障转移）
    let (response, credential_id) = match provider
        .call_api_stream(
            request_body,
            is_compact_request,
            thinking_adaptive_requested,
            &bound_ids,
        )
        .await
    {
        Ok(resp) => resp,
        Err(e) => return map_provider_error_with_context(e, model, input_tokens),
    };

    // 创建流处理上下文
    let mut ctx = StreamContext::new_with_thinking(model, input_tokens, thinking_enabled)
        .with_usage_tracking(usage_tracker, api_key_id, Some(credential_id), client_ip)
        .with_prompt_cache_usage(prompt_cache_usage)
        .with_prefix_estimated_tokens(prefix_estimated_tokens)
        .with_effort(effort);

    // 生成初始事件
    let initial_events = ctx.generate_initial_events();

    // 创建 SSE 流
    let stream = create_sse_stream(
        response,
        ctx,
        initial_events,
        stream_deadline.map(|d| Instant::now() + d),
        bridge_ctx,
        std::sync::Arc::clone(&provider),
    );

    // 返回 SSE 响应
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// Ping 事件间隔（25秒）
const PING_INTERVAL_SECS: u64 = 25;

/// 等待全局 deadline；`None` 时永不就绪，使调用方的 `select!` 分支等价于不存在
///
/// 不用 `select!` 的 `if` precondition：分支的 future 表达式必须无论 precondition
/// 真假都能构造，而 `sleep_until` 需要已解包的 `Instant`，那样得凭空造一个
/// 「很远的未来」哨兵值。
pub(crate) async fn wait_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(d) => tokio::time::sleep_until(d).await,
        None => std::future::pending::<()>().await,
    }
}

/// 创建 ping 事件的 SSE 字符串
pub(crate) fn create_ping_sse() -> Bytes {
    Bytes::from("event: ping\ndata: {\"type\": \"ping\"}\n\n")
}

/// 为上游空响应构造合适的 SSE error 事件。
///
/// - 大输入（疑似上下文过大）：返回 invalid_request_error，提示压缩上下文，
///   不鼓励原样重试（重试还是同样的大请求，仍会空）。
/// - 小输入（疑似偶发）：返回 overloaded_error，客户端可重试。
fn empty_response_error_event(oversized_context: bool) -> SseEvent {
    let (err_type, message) = if oversized_context {
        (
            "invalid_request_error",
            "Upstream returned an empty response, likely because the context is too large. \
             Reduce conversation history (e.g. /compact), system prompt, or tools, then retry.",
        )
    } else {
        (
            "overloaded_error",
            "Upstream returned an empty response. Please retry.",
        )
    };
    SseEvent::new(
        "error",
        serde_json::json!({
            "type": "error",
            "error": { "type": err_type, "message": message }
        }),
    )
}

/// 上游读流出现传输层错误（解码失败/连接中断/超时）时应返回给客户端的事件。
///
/// `Err` 只可能来自传输层异常——正常完成只通过 `None`（EOF）传达，见
/// `StreamContext::is_empty_response` 文档。因此哪怕此前已经产生了部分内容（thinking/text/
/// tool_use），也不能用 `generate_final_events`/`finish_and_get_all_events` 把中断伪装成正常的
/// end_turn/tool_use 完成，否则客户端会把截断的响应当成任务已完成而停止推进，只能靠用户手动
/// 重新输入才能恢复，且不会自动重试。
pub(crate) fn stream_interrupted_error_event() -> SseEvent {
    SseEvent::new(
        "error",
        serde_json::json!({
            "type": "error",
            "error": {
                "type": "overloaded_error",
                "message": "Upstream connection was interrupted before the response finished. Please retry."
            }
        }),
    )
}

/// /cc 全局 deadline 触发时返回给客户端的 error 事件（两处 deadline 收尾路径共用）
pub(crate) fn deadline_error_event() -> SseEvent {
    SseEvent::new(
        "error",
        serde_json::json!({
            "type": "error",
            "error": {
                "type": "overloaded_error",
                "message": "Upstream response timed out (streaming mode deadline)"
            }
        }),
    )
}

/// 创建 SSE 事件流
fn create_sse_stream(
    response: reqwest::Response,
    ctx: StreamContext,
    initial_events: Vec<SseEvent>,
    deadline: Option<Instant>,
    // web_search server tool 桥接上下文（None = 非桥接请求，零行为变化；
    // max_uses 提取为 BridgeState，其余字段由续流逻辑消费）
    bridge_ctx: Option<BridgeContext>,
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    // 先发送初始事件
    let initial_stream = stream::iter(
        initial_events
            .into_iter()
            .map(|e| Ok(Bytes::from(e.to_sse_string()))),
    );

    // 桥接状态（None = 非桥接请求，unfold 内全部分支短路）
    // 演进基底初始化为 BridgeContext.conversation_state 的 clone（D3）
    let bridge = bridge_ctx.as_ref().map(|b| BridgeState {
        evolution_base: Some(b.conversation_state.clone()),
        ..BridgeState::new(b.max_uses)
    });

    // 然后处理 Kiro 响应流，同时每25秒发送 ping 保活
    // boxed() 统一 body_stream 类型：in-flight 期间回填的占位流是
    // stream::pending()（具体类型），与 reqwest bytes_stream 的 opaque type
    // 无法直接统一，借 Box<dyn Stream> 擦除为同一类型。
    let body_stream = response.bytes_stream().boxed();

    // bridge_ctx 与 provider Arc 一并放入 unfold 状态元组：闭包为 FnMut + async move，
    // 环境捕获的 Owned 值无法逐次 move 进 future（E0507/E0373），
    // 改为状态元组内逐轮移入移出。
    // 状态元组第 10 元：进行中的桥接轮（修复③保活用，None = 无轮次执行中）。
    let processing_stream = stream::unfold(
        (body_stream, ctx, EventStreamDecoder::new(), false, interval_at(Instant::now() + Duration::from_secs(PING_INTERVAL_SECS), Duration::from_secs(PING_INTERVAL_SECS)), deadline, bridge, bridge_ctx, provider, None::<InFlightRound>),
        |(mut body_stream, mut ctx, mut decoder, finished, mut ping_interval, deadline, mut bridge, bridge_ctx, provider, mut round_in_flight)| async move {
            if finished {
                return None;
            }

            // 使用 select! 同时等待数据、桥接轮收割、ping 定时器与全局 deadline。
            // 桥接轮分支以 precondition 条件启用：仅在存在 in-flight 轮时参与竞争。
            // 注意（tokio select! 语义）：precondition 为 false 时 async expression
            // 仍会被求值，但返回的 future 永不被 poll —— expect() 必须放在 async
            // block 体内延迟到 poll 才执行，此处借 round_in_flight.is_some() 守卫。
            // 有意不加 biased：不加时 select! 对同时就绪的分支做随机选择，任一分支被
            // 连续跳过的概率指数衰减，ping 与 deadline 都不会被密集 chunk 饿死。
            // （已删除的 create_buffered_sse_stream 需要 biased，是因为它在单次 poll
            //  内用显式 loop 反复 select 且 chunk 分支不返回 —— 那才是确定性饿死源。）
            // 加 biased 会改变 /v1 现有的分支优先级。
            tokio::select! {
                // 桥接轮收割（修复③ v2）：后台任务完成时在此分支同步收割，先发
                // 配对结果块（+失败收尾事件），再把续流换入状态元组。轮次执行期间
                // 此分支的 JoinHandle.await 挂起，ping/deadline 分支照常触发——
                // 桥接轮执行期间下游心跳不中断。注意：阻断 flatten 重入误收尾的
                // 是 spawn 分支换入的 stream::pending() 占位流——已耗尽的流会立即
                // 以 None 就绪被 body 分支抢选，恰恰是必须防住的重入路径，不能删。
                joined = async {
                    // JoinHandle 实现 Future + Unpin，借 Pin::new 按 &mut 轮询：
                    // `.await` 会走 IntoFuture::into_future 按值取 receiver，
                    // 而 async block 每次重入 poll 都会重新求值表达式，
                    // &mut 形态避免 E0507 move（precondition 守卫保证 Some）。
                    std::pin::Pin::new(
                        &mut round_in_flight
                            .as_mut()
                            .expect("precondition 守卫保证 in-flight 轮存在")
                            .0,
                    )
                    .await
                }, if round_in_flight.is_some() => {
                    let (_handle, result_tool_use_id) = round_in_flight.take().expect("precondition 守卫保证存在");
                    let (new_bridge, outcome) = match joined {
                        Ok(pair) => pair,
                        Err(e) => {
                            // 后台任务 panic：按 Failed 收尾（结果块为空数组）
                            tracing::error!("web_search 桥接轮后台任务异常: {}", e);
                            (
                                bridge
                                    .take()
                                    .unwrap_or_else(|| BridgeState::new(Some(0))),
                                BridgeRoundOutcome::Failed(None),
                            )
                        }
                    };
                    bridge = Some(new_bridge);
                    let harvest = harvest_bridge_round(outcome, &result_tool_use_id, &mut ctx);
                    let bytes: Vec<Result<Bytes, Infallible>> = harvest
                        .events
                        .into_iter()
                        .map(|e| Ok(Bytes::from(e.to_sse_string())))
                        .collect();
                    // 显式 return：分支体内提前返回流产物，与下方各分支同构
                    #[allow(clippy::needless_return)]
                    return Some((
                        stream::iter(bytes),
                        (
                            harvest.new_body_stream.bytes_stream().boxed(),
                            ctx,
                            harvest.new_decoder,
                            harvest.finished,
                            ping_interval,
                            deadline,
                            bridge,
                            bridge_ctx,
                            provider,
                            None,
                        ),
                    ));
                }
                // 处理数据流
                chunk_result = body_stream.next() => {
                    match chunk_result {
                        Some(Ok(chunk)) => {
                            // 解码事件
                            if let Err(e) = decoder.feed(&chunk) {
                                tracing::warn!("缓冲区溢出: {}", e);
                            }

                            let mut events = Vec::new();
                            for result in decoder.decode_iter() {
                                match result {
                                    Ok(frame) => {
                                        if let Ok(event) = Event::from_frame(frame) {
                                            // 桥接截获优先：web_search toolUse 不透传为
                                            // 普通 tool_use SSE，改为客户端可见性块（D4/D8）
                                            let (consumed, mut bridge_events) =
                                                bridge_handle_event(&mut ctx, &mut bridge, &event);
                                            if !consumed {
                                                let sse_events = ctx.process_kiro_event(&event);
                                                bridge_events.extend(sse_events);
                                            }
                                            events.extend(bridge_events);
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("解码事件失败: {}", e);
                                    }
                                }
                            }

                            // 转换为 SSE 字节流
                            let bytes: Vec<Result<Bytes, Infallible>> = events
                                .into_iter()
                                .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                .collect();

                            Some((stream::iter(bytes), (body_stream, ctx, decoder, false, ping_interval, deadline, bridge, bridge_ctx, provider, round_in_flight)))
                        }
                        Some(Err(e)) => {
                            tracing::error!("读取响应流失败: {}", e);
                            let final_events = if ctx.is_empty_response() {
                                let oversized = ctx.empty_response_is_oversized_context();
                                tracing::warn!(
                                    oversized_context = oversized,
                                    est_input_tokens = ctx.input_tokens,
                                    "流解码错误且无内容，补发 error 事件"
                                );
                                if oversized {
                                    ctx.generate_final_events()
                                } else {
                                    vec![empty_response_error_event(false)]
                                }
                            } else {
                                tracing::warn!(
                                    est_input_tokens = ctx.input_tokens,
                                    "流读取错误但已产生部分内容，补发 error 事件防止伪装成正常完成"
                                );
                                vec![stream_interrupted_error_event()]
                            };
                            let bytes: Vec<Result<Bytes, Infallible>> = final_events
                                .into_iter()
                                .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                .collect();
                            Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval, deadline, bridge, bridge_ctx, provider, round_in_flight)))
                        }
                        None => {
                            // 桥接态（存在待执行搜索且无 in-flight 轮）→ spawn 后台
                            // 桥接轮（修复③ v2），JoinHandle 存入状态元组第 10 元，
                            // 由 select! 的条件分支收割；剩余 pending 在续流自然结束
                            // 后经本分支继续 drain 执行。最终收尾
                            // （generate_final_events 含 message_stop）由桥接在全部
                            // 轮次结束后统一执行一次。D4：上游错误/空响应兜底不触发
                            // 续请求（bridge.pending 为空）。
                            // in-flight 守卫：已有轮次执行中时本分支不可再 spawn
                            // （一个 unfold 状态同一时刻至多一轮桥接；耗尽的
                            // body_stream 在 in-flight 期间也不会再以 None 就绪
                            // 进入本分支——它已被替换为永不就绪的占位流）。
                            if let Some(mut state) =
                                bridge.take().filter(|b| !b.pending.is_empty()).filter(|_| round_in_flight.is_none())
                            {
                                let pending = state.pending.pop_front().unwrap();
                                let round_bridge_ctx = bridge_ctx
                                    .as_ref()
                                    .expect("桥接态下 bridge_ctx 必然存在")
                                    .clone();
                                let result_tool_use_id = pending.tool_use_id.clone();

                                // /cc 全局 deadline 兜底：deadline 本只在 select! 分支
                                // 中检查，桥接轮（MCP + 续流建连 + 续流本身）不感知会
                                // 使总耗时远超 300s。每轮执行前校验剩余预算，超限放弃
                                // 续请求——先补发结果块（与 server_tool_use 配对），
                                // 再按 deadline 分支同款 error 收尾
                                if deadline.is_some_and(|d| Instant::now() >= d) {
                                    tracing::warn!(
                                        "web_search 桥接轮撞上 /cc 全局 deadline，放弃续请求"
                                    );
                                    let mut out_events = build_web_search_result_events(
                                        &mut ctx,
                                        &result_tool_use_id,
                                        &None,
                                    );
                                    out_events.push(deadline_error_event());
                                    let bytes: Vec<Result<Bytes, Infallible>> = out_events
                                        .into_iter()
                                        .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                        .collect();
                                    return Some((
                                        stream::iter(bytes),
                                        (
                                            body_stream,
                                            ctx,
                                            decoder,
                                            true,
                                            ping_interval,
                                            deadline,
                                            bridge,
                                            Some(round_bridge_ctx),
                                            provider,
                                            round_in_flight,
                                        ),
                                    ));
                                }

                                // 修复③ v2：桥接轮 spawn 后台执行，handle 连同本轮
                                // tool_use_id 存入状态元组第 10 元，由 select! 条件
                                // 分支收割（执行期间 ping/deadline 分支照常触发）。
                                // bridge 按值移入任务（owned 变体带回更新后的状态）。
                                // 耗尽的 body_stream 同步换为永不就绪占位流：in-flight
                                // 期间 body 分支不得以 None 就绪被 select 抢选——
                                // 否则 flatten 重入 unfold 会误走收尾路径、丢桥接结果
                                // （CRITICAL 修复点）。
                                let provider_for_round = provider.clone();
                                let handle = tokio::spawn(bridge_execute_round_owned(
                                    provider_for_round,
                                    round_bridge_ctx.clone(),
                                    state,
                                    pending,
                                ));
                                return Some((
                                    stream::iter(Vec::<Result<Bytes, Infallible>>::new()),
                                    (
                                        // 耗尽的 body_stream 不再回填：in-flight
                                        // 期间换入永不就绪占位流，防止 body 分支
                                        // 以 None 就绪被 select 抢选（flatten 重入
                                        // 会误走收尾路径丢桥接结果，CRITICAL 修复）
                                        stream::pending().boxed(),
                                        ctx,
                                        decoder,
                                        false,
                                        ping_interval,
                                        deadline,
                                        bridge,
                                        Some(round_bridge_ctx),
                                        provider,
                                        Some((handle, result_tool_use_id)),
                                    ),
                                ));
                                }

                            // 非桥接态（或桥接无待执行搜索）→ 现有收尾路径（零行为变化）
                            let mut out_events = Vec::new();
                            if ctx.is_empty_response() {
                                let oversized = ctx.empty_response_is_oversized_context();
                                tracing::warn!(
                                    oversized_context = oversized,
                                    est_input_tokens = ctx.input_tokens,
                                    "上游返回空响应（无任何内容事件），补发 error 事件"
                                );
                                if oversized {
                                    out_events = ctx.generate_final_events();
                                } else {
                                    out_events.push(empty_response_error_event(false));
                                }
                            } else {
                                out_events = ctx.generate_final_events();
                            }
                            let bytes: Vec<Result<Bytes, Infallible>> = out_events
                                .into_iter()
                                .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                .collect();
                            Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval, deadline, bridge, bridge_ctx, provider, round_in_flight)))
                        }
                    }
                }
                // 发送 ping 保活
                _ = ping_interval.tick() => {
                    tracing::trace!("发送 ping 保活事件");
                    let bytes: Vec<Result<Bytes, Infallible>> = vec![Ok(create_ping_sse())];
                    Some((stream::iter(bytes), (body_stream, ctx, decoder, false, ping_interval, deadline, bridge, bridge_ctx, provider, round_in_flight)))
                }
                // 全局 deadline：防止上游挂起导致请求永不结束（deadline 为 None 时永不就绪）。
                // in-flight 桥接轮存在时 deadline 触发同样会终止流——先补发本轮配对
                // 结果块再发 error（server_tool_use / web_search_tool_result 必须成对）
                _ = wait_deadline(deadline) => {
                    tracing::error!("流式转发全局超时，强制终止");
                    let mut events = if let Some((handle, result_tool_use_id)) =
                        round_in_flight.take()
                    {
                        // 后台任务可能仍在执行，abort 即可（配对块按 Failed(None) 空数组兜底）
                        handle.abort();
                        bridge = Some(bridge.take().unwrap_or_else(|| BridgeState::new(Some(0))));
                        build_web_search_result_events(&mut ctx, &result_tool_use_id, &None)
                    } else {
                        Vec::new()
                    };
                    events.push(deadline_error_event());
                    let bytes = events
                        .into_iter()
                        .map(|e| Ok(Bytes::from(e.to_sse_string())))
                        .collect::<Vec<_>>();
                    Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval, deadline, bridge, bridge_ctx, provider, round_in_flight)))
                }
            }
        },
    )
    .flatten();

    initial_stream.chain(processing_stream)
}
