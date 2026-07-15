// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic API Handler 函数

use std::convert::Infallible;

use crate::kiro::model::events::Event;
use crate::kiro::model::requests::kiro::KiroRequest;
use crate::kiro::parser::decoder::EventStreamDecoder;
use crate::token;
use anyhow::Error;
use axum::{
    Extension, Json as JsonExtractor,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Json, Response},
};
use bytes::Bytes;
use futures::{Stream, StreamExt, stream};
use serde_json::json;
use std::time::Duration;
use tokio::time::{Instant, interval_at};
use uuid::Uuid;

use super::converter::{ConversionError, convert_request};
use super::middleware::{ApiKeyContext, AppState};
use super::stream::{BufferedStreamContext, SseEvent, StreamContext};
use super::types::{
    CountTokensRequest, CountTokensResponse, ErrorResponse, MessagesRequest, Model, ModelsResponse,
    OutputConfig, Thinking,
};
use super::websearch;

/// GET /v1/ping
///
/// 诊断端点（无需认证），返回请求的关键信息，用于排查客户端连接问题
pub async fn ping(request: axum::http::Request<Body>) -> impl IntoResponse {
    let method = request.method().to_string();
    let uri = request.uri().to_string();
    let headers: serde_json::Map<String, serde_json::Value> = request
        .headers()
        .iter()
        .filter(|(name, _)| {
            let n = name.as_str();
            // 只返回有用的 header，隐藏 API key
            n != "x-api-key" && n != "authorization"
        })
        .map(|(name, value)| {
            (
                name.to_string(),
                serde_json::Value::String(value.to_str().unwrap_or("<binary>").to_string()),
            )
        })
        .collect();

    Json(json!({
        "status": "ok",
        "method": method,
        "uri": uri,
        "headers": headers,
        "models_count": build_model_list().len(),
        "hint": "If you see this, the proxy is reachable. Try GET /v1/models with your API key to verify auth."
    }))
}

fn map_provider_error_with_context(
    err: Error,
    model: &str,
    estimated_input_tokens: i32,
) -> Response {
    let err_str = err.to_string();

    // 上下文窗口满了（对话历史累积超出模型上下文窗口限制）
    if err_str.contains("CONTENT_LENGTH_EXCEEDS_THRESHOLD") {
        tracing::warn!(
            error = %err,
            model = %model,
            estimated_input_tokens = estimated_input_tokens,
            "上游拒绝请求：上下文窗口已满（不应重试）— 请检查是否真正达到 1M 上下文限制"
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "invalid_request_error",
                "Context window is full. Reduce conversation history, system prompt, or tools.",
            )),
        )
            .into_response();
    }

    // 单次输入太长（请求体本身超出上游限制）
    if err_str.contains("Input is too long") {
        tracing::warn!(error = %err, "上游拒绝请求：输入过长（不应重试）");
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "invalid_request_error",
                "Input is too long. Reduce the size of your messages.",
            )),
        )
            .into_response();
    }
    // 上游限流（429 Too Many Requests）：所有账号重试后仍被限流。
    // 必须把 429 透传给客户端（而非转成 502），让 Claude Code 等客户端的
    // 内置指数退避重试接管 —— 502 会被客户端判定为硬失败，导致"请求那一轮直接废掉"
    // （表现为工具调用不执行 / 卡住），而 429 会触发客户端自动等待重试。
    if err_str.contains("429") || err_str.contains("Too Many Requests") {
        tracing::warn!(error = %err, "上游限流（所有账号 429 耗尽）：透传 429 给客户端以触发其退避重试");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "5")],
            Json(ErrorResponse::new(
                "rate_limit_error",
                "Upstream rate limit reached on all accounts. Please retry shortly.",
            )),
        )
            .into_response();
    }

    tracing::error!("Kiro API 调用失败: {}", err);
    (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse::new(
            "api_error",
            format!("上游 API 调用失败: {}", err),
        )),
    )
        .into_response()
}

