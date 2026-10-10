// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! /v1/messages 端点主函数：接收 Anthropic 请求、直通流式转发至 Kiro，并提供请求体调试转储（dump_kiro_request_body）。

use crate::anthropic::converter::{
    ConversionError, additional_fields_skipped, convert_request, is_gpt_model, is_luna_model,
};
use crate::anthropic::middleware::{ApiKeyContext, AppState};
use crate::anthropic::types::ErrorResponse;

use crate::kiro::model::requests::kiro::KiroRequest;
use crate::token;
use axum::{
    Extension,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use bytes::Bytes;

use super::bridge::build_bridge_context;
use super::error::parse_messages_request;
use super::helpers::{
    extract_client_ip, is_suggestion_mode_request, override_thinking_from_model_name,
    resolve_thinking_enabled, suggestion_mode_response,
};
use super::nonstream::handle_non_stream_request;
use super::stream::handle_stream_request;
use crate::anthropic::websearch;

/// POST /v1/messages
///
/// 创建消息（对话）
pub async fn post_messages(
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

    // 拦截 Claude Code 输入建议请求（Suggestion Mode）：不转发 Kiro，直接返回空响应。
    // 这类请求携带全量上下文却只产出候选短语，转发会按全价计费且污染 prompt cache。
    // forward_suggestion_mode=true 时放行，正常转发上游（设置页可运行时热切换）。
    if !state
        .forward_suggestion_mode
        .load(std::sync::atomic::Ordering::Relaxed)
        && is_suggestion_mode_request(&payload)
    {
        tracing::info!(
            message_count = %payload.messages.len(),
            "拦截 Suggestion Mode 输入建议请求，返回空响应（不转发上游）"
        );
        return suggestion_mode_response(payload.stream);
    }

    tracing::info!(
        model = %payload.model,
        max_tokens = %payload.max_tokens,
        stream = %payload.stream,
        message_count = %payload.messages.len(),
        "Received POST /v1/messages request"
    );

    // 记录 RPM（全局 + per-API-Key）
    if let Some(rpm_tracker) = &state.rpm_tracker {
        let api_key_id = identity.as_ref().map(|ext| ext.0.id);
        rpm_tracker.record_request(api_key_id);
    }

    let bound_ids: Vec<u64> = identity
        .as_ref()
        .and_then(|ext| ext.0.bound_credential_ids.clone())
        .unwrap_or_default();

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
                ConversionError::MissingUserMessage
                | ConversionError::UnsupportedRole { .. }
                | ConversionError::InvalidSystemContent { .. } => {
                    ("invalid_request_error", e.to_string())
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

    // 请求体可能包含 API Key 与敏感上下文，禁止整包入日志（cr-result C3）

    // 临时取证：KIRO_DUMP_BODY=1 时把最终发往 Kiro 的 body 写入独立文件（定位后删除）
    if std::env::var_os("KIRO_DUMP_BODY").is_some_and(|v| v == "1") {
        dump_kiro_request_body(&request_body);
    }

    // 构造 fingerprint profile（在消耗 payload 前 clone system/messages）
    let fp_tracker = state.fingerprint_tracker.clone();
    let fp_profile = fp_tracker.as_ref().map(|_| {
        crate::cache::fingerprint::FingerprintTracker::build_profile_with_tools(
            payload.system.as_deref(),
            &payload.messages,
            payload.tools.as_deref(),
        )
    });

    // 估算"缓存前缀" token 数（system + tools + history 除最后一条 user 外的全部）
    // 必须在 count_all_tokens 消费 payload 之前先借用计算。
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
    // 每请求仅此一条 thinking 状态日志（流式 token 转发阶段不再输出）。
    // 双维度区分"客户端请求"与"实际生效"：
    // - 请求：客户端是否携带 thinking（enabled/adaptive），未携带 = 关闭
    // - 生效：请求开启且模型实际支持注入。判定顺序与真实生效链路一致：
    //   luna 强制关闭（resolve_thinking_enabled）→ GPT 系走 reasoning.effort →
    //   4.5 代际 / 第三方模型整体跳过字段（additional_fields_skipped）。
    //   注意必须用 map_model 归一化后的规范名判定——payload.model 是客户端
    //   原始名（如别名 "glm-4.6"），直接传谓词会判不准。
    let requested = payload
        .thinking
        .as_ref()
        .map(|t| t.is_enabled())
        .unwrap_or(false);
    let effective = if !requested {
        "关闭"
    } else if is_luna_model(&payload.model) {
        "不生效(luna不支持)"
    } else if is_gpt_model(&payload.model) {
        "不生效(GPT系走reasoning.effort)"
    } else {
        let mapped = crate::anthropic::converter::map_model(&payload.model)
            .unwrap_or_else(|| payload.model.clone());
        if additional_fields_skipped(&mapped) {
            "不生效(模型不支持该字段)"
        } else {
            "开启"
        }
    };
    tracing::info!(
        model = %payload.model,
        request = if requested { "开启" } else { "关闭" },
        effective,
        "[THINKING] 深度思考状态"
    );
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

    // 计算 prompt cache 模拟 usage（message_start 早期值；终值会被降级链覆盖）
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

    // 思考文本化仅对 Claude Code 客户端生效（OpenAI 兼容端点等保持原生 thinking 语义）
    let thinking_as_text = crate::anthropic::stream::thinking_as_text_enabled()
        && crate::anthropic::stream::is_claude_code_client(&headers);

    if payload.stream {
        // 流式响应
        handle_stream_request(
            provider,
            &request_body,
            &payload.model,
            input_tokens,
            prefix_estimated_tokens,
            thinking_enabled,
            usage_tracker,
            api_key_id,
            prompt_cache_usage,
            bound_ids,
            client_ip,
            None, // /v1 无全局 deadline（保持现有行为）
            is_compact_request,
            thinking_adaptive_requested,
            bridge_ctx,
            effort,
            thinking_as_text,
        )
        .await
    } else {
        // 非流式响应
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
            thinking_enabled,
        )
        .await
    }
}

// 临时取证：把最终发往 Kiro 的完整请求体落盘，仅 KIRO_DUMP_BODY=1 时启用。
// 文件落在 app/config/kiro-body-dump/（gitignore 目录），权限 0600，定位后删除。
fn dump_kiro_request_body(request_body: &str) {
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    let dir = std::path::Path::new("app/config/kiro-body-dump");
    if std::fs::create_dir_all(dir).is_err() {
        tracing::warn!("[BODY-DUMP] 创建 dump 目录失败");
        return;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let path = dir.join(format!("body-{ts}.json"));
    // 0600 权限仅 Unix 有意义；Windows 依赖目录 ACL（用户目录默认私有）
    #[cfg(unix)]
    let open = {
        use std::os::unix::fs::OpenOptionsExt;
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create_new(true).mode(0o600);
        o
    };
    #[cfg(not(unix))]
    let open = {
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create_new(true);
        o
    };
    match open
        .open(&path)
        .and_then(|mut f| f.write_all(request_body.as_bytes()))
    {
        Ok(()) => tracing::info!("[BODY-DUMP] 已写入 {}", path.display()),
        Err(e) => tracing::warn!("[BODY-DUMP] 写入失败: {}", e),
    }
}
