// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic API Handler 函数

use crate::anthropic::types::ErrorResponse;

use crate::kiro::model::events::Event;
use crate::kiro::model::requests::conversation::ConversationState;
use crate::kiro::parser::decoder::EventStreamDecoder;
use crate::token;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;
use std::collections::VecDeque;
use uuid::Uuid;

use super::super::websearch;
use super::bridge::{BridgeContext, PendingSearch, parse_bridge_query};
use super::bridge::{
    build_continuation_request, build_search_tool_result, build_web_search_result_block,
    flush_unpaired_search_blocks,
};
use super::error::map_provider_error_with_context;
use super::helpers::strip_json_fences;

/// 组装非流式响应的 content 数组
///
/// 块顺序与流式路径一致：thinking → text → tool_use。
///
/// 返回 `(content, thinking_only)`。`thinking_only` 为 true 表示整段响应只有
/// thinking、既无可见文本也无工具调用 —— 此时补一个占位空格 text 块，避免客户端
/// 把 content 判定为空响应而卡住（流式路径在 `generate_final_events` 里有等价
/// 兜底）。调用方需据此把 `stop_reason` 调整为 `max_tokens`。
pub(crate) fn build_non_stream_content(
    thinking_content: &str,
    text_content: &str,
    tool_uses: Vec<serde_json::Value>,
) -> (Vec<serde_json::Value>, bool) {
    let thinking_only =
        !thinking_content.is_empty() && text_content.is_empty() && tool_uses.is_empty();

    let mut content: Vec<serde_json::Value> = Vec::new();

    // thinking 块必须排在可见内容之前。上游不返回真实签名，沿用流式路径同一份
    // 伪造实现，保证两端 thinking 块结构一致。
    if !thinking_content.is_empty() {
        content.push(json!({
            "type": "thinking",
            "thinking": thinking_content,
            "signature": super::super::stream::generate_fake_signature()
        }));
    }

    let visible = if thinking_only { " " } else { text_content };
    if !visible.is_empty() {
        content.push(json!({
            "type": "text",
            "text": visible
        }));
    }

    content.extend(tool_uses);
    (content, thinking_only)
}

/// 非流式桥接的单步状态转移（D4 非流式段）
///
/// 与流式 `bridge_handle_event` 的语义对齐，返回 `(intercepted, completed)`：
/// - `intercepted = true`：该 toolUse 被桥接截获，调用方不得再按普通 tool_use 处理
///   （不置 has_tool_use、不 push tool_uses，避免 stop_reason 误覆盖）
/// - `completed = Some(PendingSearch)`：本次 toolUse.stop 使截获完成，query 已解析，
///   调用方在事件循环读取完毕后统一执行 MCP → 续请求
///
/// `rounds_used >= max_rounds`（轮次耗尽）或 `max_rounds == 0`（非桥接请求）时
/// 不截获，web_search toolUse 按普通路径透传（D8）。
pub(crate) fn non_stream_bridge_step(
    collecting: &mut Option<(String, String)>,
    rounds_used: usize,
    max_rounds: usize,
    tu: &crate::kiro::model::events::ToolUseEvent,
) -> (bool, Option<PendingSearch>) {
    // 已在聚合中：继续累积（无论是否 web_search 名称，按 tool_use_id 归属判定）
    if let Some((id, buffer)) = collecting.as_mut() {
        if tu.tool_use_id == *id {
            buffer.push_str(&tu.input);
            if tu.stop {
                let (id, buf) = collecting.take().expect("collecting 已判定存在");
                let query = parse_bridge_query(&buf);
                return (
                    true,
                    Some(PendingSearch {
                        tool_use_id: id,
                        query,
                    }),
                );
            }
            return (true, None);
        }
        // 其他工具的事件不干扰当前聚合
        return (false, None);
    }

    // 新的 web_search toolUse 且轮次未达上限 → 开始截获
    if tu.name == "web_search" && rounds_used < max_rounds {
        if tu.stop {
            // 单事件完整调用，直接完成截获
            let query = parse_bridge_query(&tu.input);
            return (
                true,
                Some(PendingSearch {
                    tool_use_id: tu.tool_use_id.clone(),
                    query,
                }),
            );
        }
        *collecting = Some((tu.tool_use_id.clone(), tu.input.clone()));
        return (true, None);
    }

    (false, None)
}