/// 从原始请求体反序列化 MessagesRequest，失败时记录详细的 serde 错误用于诊断。
///
/// 替代 axum 的 `Json<MessagesRequest>` 提取器——后者反序列化失败时直接返回 400
/// 且不记录任何信息，导致无法定位是哪个字段/格式导致客户端请求被拒。
/// 此函数在失败时打印 serde 错误（行列+字段路径）、body 长度、出错位置附近的片段。
#[allow(clippy::result_large_err)]
fn parse_messages_request(body: &[u8]) -> Result<MessagesRequest, Response> {
    match serde_json::from_slice::<MessagesRequest>(body) {
        Ok(req) => Ok(req),
        Err(e) => {
            // serde_json 错误自带行列号；定位出错字节附近的片段辅助判断
            let line = e.line();
            let col = e.column();
            // 估算出错字节偏移附近的上下文（按行列粗略定位，取该行附近 200 字节）
            let body_str = String::from_utf8_lossy(body);
            let snippet: String = body_str
                .lines()
                .nth(line.saturating_sub(1))
                .map(|l| {
                    let start = col.saturating_sub(80);
                    l.chars().skip(start).take(200).collect()
                })
                .unwrap_or_default();
            tracing::error!(
                error = %e,
                serde_line = line,
                serde_col = col,
                body_len = body.len(),
                snippet = %snippet,
                "[REQ-DIAG] /v1/messages 请求体反序列化失败（导致 400，客户端那轮中断）"
            );
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "invalid_request_error",
                    format!("Request body could not be parsed: {}", e),
                )),
            )
                .into_response())
        }
    }
}

/// GET /v1/models
///
/// 返回可用的模型列表
pub async fn get_models() -> impl IntoResponse {
    tracing::info!("Received GET /v1/models request");

    Json(ModelsResponse {
        object: "list".to_string(),
        data: build_model_list(),
    })
}

