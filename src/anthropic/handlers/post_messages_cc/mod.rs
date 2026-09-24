// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic API Handler 函数

use crate::kiro::model::requests::kiro::KiroRequest;
use crate::token;
use axum::{
    Extension,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use bytes::Bytes;
use std::time::Duration;

use super::bridge::build_bridge_context;
use crate::anthropic::converter::{ConversionError, convert_request};
use crate::anthropic::middleware::{ApiKeyContext, AppState};
use crate::anthropic::types::ErrorResponse;

use super::error::parse_messages_request;
use super::helpers::{
    extract_client_ip, override_thinking_from_model_name, resolve_thinking_enabled,
};
use super::nonstream::handle_non_stream_request;
use super::stream::handle_stream_request;
use crate::anthropic::websearch;

/// POST /cc/v1/messages
///
/// Claude Code 兼容端点，与 /v1/messages 的区别在于：
/// - 流式响应带 300s 全局 deadline，防止上游挂起导致请求永不结束（/v1 无此限制）
///
/// 其余行为与 /v1/messages 完全一致：同为实时转发，`message_start` 先给估算
/// `input_tokens`，末尾 `message_delta` 给出终值。
pub async fn post_messages_cc(
    State(state): State<AppState>,
    identity: Option<Extension<ApiKeyContext>>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    let mut payload = match parse_messages_request(&body) {
        Ok(p) => p,
        Err(resp) => return resp,
    };
    tracing::info!(
        model = %payload.model,
        max_tokens = %payload.max_tokens,
        stream = %payload.stream,
        message_count = %payload.messages.len(),
        "Received POST /cc/v1/messages request"
    );

    // 检查 KiroProvider 是否可用
    let provider = match &state.kiro_provider {
        Some(p) => p.clone(),
        None => {
            tracing::error!("KiroProvider 未配置");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new(
                    "service_unavailable",
                    "Kiro API provider not configured",
                )),
            )
                .into_response();
        }
    };

    // 检测模型名是否包含 "thinking" 后缀，若包含则覆写 thinking 配置
    override_thinking_from_model_name(&mut payload);
    tracing::info!(
        thinking_type = ?payload.thinking.as_ref().map(|t| t.thinking_type.as_str()),
        budget_tokens = ?payload.thinking.as_ref().map(|t| t.budget_tokens),
        "[thinking] 配置"
    );

    let bound_ids: Vec<u64> = identity
        .as_ref()
        .and_then(|ext| ext.0.bound_credential_ids.clone())
        .unwrap_or_default();

    // 检查是否为 WebSearch 请求
    if websearch::has_web_search_tool(&payload) {
        tracing::info!("检测到 WebSearch 工具，路由到 WebSearch 处理");

        // 估算输入 tokens
        let input_tokens = token::count_all_tokens(
            payload.model.clone(),
            payload.system.clone(),
            payload.messages.clone(),
            payload.tools.clone(),
        ) as i32;

        return websearch::handle_websearch_request(provider, &payload, input_tokens, &bound_ids)
            .await;
    }

    // 转换请求
    let conversion_result = match convert_request(&payload) {
        Ok(result) => result,
        Err(e) => {
            let (error_type, message) = match &e {
                ConversionError::UnsupportedModel(model) => {
                    ("invalid_request_error", format!("模型不支持: {}", model))
                }
                ConversionError::EmptyMessages => {
                    ("invalid_request_error", "消息列表为空".to_string())
                }
            };
            tracing::warn!("请求转换失败: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(error_type, message)),
            )
                .into_response();
        }
    };

    // 是否为 Claude Code /compact 压缩请求（决定上游超时：普通 180s / 压缩 1000s）
    let is_compact_request = conversion_result.is_compact_request;
    // 客户端是否请求了 thinking adaptive（与账号级开关在 provider 侧共同决定注入）
    let thinking_adaptive_requested = conversion_result.thinking_adaptive_requested;

    // web_search server tool 桥接上下文（D5/D7：未携带时为 None，零行为变化）
    // 必须在 KiroRequest 构建（conversation_state 被 move）前构造
    let bridge_ctx = build_bridge_context(
        &conversion_result,
        state.profile_arn.clone(),
        bound_ids.clone(),
    );

    // 构建 Kiro 请求
    let kiro_request = KiroRequest {
        conversation_state: conversion_result.conversation_state,
        profile_arn: state.profile_arn.clone(),
        additional_model_request_fields: conversion_result.additional_model_request_fields,
    };

    let request_body = match serde_json::to_string(&kiro_request) {
        Ok(body) => body,
        Err(e) => {
            tracing::error!("序列化请求失败: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    "internal_error",
                    format!("序列化请求失败: {}", e),
                )),
            )
                .into_response();
        }
    };

    // 请求体可能包含 API Key 与敏感上下文，禁止整包入日志（cr-result C4）

    // 构造 fingerprint profile（cc 端点同样接入指纹追踪）
    let fp_tracker = state.fingerprint_tracker.clone();
    let fp_profile = fp_tracker.as_ref().map(|_| {
        crate::cache::fingerprint::FingerprintTracker::build_profile_with_tools(
            payload.system.as_deref(),
            &payload.messages,
            payload.tools.as_deref(),
        )
    });

    // 估算"缓存前缀" token 数（与 post_messages 同口径，先借用后消费）
    let prefix_estimated_tokens = {
        let n = payload.messages.len();
        let prior: &[_] = if n > 0 {
            &payload.messages[..n - 1]
        } else {
            &[]
        };
        token::count_prefix_tokens(payload.system.as_deref(), prior, payload.tools.as_deref())
            as i32
    };

    // 估算输入 tokens（复用上方已计算的 prefix_estimated_tokens，避免重复编码历史消息）
    // 先取出 thinking_enabled 判断所需字段，避免 payload.tools 等被移动后无法整体借用
    let thinking_enabled = resolve_thinking_enabled(&payload.model, &payload.thinking);
    let input_tokens = token::count_all_tokens_with_prefix(
        payload.model.clone(),
        payload.system,
        payload.messages,
        payload.tools,
        prefix_estimated_tokens as u64,
    ) as i32;

    // 提取用量追踪信息
    let api_key_id = identity.map(|ext| ext.0.id);
    let usage_tracker = state.usage_tracker.clone();
    let client_ip = extract_client_ip(&headers, Some(&addr));

    // 计算 prompt cache 模拟 usage
    let prompt_cache_usage = crate::cache::PromptCacheUsage::from_ratio_config(
        input_tokens,
        crate::cache::CacheSimulationRatioConfig::fixed(0.85),
        0.1,
    );

    let json_schema_requested = payload
        .output_config
        .as_ref()
        .and_then(|c| c.format.as_ref())
        .map(|f| f.format_type == "json_schema")
        .unwrap_or(false);

    // effort 级别（output_config 整体存在时取 effort 字符串，否则 None）
    let effort = payload.output_config.as_ref().map(|c| c.effort.clone());

    if payload.stream {
        // 流式响应：与 /v1 相同的实时转发，额外带 300s 全局 deadline
        // （上游挂起保护，沿用此端点历史上一直具备的 5min 上限）
        handle_stream_request(
            provider,
            &request_body,
            &payload.model,
            input_tokens,
            prefix_estimated_tokens,
            thinking_enabled,
            usage_tracker.clone(),
            api_key_id,
            prompt_cache_usage,
            bound_ids,
            client_ip,
            Some(Duration::from_secs(300)),
            is_compact_request,
            thinking_adaptive_requested,
            bridge_ctx,
            effort,
        )
        .await
    } else {
        // 非流式响应（复用现有逻辑，已经使用正确的 input_tokens）
        handle_non_stream_request(
            provider,
            &request_body,
            &payload.model,
            input_tokens,
            prefix_estimated_tokens,
            usage_tracker,
            api_key_id,
            prompt_cache_usage,
            bound_ids,
            client_ip,
            json_schema_requested,
            fp_tracker,
            fp_profile,
            is_compact_request,
            thinking_adaptive_requested,
            bridge_ctx,
            effort,
        )
        .await
    }
}