/// 处理非流式请求
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_non_stream_request(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    input_tokens: i32,
    prefix_estimated_tokens: i32,
    usage_tracker: Option<std::sync::Arc<crate::model::usage::UsageTracker>>,
    api_key_id: Option<u32>,
    prompt_cache_usage: crate::cache::PromptCacheUsage,
    bound_ids: Vec<u64>,
    client_ip: Option<String>,
    json_schema_requested: bool,
    fp_tracker: Option<std::sync::Arc<crate::cache::fingerprint::FingerprintTracker>>,
    fp_profile: Option<Vec<crate::cache::fingerprint::ContentSegment>>,
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
        .call_api(
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

    // 读取响应体
    let body_bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("读取响应体失败: {}", e);
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse::new(
                    "api_error",
                    format!("读取响应失败: {}", e),
                )),
            )
                .into_response();
        }
    };

    // ---- 事件收集状态（首次请求与每轮续请求共用）----
    let mut text_content = String::new();
    let mut tool_uses: Vec<serde_json::Value> = Vec::new();
    let mut has_tool_use = false;
    let mut stop_reason = "end_turn".to_string();
    // 从 contextUsageEvent 计算的实际输入 tokens（已弃用，保留诊断字段恒为 None）
    let context_input_tokens: Option<i32> = None;
    let mut metering_cache_read_tokens: Option<i32> = None;
    let mut metering_cache_creation_tokens: Option<i32> = None;
    let mut metering_usage: Option<f64> = None;

    // 收集工具调用的增量 JSON
    let mut tool_json_buffers: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // ---- web_search 非流式桥接状态（D4 非流式段 / D5）----
    // 多轮上限 min(max_uses, 5)（D8）；bridge_ctx 为 None 时 max_rounds = 0，
    // 截获分支短路，非桥接请求零行为变化
    let max_rounds = bridge_ctx
        .as_ref()
        .map(|ctx| ctx.max_uses.unwrap_or(5).clamp(0, 5) as usize)
        .unwrap_or(0);
    let mut rounds_used: usize = 0;
    // 已截获完成、待执行的搜索队列（每轮事件读取完毕后按截获顺序逐个执行
    // MCP → 续请求；Collecting 期间上游连发多次 web_search 时不丢失）
    let mut pending_search: VecDeque<PendingSearch> = VecDeque::new();
    // 正在聚合 input 分片的 web_search toolUse（(tool_use_id, buffer)）
    let mut collecting: Option<(String, String)> = None;
    // 多轮桥接的演进基底（D3：每轮续请求基于上一轮续请求所用状态演进）
    let mut evolution_base: Option<ConversationState> = None;
    // 客户端可见性块（D5 非流式顺序：server_tool_use → web_search_tool_result
    // 逐轮交错，前置于 thinking/text/tool_use，无裸 tool_use 块）
    let mut visibility_blocks: Vec<serde_json::Value> = Vec::new();
    // 截获的 web_search input 字符累计（S2 计费口径对齐流式 bridge_handle_event：
    // 上游已生成分片即已计费；每轮所有 intercepted 事件的 input 均属截获调用）
    let mut intercepted_input_chars: i64 = 0;

    let mut body_bytes = body_bytes;
    'rounds: loop {
        // 解析事件流（首次请求与每轮续请求共用同一套收集逻辑）
        let mut decoder = EventStreamDecoder::new();
        if let Err(e) = decoder.feed(&body_bytes) {
            tracing::warn!("缓冲区溢出: {}", e);
        }

        for result in decoder.decode_iter() {
            match result {
                Ok(frame) => {
                    if let Ok(event) = Event::from_frame(frame) {
                        match event {
                            Event::AssistantResponse(resp) => {
                                text_content.push_str(&resp.content);
                            }
                            Event::ToolUse(tool_use) => {
                                // 桥接截获（D4 非流式段）：轮次未达上限的
                                // web_search toolUse 聚合分片，不解析为普通
                                // tool_use 块（不置 has_tool_use，避免
                                // stop_reason 误覆盖）；轮次耗尽按普通路径透传
                                let (intercepted, completed) = non_stream_bridge_step(
                                    &mut collecting,
                                    rounds_used,
                                    max_rounds,
                                    &tool_use,
                                );
                                if intercepted {
                                    // 计费口径（S2，对齐流式 bridge_handle_event 与
                                    // process_tool_use）：截获调用的全部分片 input
                                    // 无条件计入——上游已生成即已计费
                                    intercepted_input_chars += tool_use.input.len() as i64;
                                    if let Some(pending) = completed {
                                        visibility_blocks.push(json!({
                                            "type": "server_tool_use",
                                            "id": pending.tool_use_id.clone(),
                                            "name": "web_search",
                                            "input": { "query": pending.query.clone() }
                                        }));
                                        pending_search.push_back(pending);
                                        rounds_used += 1;
                                    }
                                    continue;
                                }

                                has_tool_use = true;

                                // 累积工具的 JSON 输入
                                let buffer = tool_json_buffers
                                    .entry(tool_use.tool_use_id.clone())
                                    .or_default();
                                buffer.push_str(&tool_use.input);

                                // 如果是完整的工具调用，添加到列表
                                if tool_use.stop {
                                    let input: serde_json::Value = if buffer.is_empty() {
                                        serde_json::json!({})
                                    } else {
                                        serde_json::from_str(buffer).unwrap_or_else(|e| {
                                            tracing::warn!(
                                                "工具输入 JSON 解析失败: {}, tool_use_id: {}",
                                                e,
                                                tool_use.tool_use_id
                                            );
                                            serde_json::json!({})
                                        })
                                    };

                                    tool_uses.push(json!({
                                        "type": "tool_use",
                                        "id": tool_use.tool_use_id,
                                        "name": tool_use.name,
                                        "input": input
                                    }));
                                }
                            }
                            Event::ContextUsage(context_usage) => {
                                // contextUsage 本地化：弃用 percentage × window 反算，
                                // 仅保留 100% 触发 stop_reason 兜底
                                if context_usage.context_usage_percentage >= 100.0 {
                                    stop_reason = "model_context_window_exceeded".to_string();
                                }
                                tracing::debug!(
                                    "[deprecated] contextUsageEvent: {:.2}% (仅记录, 不参与 input_tokens 反算)",
                                    context_usage.context_usage_percentage,
                                );
                            }
                            Event::Metering(metering) => {
                                metering_cache_read_tokens = metering.cache_read_input_tokens;
                                metering_cache_creation_tokens =
                                    metering.cache_creation_input_tokens;
                                metering_usage = Some(metering.usage);
                            }
                            Event::Exception { exception_type, .. }
                                if exception_type == "ContentLengthExceededException" =>
                            {
                                stop_reason = "max_tokens".to_string();
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("解码事件失败: {}", e);
                }
            }
        }

        // 事件读取完毕：无待执行搜索 → 全部轮次结束，退出收集循环
        // （thinking 剥离与 JSON 组装在循环后统一进行，见下方）
        let Some(pending) = pending_search.pop_front() else {
            break 'rounds;
        };
        let Some(ctx) = bridge_ctx.as_ref() else {
            break 'rounds;
        };

        // 1. MCP 真实搜索（失败降级 error ToolResult，仍发续请求让模型解读）
        let (_mcp_tool_use_id, mcp_request) = websearch::create_mcp_request(&pending.query);
        let search_results =
            match websearch::call_mcp_api(&provider, &mcp_request, &ctx.bound_ids).await {
                Ok(resp) => websearch::parse_search_results(&resp),
                Err(e) => {
                    tracing::warn!(
                        tool_use_id = %pending.tool_use_id,
                        "web_search MCP 调用失败，降级为 error ToolResult: {}",
                        e
                    );
                    None
                }
            };
        // web_search_tool_result 可见性块（MCP 完成后携带真实结果；失败为空数组）
        visibility_blocks.push(build_web_search_result_block(
            &pending.tool_use_id,
            &search_results,
        ));

        // 2. 构建续请求（D3：仅替换 current_message.tool_results，
        //    conversationId/agentContinuationId/history 逐字节不变）
        let tool_result =
            build_search_tool_result(&pending.tool_use_id, &pending.query, &search_results);
        let kiro_request =
            build_continuation_request(ctx, evolution_base.take(), vec![tool_result]);
        let request_body = match serde_json::to_string(&kiro_request) {
            Ok(body) => body,
            Err(e) => {
                // 降级：放弃续请求，保留首轮已收集内容走正常组装路径
                // （与流式 Failed 分支语义对齐——结果块已入 visibility_blocks）
                tracing::error!("web_search 续请求序列化失败，降级返回已收集内容: {}", e);
                // 不写回 evolution_base：break 后直接退出 'rounds 循环，
                // 该变量不再被读取，写回无实际效果
                flush_unpaired_search_blocks(&mut pending_search, &mut visibility_blocks);
                break 'rounds;
            }
        };

        // 3. 续请求：响应体继续进入同一收集循环（多轮在同一 loop 内演进）
        match provider
            .call_api(
                &request_body,
                ctx.is_compact_request,
                ctx.thinking_adaptive_requested,
                &ctx.bound_ids,
            )
            .await
        {
            Ok((resp, _credential_id)) => {
                body_bytes = match resp.bytes().await {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        // 降级：续请求响应读取失败，保留已收集内容正常收尾
                        tracing::error!(
                            "读取 web_search 续请求响应体失败，降级返回已收集内容: {}",
                            e
                        );
                        // 不写回 evolution_base：break 后直接退出 'rounds 循环
                        flush_unpaired_search_blocks(&mut pending_search, &mut visibility_blocks);
                        break 'rounds;
                    }
                };
                // 演进基底更新为本轮续请求所用状态（下一轮基于它演进）
                evolution_base = Some(kiro_request.conversation_state);
            }
            Err(e) => {
                // 降级：续请求发起失败，保留首轮已收集内容走正常组装路径
                // （与流式 Failed 分支语义对齐，不再丢弃已有响应）
                tracing::error!("web_search 续请求发起失败，降级返回已收集内容: {}", e);
                // 不写回 evolution_base：break 后直接退出 'rounds 循环
                flush_unpaired_search_blocks(&mut pending_search, &mut visibility_blocks);
                break 'rounds;
            }
        }
    }

    // 确定 stop_reason：tool_use 优先级最高，存在工具调用时无条件覆盖
    // max_tokens / model_context_window_exceeded（这些是下一轮才该报告的状态，
    // 不能盖掉本轮的 tool_use，否则客户端只渲染工具块而不执行）。
    if has_tool_use {
        // [TOOLUSE-DIAG] 非流式工具调用收尾诊断：记录覆盖前的原始 stop_reason。
        //
        // 仅在收到过 ToolUse 事件却没拼出任何完整调用时用 warn 上报 —— 此时客户端会
        // 收到 stop_reason=tool_use 但 content 里没有 tool_use 块，即"只显示 call
        // 不执行"的结构特征。正常情况降为 debug，避免每个响应刷一条警告。
        if tool_uses.is_empty() {
            tracing::warn!(
                "[TOOLUSE-DIAG] non_stream 结构异常: has_tool_use=true 但无完整工具调用 \
                 raw_stop_reason={} tool_use_count=0 final_stop_reason=tool_use",
                stop_reason,
            );
        } else {
            tracing::debug!(
                "[TOOLUSE-DIAG] non_stream has_tool_use=true raw_stop_reason={} \
                 tool_use_count={} final_stop_reason=tool_use",
                stop_reason,
                tool_uses.len(),
            );
        }
        stop_reason = "tool_use".to_string();
    }

    // 上游把推理内容内联在 AssistantResponse.content 的 <thinking> 标签里（与是否
    // 流式无关）。流式路径由 process_content_with_thinking 剥离；非流式此前直接把
    // 整段当可见文本，导致标签原文发给客户端、混入 output_tokens，并让
    // strip_json_fences 无法产出可解析的结构化输出。
    let (thinking_content, visible_text) =
        super::super::stream::split_thinking_and_visible(&text_content);
    text_content = visible_text;

    // JSON schema 结构化输出：去除模型可能添加的 Markdown 代码围栏
    if json_schema_requested && !text_content.is_empty() {
        text_content = strip_json_fences(text_content);
    }

    // 构建响应内容
    let (mut content, thinking_only) =
        build_non_stream_content(&thinking_content, &text_content, tool_uses);

    // 估算输出 tokens——必须先于可见性块拼接（H2）：server_tool_use /
    // web_search_tool_result 是桥接可见性元数据，不代表模型真实输出量。
    // 截获的 web_search input 单独叠加（S2，对齐流式 output_chars_other 口径）：
    // 上游已生成这段内容即已计费，但可见性元数据本身不计入
    let mut output_tokens = token::estimate_output_tokens(&content);
    if intercepted_input_chars > 0 {
        // 与流式同口径（非中文桶 `(chars+3)/4`，见 tokens_from_chars）
        output_tokens += ((intercepted_input_chars + 3) / 4) as i32;
    }

    // web_search 桥接可见性块前置于 thinking/text/tool_use（D5 非流式顺序：
    // server_tool_use → web_search_tool_result 逐轮交错在前，无裸 tool_use 块）
    if !visibility_blocks.is_empty() {
        let mut with_visibility = visibility_blocks;
        with_visibility.append(&mut content);
        content = with_visibility;
    }

    // 退化响应（只有 thinking）与流式路径对齐报 max_tokens；但绝不覆盖 tool_use ——
    // 那会让客户端只渲染工具块而不执行（见上方 [TOOLUSE-DIAG] 注释）。
    if thinking_only && !has_tool_use {
        stop_reason = "max_tokens".to_string();
    }

    // contextUsage 本地化后 input_tokens 来源优先级：metering 真值 → 本地 count_all_tokens 估算
    // `context_input_tokens` 已弃用（始终为 None），保留参数仅供 cap_input_tokens 签名兼容
    let _ = context_input_tokens; // 标记已读以避免 unused
    let raw_final_input_tokens = input_tokens;
    let final_input_tokens =
        super::super::stream::cap_input_tokens_pub(raw_final_input_tokens, input_tokens, model);

    // 本地估算 ≥ 1M 兜底触发 stop_reason
    if final_input_tokens >= 1_000_000 && stop_reason == "end_turn" {
        stop_reason = "model_context_window_exceeded".to_string();
    }

    tracing::info!(
        "[input_tokens] 本地化: estimated={} final={}",
        input_tokens,
        final_input_tokens
    );

    // 四层降级链：metering 真值 → prefix 估算 → 指纹追踪 → 比例模拟
    let sim_usage = prompt_cache_usage.scale_to(final_input_tokens);
    let metering_pair = match (metering_cache_read_tokens, metering_cache_creation_tokens) {
        (Some(read), Some(creation)) => Some((read, creation)),
        _ => None,
    };
    // 显式注入：handler 始终算出了 prefix_estimated_tokens（可能为 0），
    // 直接用 Some 让 select_final_usage 选用 prefix 分支而非降级到 fingerprint/模拟
    let prefix_estimated = Some(prefix_estimated_tokens.max(0));
    let fingerprint_usage = match (fp_tracker.as_ref(), fp_profile.as_ref()) {
        (Some(tracker), Some(profile)) => {
            let account_id = credential_id.to_string();
            tracker.compute(&account_id, profile, final_input_tokens)
        }
        _ => None,
    };
    let final_usage = crate::cache::select_final_usage(
        final_input_tokens,
        metering_pair,
        prefix_estimated,
        fingerprint_usage,
        sim_usage,
    );

    // 流结束后写入指纹表（仅当 credential_id 确定）
    if let (Some(tracker), Some(profile)) = (fp_tracker.as_ref(), fp_profile.clone()) {
        let account_id = credential_id.to_string();
        tracker.update(&account_id, profile);
    }

    let report_input = final_usage.input_tokens;
    let report_cache_creation = final_usage.cache_creation_input_tokens;
    let report_cache_read = final_usage.cache_read_input_tokens;
    let report_creation_5m = final_usage.cache_creation_5m_input_tokens;
    let report_creation_1h = final_usage.cache_creation_1h_input_tokens;

    // 记录用量（内部使用真实值）
    if let (Some(tracker), Some(key_id)) = (&usage_tracker, api_key_id) {
        tracing::info!(
            "[usage] 入库: model={} input={} output={} metering_credits={:?} cache_read={} cache_creation={} api_key={} credential=Some({})",
            model,
            final_input_tokens,
            output_tokens,
            metering_usage,
            report_cache_read,
            report_cache_creation,
            key_id,
            credential_id
        );
        tracker.record(
            key_id,
            Some(credential_id),
            model.to_string(),
            final_input_tokens,
            output_tokens,
            client_ip,
            metering_usage,
            Some(report_cache_read),
            Some(report_cache_creation),
            effort.clone(),
        );
    }

    // 构建 Anthropic 响应
    let response_body = json!({
        "id": format!("msg_{}", Uuid::new_v4().to_string().replace('-', "")),
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": model,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        // 客户端展示缩放（output_tokens 不缩放）；tracker 已写入真实值。
        // content 构建前已分离 thinking，且 estimate_output_tokens 只累加 text 与
        // tool_use.input（thinking 块的字段名是 "thinking"，不参与统计），
        // 因此 output_tokens 就是可见输出，直接上报真值，不再套 min(380) 上限。
        "usage": {
            "input_tokens": super::super::stream::scale_for_client(report_input, model),
            "output_tokens": output_tokens,
            "cache_creation_input_tokens": super::super::stream::scale_for_client(report_cache_creation, model),
            "cache_read_input_tokens": super::super::stream::scale_for_client(report_cache_read, model),
            "cache_creation": {
                "ephemeral_5m_input_tokens": super::super::stream::scale_for_client(report_creation_5m, model),
                "ephemeral_1h_input_tokens": super::super::stream::scale_for_client(report_creation_1h, model)
            }
        }
    });

    (StatusCode::OK, Json(response_body)).into_response()
}