/// 构建可用模型列表（供 get_models 和 get_model 共用）
fn build_model_list() -> Vec<Model> {
    vec![
        // === 旧版模型 ID（兼容旧版 Claude Code 客户端） ===
        // 这些旧 ID 在 map_model() 中会被正确映射到对应的 Kiro 模型
        Model {
            id: "claude-3-5-sonnet-20241022".to_string(),
            object: "model".to_string(),
            created: 1729555200,
            owned_by: "anthropic".to_string(),
            display_name: "Claude 3.5 Sonnet".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 8192,
        },
        Model {
            id: "claude-3-5-haiku-20241022".to_string(),
            object: "model".to_string(),
            created: 1729555200,
            owned_by: "anthropic".to_string(),
            display_name: "Claude 3.5 Haiku".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 8192,
        },
        Model {
            id: "claude-3-opus-20240229".to_string(),
            object: "model".to_string(),
            created: 1709164800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude 3 Opus".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 4096,
        },
        Model {
            id: "claude-3-haiku-20240307".to_string(),
            object: "model".to_string(),
            created: 1709769600,
            owned_by: "anthropic".to_string(),
            display_name: "Claude 3 Haiku".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 4096,
        },
        Model {
            id: "claude-3-sonnet-20240229".to_string(),
            object: "model".to_string(),
            created: 1709164800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude 3 Sonnet".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 4096,
        },
        // === Claude 4.x 过渡期模型 ID ===
        Model {
            id: "claude-sonnet-4-20250514".to_string(),
            object: "model".to_string(),
            created: 1747180800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-20250514".to_string(),
            object: "model".to_string(),
            created: 1747180800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        // === 当前主力模型 ===
        Model {
            id: "claude-sonnet-4-5-20250929".to_string(),
            object: "model".to_string(),
            created: 1727568000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-5-20250929-thinking".to_string(),
            object: "model".to_string(),
            created: 1727568000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-5-20251101".to_string(),
            object: "model".to_string(),
            created: 1730419200,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-5-20251101-thinking".to_string(),
            object: "model".to_string(),
            created: 1730419200,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-6".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.6".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-6-thinking".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.6 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-5".to_string(),
            object: "model".to_string(),
            created: 1775600000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-5-thinking".to_string(),
            object: "model".to_string(),
            created: 1775600000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-6".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.6".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-4-6-thinking".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.6 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-4-7".to_string(),
            object: "model".to_string(),
            created: 1773000000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.7".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-4-7-thinking".to_string(),
            object: "model".to_string(),
            created: 1773000000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.7 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-4-8".to_string(),
            object: "model".to_string(),
            created: 1775600000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.8".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-4-8-thinking".to_string(),
            object: "model".to_string(),
            created: 1775600000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.8 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-fable-5".to_string(),
            object: "model".to_string(),
            created: 1772582400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Fable 5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-fable-5-thinking".to_string(),
            object: "model".to_string(),
            created: 1772582400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Fable 5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-haiku-4-5-20251001".to_string(),
            object: "model".to_string(),
            created: 1727740800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Haiku 4.5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-haiku-4-5-20251001-thinking".to_string(),
            object: "model".to_string(),
            created: 1727740800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Haiku 4.5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        // === 非 Claude 模型 ===
        Model {
            id: "auto".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "kiro".to_string(),
            display_name: "Auto (智能路由)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "deepseek-3.2".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "deepseek".to_string(),
            display_name: "DeepSeek 3.2".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "glm-5".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "glm".to_string(),
            display_name: "GLM-5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "minimax-m2.5".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "minimax".to_string(),
            display_name: "MiniMax M2.5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "minimax-m2.1".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "minimax".to_string(),
            display_name: "MiniMax M2.1".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "qwen3-coder-next".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "qwen".to_string(),
            display_name: "Qwen3 Coder Next".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
    ]
}

/// GET /v1/models/:model_id
///
/// 返回指定模型的信息
pub async fn get_model(axum::extract::Path(model_id): axum::extract::Path<String>) -> Response {
    tracing::info!(model_id = %model_id, "Received GET /v1/models/:model_id request");

    // 复用 get_models 的模型列表，查找匹配的模型
    let models = build_model_list();
    if let Some(model) = models.into_iter().find(|m| m.id == model_id) {
        Json(model).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                "not_found_error",
                format!("Model '{}' not found", model_id),
            )),
        )
            .into_response()
    }
}

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
            };
            tracing::warn!("请求转换失败: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(error_type, message)),
            )
                .into_response();
        }
    };

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

    tracing::debug!("Kiro request body: {}", request_body);

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
        let prior: &[_] = if n > 0 { &payload.messages[..n - 1] } else { &[] };
        token::count_prefix_tokens(
            payload.system.as_deref(),
            prior,
            payload.tools.as_deref(),
        ) as i32
    };

    // 估算输入 tokens
    let input_tokens = token::count_all_tokens(
        payload.model.clone(),
        payload.system,
        payload.messages,
        payload.tools,
    ) as i32;

    // 检查是否启用了thinking
    let thinking_enabled = payload
        .thinking
        .as_ref()
        .map(|t| t.is_enabled())
        .unwrap_or(false);

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
        )
        .await
    }
}

/// 处理流式请求
#[allow(clippy::too_many_arguments)]
async fn handle_stream_request(
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
) -> Response {
    // 调用 Kiro API（支持多账号故障转移）
    let (response, credential_id) = match provider.call_api_stream(request_body, &bound_ids).await {
        Ok(resp) => resp,
        Err(e) => return map_provider_error_with_context(e, model, input_tokens),
    };

    // 创建流处理上下文
    let mut ctx = StreamContext::new_with_thinking(model, input_tokens, thinking_enabled)
        .with_usage_tracking(usage_tracker, api_key_id, Some(credential_id), client_ip)
        .with_prompt_cache_usage(prompt_cache_usage)
        .with_prefix_estimated_tokens(prefix_estimated_tokens);

    // 生成初始事件
    let initial_events = ctx.generate_initial_events();

    // 创建 SSE 流
    let stream = create_sse_stream(response, ctx, initial_events);

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

/// 创建 ping 事件的 SSE 字符串
fn create_ping_sse() -> Bytes {
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
fn stream_interrupted_error_event() -> SseEvent {
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

/// 创建 SSE 事件流
fn create_sse_stream(
    response: reqwest::Response,
    ctx: StreamContext,
    initial_events: Vec<SseEvent>,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    // 先发送初始事件
    let initial_stream = stream::iter(
        initial_events
            .into_iter()
            .map(|e| Ok(Bytes::from(e.to_sse_string()))),
    );

    // 然后处理 Kiro 响应流，同时每25秒发送 ping 保活
    let body_stream = response.bytes_stream();

    let processing_stream = stream::unfold(
        (body_stream, ctx, EventStreamDecoder::new(), false, interval_at(Instant::now() + Duration::from_secs(PING_INTERVAL_SECS), Duration::from_secs(PING_INTERVAL_SECS))),
        |(mut body_stream, mut ctx, mut decoder, finished, mut ping_interval)| async move {
            if finished {
                return None;
            }

            // 使用 select! 同时等待数据和 ping 定时器
            tokio::select! {
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
                                            let sse_events = ctx.process_kiro_event(&event);
                                            events.extend(sse_events);
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

                            Some((stream::iter(bytes), (body_stream, ctx, decoder, false, ping_interval)))
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
                            Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval)))
                        }
                        None => {
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
                            Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval)))
                        }
                    }
                }
                // 发送 ping 保活
                _ = ping_interval.tick() => {
                    tracing::trace!("发送 ping 保活事件");
                    let bytes: Vec<Result<Bytes, Infallible>> = vec![Ok(create_ping_sse())];
                    Some((stream::iter(bytes), (body_stream, ctx, decoder, false, ping_interval)))
                }
            }
        },
    )
    .flatten();

    initial_stream.chain(processing_stream)
}

/// 处理非流式请求
#[allow(clippy::too_many_arguments)]
async fn handle_non_stream_request(
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
) -> Response {
    // 调用 Kiro API（支持多账号故障转移）
    let (response, credential_id) = match provider.call_api(request_body, &bound_ids).await {
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

    // 解析事件流
    let mut decoder = EventStreamDecoder::new();
    if let Err(e) = decoder.feed(&body_bytes) {
        tracing::warn!("缓冲区溢出: {}", e);
    }

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

    for result in decoder.decode_iter() {
        match result {
            Ok(frame) => {
                if let Ok(event) = Event::from_frame(frame) {
                    match event {
                        Event::AssistantResponse(resp) => {
                            text_content.push_str(&resp.content);
                        }
                        Event::ToolUse(tool_use) => {
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
                            metering_cache_creation_tokens = metering.cache_creation_input_tokens;
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

    // 确定 stop_reason：tool_use 优先级最高，存在工具调用时无条件覆盖
    // max_tokens / model_context_window_exceeded（这些是下一轮才该报告的状态，
    // 不能盖掉本轮的 tool_use，否则客户端只渲染工具块而不执行）。
    if has_tool_use {
        // [TOOLUSE-DIAG] 非流式工具调用收尾诊断：记录覆盖前的原始 stop_reason，
        // 用于定位"客户端只显示 call 不执行"的根因。复现后离线分析。
        tracing::warn!(
            "[TOOLUSE-DIAG] non_stream has_tool_use=true raw_stop_reason={} \
             tool_use_count={} final_stop_reason=tool_use",
            stop_reason,
            tool_uses.len(),
        );
        stop_reason = "tool_use".to_string();
    }

    // JSON schema 结构化输出：去除模型可能添加的 Markdown 代码围栏
    if json_schema_requested && !text_content.is_empty() {
        text_content = strip_json_fences(text_content);
    }

    // 构建响应内容
    let mut content: Vec<serde_json::Value> = Vec::new();

    if !text_content.is_empty() {
        content.push(json!({
            "type": "text",
            "text": text_content
        }));
    }

    content.extend(tool_uses);

    // 估算输出 tokens
    let output_tokens = token::estimate_output_tokens(&content);

    // contextUsage 本地化后 input_tokens 来源优先级：metering 真值 → 本地 count_all_tokens 估算
    // `context_input_tokens` 已弃用（始终为 None），保留参数仅供 cap_input_tokens 签名兼容
    let _ = context_input_tokens; // 标记已读以避免 unused
    let raw_final_input_tokens = input_tokens;
    let final_input_tokens =
        super::stream::cap_input_tokens_pub(raw_final_input_tokens, input_tokens, model);

    // 本地估算 ≥ 1M 兜底触发 stop_reason
    if final_input_tokens >= 1_000_000 && stop_reason == "end_turn" {
        stop_reason = "model_context_window_exceeded".to_string();
    }

    tracing::info!(
        "[input_tokens] 本地化: estimated={} final={}",
        input_tokens,
        final_input_tokens
    );

    // 对外报告的 output_tokens 限制在安全范围
    let reported_output_tokens = output_tokens.min(380);

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
        // 客户端展示缩放（output_tokens 不缩放）；tracker 已写入真实值
        "usage": {
            "input_tokens": super::stream::scale_for_client(report_input, model),
            "output_tokens": reported_output_tokens,
            "cache_creation_input_tokens": super::stream::scale_for_client(report_cache_creation, model),
            "cache_read_input_tokens": super::stream::scale_for_client(report_cache_read, model),
            "cache_creation": {
                "ephemeral_5m_input_tokens": super::stream::scale_for_client(report_creation_5m, model),
                "ephemeral_1h_input_tokens": super::stream::scale_for_client(report_creation_1h, model)
            }
        }
    });

    (StatusCode::OK, Json(response_body)).into_response()
}

/// 去除 JSON 响应中模型可能添加的 Markdown 代码围栏
///
/// 当请求 JSON schema 结构化输出时，部分模型仍会将结果包裹在 ```json...``` 中。
/// 此函数识别并剥离这些围栏，返回纯 JSON 文本。
fn strip_json_fences(text: String) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return text;
    }
    let after_fence = if let Some(rest) = trimmed.strip_prefix("```json\n") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("```json\r\n") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("```\n") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("```\r\n") {
        rest
    } else {
        return text;
    };
    let result = after_fence
        .strip_suffix("\n```")
        .or_else(|| after_fence.strip_suffix("\r\n```"))
        .or_else(|| after_fence.strip_suffix("```"))
        .unwrap_or(after_fence);
    result.to_string()
}

/// 从请求头或连接信息提取客户端真实 IP
fn extract_client_ip(
    headers: &axum::http::HeaderMap,
    connect_info: Option<&std::net::SocketAddr>,
) -> Option<String> {
    if let Some(val) = headers.get("x-forwarded-for")
        && let Ok(s) = val.to_str()
    {
        let ip = s.split(',').next().unwrap_or("").trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }
    if let Some(val) = headers.get("x-real-ip")
        && let Ok(s) = val.to_str()
    {
        let ip = s.trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }
    connect_info.map(|addr| addr.ip().to_string())
}

/// 检测模型名是否包含 "thinking" 后缀，若包含则覆写 thinking 配置
///
/// - Opus 4.6：覆写为 adaptive 类型
/// - 其他模型：覆写为 enabled 类型
/// - budget_tokens 固定为 20000
fn override_thinking_from_model_name(payload: &mut MessagesRequest) {
    let model_lower = payload.model.to_lowercase();
    if !model_lower.contains("thinking") {
        return;
    }

    let is_opus_adaptive = model_lower.contains("opus")
        && (model_lower.contains("4-6")
            || model_lower.contains("4.6")
            || model_lower.contains("4-8")
            || model_lower.contains("4.8"));

    let thinking_type = if is_opus_adaptive {
        "adaptive"
    } else {
        "enabled"
    };

    tracing::info!(
        model = %payload.model,
        thinking_type = thinking_type,
        "模型名包含 thinking 后缀，覆写 thinking 配置"
    );

    payload.thinking = Some(Thinking {
        thinking_type: thinking_type.to_string(),
        budget_tokens: 20000,
    });

    if is_opus_adaptive {
        payload.output_config = Some(OutputConfig {
            effort: "high".to_string(),
            format: None,
        });
    }
}

/// POST /v1/messages/count_tokens
///
/// 计算消息的 token 数量
pub async fn count_tokens(
    JsonExtractor(payload): JsonExtractor<CountTokensRequest>,
) -> impl IntoResponse {
    tracing::info!(
        model = %payload.model,
        message_count = %payload.messages.len(),
        "Received POST /v1/messages/count_tokens request"
    );

    let total_tokens = token::count_all_tokens(
        payload.model,
        payload.system,
        payload.messages,
        payload.tools,
    ) as i32;

    Json(CountTokensResponse {
        input_tokens: total_tokens.max(1),
    })
}

/// POST /cc/v1/messages
///
/// Claude Code 兼容端点，与 /v1/messages 的区别在于：
/// - 流式响应会等待 kiro 端返回 contextUsageEvent 后再发送 message_start
/// - message_start 中的 input_tokens 是从 contextUsageEvent 计算的准确值
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

    tracing::debug!("Kiro request body: {}", request_body);

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
        let prior: &[_] = if n > 0 { &payload.messages[..n - 1] } else { &[] };
        token::count_prefix_tokens(
            payload.system.as_deref(),
            prior,
            payload.tools.as_deref(),
        ) as i32
    };

    // 估算输入 tokens
    let input_tokens = token::count_all_tokens(
        payload.model.clone(),
        payload.system,
        payload.messages,
        payload.tools,
    ) as i32;

    // 检查是否启用了thinking
    let thinking_enabled = payload
        .thinking
        .as_ref()
        .map(|t| t.is_enabled())
        .unwrap_or(false);

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

    if payload.stream {
        // 流式响应（缓冲模式）
        handle_stream_request_buffered(
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
        )
        .await
    }
}

/// 处理流式请求（缓冲版本）
///
/// 与 `handle_stream_request` 不同，此函数会缓冲所有事件直到流结束，
/// 然后用从 contextUsageEvent 计算的正确 input_tokens 生成 message_start 事件。
#[allow(clippy::too_many_arguments)]
async fn handle_stream_request_buffered(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    request_body: &str,
    model: &str,
    estimated_input_tokens: i32,
    prefix_estimated_tokens: i32,
    thinking_enabled: bool,
    usage_tracker: Option<std::sync::Arc<crate::model::usage::UsageTracker>>,
    api_key_id: Option<u32>,
    prompt_cache_usage: crate::cache::PromptCacheUsage,
    bound_ids: Vec<u64>,
    client_ip: Option<String>,
) -> Response {
    // 调用 Kiro API（支持多账号故障转移）
    let (response, credential_id) = match provider.call_api_stream(request_body, &bound_ids).await {
        Ok(resp) => resp,
        Err(e) => return map_provider_error_with_context(e, model, estimated_input_tokens),
    };

    // 创建缓冲流处理上下文
    let ctx = BufferedStreamContext::new(model, estimated_input_tokens, thinking_enabled)
        .with_usage_tracking(usage_tracker, api_key_id, Some(credential_id), client_ip)
        .with_prompt_cache_usage(prompt_cache_usage)
        .with_prefix_estimated_tokens(prefix_estimated_tokens);

    // 创建缓冲 SSE 流
    let stream = create_buffered_sse_stream(response, ctx);

    // 返回 SSE 响应
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// 创建缓冲 SSE 事件流
///
/// 工作流程：
/// 1. 等待上游流完成，期间只发送 ping 保活信号
/// 2. 使用 StreamContext 的事件处理逻辑处理所有 Kiro 事件，结果缓存
/// 3. 流结束后，用正确的 input_tokens 更正 message_start 事件
/// 4. 一次性发送所有事件
fn create_buffered_sse_stream(
    response: reqwest::Response,
    ctx: BufferedStreamContext,
) -> impl Stream<Item = Result<Bytes, Infallible>> {
    let body_stream = response.bytes_stream();
    let deadline = Instant::now() + Duration::from_secs(300);

    stream::unfold(
        (
            body_stream,
            ctx,
            EventStreamDecoder::new(),
            false,
            interval_at(Instant::now() + Duration::from_secs(PING_INTERVAL_SECS), Duration::from_secs(PING_INTERVAL_SECS)),
            deadline,
        ),
        |(mut body_stream, mut ctx, mut decoder, finished, mut ping_interval, deadline)| async move {
            if finished {
                return None;
            }

            loop {
                tokio::select! {
                    // 使用 biased 模式，优先检查 ping 定时器
                    // 避免在上游 chunk 密集时 ping 被"饿死"
                    biased;

                    // 全局 deadline：防止上游挂起导致请求永不结束
                    _ = tokio::time::sleep_until(deadline) => {
                        tracing::error!("缓冲模式全局超时（5分钟），强制终止");
                        let err_event = SseEvent::new("error", serde_json::json!({
                            "type": "error",
                            "error": {
                                "type": "overloaded_error",
                                "message": "Upstream response timed out (buffered mode, 5min deadline)"
                            }
                        }));
                        let bytes = vec![Ok(Bytes::from(err_event.to_sse_string()))];
                        return Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval, deadline)));
                    }

                    // 优先检查 ping 保活（等待期间唯一发送的数据）
                    _ = ping_interval.tick() => {
                        tracing::trace!("发送 ping 保活事件（缓冲模式）");
                        let bytes: Vec<Result<Bytes, Infallible>> = vec![Ok(create_ping_sse())];
                        return Some((stream::iter(bytes), (body_stream, ctx, decoder, false, ping_interval, deadline)));
                    }

                    // 然后处理数据流
                    chunk_result = body_stream.next() => {
                        match chunk_result {
                            Some(Ok(chunk)) => {
                                // 解码事件
                                if let Err(e) = decoder.feed(&chunk) {
                                    tracing::warn!("缓冲区溢出: {}", e);
                                }

                                for result in decoder.decode_iter() {
                                    match result {
                                        Ok(frame) => {
                                            if let Ok(event) = Event::from_frame(frame) {
                                                // 缓冲事件（复用 StreamContext 的处理逻辑）
                                                ctx.process_and_buffer(&event);
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!("解码事件失败: {}", e);
                                        }
                                    }
                                }
                                // 继续读取下一个 chunk，不发送任何数据
                            }
                            Some(Err(e)) => {
                                tracing::error!("读取响应流失败: {}", e);
                                let all_events = if ctx.is_empty_response() {
                                    let oversized = ctx.empty_response_is_oversized_context();
                                    tracing::warn!(
                                        oversized_context = oversized,
                                        "流解码错误且无内容（buffered 路径），补发 error 事件"
                                    );
                                    if oversized {
                                        ctx.finish_and_get_all_events()
                                    } else {
                                        vec![empty_response_error_event(false)]
                                    }
                                } else {
                                    // buffered 端点是 all-or-nothing 契约：此前缓冲的 message_start/
                                    // thinking/text/tool_use 从未发给客户端，中断时随本次 error 事件一并
                                    // 丢弃即可，无需（也无法）像 streaming 路径那样只替换收尾帧。
                                    tracing::warn!(
                                        est_input_tokens = ctx.input_tokens(),
                                        "流读取错误但已产生部分内容（buffered 路径），补发 error 事件防止伪装成正常完成"
                                    );
                                    vec![stream_interrupted_error_event()]
                                };
                                let bytes: Vec<Result<Bytes, Infallible>> = all_events
                                    .into_iter()
                                    .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                    .collect();
                                return Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval, deadline)));
                            }
                            None => {
                                if ctx.is_empty_response() {
                                    let oversized = ctx.empty_response_is_oversized_context();
                                    tracing::warn!(
                                        oversized_context = oversized,
                                        "上游返回空响应（buffered 路径，无任何内容事件），补发 error 事件"
                                    );
                                    if !oversized {
                                        let err_event = empty_response_error_event(false);
                                        let bytes = vec![Ok(Bytes::from(err_event.to_sse_string()))];
                                        return Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval, deadline)));
                                    }
                                }
                                // 流结束，完成处理并返回所有事件（已更正 input_tokens）
                                let all_events = ctx.finish_and_get_all_events();
                                let bytes: Vec<Result<Bytes, Infallible>> = all_events
                                    .into_iter()
                                    .map(|e| Ok(Bytes::from(e.to_sse_string())))
                                    .collect();
                                return Some((stream::iter(bytes), (body_stream, ctx, decoder, true, ping_interval, deadline)));
                            }
                        }
                    }
                }
            }
        },
    )
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_by_id(id: &str) -> Option<Model> {
        build_model_list().into_iter().find(|m| m.id == id)
    }

    #[test]
    fn test_opus_4_6_max_tokens_is_128k() {
        let m = find_by_id("claude-opus-4-6").expect("claude-opus-4-6 缺失");
        assert_eq!(m.max_tokens, 128000);
        let mt = find_by_id("claude-opus-4-6-thinking").expect("claude-opus-4-6-thinking 缺失");
        assert_eq!(mt.max_tokens, 128000);
    }

    #[test]
    fn test_fable_5_present() {
        let m = find_by_id("claude-fable-5").expect("claude-fable-5 应存在");
        assert_eq!(m.max_tokens, 128000);
        assert_eq!(m.owned_by, "anthropic");
        assert_eq!(m.object, "model");
        assert_eq!(m.model_type, "chat");
        assert_eq!(m.display_name, "Claude Fable 5");
    }

    #[test]
    fn test_fable_5_thinking_present() {
        let m = find_by_id("claude-fable-5-thinking").expect("claude-fable-5-thinking 应存在");
        assert_eq!(m.max_tokens, 128000);
        assert_eq!(m.display_name, "Claude Fable 5 (Thinking)");
    }

    #[test]
    fn test_haiku_4_5_max_tokens_unchanged() {
        // 回归：haiku-4-5 max_tokens 维持 64000
        let m = find_by_id("claude-haiku-4-5-20251001").expect("haiku 条目缺失");
        assert_eq!(m.max_tokens, 64000);
    }

    #[test]
    fn test_opus_4_7_4_8_max_tokens_unchanged() {
        // 回归
        assert_eq!(find_by_id("claude-opus-4-7").unwrap().max_tokens, 128000);
        assert_eq!(find_by_id("claude-opus-4-8").unwrap().max_tokens, 128000);
    }

    #[test]
    fn test_sonnet_4_6_max_tokens_unchanged() {
        // 回归
        assert_eq!(find_by_id("claude-sonnet-4-6").unwrap().max_tokens, 64000);
    }

    #[test]
    fn test_stream_interrupted_error_event_signals_failure_not_success() {
        // 流中断（已有部分内容）必须报错重试，不能是伪装成功的 message_delta/message_stop
        let event = stream_interrupted_error_event();
        assert_eq!(event.event, "error");
        assert_eq!(event.data["type"], "error");
        assert_eq!(event.data["error"]["type"], "overloaded_error");
        assert!(
            event.data["error"]["message"]
                .as_str()
                .unwrap()
                .contains("interrupted"),
            "错误信息应说明是连接中断导致，而非正常结束"
        );
    }
}
