// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic API Handler 函数

use std::convert::Infallible;

use crate::kiro::model::events::Event;
use crate::kiro::model::requests::conversation::ConversationState;
use crate::kiro::model::requests::kiro::KiroRequest;
use crate::kiro::model::requests::tool::ToolResult;
use crate::kiro::parser::decoder::EventStreamDecoder;
use crate::kiro::token_manager::QUOTA_EXHAUSTED_ALL_MARKER;
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
use std::collections::VecDeque;
use std::time::Duration;
use tokio::time::{Instant, interval_at};
use uuid::Uuid;

use super::converter::{ConversionError, convert_request, is_luna_model};
use super::middleware::{ApiKeyContext, AppState};
use super::stream::{CLIENT_ASSUMED_CONTEXT_WINDOW, SseEvent, StreamContext, scale_for_client};
use super::types::{
    CountTokensRequest, CountTokensResponse, ErrorResponse, MessagesRequest, Model, ModelsResponse,
    OutputConfig, Thinking,
};
use super::websearch;

/// GET /v1/ping
///
/// 诊断端点（无需认证），返回请求的关键信息，用于排查客户端连接问题
pub async fn ping(
    State(state): State<AppState>,
    request: axum::http::Request<Body>,
) -> impl IntoResponse {
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

    // 诊断端点无需认证，不主动触发上游刷新：有动态缓存则报缓存数，否则报静态表数
    let models_count = state
        .model_cache
        .read()
        .as_ref()
        .map(|c| c.models.len())
        .unwrap_or_else(|| build_model_list().len());

    Json(json!({
        "status": "ok",
        "method": method,
        "uri": uri,
        "headers": headers,
        "models_count": models_count,
        "hint": "If you see this, the proxy is reachable. Try GET /v1/models with your API key to verify auth."
    }))
}

/// 超窗错误文案（对齐 Anthropic 官方 `prompt is too long: N tokens > M maximum`）。
///
/// N 取客户端展示口径（`scale_for_client`）、M 取 `CLIENT_ASSUMED_CONTEXT_WINDOW`，
/// 与同一会话中 usage 字段口径一致。N 兜底为 M+1：上游报超窗但本地估算异常偏小
/// （远程 count_tokens 返回 0 等）时，照实填会产出 `0 tokens > 200000 maximum`
/// 这种 N ≤ M 的自相矛盾文案 —— 正是本函数要消除的形态。
fn format_prompt_too_long(estimated_input_tokens: i32, model: &str) -> String {
    let n = scale_for_client(estimated_input_tokens, model).max(CLIENT_ASSUMED_CONTEXT_WINDOW + 1);
    format!(
        "prompt is too long: {} tokens > {} maximum",
        n, CLIENT_ASSUMED_CONTEXT_WINDOW
    )
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
        // 文案对齐 Anthropic 官方超窗格式 `prompt is too long: N tokens > M maximum`。
        // 原自造文案不匹配任何客户端识别模式，Claude Code 收到后只会硬报错中断（#25）；
        // 官方格式才有机会被识别为「压缩后重试」。两个数字统一用客户端展示口径
        // （N 经 scale_for_client 缩放、M 取客户端假设的 200K 窗口），与同一会话中
        // usage 字段的口径一致，客户端自算 Ctx% 不会与这段文案矛盾。
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "invalid_request_error",
                format_prompt_too_long(estimated_input_tokens, model),
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
    // 额度耗尽（402）：range 内所有（模型过滤后）账号本月额度均已用尽。
    // 必须优先于 429 判断，且只信任 describe_unavailable 产出的机器可识别标记——
    // 该标记只在"scope 内 100% 账号确认为 QuotaExceeded"时才会写入，裸匹配
    // "MONTHLY_REQUEST_COUNT" 会在"单账号耗尽、其余账号仍可用"时被误判为全部耗尽。
    // 额度当月不会恢复，必须返回 402 而非 5xx/429 —— 否则 Claude Code 等客户端会
    // 判定为 temporary 故障并反复重试，定时任务会整轮空转。
    if err_str.contains(QUOTA_EXHAUSTED_ALL_MARKER) {
        tracing::error!(error = %err, "上游额度耗尽：返回 402 告知客户端不可重试");
        return (
            StatusCode::PAYMENT_REQUIRED,
            Json(ErrorResponse::new(
                "quota_exceeded_error",
                "All bound Kiro accounts have exhausted their monthly request quota. \
                 Quota resets at the start of next month, or add/enable another account \
                 in the admin panel.",
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

    // 兜底：完整错误详情只进日志，不回显给客户端——describe_unavailable 等诊断文案
    // 含账号数量/禁用原因拆解，属内部状态，不应通过客户端可见的响应体外泄。
    tracing::error!(error = %err, "Kiro API 调用失败");
    (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse::new(
            "api_error",
            "Upstream API call failed. Please retry shortly.",
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
/// 返回可用的模型列表（动态来源，带 TTL 缓存 + 静态表回退）
pub async fn get_models(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("Received GET /v1/models request");

    Json(ModelsResponse {
        object: "list".to_string(),
        data: fetch_models_dynamic(&state).await,
    })
}

/// 模型缓存别名，简化签名
type ModelCache = std::sync::Arc<parking_lot::RwLock<Option<super::middleware::CachedModels>>>;

/// 动态获取模型列表：优先上游实时响应（带 TTL 缓存），失败按顺序回退。
///
/// 分支：
/// 1. 缓存存在且未过期 → 直接返回缓存，不打上游
/// 2. 缓存过期/缺失且上游成功返回非空模型集 → 映射、写缓存、返回
/// 3. 上游失败但存在旧缓存 → 续用旧缓存（warn）
/// 4. 上游失败且无缓存 → 回退静态表 `build_model_list()`（warn）
/// 5. 未配置 `kiro_provider` → 直接回退静态表（无上游可查）
pub(crate) async fn fetch_models_dynamic(state: &AppState) -> Vec<Model> {
    // 分支 5：无上游 provider，直接静态表
    let Some(provider) = state.kiro_provider.as_ref() else {
        return build_model_list();
    };

    let ttl = Duration::from_secs(provider.token_manager().config().model_cache_ttl_secs);

    // 分支 1：缓存命中且未过期
    if let Some(hit) = cached_if_fresh(&state.model_cache, ttl) {
        return hit;
    }

    // 缓存缺失/过期，尝试刷新上游（仅此处涉及网络；结果归一化为 Option<Vec<Model>>）
    let refreshed: Option<Vec<Model>> = match provider.token_manager().list_available_models().await
    {
        Ok(resp) if !resp.models.is_empty() => {
            Some(resp.models.iter().map(available_model_to_model).collect())
        }
        Ok(_) => {
            tracing::warn!("上游模型列表为空，回退缓存/静态表");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "刷新上游模型列表失败，回退缓存/静态表");
            None
        }
    };

    resolve_after_refresh(&state.model_cache, refreshed)
}

/// 缓存命中判定（纯逻辑，无网络）：存在且未超过 TTL 时返回克隆的模型列表。
fn cached_if_fresh(cache: &ModelCache, ttl: Duration) -> Option<Vec<Model>> {
    let guard = cache.read();
    guard
        .as_ref()
        .filter(|cached| cached.fetched_at.elapsed() < ttl)
        .map(|cached| cached.models.clone())
}

/// 刷新结果落地（纯逻辑，无网络）：
/// - `Some(非空)` → 写缓存并返回（分支 2）
/// - `None` 且有旧缓存 → 续用旧缓存（分支 3）
/// - `None` 且无缓存 → 静态表（分支 4）
fn resolve_after_refresh(cache: &ModelCache, refreshed: Option<Vec<Model>>) -> Vec<Model> {
    if let Some(models) = refreshed {
        *cache.write() = Some(super::middleware::CachedModels {
            models: models.clone(),
            fetched_at: std::time::Instant::now(),
        });
        return models;
    }
    if let Some(cached) = cache.read().as_ref() {
        return cached.models.clone();
    }
    build_model_list()
}

/// 构建可用模型列表（供 get_models 和 get_model 共用）
pub(crate) fn build_model_list() -> Vec<Model> {
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
            id: "claude-sonnet-4".to_string(),
            object: "model".to_string(),
            created: 1747180800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
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
            id: "claude-opus-5".to_string(),
            object: "model".to_string(),
            created: 1777500000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-5-thinking".to_string(),
            object: "model".to_string(),
            created: 1777500000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 5 (Thinking)".to_string(),
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
        Model {
            id: "gpt-5.6-sol".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "openai".to_string(),
            display_name: "GPT-5.6 Sol".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "gpt-5.6-terra".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "openai".to_string(),
            display_name: "GPT-5.6 Terra".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "gpt-5.6-luna".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "openai".to_string(),
            display_name: "GPT-5.6 Luna".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
    ]
}

/// 根据模型 ID 前缀推断提供方（ListAvailableModels 响应不含厂商归属字段）
///
/// 命名规则与 `build_model_list()` 中手工维护的 `owned_by` 保持一致；未知前缀返回 `"unknown"`。
/// 供 `/v1/models` 动态映射与 Admin 端共享，避免两份实现漂移。
pub(crate) fn guess_owned_by(model_id: &str) -> &'static str {
    let id = model_id.to_lowercase();
    if id.contains("claude") {
        "anthropic"
    } else if id.contains("gpt") {
        "openai"
    } else if id == "auto" {
        "kiro"
    } else if id.contains("deepseek") {
        "deepseek"
    } else if id.contains("minimax") {
        "minimax"
    } else if id.contains("glm") {
        "glm"
    } else if id.contains("qwen") {
        "qwen"
    } else {
        "unknown"
    }
}

/// 将上游 `ListAvailableModels` 返回的单条模型映射为 Anthropic `Model`
///
/// 纯函数，不涉及网络调用，可直接用 fake `AvailableModelInfo` 单测。
pub(crate) fn available_model_to_model(
    info: &crate::kiro::model::available_models::AvailableModelInfo,
) -> Model {
    Model {
        id: info.model_id.clone(),
        object: "model".to_string(),
        created: 0,
        owned_by: guess_owned_by(&info.model_id).to_string(),
        display_name: info.model_name.clone(),
        model_type: "chat".to_string(),
        max_tokens: info.token_limits.max_output_tokens as i32,
    }
}

/// GET /v1/models/:model_id
///
/// 返回指定模型的信息
pub async fn get_model(
    State(state): State<AppState>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> Response {
    tracing::info!(model_id = %model_id, "Received GET /v1/models/:model_id request");

    // 与 /v1/models 相同的动态来源，查找匹配的模型
    let models = fetch_models_dynamic(&state).await;
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

    // 是否为 Claude Code /compact 压缩请求（决定上游超时：普通 180s / 压缩 1000s）
    let is_compact_request = conversion_result.is_compact_request;

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
            None, // /v1 无全局 deadline（保持现有行为）
            is_compact_request,
            bridge_ctx,
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
            bridge_ctx,
        )
        .await
    }
}

/// web_search server tool 桥接上下文（D5）
///
/// 请求命中 web_search server tool（`split_web_search_tool` 返回 Some）时，
/// `post_messages` / `post_messages_cc` 在构建 KiroRequest 前（conversation_state
/// 被 move 前）构造本结构，随调用链传入流式/非流式处理函数，供桥接层：
/// - 基于首次转换的 `conversation_state` clone 演进续请求（D3，conversationId/
///   agentContinuationId/history 逐字节不变，保 prompt cache）
/// - 以同参序列化续请求（profile_arn / additional_model_request_fields）
/// - 计算多轮搜索上限 `min(max_uses, 5)` 并获取 call_api_stream / call_mcp_api
///   所需的 bound_ids 与上游超时分档（is_compact_request）
///
/// `None` 表示请求未携带 web_search server tool，走现有路径，零行为变化。
#[derive(Debug, Clone)]
pub(crate) struct BridgeContext {
    /// 首次转换的 ConversationState clone（多轮桥接的演进基底，D3）
    pub conversation_state: ConversationState,
    /// 续请求需要同参序列化
    pub profile_arn: Option<String>,
    /// 模型专属请求参数（thinking、output_config、max_tokens）
    pub additional_model_request_fields: Option<serde_json::Value>,
    /// server tool 声明的次数上限（未声明时为 None，桥接层兜底 5）
    pub max_uses: Option<i32>,
    /// call_api_stream / call_mcp_api 均需要；unfold 闭包作用域内不可得，必须随 BridgeContext 携带
    pub bound_ids: Vec<u64>,
    /// 决定续请求的上游超时分档（普通 180s / compact 1000s）
    pub is_compact_request: bool,
}

/// 构造桥接上下文（D5/D7）
///
/// `split_web_search_tool` 命中（请求携带 web_search server tool）时返回
/// `Some(BridgeContext)`，未命中返回 `None`。流式/非流式共用同一判定（D7）。
///
/// 必须在 KiroRequest 构建（conversation_state 被 move）之前调用。
fn build_bridge_context(
    conversion_result: &super::converter::ConversionResult,
    profile_arn: Option<String>,
    bound_ids: Vec<u64>,
) -> Option<BridgeContext> {
    // 外层 None = 请求未携带 web_search server tool → 不构造桥接上下文
    let max_uses = conversion_result.web_search_max_uses?;
    Some(BridgeContext {
        conversation_state: conversion_result.conversation_state.clone(),
        profile_arn,
        additional_model_request_fields: conversion_result.additional_model_request_fields.clone(),
        max_uses,
        bound_ids,
        is_compact_request: conversion_result.is_compact_request,
    })
}

/// web_search server tool 桥接状态机（D4 流式段，嵌入 create_sse_stream 的 unfold 状态）
///
/// `None`（bridge 整体不存在）= 非桥接请求，全部分支短路，零行为变化。
#[derive(Debug)]
struct BridgeState {
    /// 当前所处阶段
    phase: BridgePhase,
    /// 已完成的截获轮数
    rounds_used: usize,
    /// 多轮搜索硬上限 `min(max_uses, 5)`（D8）
    max_rounds: usize,
    /// 已截获完成、待在流结束后执行的搜索队列（D8：Kiro 流读到自然结束才发起 MCP；
    /// 队列化支持 Collecting 期间上游连发多次 web_search 的场景，按截获顺序执行）
    pending: VecDeque<PendingSearch>,
    /// 多轮桥接的演进基底（D3）：初始为 BridgeContext.conversation_state 的 clone；
    /// 每轮续请求基于上一轮续请求所用的状态演进，保证 conversationId/
    /// agentContinuationId/history 跨轮次逐字节不变
    evolution_base: Option<ConversationState>,
}

/// 一轮已截获完成、待执行的搜索
#[derive(Debug)]
struct PendingSearch {
    /// Kiro 流中截获的 toolUse id（续请求回填 ToolResult 必须用它，
    /// 而非 create_mcp_request 返回的 srvtoolu_ id）
    tool_use_id: String,
    /// 聚合出的搜索词
    query: String,
}

/// 桥接阶段
#[derive(Debug)]
enum BridgePhase {
    /// 正常透传：非 web_search 事件全部走现有 process_kiro_event 路径
    PassThrough,
    /// 聚合中：input 分片累积到 `input_buffer`，`stop == true` 时截获完成；
    /// 不透传为普通 tool_use SSE（客户端看不到裸 tool_use 块）
    Collecting {
        tool_use_id: String,
        input_buffer: String,
    },
}

impl BridgeState {
    /// 创建桥接状态（初始为 PassThrough）
    ///
    /// `max_uses` 为 server tool 声明的次数上限（None 时兜底 5），
    /// 实际上限取 `min(max_uses, 5)`（D8：多轮硬上限 5）。
    fn new(max_uses: Option<i32>) -> Self {
        Self {
            phase: BridgePhase::PassThrough,
            rounds_used: 0,
            max_rounds: max_uses.unwrap_or(5).clamp(0, 5) as usize,
            pending: VecDeque::new(),
            evolution_base: None,
        }
    }

    /// 是否还有剩余截获轮次
    fn has_remaining_rounds(&self) -> bool {
        self.rounds_used < self.max_rounds
    }
}

/// 解析截获聚合的 input JSON 中的 `query` 字段（解析失败返回空串，由 MCP 侧报错）
fn parse_bridge_query(input_json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(input_json)
        .ok()
        .and_then(|v| {
            v.get("query")
                .and_then(|q| q.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

/// 桥接状态机对单个 Kiro 事件的处理（D4/D8）
///
/// 返回 `(consumed, events)`：
/// - `consumed == true`：事件被桥接截获，**不**透传给 `process_kiro_event`
///   （即不产生普通 tool_use SSE）；`events` 为需发给客户端的可见性块
/// - `consumed == false`：事件按现有路径透传（含 Collecting 期间的
///   AssistantResponse 说明文字与非目标工具调用）
///
/// 计费口径（对齐 `process_tool_use`）：截获的 input 分片同样无条件计入
/// `output_chars_other`——上游已生成这段内容即已计费；可见性块把 query 回传给了
/// 客户端，故 `visible_chars_other` 同步累加。
fn bridge_handle_event(
    ctx: &mut StreamContext,
    bridge: &mut Option<BridgeState>,
    event: &Event,
) -> (bool, Vec<SseEvent>) {
    let Some(state) = bridge else {
        return (false, Vec::new());
    };

    match &mut state.phase {
        BridgePhase::Collecting {
            tool_use_id,
            input_buffer,
        } => {
            if let Event::ToolUse(tu) = event
                && tu.tool_use_id == *tool_use_id
            {
                input_buffer.push_str(&tu.input);
                if tu.stop {
                    // 截获完成：解析 query → 发可见性块 → 记录待执行搜索 → 回 PassThrough
                    let chars = input_buffer.len() as i64;
                    ctx.output_chars_other += chars;
                    ctx.visible_chars_other += chars;

                    let query = parse_bridge_query(input_buffer);
                    let events = build_web_search_visibility_events(ctx, tool_use_id, &query);
                    state.pending.push_back(PendingSearch {
                        tool_use_id: tool_use_id.clone(),
                        query,
                    });

                    state.phase = BridgePhase::PassThrough;
                    state.rounds_used += 1;
                    return (true, events);
                }
                return (true, Vec::new());
            }
            // Collecting 期间的其他事件（AssistantResponse 说明文字、非目标
            // ToolUse）正常透传（D8）
            (false, Vec::new())
        }
        BridgePhase::PassThrough => {
            if let Event::ToolUse(tu) = event
                && tu.name == "web_search"
                && state.has_remaining_rounds()
            {
                // 轮次未达上限 → 截获，转 Collecting 聚合
                let id = tu.tool_use_id.clone();
                state.phase = BridgePhase::Collecting {
                    tool_use_id: id.clone(),
                    input_buffer: tu.input.clone(),
                };
                // 首个分片即带 stop=true（单事件完整调用）→ 立即完成截获
                if tu.stop {
                    let chars = tu.input.len() as i64;
                    ctx.output_chars_other += chars;
                    ctx.visible_chars_other += chars;

                    let query = parse_bridge_query(&tu.input);
                    let events = build_web_search_visibility_events(ctx, &id, &query);
                    state.pending.push_back(PendingSearch {
                        tool_use_id: id.clone(),
                        query,
                    });
                    state.phase = BridgePhase::PassThrough;
                    state.rounds_used += 1;
                    return (true, events);
                }
                return (true, Vec::new());
            }
            // 非 web_search，或轮次已耗尽 → 按现有普通 tool_use 逻辑透传（D8）
            (false, Vec::new())
        }
    }
}

/// 构造截获可见性块：`server_tool_use`（D4 客户端可见性，截获完成时立即发送）
///
/// 复用 websearch.rs 拦截式 `generate_websearch_events` 的块格式（②-④ 步），差异：
/// - 不重发 message_start（主响应的 message_start 已在 initial_events 发出）
/// - 不发 text 摘要块与 message_delta/message_stop——模型解读与流收尾由
///   续流轮次与 `generate_final_events` 统一处理
/// - 块索引经 `state_manager.next_block_index()` 分配（单调延续，不与已有块冲突）
///
/// `web_search_tool_result` 块由 `build_web_search_result_events` 在 MCP 调用
/// 完成后（流自然结束、续请求发起前）携带真实结果发出。
fn build_web_search_visibility_events(
    ctx: &mut StreamContext,
    tool_use_id: &str,
    query: &str,
) -> Vec<SseEvent> {
    let mut events = Vec::new();

    // server_tool_use 块：start + input_json_delta + stop
    let idx = ctx.state_manager.next_block_index();
    events.extend(ctx.state_manager.handle_content_block_start(
        idx,
        "server_tool_use",
        json!({
            "type": "content_block_start",
            "index": idx,
            "content_block": {
                "id": tool_use_id,
                "type": "server_tool_use",
                "name": "web_search",
                "input": {}
            }
        }),
    ));
    let input_json = json!({ "query": query });
    if let Some(delta) = ctx.state_manager.handle_content_block_delta(
        idx,
        json!({
            "type": "content_block_delta",
            "index": idx,
            "delta": {
                "type": "input_json_delta",
                "partial_json": serde_json::to_string(&input_json).unwrap_or_default()
            }
        }),
    ) {
        events.push(delta);
    }
    if let Some(stop) = ctx.state_manager.handle_content_block_stop(idx) {
        events.push(stop);
    }

    events
}

/// 构造 `web_search_tool_result` 可见性块（D4，携带真实 MCP 搜索结果）
///
/// 复用 websearch.rs 拦截式 `generate_websearch_events` 的条目格式（⑤-⑥ 步）：
/// 每条结果为 `{type, title, url, encrypted_content(snippet), page_age}`。
/// `search_results` 为 None（MCP 失败/解析失败）时 content 为空数组。
fn build_web_search_result_events(
    ctx: &mut StreamContext,
    tool_use_id: &str,
    search_results: &Option<websearch::WebSearchResults>,
) -> Vec<SseEvent> {
    let mut events = Vec::new();

    let search_content = if let Some(results) = search_results {
        results
            .results
            .iter()
            .map(|r| {
                json!({
                    "type": "web_search_result",
                    "title": r.title,
                    "url": r.url,
                    "encrypted_content": r.snippet.clone().unwrap_or_default(),
                    "page_age": null
                })
            })
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    let result_idx = ctx.state_manager.next_block_index();
    events.extend(ctx.state_manager.handle_content_block_start(
        result_idx,
        "web_search_tool_result",
        json!({
            "type": "content_block_start",
            "index": result_idx,
            "content_block": {
                "type": "web_search_tool_result",
                "tool_use_id": tool_use_id,
                "content": search_content
            }
        }),
    ));
    if let Some(stop) = ctx.state_manager.handle_content_block_stop(result_idx) {
        events.push(stop);
    }

    events
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
    // 上游流的全局超时；None 表示不限时（/v1 的现有行为）
    stream_deadline: Option<Duration>,
    // 是否为 Claude Code /compact 压缩请求（决定上游超时：普通 180s / 压缩 1000s）
    is_compact_request: bool,
    // web_search server tool 桥接上下文（None = 非桥接请求，零行为变化）
    bridge_ctx: Option<BridgeContext>,
) -> Response {
    // 调用 Kiro API（支持多账号故障转移）
    let (response, credential_id) = match provider
        .call_api_stream(request_body, is_compact_request, &bound_ids)
        .await
    {
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
async fn wait_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(d) => tokio::time::sleep_until(d).await,
        None => std::future::pending::<()>().await,
    }
}

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

/// /cc 全局 deadline 触发时返回给客户端的 error 事件（两处 deadline 收尾路径共用）
fn deadline_error_event() -> SseEvent {
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

/// 构建续请求体（D3：手工构建，绕过 validate_tool_pairing）
///
/// 基于演进基底 clone（多轮时为上一轮续请求所用状态，首轮为
/// `BridgeContext.conversation_state`），仅替换 `current_message` 的
/// `tool_results` 为本轮结果；`conversation_id` / `agent_continuation_id` /
/// `history` / `agent_task_type` / `chat_trigger_type` 逐字节不变。
///
/// MCP 失败（`search_results == None`）→ `ToolResult::error` 降级，
/// 仍发续请求让模型自行告知用户搜索失败，流不中断。
///
/// 每轮续请求回填恰好 1 条 PendingSearch 的结果；多条截获搜索按队列顺序
/// 在各自续流结束后逐轮 drain（每条一次 MCP 调用 + 一次续请求）。
fn build_continuation_request(
    bridge_ctx: &BridgeContext,
    evolution_base: Option<ConversationState>,
    tool_results: Vec<ToolResult>,
) -> KiroRequest {
    let mut conversation_state =
        evolution_base.unwrap_or_else(|| bridge_ctx.conversation_state.clone());
    conversation_state
        .current_message
        .user_input_message
        .user_input_message_context
        .tool_results = tool_results;

    KiroRequest {
        conversation_state,
        profile_arn: bridge_ctx.profile_arn.clone(),
        additional_model_request_fields: bridge_ctx.additional_model_request_fields.clone(),
    }
}

/// 按单条待执行搜索构建其 ToolResult（MCP 成功 → success 摘要；失败 → error 降级）
fn build_search_tool_result(
    tool_use_id: &str,
    query: &str,
    search_results: &Option<websearch::WebSearchResults>,
) -> ToolResult {
    match search_results {
        Some(_) => ToolResult::success(
            tool_use_id,
            websearch::generate_search_summary(query, search_results),
        ),
        None => ToolResult::error(
            tool_use_id,
            format!("Web search failed for query: {}", query),
        ),
    }
}

/// 一轮桥接的执行结果
///
/// `Continued`：MCP 搜索与续流建连均成功，携带新响应流与搜索结果；
/// `Failed(search_results)`：续请求发起/序列化失败，但 MCP 搜索结果可能
/// 已产出——调用方必须先把 `web_search_tool_result` 结果块发给客户端
/// （与已发出的 `server_tool_use` 块配对），再补发 error 事件收尾。
#[allow(clippy::large_enum_variant)] // 变体大小差异是桥接语义所需（Failed 不携带流）
enum BridgeRoundOutcome {
    Continued(
        reqwest::Response,
        EventStreamDecoder,
        Option<websearch::WebSearchResults>,
    ),
    Failed(Option<websearch::WebSearchResults>),
}

/// in-flight 桥接轮（修复③ v2：select! 条件分支保活）
///
/// 一轮桥接（MCP 搜索 + 续流建连）spawn 到后台任务执行，`JoinHandle` 连同
/// 本轮 tool_use_id 存入 unfold 状态元组第 10 元。收割通过 select! 的条件
/// 分支完成（`if round_in_flight.is_some()`）：轮次执行期间该分支挂起等待，
/// ping/deadline 分支照常就绪触发——桥接轮执行期间下游心跳不再中断，
/// 且耗尽的 body_stream 借条件前置不再被 poll（避免 flatten 重入误收尾）。
type InFlightRound = (
    tokio::task::JoinHandle<(BridgeState, BridgeRoundOutcome)>,
    // 本轮对应的 web_search tool_use_id（轮次完成后构建配对结果块用）
    String,
);

/// 执行一轮桥接：MCP 真实搜索 → 构建续请求 → 发起续流（D3/D4/D8）
///
/// 在 unfold 的上游 None 分支内调用（Kiro 流已自然结束）。成功时返回
/// `BridgeRoundOutcome::Continued((新响应流, 新解码器, 本轮搜索结果))`——
/// unfold 状态元组的 `body_stream`/`decoder` 被替换为续请求的响应，
/// `ctx`/`bridge` 原样携带，对 unfold 而言续流只是"换了一个上游 body 继续
/// unfold"；搜索结果供调用方构建 `web_search_tool_result` 可见性块；
/// `Failed` 表示续请求发起失败（调用方须先发结果块再发 error 事件收尾）。
///
/// 流程：
/// 1. `call_mcp_api` 真实搜索；失败 → `ToolResult::error` 降级，流不中断
/// 2. 基于演进基底 clone 构建 KiroRequest（仅替换 current_message 的
///    tool_results，conversationId/agentContinuationId/history 逐字节不变，
///    绕过 validate_tool_pairing）
/// 3. `call_api_stream` 续流；失败 → `Failed(search_results)`（调用方收尾）
#[allow(clippy::type_complexity)]
async fn bridge_execute_round(
    provider: &crate::kiro::provider::KiroProvider,
    bridge_ctx: &BridgeContext,
    bridge: &mut BridgeState,
    pending: PendingSearch,
) -> BridgeRoundOutcome {
    // 1. MCP 真实搜索（失败降级为 error ToolResult，仍发续请求让模型解读）
    let (_mcp_tool_use_id, mcp_request) = websearch::create_mcp_request(&pending.query);
    let search_results =
        match websearch::call_mcp_api(provider, &mcp_request, &bridge_ctx.bound_ids).await {
            Ok(response) => websearch::parse_search_results(&response),
            Err(e) => {
                tracing::warn!(
                    tool_use_id = %pending.tool_use_id,
                    "web_search MCP 调用失败，降级为 error ToolResult: {}",
                    e
                );
                None
            }
        };

    // 2. 基于演进基底构建续请求（D3 多轮语义：第 N+1 轮 clone 第 N 轮所用状态）
    let tool_result =
        build_search_tool_result(&pending.tool_use_id, &pending.query, &search_results);
    let kiro_request =
        build_continuation_request(bridge_ctx, bridge.evolution_base.take(), vec![tool_result]);
    let request_body = match serde_json::to_string(&kiro_request) {
        Ok(body) => body,
        Err(e) => {
            tracing::error!("web_search 续请求序列化失败: {}", e);
            // 与 call_api_stream Err 分支对齐：写回取出的演进基底
            bridge.evolution_base = Some(kiro_request.conversation_state);
            return BridgeRoundOutcome::Failed(search_results);
        }
    };

    // 3. 续流：call_api_stream（换上游 body 继续 unfold，ctx/bridge 原样携带）
    match provider
        .call_api_stream(
            &request_body,
            bridge_ctx.is_compact_request,
            &bridge_ctx.bound_ids,
        )
        .await
    {
        Ok((response, _credential_id)) => {
            // 演进基底更新为本轮续请求所用的状态（下一轮 clone 它）
            bridge.evolution_base = Some(kiro_request.conversation_state);
            BridgeRoundOutcome::Continued(response, EventStreamDecoder::new(), search_results)
        }
        Err(e) => {
            tracing::error!("web_search 续请求发起失败: {}", e);
            // 演进基底保持取出的状态，避免下一轮基于未知状态演进
            bridge.evolution_base = Some(kiro_request.conversation_state);
            // 搜索结果必须带回：客户端已收到 server_tool_use 块，缺结果块会破坏配对
            BridgeRoundOutcome::Failed(search_results)
        }
    }
}

/// 执行一轮桥接的 owned 变体（修复③ v2：供 `tokio::spawn` 后台任务调用）
///
/// 与 `bridge_execute_round` 逻辑一致，差别仅在所有权形态：`BridgeState`
/// 按值进出（后台任务无法持有 unfold 状态元组的借用）。`InFlightRound`
/// 持有的 JoinHandle 完成时把更新后的 `BridgeState` 一并带回。
async fn bridge_execute_round_owned(
    provider: std::sync::Arc<crate::kiro::provider::KiroProvider>,
    bridge_ctx: BridgeContext,
    mut bridge: BridgeState,
    pending: PendingSearch,
) -> (BridgeState, BridgeRoundOutcome) {
    let outcome = bridge_execute_round(&provider, &bridge_ctx, &mut bridge, pending).await;
    (bridge, outcome)
}

/// 收割已完成的桥接轮（修复③）：按 outcome 补发配对结果块与收尾事件，
/// 返回本轮应下发的 SSE 字节流与 `body_stream`/`decoder` 换流结果及 finished
/// 标志（状态元组其余元素由调用方原样回填）。
///
/// `Continued` → 先补发 `web_search_tool_result` 结果块（与已下发的
/// `server_tool_use` 配对），再换入续流（finished = false，unfold 对续流的
/// 处理与普通上游流完全一致）；
/// `Failed` → 补发结果块（携带已产出的搜索结果，MCP 失败为空数组）后补发
/// error 事件收尾（finished = true）；后台任务 panic 兜底复用 `Failed(None)`。
struct BridgeRoundHarvest {
    events: Vec<SseEvent>,
    new_body_stream: reqwest::Response,
    new_decoder: EventStreamDecoder,
    finished: bool,
}

fn harvest_bridge_round(
    outcome: BridgeRoundOutcome,
    result_tool_use_id: &str,
    ctx: &mut StreamContext,
) -> BridgeRoundHarvest {
    match outcome {
        BridgeRoundOutcome::Continued(response, new_decoder, search_results) => {
            let events = build_web_search_result_events(ctx, result_tool_use_id, &search_results);
            // 续流的首个事件前先补发本轮配对结果块，保持块序：
            // server_tool_use → web_search_tool_result → 续流内容
            BridgeRoundHarvest {
                events,
                new_body_stream: response,
                new_decoder,
                finished: false,
            }
        }
        BridgeRoundOutcome::Failed(search_results) => {
            let mut events =
                build_web_search_result_events(ctx, result_tool_use_id, &search_results);
            events.push(stream_interrupted_error_event());
            // Failed 无续流，调用方以 finished = true 收尾，body_stream/decoder
            // 原值不再被消费（decoder 原样带回占位）
            BridgeRoundHarvest {
                events,
                new_body_stream: reqwest::Response::from(http::Response::new(body_dummy_bytes())),
                new_decoder: EventStreamDecoder::new(),
                finished: true,
            }
        }
    }
}

fn body_dummy_bytes() -> reqwest::Body {
    reqwest::Body::from(Bytes::new())
}

/// 非流式降级收尾前，为 pending_search 队列中尚未执行的搜索补发空结果块
///
/// 三个 `break 'rounds` 降级路径（序列化失败/响应读取失败/续请求发起失败）共
/// 用：队列中每条 pending 的 `server_tool_use` 块均已进入 visibility_blocks，
/// 缺对应结果块会破坏 server_tool_use / web_search_tool_result 成对不变量。
fn flush_unpaired_search_blocks(
    pending_search: &mut VecDeque<PendingSearch>,
    visibility_blocks: &mut Vec<serde_json::Value>,
) {
    for leftover in pending_search.drain(..) {
        visibility_blocks.push(build_web_search_result_block(&leftover.tool_use_id, &None));
    }
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

/// 组装非流式响应的 content 数组
///
/// 块顺序与流式路径一致：thinking → text → tool_use。
///
/// 返回 `(content, thinking_only)`。`thinking_only` 为 true 表示整段响应只有
/// thinking、既无可见文本也无工具调用 —— 此时补一个占位空格 text 块，避免客户端
/// 把 content 判定为空响应而卡住（流式路径在 `generate_final_events` 里有等价
/// 兜底）。调用方需据此把 `stop_reason` 调整为 `max_tokens`。
fn build_non_stream_content(
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
            "signature": super::stream::generate_fake_signature()
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
fn non_stream_bridge_step(
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

/// 构造非流式 `web_search_tool_result` 可见性块（D5 非流式段，直接组 JSON）
///
/// 条目格式与流式 `build_web_search_result_events` 一致：
/// `{type, title, url, encrypted_content(snippet), page_age}`。
/// `search_results` 为 None（MCP 失败/解析失败）时 content 为空数组。
fn build_web_search_result_block(
    tool_use_id: &str,
    search_results: &Option<websearch::WebSearchResults>,
) -> serde_json::Value {
    let content = if let Some(results) = search_results {
        results
            .results
            .iter()
            .map(|r| {
                json!({
                    "type": "web_search_result",
                    "title": r.title,
                    "url": r.url,
                    "encrypted_content": r.snippet.clone().unwrap_or_default(),
                    "page_age": null
                })
            })
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    json!({
        "type": "web_search_tool_result",
        "tool_use_id": tool_use_id,
        "content": content
    })
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
    // 是否为 Claude Code /compact 压缩请求（决定上游超时：普通 180s / 压缩 1000s）
    is_compact_request: bool,
    // web_search server tool 桥接上下文（None = 非桥接请求，零行为变化）
    bridge_ctx: Option<BridgeContext>,
) -> Response {
    // 调用 Kiro API（支持多账号故障转移）
    let (response, credential_id) = match provider
        .call_api(request_body, is_compact_request, &bound_ids)
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
            .call_api(&request_body, ctx.is_compact_request, &ctx.bound_ids)
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
    let (thinking_content, visible_text) = super::stream::split_thinking_and_visible(&text_content);
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
        // 客户端展示缩放（output_tokens 不缩放）；tracker 已写入真实值。
        // content 构建前已分离 thinking，且 estimate_output_tokens 只累加 text 与
        // tool_use.input（thinking 块的字段名是 "thinking"，不参与统计），
        // 因此 output_tokens 就是可见输出，直接上报真值，不再套 min(380) 上限。
        "usage": {
            "input_tokens": super::stream::scale_for_client(report_input, model),
            "output_tokens": output_tokens,
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
/// - Opus 4.6/4.7/4.8/5：覆写为 adaptive 类型
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
            || model_lower.contains("4-7")
            || model_lower.contains("4.7")
            || model_lower.contains("4-8")
            || model_lower.contains("4.8")
            || model_lower.contains("opus-5")
            || model_lower.contains("opus.5")
            || model_lower.contains("opus 5"));

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

/// 判断本次请求是否应启用流式 thinking 处理路径（`<thinking>` 标签的提取与转换）。
///
/// **范围仅限 `gpt-5.6-luna`**，不包括 terra/sol。原因：
/// - `converter::generate_thinking_prefix` / `build_additional_model_request_fields`
///   对全部 GPT 系模型跳过 thinking 注入（有 400 REQUEST_BODY_INVALID 实测依据，
///   范围覆盖整个 gpt-5.6 系列），这一点本函数无需重复处理。
/// - 但"上游是否会真的输出 `<thinking>...</thinking>` 标签、是否具备可用的推理
///   能力"是另一件事，代码库里只有 luna 的实测记录（"上游恒返回 `thinking=0`"，
///   见 README/`openai::model_map` 已知限制）。terra/sol 没有类似证据，不应假定
///   它们也不产出 thinking——若在此处也对它们强制关闭，会误伤其本该具备的、可能
///   仍受客户端 thinking 请求影响的推理能力。
///
/// 因此仅对 luna 强制关闭 `thinking_enabled`：若仍按客户端请求启用，流式状态机会
/// 持续寻找永不出现的 `<thinking>` 标签，而 luna 在缺乏协议约束时有概率自行选择用
/// `<analysis>`/`<summary>` 等自造伪标签组织输出，被当作普通可见文本原样转发给
/// 客户端（配合 `converter::gpt_anti_pseudo_tag_hint` 的提示词引导兜底）。
/// terra/sol 恢复原有行为：`thinking_enabled` 仅取决于客户端是否请求。
fn resolve_thinking_enabled(model: &str, thinking: &Option<Thinking>) -> bool {
    if is_luna_model(model) {
        return false;
    }
    thinking.as_ref().map(|t| t.is_enabled()).unwrap_or(false)
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
            bridge_ctx,
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
            bridge_ctx,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_by_id(id: &str) -> Option<Model> {
        build_model_list().into_iter().find(|m| m.id == id)
    }

    #[test]
    fn test_guess_owned_by_known_and_unknown_prefixes() {
        assert_eq!(guess_owned_by("claude-sonnet-4.6"), "anthropic");
        assert_eq!(guess_owned_by("gpt-5.6-sol"), "openai");
        assert_eq!(guess_owned_by("auto"), "kiro");
        assert_eq!(guess_owned_by("deepseek-3.2"), "deepseek");
        assert_eq!(guess_owned_by("minimax-m2.5"), "minimax");
        assert_eq!(guess_owned_by("glm-5"), "glm");
        assert_eq!(guess_owned_by("qwen3-coder-next"), "qwen");
        assert_eq!(guess_owned_by("foo-model"), "unknown");
    }

    fn fake_model(id: &str) -> Model {
        Model {
            id: id.to_string(),
            object: "model".to_string(),
            created: 0,
            owned_by: "test".to_string(),
            display_name: id.to_string(),
            model_type: "chat".to_string(),
            max_tokens: 8192,
        }
    }

    fn new_cache(entry: Option<super::super::middleware::CachedModels>) -> ModelCache {
        std::sync::Arc::new(parking_lot::RwLock::new(entry))
    }

    // 分支 1：缓存命中且未过期 → 返回缓存，不触发刷新
    #[test]
    fn test_cached_if_fresh_hit() {
        let cache = new_cache(Some(super::super::middleware::CachedModels {
            models: vec![fake_model("cached-a")],
            fetched_at: std::time::Instant::now(),
        }));
        let hit = cached_if_fresh(&cache, Duration::from_secs(3600));
        assert!(hit.is_some());
        assert_eq!(hit.unwrap()[0].id, "cached-a");
    }

    // 分支 1 反例：缓存过期 → 视为未命中
    #[test]
    fn test_cached_if_fresh_expired() {
        let cache = new_cache(Some(super::super::middleware::CachedModels {
            models: vec![fake_model("stale")],
            fetched_at: std::time::Instant::now() - Duration::from_secs(10),
        }));
        // TTL 5s，已过 10s
        assert!(cached_if_fresh(&cache, Duration::from_secs(5)).is_none());
        // 空缓存亦未命中
        assert!(cached_if_fresh(&new_cache(None), Duration::from_secs(3600)).is_none());
    }

    // 分支 2：刷新成功 → 写缓存并返回
    #[test]
    fn test_resolve_after_refresh_success_writes_cache() {
        let cache = new_cache(None);
        let out = resolve_after_refresh(&cache, Some(vec![fake_model("fresh")]));
        assert_eq!(out[0].id, "fresh");
        // 缓存已写入
        let guard = cache.read();
        assert_eq!(guard.as_ref().unwrap().models[0].id, "fresh");
    }

    // 分支 3：刷新失败但有旧缓存 → 续用旧缓存
    #[test]
    fn test_resolve_after_refresh_failure_uses_old_cache() {
        let cache = new_cache(Some(super::super::middleware::CachedModels {
            models: vec![fake_model("old")],
            fetched_at: std::time::Instant::now(),
        }));
        let out = resolve_after_refresh(&cache, None);
        assert_eq!(out[0].id, "old");
    }

    // 分支 4：刷新失败且无缓存 → 回退静态表
    #[test]
    fn test_resolve_after_refresh_failure_no_cache_falls_back_static() {
        let cache = new_cache(None);
        let out = resolve_after_refresh(&cache, None);
        assert_eq!(out.len(), build_model_list().len());
        assert!(out.iter().any(|m| m.id == "claude-3-5-sonnet-20241022"));
    }

    // 分支 5：无 provider → fetch_models_dynamic 直接回退静态表
    #[tokio::test]
    async fn test_fetch_models_dynamic_no_provider_static() {
        let state = AppState::new();
        assert!(state.kiro_provider.is_none());
        let out = fetch_models_dynamic(&state).await;
        assert_eq!(out.len(), build_model_list().len());
    }

    fn test_state() -> AppState {
        AppState::new()
    }

    // get_model 命中（无 provider → 静态表来源）
    #[tokio::test]
    async fn test_get_model_hit() {
        let resp = get_model(
            State(test_state()),
            axum::extract::Path("claude-3-5-sonnet-20241022".to_string()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // get_model 未命中 → 404
    #[tokio::test]
    async fn test_get_model_not_found() {
        let resp = get_model(
            State(test_state()),
            axum::extract::Path("no-such-model-xyz".to_string()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_available_model_to_model_maps_fields() {
        use crate::kiro::model::available_models::{AvailableModelInfo, TokenLimits};
        let info = AvailableModelInfo {
            model_id: "claude-sonnet-4.6".to_string(),
            model_name: "Claude Sonnet 4.6".to_string(),
            rate_multiplier: Some(1.3),
            token_limits: TokenLimits {
                max_input_tokens: 1_000_000,
                max_output_tokens: 64_000,
            },
            additional_model_request_fields_schema: None,
        };
        let m = available_model_to_model(&info);
        assert_eq!(m.id, "claude-sonnet-4.6");
        assert_eq!(m.display_name, "Claude Sonnet 4.6");
        assert_eq!(m.owned_by, "anthropic");
        assert_eq!(m.max_tokens, 64_000);
        assert_eq!(m.object, "model");
        assert_eq!(m.model_type, "chat");
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
    fn test_build_model_list_includes_opus_5() {
        let list = build_model_list();
        let ids: std::collections::HashSet<&str> = list.iter().map(|m| m.id.as_str()).collect();

        assert!(ids.contains("claude-opus-5"), "缺 claude-opus-5 静态表项");
        assert!(
            ids.contains("claude-opus-5-thinking"),
            "缺 claude-opus-5-thinking 静态表项"
        );

        let opus5 = list.iter().find(|m| m.id == "claude-opus-5").unwrap();
        assert_eq!(opus5.owned_by, "anthropic");
        assert_eq!(opus5.display_name, "Claude Opus 5");
        assert_eq!(opus5.max_tokens, 128000);

        let opus5t = list
            .iter()
            .find(|m| m.id == "claude-opus-5-thinking")
            .unwrap();
        assert_eq!(opus5t.owned_by, "anthropic");
        assert_eq!(opus5t.display_name, "Claude Opus 5 (Thinking)");
        assert_eq!(opus5t.max_tokens, 128000);

        // 回归：sonnet-5 仍在
        assert!(ids.contains("claude-sonnet-5"));
        assert!(ids.contains("claude-sonnet-5-thinking"));
        // 回归：opus-4.7/4.8 仍在
        assert!(ids.contains("claude-opus-4-7"));
        assert!(ids.contains("claude-opus-4-8"));
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

    async fn response_body_text(resp: Response) -> String {
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(body.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_map_provider_error_quota_marker_returns_402() {
        let err = anyhow::anyhow!("绑定的账号本月请求额度已用尽（共 1 个）[QUOTA_EXHAUSTED_ALL]");
        let resp = map_provider_error_with_context(err, "claude-sonnet-4-6", 100);
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn test_map_provider_error_mixed_marker_and_429_prefers_402() {
        // H2 回归：402 分支必须排在 429 分支之前，混合错误串不能被 429 抢先命中
        let err = anyhow::anyhow!("账号A: 429 Too Many Requests；账号B: [QUOTA_EXHAUSTED_ALL]");
        let resp = map_provider_error_with_context(err, "claude-sonnet-4-6", 100);
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn test_map_provider_error_bare_monthly_request_count_does_not_trigger_402() {
        // H1 回归：裸串 "MONTHLY_REQUEST_COUNT" 不再单独触发 402——必须要有
        // describe_unavailable 产出的、已确认"scope 内 100% 耗尽"的机器标记，
        // 否则"单账号耗尽、其余账号可用"的场景会被误判为不可重试
        let err = anyhow::anyhow!("账号 #1 Token 刷新失败，尝试下一个账号: MONTHLY_REQUEST_COUNT");
        let resp = map_provider_error_with_context(err, "claude-sonnet-4-6", 100);
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn test_map_provider_error_429_without_marker_returns_429() {
        let err = anyhow::anyhow!("上游限流：429 Too Many Requests");
        let resp = map_provider_error_with_context(err, "claude-sonnet-4-6", 100);
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn test_map_provider_error_default_branch_does_not_leak_internal_detail() {
        // M3 回归：502 兜底分支不应把账号池内部细节（数量/禁用原因拆解）
        // 透传给客户端，完整信息只应进 tracing 日志
        let err = anyhow::anyhow!(
            "绑定的账号均不可用（共 3 个：1 个额度用尽，1 个连续认证失败，1 个手动禁用）"
        );
        let resp = map_provider_error_with_context(err, "claude-sonnet-4-6", 100);
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        let text = response_body_text(resp).await;
        assert!(
            !text.contains("额度用尽") && !text.contains("连续认证失败"),
            "502 响应体不应回显内部账号池细节: {}",
            text
        );
    }

    #[tokio::test]
    async fn test_map_provider_error_context_length_uses_official_too_long_format() {
        // #25 回归：自造文案不被客户端识别为超窗，只会硬报错中断。必须对齐
        // Anthropic 官方 `prompt is too long: N tokens > M maximum`，且 N > M 才成立。
        let err = anyhow::anyhow!(
            r#"流式 API 请求失败: 400 Bad Request {{"message":"Input content length exceeds threshold.","reason":"CONTENT_LENGTH_EXCEEDS_THRESHOLD"}}"#
        );
        let resp = map_provider_error_with_context(err, "claude-opus-5", 754_234);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // 期望值动态取自缩放函数：系数是校准量，改它不应弄红这条错误映射测试
        let n = scale_for_client(754_234, "claude-opus-5");
        assert!(
            n > CLIENT_ASSUMED_CONTEXT_WINDOW,
            "N({}) 必须大于 M({}) 才构成超窗语义",
            n,
            CLIENT_ASSUMED_CONTEXT_WINDOW
        );
        let text = response_body_text(resp).await;
        assert!(
            text.contains(&format_prompt_too_long(754_234, "claude-opus-5")),
            "超窗文案未对齐官方格式: {}",
            text
        );
    }

    #[tokio::test]
    async fn test_map_provider_error_context_length_never_emits_n_le_m() {
        // 上游报超窗但本地估算异常偏小（远程 count_tokens 返回 0）时，照实填会产出
        // `0 tokens > 200000 maximum` —— N ≤ M 自相矛盾，正是本次修复要消除的形态。
        let err = anyhow::anyhow!("CONTENT_LENGTH_EXCEEDS_THRESHOLD");
        let resp = map_provider_error_with_context(err, "claude-opus-5", 0);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let text = response_body_text(resp).await;
        // 期望值不复用被测函数，避免同义反复
        assert!(
            text.contains(&format!(
                "prompt is too long: {} tokens > {} maximum",
                CLIENT_ASSUMED_CONTEXT_WINDOW + 1,
                CLIENT_ASSUMED_CONTEXT_WINDOW
            )),
            "N 未兜底到 M+1: {}",
            text
        );
    }

    // deadline 为 None 时 wait_deadline 必须永不就绪 —— 这是 /v1 行为零变化的前提：
    // create_sse_stream 的 select! 里该分支等价于不存在。
    #[tokio::test]
    async fn test_wait_deadline_none_never_resolves() {
        let r = tokio::time::timeout(Duration::from_millis(50), wait_deadline(None)).await;
        assert!(r.is_err(), "deadline 为 None 时 wait_deadline 不应就绪");
    }

    // deadline 已过期时立即就绪，保证撞线后 select! 当轮即可选中该分支。
    #[tokio::test]
    async fn test_wait_deadline_past_instant_resolves_immediately() {
        let past = Instant::now() - Duration::from_secs(1);
        let r = tokio::time::timeout(Duration::from_millis(50), wait_deadline(Some(past))).await;
        assert!(r.is_ok(), "deadline 已过期时 wait_deadline 应立即就绪");
    }

    /// 非流式 content 组装：块顺序 thinking → text → tool_use；
    /// 只有 thinking 的退化响应补占位空格并回报 thinking_only。
    #[test]
    fn test_build_non_stream_content() {
        // thinking + text：thinking 块在前，可见文本原样保留
        let (content, thinking_only) = build_non_stream_content("推理过程", "最终回答", Vec::new());
        assert!(!thinking_only);
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["thinking"], "推理过程");
        assert!(content[0]["signature"].as_str().unwrap().len() >= 100);
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], "最终回答");

        // 只有 thinking：补一个占位空格 text 块，避免客户端判定空响应
        let (content, thinking_only) = build_non_stream_content("只有推理", "", Vec::new());
        assert!(thinking_only);
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[1]["text"], " ");

        // 无 thinking：只有一个 text 块
        let (content, thinking_only) = build_non_stream_content("", "普通回答", Vec::new());
        assert!(!thinking_only);
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");

        // thinking + tool_use（无可见文本）：不是退化响应，不补占位空格
        let tool = json!({"type": "tool_use", "id": "tu_1", "name": "Read", "input": {}});
        let (content, thinking_only) = build_non_stream_content("推理", "", vec![tool]);
        assert!(!thinking_only);
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[1]["type"], "tool_use");
    }

    #[test]
    fn test_resolve_thinking_enabled_luna_forced_off_even_when_requested() {
        // 收窄后的核心断言：只有 gpt-5.6-luna 即使客户端显式请求了 thinking，也必须
        // 强制返回 false —— 该模型上游恒返回 thinking=0（已知限制），若仍按请求启用，
        // 流式状态机会持续寻找永不出现的 `<thinking>` 标签，导致模型自造的伪标签
        // （<analysis>/<summary>）原样泄漏。
        let thinking = Some(Thinking {
            thinking_type: "adaptive".to_string(),
            budget_tokens: 20000,
        });
        assert!(!resolve_thinking_enabled("gpt-5.6-luna", &thinking));

        let thinking = Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 20000,
        });
        assert!(!resolve_thinking_enabled("gpt-5.6-luna", &thinking));
    }

    #[test]
    fn test_resolve_thinking_enabled_terra_and_sol_respect_client_request() {
        // 范围收窄：terra/sol 没有 luna 那样的 thinking=0 实测依据，不应被一并强制
        // 关闭，恢复原有行为——thinking_enabled 仅取决于客户端是否请求。
        let thinking = Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 20000,
        });
        assert!(resolve_thinking_enabled("gpt-5.6-terra", &thinking));
        assert!(resolve_thinking_enabled("gpt-5.6-sol", &thinking));

        assert!(!resolve_thinking_enabled("gpt-5.6-terra", &None));
        assert!(!resolve_thinking_enabled("gpt-5.6-sol", &None));
    }

    #[test]
    fn test_resolve_thinking_enabled_non_gpt_model_respects_request() {
        // 非 GPT 模型（Claude 系）走既有 Kiro thinking 协议，行为不变：
        // 请求了就启用，没请求就不启用。
        let thinking = Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 20000,
        });
        assert!(resolve_thinking_enabled("claude-sonnet-4", &thinking));

        assert!(!resolve_thinking_enabled("claude-sonnet-4", &None));
    }

    #[test]
    fn test_resolve_thinking_enabled_luna_without_thinking_request() {
        // luna 且客户端未请求 thinking：本就应为 false，确认无 panic 且结果正确。
        assert!(!resolve_thinking_enabled("gpt-5.6-luna", &None));
    }

    // ---- build_bridge_context 构造条件（任务 2 单测，D5/D7）----

    use crate::anthropic::converter as converter_mod;

    /// 构造只携带一条 user 消息的最小 MessagesRequest（tools 可选）
    fn bridge_test_request(
        tools: Option<Vec<super::super::types::Tool>>,
    ) -> super::super::types::MessagesRequest {
        super::super::types::MessagesRequest {
            model: "claude-sonnet-4".to_string(),
            max_tokens: 1024,
            messages: vec![super::super::types::Message {
                role: "user".to_string(),
                content: serde_json::json!("搜索一下今天的新闻"),
            }],
            stream: false,
            system: None,
            tools,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        }
    }

    fn ws_tool_for_bridge(
        tool_type: Option<&str>,
        name: &str,
        max_uses: Option<i32>,
    ) -> super::super::types::Tool {
        super::super::types::Tool {
            tool_type: tool_type.map(|s| s.to_string()),
            name: name.to_string(),
            description: String::new(),
            input_schema: Default::default(),
            max_uses,
            defer_loading: None,
        }
    }

    #[test]
    fn test_build_bridge_context_hit_constructs() {
        // 携带 web_search server tool → Some(BridgeContext)，字段取自 conversion_result
        let req = bridge_test_request(Some(vec![ws_tool_for_bridge(
            Some("web_search_20250305"),
            "web_search",
            Some(3),
        )]));
        let conversion = converter_mod::convert_request(&req).unwrap();

        let ctx = build_bridge_context(&conversion, Some("arn:test".to_string()), vec![1, 2]);
        let ctx = ctx.expect("命中 server tool 应构造 BridgeContext");
        assert_eq!(ctx.max_uses, Some(3));
        assert_eq!(ctx.profile_arn, Some("arn:test".to_string()));
        assert_eq!(ctx.bound_ids, vec![1, 2]);
        assert_eq!(ctx.is_compact_request, conversion.is_compact_request);
        // conversation_state 是首次转换结果的 clone（D3 演进基底）
        assert_eq!(
            serde_json::to_string(&ctx.conversation_state).unwrap(),
            serde_json::to_string(&conversion.conversation_state).unwrap()
        );
    }

    #[test]
    fn test_build_bridge_context_no_hit_returns_none() {
        // 未携带 web_search server tool → None（零行为变化路径）
        let req = bridge_test_request(Some(vec![ws_tool_for_bridge(None, "Read", None)]));
        let conversion = converter_mod::convert_request(&req).unwrap();

        assert!(build_bridge_context(&conversion, None, Vec::new()).is_none());
    }

    #[test]
    fn test_build_bridge_context_hit_without_max_uses_still_constructs() {
        // 命中但未声明 max_uses（内层 None）→ 仍构造，max_uses 为 None（上限由桥接层兜底 5）
        let req = bridge_test_request(Some(vec![ws_tool_for_bridge(
            Some("web_search_20250305"),
            "web_search",
            None,
        )]));
        let conversion = converter_mod::convert_request(&req).unwrap();

        let ctx = build_bridge_context(&conversion, None, Vec::new())
            .expect("命中但未声明 max_uses 仍应构造");
        assert_eq!(ctx.max_uses, None);
    }

    #[test]
    fn test_build_bridge_context_mixed_list_hit() {
        // 混合工具列表（普通工具 + web_search server tool）→ 命中构造；
        // 且构造条件与 stream 无关（D7：流式/非流式共用同一判定，构造函数不接收 stream 字段）
        let req = bridge_test_request(Some(vec![
            ws_tool_for_bridge(None, "Bash", None),
            ws_tool_for_bridge(Some("web_search_20250305"), "web_search", Some(5)),
        ]));
        let conversion = converter_mod::convert_request(&req).unwrap();

        let ctx = build_bridge_context(&conversion, None, Vec::new())
            .expect("混合列表命中 server tool 应构造");
        assert_eq!(ctx.max_uses, Some(5));

        // 对照：同一请求 stream=true 的转换结果构造条件一致
        let mut req_stream = req;
        req_stream.stream = true;
        let conversion_stream = converter_mod::convert_request(&req_stream).unwrap();
        assert!(build_bridge_context(&conversion_stream, None, Vec::new()).is_some());
    }

    // ---- 桥接状态机截获与聚合（任务 3 单测，D4/D8）----

    use crate::kiro::model::events::Event;
    use crate::kiro::model::events::ToolUseEvent;
    use crate::kiro::model::requests::conversation::Message;

    /// 构造最小 StreamContext（thinking 关闭；字段无外部依赖）
    fn bridge_stream_context() -> StreamContext {
        StreamContext::new_with_thinking("claude-sonnet-4", 1000, false)
    }

    /// 构造 Kiro ToolUse 事件
    fn tool_use_event(name: &str, id: &str, input: &str, stop: bool) -> Event {
        Event::ToolUse(ToolUseEvent {
            name: name.to_string(),
            tool_use_id: id.to_string(),
            input: input.to_string(),
            stop,
        })
    }

    #[test]
    fn test_bridge_input_fragments_aggregated() {
        // 分片到达（stop=false）→ Collecting 聚合，不透传也不发块；
        // stop=true → 截获完成，发 server_tool_use + web_search_tool_result 可见性块
        let mut ctx = bridge_stream_context();
        let mut bridge = Some(BridgeState::new(Some(3)));

        // 分片 1：不透传、无可见性块
        let (consumed, events) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"que"#, false),
        );
        assert!(consumed, "web_search toolUse 分片应被截获");
        assert!(events.is_empty(), "聚合期间不应发任何 SSE 块");
        assert!(matches!(
            bridge.as_ref().unwrap().phase,
            BridgePhase::Collecting { .. }
        ));

        // 分片 2 + stop：截获完成，发出可见性块
        let (consumed, events) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"ry":"rust programming"}"#, true),
        );
        assert!(consumed, "stop 分片仍属于同一 toolUse，应被截获");
        // 截获完成只发 server_tool_use(start/delta/stop) 共 3 个事件；
        // web_search_tool_result 结果块由续流阶段（unfold None 分支）携带真实 MCP 结果发出
        assert_eq!(
            events.len(),
            3,
            "应发出 server_tool_use(start/delta/stop) 共 3 个事件"
        );
        let types: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();
        assert!(
            types.contains(&"content_block_start"),
            "应含 content_block_start"
        );
        assert!(
            types.contains(&"content_block_stop"),
            "应含 content_block_stop"
        );
        // 回到 PassThrough 且轮次计数 +1
        assert!(matches!(
            bridge.as_ref().unwrap().phase,
            BridgePhase::PassThrough
        ));
        assert_eq!(bridge.as_ref().unwrap().rounds_used, 1);
    }

    #[test]
    fn test_bridge_non_target_tool_passthrough() {
        // 非目标工具（Read）与 Collecting 期间的 AssistantResponse 均正常透传（D8）
        let mut ctx = bridge_stream_context();
        let mut bridge = Some(BridgeState::new(Some(3)));

        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("Read", "tu0", r#"{"file_path":"a.rs"}"#, true),
        );
        assert!(!consumed, "非 web_search 工具应走现有透传路径");
        // 透传 SSE 由 unfold 调用方调用 process_kiro_event 产生（bridge_handle_event 返回空 Vec）
        let sse = ctx.process_kiro_event(&tool_use_event(
            "Read",
            "tu0",
            r#"{"file_path":"a.rs"}"#,
            true,
        ));
        assert!(
            !sse.is_empty(),
            "透传时 process_kiro_event 应产生 tool_use SSE"
        );
        assert!(matches!(
            bridge.as_ref().unwrap().phase,
            BridgePhase::PassThrough
        ));
        // 透传路径应分配 tool_use 块
        assert!(
            !ctx.tool_block_indices.is_empty(),
            "透传应分配 tool_use 块索引"
        );

        // 进入 Collecting 后，AssistantResponse 说明文字仍透传
        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"q"#, false),
        );
        assert!(consumed);
        // extra 字段私有，走 serde 反序列化构造
        let resp: crate::kiro::model::events::AssistantResponseEvent =
            serde_json::from_str(r#"{"content":"让我搜索一下"}"#).unwrap();
        let resp_event = Event::AssistantResponse(resp);
        let (consumed, _) = bridge_handle_event(&mut ctx, &mut bridge, &resp_event);
        assert!(!consumed, "Collecting 期间 AssistantResponse 应透传");
        let sse = ctx.process_kiro_event(&resp_event);
        assert!(!sse.is_empty(), "AssistantResponse 透传应产生 text SSE");
    }

    #[test]
    fn test_bridge_no_tool_use_sse_leak() {
        // 截获的 web_search 不产生普通 tool_use 块：state_manager 未分配 tool_use 块，
        // 客户端可见的是 server_tool_use 块（索引由 next_block_index 单调分配）
        let mut ctx = bridge_stream_context();
        let mut bridge = Some(BridgeState::new(None));

        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"query":"rust"}"#, true),
        );
        assert!(consumed);
        assert!(
            ctx.tool_block_indices.is_empty(),
            "截获路径不得分配普通 tool_use 块索引"
        );
        // 截获完成时 server_tool_use 块消耗了 1 个块索引；
        // web_search_tool_result 结果块由续流阶段发出（届时再消耗 1 个）
        let next = ctx.state_manager.next_block_index();
        assert!(
            next >= 1,
            "server_tool_use 块应已占用至少 1 个块索引，实际 next={next}"
        );
        // max_uses 未声明（内层 None）时兜底上限 5
        assert_eq!(bridge.as_ref().unwrap().max_rounds, 5);
    }

    #[test]
    fn test_bridge_rounds_exhausted_passthrough() {
        // 轮次耗尽后 web_search 不再截获，按普通 tool_use 透传（D8 上限语义）
        let mut ctx = bridge_stream_context();
        // 上限 0（max_uses=0 时 clamp 到 0）
        let mut bridge = Some(BridgeState::new(Some(0)));

        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"query":"x"}"#, true),
        );
        assert!(!consumed, "轮次耗尽后应透传为普通 tool_use");
        // 透传 SSE 由 unfold 调用方调用 process_kiro_event 产生
        ctx.process_kiro_event(&tool_use_event(
            "web_search",
            "tu1",
            r#"{"query":"x"}"#,
            true,
        ));
        assert!(
            !ctx.tool_block_indices.is_empty(),
            "透传路径应分配普通 tool_use 块"
        );
    }

    // ---- 续请求体构建（任务 4 单测，D3）----

    /// 构造测试用 BridgeContext（conversation_state 带完整字段供不变量断言）
    fn bridge_ctx_for_continuation() -> BridgeContext {
        let mut state = ConversationState::new("conv-123");
        state.agent_continuation_id = Some("cont-456".to_string());
        state.agent_task_type = Some("vibe".to_string());
        state.chat_trigger_type = Some("MANUAL".to_string());
        state.current_message.user_input_message.content = "搜索一下今天的新闻".to_string();
        state.history = vec![
            Message::user("历史用户消息", "claude-sonnet-4"),
            Message::assistant("历史助手回复"),
        ];
        BridgeContext {
            conversation_state: state,
            profile_arn: Some("arn:test".to_string()),
            additional_model_request_fields: Some(serde_json::json!({"max_tokens": 1024})),
            max_uses: Some(3),
            bound_ids: vec![1],
            is_compact_request: false,
        }
    }

    fn sample_search_results() -> websearch::WebSearchResults {
        websearch::WebSearchResults {
            results: vec![websearch::WebSearchResult {
                title: "Rust 官方文档".to_string(),
                url: "https://doc.rust-lang.org".to_string(),
                snippet: Some("The Rust programming language".to_string()),
                published_date: None,
                id: None,
                domain: None,
                max_verbatim_word_limit: None,
                public_domain: None,
            }],
            total_results: Some(1),
            query: Some("rust".to_string()),
            error: None,
        }
    }

    #[test]
    fn test_continuation_request_invariants() {
        // 续请求体不变量（D3）：conversationId/agentContinuationId/agentTaskType/
        // chatTriggerType/history 逐字节不变；仅 current_message.tool_results 回填，
        // toolUseId 用 Kiro 流截获的 tu.tool_use_id（非 create_mcp_request 的 srvtoolu_ id）
        let ctx = bridge_ctx_for_continuation();
        let baseline = serde_json::to_string(&ctx.conversation_state).unwrap();
        let results = sample_search_results();

        let tool_result = build_search_tool_result("tu-kiro-new-1", "rust", &Some(results));
        let kiro_request = build_continuation_request(
            &ctx,
            Some(ctx.conversation_state.clone()),
            vec![tool_result],
        );
        let body = serde_json::to_string(&kiro_request.conversation_state).unwrap();
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();

        // 不变量字段逐字节一致
        assert!(
            value["conversationId"]
                .as_str()
                .unwrap()
                .contains("conv-123")
        );
        assert_eq!(value["agentContinuationId"], serde_json::json!("cont-456"));
        assert_eq!(value["agentTaskType"], serde_json::json!("vibe"));
        assert_eq!(value["chatTriggerType"], serde_json::json!("MANUAL"));
        assert_eq!(
            value["history"],
            serde_json::to_value(&ctx.conversation_state.history).unwrap(),
            "history 必须逐字节不变"
        );
        // current_message 的 content 不变，tool_results 回填为 1 条
        assert_eq!(
            value["currentMessage"]["userInputMessage"]["content"],
            serde_json::json!("搜索一下今天的新闻")
        );
        let tool_results =
            &value["currentMessage"]["userInputMessage"]["userInputMessageContext"]["toolResults"];
        assert_eq!(tool_results.as_array().unwrap().len(), 1);
        assert_eq!(
            tool_results[0]["toolUseId"],
            serde_json::json!("tu-kiro-new-1")
        );
        assert_eq!(tool_results[0]["status"], serde_json::json!("success"));
        // is_error=false 时被 is_false 跳过序列化，出现即为缺陷
        assert!(
            tool_results[0].get("isError").is_none(),
            "success 路径不应序列化 isError"
        );
        // 回填只改 tool_results，其余部分与基底完全一致
        assert_ne!(body, baseline, "tool_results 回填后应与基底不同");
        // profile_arn / additional_model_request_fields 同参序列化
        assert_eq!(
            serde_json::to_string(&kiro_request.profile_arn).unwrap(),
            serde_json::to_string(&Some("arn:test".to_string())).unwrap()
        );
        assert!(kiro_request.additional_model_request_fields.is_some());
    }

    #[test]
    fn test_continuation_request_multiround_evolution() {
        // 多轮演进语义（D3）：第 2 轮续请求基于第 1 轮所用状态演进（bridge_execute_round
        // 把本轮 kiro_request.conversation_state 写回 evolution_base），history 仍逐字节不变
        let ctx = bridge_ctx_for_continuation();
        let results = sample_search_results();

        // 第 1 轮：evolution_base = BridgeContext.conversation_state（create_sse_stream 初始化语义）
        let round1 = build_continuation_request(
            &ctx,
            Some(ctx.conversation_state.clone()),
            vec![build_search_tool_result(
                "tu-round-1",
                "rust",
                &Some(sample_search_results()),
            )],
        );
        // 第 2 轮：bridge_execute_round 将 round1 的状态写回 evolution_base（此处模拟）
        let round2 = build_continuation_request(
            &ctx,
            Some(round1.conversation_state.clone()),
            vec![build_search_tool_result(
                "tu-round-2",
                "tokio",
                &Some(results),
            )],
        );

        let v1: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&round1.conversation_state).unwrap())
                .unwrap();
        let v2: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&round2.conversation_state).unwrap())
                .unwrap();

        // 第 2 轮 history 与第 1 轮一致（中间轮 toolResults 不追加进 history）
        assert_eq!(v2["history"], v1["history"], "多轮 history 必须逐字节不变");
        assert_eq!(v2["conversationId"], v1["conversationId"]);
        assert_eq!(v2["agentContinuationId"], v1["agentContinuationId"]);
        // 第 2 轮 tool_results 替换为本轮结果（承载最新 toolUseId）
        let tr2 =
            &v2["currentMessage"]["userInputMessage"]["userInputMessageContext"]["toolResults"];
        assert_eq!(tr2[0]["toolUseId"], serde_json::json!("tu-round-2"));
        // 第 2 轮的其余字段与第 1 轮状态一致（基于第 1 轮演进，非从原始 clone 重新出发）
        assert_eq!(
            v2["currentMessage"]["userInputMessage"]["content"],
            v1["currentMessage"]["userInputMessage"]["content"]
        );
    }

    #[test]
    fn test_continuation_request_mcp_failure_error_tool_result() {
        // MCP 失败（search_results=None）→ ToolResult::error 降级，仍发续请求（流不中断）
        let ctx = bridge_ctx_for_continuation();

        let tool_result = build_search_tool_result("tu-fail-1", "rust", &None);
        let kiro_request = build_continuation_request(
            &ctx,
            Some(ctx.conversation_state.clone()),
            vec![tool_result],
        );
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&kiro_request.conversation_state).unwrap())
                .unwrap();
        let tool_results =
            &value["currentMessage"]["userInputMessage"]["userInputMessageContext"]["toolResults"];
        assert_eq!(tool_results.as_array().unwrap().len(), 1);
        assert_eq!(tool_results[0]["toolUseId"], serde_json::json!("tu-fail-1"));
        assert_eq!(tool_results[0]["status"], serde_json::json!("error"));
        assert_eq!(tool_results[0]["isError"], serde_json::json!(true));
        let text = tool_results[0]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("Web search failed for query: rust"),
            "error 文案应说明搜索失败，实际: {text}"
        );
        // 降级路径不变量保持：history/会话标识仍逐字节不变
        assert_eq!(
            value["history"],
            serde_json::to_value(&ctx.conversation_state.history).unwrap()
        );
        assert_eq!(
            value["conversationId"],
            serde_json::to_value("conv-123").unwrap()
        );
        assert_eq!(value["agentContinuationId"], serde_json::json!("cont-456"));
    }

    #[test]
    fn test_continuation_request_fallback_to_bridge_ctx_state() {
        // evolution_base 为 None（防御路径）→ 回退到 BridgeContext.conversation_state clone
        let ctx = bridge_ctx_for_continuation();
        let results = sample_search_results();

        let tool_result = build_search_tool_result("tu-fb-1", "rust", &Some(results));
        let kiro_request = build_continuation_request(&ctx, None, vec![tool_result]);
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&kiro_request.conversation_state).unwrap())
                .unwrap();
        assert_eq!(
            value["conversationId"],
            serde_json::to_value("conv-123").unwrap()
        );
        let tool_results =
            &value["currentMessage"]["userInputMessage"]["userInputMessageContext"]["toolResults"];
        assert_eq!(tool_results[0]["toolUseId"], serde_json::json!("tu-fb-1"));
        assert_eq!(tool_results[0]["status"], serde_json::json!("success"));
    }

    // ---- 多轮上限与收尾（任务 5 单测，D8）----

    #[test]
    fn test_bridge_round_counting_up_to_limit() {
        // 上限计数：每完成一轮截获 rounds_used +1；达到上限后 has_remaining_rounds 为 false
        let mut ctx = bridge_stream_context();
        let mut bridge = Some(BridgeState::new(Some(2)));

        // 第 1 轮截获完成
        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"query":"a"}"#, true),
        );
        assert!(consumed);
        assert_eq!(bridge.as_ref().unwrap().rounds_used, 1);
        assert!(bridge.as_ref().unwrap().has_remaining_rounds());

        // 第 2 轮截获完成 → 达到上限
        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu2", r#"{"query":"b"}"#, true),
        );
        assert!(consumed);
        assert_eq!(bridge.as_ref().unwrap().rounds_used, 2);
        assert!(
            !bridge.as_ref().unwrap().has_remaining_rounds(),
            "达到 min(max_uses,5) 后不应再有剩余轮次"
        );

        // 上限为 None 时兜底 5，5 轮内均有剩余
        let bridge5 = BridgeState::new(None);
        assert_eq!(bridge5.max_rounds, 5);
        assert!(bridge5.has_remaining_rounds());

        // max_uses 超过 5 时 clamp 到 5（D8 硬上限）
        let bridge_clamped = BridgeState::new(Some(99));
        assert_eq!(bridge_clamped.max_rounds, 5);
    }

    #[test]
    fn test_bridge_exhausted_web_search_passthrough_non_target_still_intercepts() {
        // 上限后透传：轮次耗尽后新 web_search toolUse 不再截获，按普通 tool_use 走
        // process_kiro_event 产生 tool_use SSE 块；随后继续正常透传（状态不被污染）
        let mut ctx = bridge_stream_context();
        let mut bridge = Some(BridgeState::new(Some(1)));

        // 唯一轮次用掉
        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"query":"a"}"#, true),
        );
        assert!(consumed);

        // 轮次耗尽后的 web_search：不截获 → process_kiro_event 分配普通 tool_use 块
        let exhausted = tool_use_event("web_search", "tu2", r#"{"query":"b"}"#, true);
        let (consumed, events) = bridge_handle_event(&mut ctx, &mut bridge, &exhausted);
        assert!(!consumed, "轮次耗尽后 web_search 应透传");
        assert!(events.is_empty());
        let sse = ctx.process_kiro_event(&exhausted);
        assert!(!sse.is_empty(), "透传应产生 tool_use SSE");
        assert_eq!(
            ctx.tool_block_indices.len(),
            1,
            "透传路径应恰好分配 1 个普通 tool_use 块"
        );

        // 透传后状态仍为 PassThrough、轮次不再增长；pending 保持第 1 轮的待执行搜索
        let state = bridge.as_ref().unwrap();
        assert!(matches!(state.phase, BridgePhase::PassThrough));
        assert_eq!(state.rounds_used, 1);
        assert_eq!(state.pending.len(), 1, "应恰好保留 1 条待执行搜索");
        assert_eq!(state.pending.front().unwrap().tool_use_id, "tu1");
        assert_eq!(state.pending.front().unwrap().query, "a");

        // 之后又截获到新轮次窗口的情形不存在（rounds_used 不回退），上限语义稳定
        assert!(!state.has_remaining_rounds());
    }

    #[test]
    fn test_generate_final_events_message_stop_exactly_once() {
        // 桥接收尾恰好一次：generate_final_events 的 message_stop 由 message_ended
        // 门控——首次调用补发 message_stop，重复调用不再产生（防客户端双 message_stop）
        let mut ctx = bridge_stream_context();
        ctx.process_kiro_event(&Event::AssistantResponse(
            serde_json::from_str(r#"{"content":"回答正文"}"#).unwrap(),
        ));
        // 模拟流结束：置位 message_ended 前的最终事件序列
        let final_events = ctx.generate_final_events();
        let stops: Vec<_> = final_events
            .iter()
            .filter(|e| e.event == "message_stop")
            .collect();
        assert_eq!(stops.len(), 1, "首次收尾应恰好发出 1 个 message_stop");
        // 桥接失败兜底路径可能再次调用收尾——message_ended 门控保证不重发
        let repeated = ctx.generate_final_events();
        assert!(
            !repeated.iter().any(|e| e.event == "message_stop"),
            "重复收尾不得再次发出 message_stop"
        );
    }

    // ---- 非流式桥接（任务 5.5 单测，D4 非流式段 / D5 非流式段）----

    fn non_stream_tool_use_event(
        name: &str,
        id: &str,
        input: &str,
        stop: bool,
    ) -> crate::kiro::model::events::ToolUseEvent {
        // input 与上游流一致，是原始 JSON 字符串（未解析），可传分片
        crate::kiro::model::events::ToolUseEvent {
            name: name.to_string(),
            tool_use_id: id.to_string(),
            input: input.to_string(),
            stop,
        }
    }

    #[test]
    fn test_non_stream_bridge_step_intercept_and_aggregate() {
        // 截获聚合：轮次未达上限的 web_search 分片被截获，stop 时完成并解析 query；
        // 期间不产生普通 tool_use 语义（调用方据此不置 has_tool_use）
        let mut collecting: Option<(String, String)> = None;

        // 分片 1（非 stop）→ 截获、未完成
        let (intercepted, completed) = non_stream_bridge_step(
            &mut collecting,
            0,
            5,
            &non_stream_tool_use_event("web_search", "tu1", r#"{"query":"rus"#, false),
        );
        assert!(intercepted);
        assert!(completed.is_none());
        assert!(collecting.is_some());

        // 分片 2（stop）→ 完成截获，query 聚合完整
        let (intercepted, completed) = non_stream_bridge_step(
            &mut collecting,
            0,
            5,
            &non_stream_tool_use_event("web_search", "tu1", r#"t"}"#, true),
        );
        assert!(intercepted);
        let pending = completed.expect("stop 分片应完成截获");
        assert_eq!(pending.tool_use_id, "tu1");
        assert_eq!(pending.query, "rust");
        assert!(collecting.is_none());
    }

    #[test]
    fn test_non_stream_bridge_step_query_parse_failure_yields_empty() {
        // input 非 JSON（query 解析失败）→ parse_bridge_query 兜底空串，仍完成截获
        let (intercepted, completed) = non_stream_bridge_step(
            &mut None,
            0,
            5,
            &non_stream_tool_use_event("web_search", "tu-bad", "not-json", true),
        );
        assert!(intercepted);
        assert_eq!(completed.unwrap().query, "");
    }

    #[test]
    fn test_non_stream_bridge_step_limit_passthrough() {
        // 轮次耗尽（rounds_used >= max_rounds）→ 不截获，按普通 tool_use 透传；
        // 非桥接请求（max_rounds=0）同样不截获
        let (intercepted, completed) = non_stream_bridge_step(
            &mut None,
            3,
            3,
            &non_stream_tool_use_event("web_search", "tu1", r#"{"query":"a"}"#, true),
        );
        assert!(!intercepted, "轮次耗尽后应按普通 tool_use 透传");
        assert!(completed.is_none());

        let (intercepted, _) = non_stream_bridge_step(
            &mut None,
            0,
            0,
            &non_stream_tool_use_event("web_search", "tu2", r#"{"query":"b"}"#, true),
        );
        assert!(!intercepted, "非桥接请求（max_rounds=0）不应截获");
    }

    #[test]
    fn test_non_stream_bridge_step_other_tool_passthrough() {
        // 非目标工具不截获、不污染聚合状态
        let mut collecting: Option<(String, String)> = None;
        let (intercepted, _) = non_stream_bridge_step(
            &mut collecting,
            0,
            5,
            &non_stream_tool_use_event("Read", "tu-file", r#"{"path":"a.rs"}"#, true),
        );
        assert!(!intercepted);
        assert!(collecting.is_none(), "非 web_search 不得进入聚合状态");
    }

    #[test]
    fn test_web_search_result_block_shape() {
        // web_search_tool_result 块格式（D5 非流式段）：与流式条目格式一致；
        // MCP 失败（None）时 content 为空数组
        let block = build_web_search_result_block("tu-ok-1", &Some(sample_search_results()));
        assert_eq!(block["type"], "web_search_tool_result");
        assert_eq!(block["tool_use_id"], "tu-ok-1");
        let items = block["content"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["type"], "web_search_result");
        assert_eq!(items[0]["title"], "Rust 官方文档");
        assert_eq!(items[0]["url"], "https://doc.rust-lang.org");
        assert_eq!(
            items[0]["encrypted_content"],
            "The Rust programming language"
        );
        assert!(items[0]["page_age"].is_null());

        let empty = build_web_search_result_block("tu-fail-1", &None);
        assert_eq!(
            empty["content"].as_array().unwrap().len(),
            0,
            "MCP 失败时 content 应为空数组"
        );
    }

    // ---- H3：续请求失败 → error 事件收尾（任务 4 单测，unfold None 分支语义）----

    use crate::kiro::model::credentials::KiroCredentials;
    use crate::kiro::token_manager::MultiTokenManager;
    use crate::model::config::Config;
    use chrono::Utc;

    /// 构造"必然刷新失败"的测试 Provider：凭据已过期且无 refreshToken，
    /// validate_refresh_token 阶段立即失败（无需真实网络请求），
    /// MCP 调用与续请求 call_api_stream 均快速返回 Err
    fn bridge_test_provider() -> crate::kiro::provider::KiroProvider {
        let mut cred = KiroCredentials::default();
        cred.access_token = Some("test-invalid-token".to_string());
        cred.expires_at = Some((Utc::now() - chrono::Duration::hours(1)).to_rfc3339());
        let manager = std::sync::Arc::new(
            MultiTokenManager::new(Config::default(), vec![cred], None, None, false)
                .expect("构造 MultiTokenManager 失败"),
        );
        crate::kiro::provider::KiroProvider::new(manager)
    }

    #[tokio::test]
    async fn test_bridge_execute_round_returns_failed_on_continuation_failure() {
        // unfold None 分支语义（H3）：bridge_execute_round 在续请求发起失败时
        // 返回 Failed（携带 MCP 搜索结果供调用方补发结果块配对），调用方据此
        // 先发 web_search_tool_result 结果块、再补发 stream_interrupted_error_event
        // 并置 finished=true 正常收尾（不再发 message_stop），防止客户端流悬挂
        let provider = bridge_test_provider();
        let bridge_ctx = bridge_ctx_for_continuation();
        let mut bridge = BridgeState::new(Some(1));
        let pending = PendingSearch {
            tool_use_id: "tu-fail-net".to_string(),
            query: "rust".to_string(),
        };

        let result = bridge_execute_round(&provider, &bridge_ctx, &mut bridge, pending).await;
        assert!(
            matches!(result, BridgeRoundOutcome::Failed(_)),
            "续请求发起失败时应返回 Failed"
        );
        // Err 分支：演进基底写回取出的状态，避免下一轮基于未知状态演进
        assert!(bridge.evolution_base.is_some());

        // None → unfold 补发的 error 事件结构（与既有
        // test_stream_interrupted_error_event_signals_failure_not_success 同口径）
        let event = stream_interrupted_error_event();
        assert_eq!(event.event, "error");
        assert_eq!(event.data["error"]["type"], "overloaded_error");
        assert!(
            event.data["error"]["message"]
                .as_str()
                .unwrap()
                .contains("interrupted")
        );
    }

    // ---- ⚠️#2（第 2 轮增量 CR）：harvest_bridge_round 三条收尾不变量 ----

    #[test]
    fn test_harvest_bridge_round_continued_pairs_result_block() {
        // Continued → 先发配对结果块（finished=false），结果块与 server_tool_use
        // 成对，且不含 error/message_stop 收尾事件
        let mut ctx = StreamContext::new_with_thinking("claude-sonnet-4", 1000, false);
        let response = reqwest::Response::from(http::Response::new(body_dummy_bytes()));
        let outcome = BridgeRoundOutcome::Continued(
            response,
            EventStreamDecoder::new(),
            Some(sample_search_results()),
        );

        let harvest = harvest_bridge_round(outcome, "tu-cont-1", &mut ctx);
        assert!(!harvest.finished, "Continued 应换入续流，finished=false");
        assert_eq!(
            harvest.events.len(),
            2,
            "Continued 补发 web_search_tool_result 结果块（start + stop 两事件）"
        );
        assert_eq!(
            harvest.events[0].data["content_block"]["type"],
            "web_search_tool_result"
        );
        assert_eq!(
            harvest.events[1].event, "content_block_stop",
            "配对结果块以 stop 事件收尾"
        );
    }

    #[test]
    fn test_harvest_bridge_round_failed_emits_result_then_error() {
        // Failed → 结果块（携带已产出搜索结果）+ error 收尾事件，finished=true
        let mut ctx = StreamContext::new_with_thinking("claude-sonnet-4", 1000, false);
        let outcome = BridgeRoundOutcome::Failed(Some(sample_search_results()));

        let harvest = harvest_bridge_round(outcome, "tu-fail-1", &mut ctx);
        assert!(harvest.finished, "Failed 应立即收尾，finished=true");
        assert_eq!(
            harvest.events.len(),
            3,
            "Failed = 结果块(start + stop) + error 事件"
        );
        assert_eq!(
            harvest.events[0].data["content_block"]["type"],
            "web_search_tool_result"
        );
        assert_eq!(harvest.events[2].event, "error");
    }

    #[test]
    fn test_harvest_bridge_round_panic_fallback_empty_results() {
        // 后台任务 panic 兜底 = Failed(None) → 空数组结果块 + error 收尾，
        // 与 MCP 失败同口径（保证 server_tool_use / web_search_tool_result 成对）
        let mut ctx = StreamContext::new_with_thinking("claude-sonnet-4", 1000, false);
        let outcome = BridgeRoundOutcome::Failed(None);

        let harvest = harvest_bridge_round(outcome, "tu-panic-1", &mut ctx);
        assert!(harvest.finished);
        assert_eq!(harvest.events.len(), 3, "panic 兜底同 Failed 收尾结构");
        assert_eq!(
            harvest.events[0].data["content_block"]["content"],
            serde_json::json!([]),
            "panic 兜底结果块 content 应为空数组"
        );
        assert_eq!(harvest.events[2].event, "error");
    }

    #[test]
    fn test_flush_unpaired_search_blocks_drains_queue() {
        // 非流式降级收尾：队列剩余 pending 全部补发空结果块，且队列被清空
        let mut pending = VecDeque::new();
        pending.push_back(PendingSearch {
            tool_use_id: "tu-left-1".to_string(),
            query: "a".to_string(),
        });
        pending.push_back(PendingSearch {
            tool_use_id: "tu-left-2".to_string(),
            query: "b".to_string(),
        });
        let mut blocks = Vec::new();

        flush_unpaired_search_blocks(&mut pending, &mut blocks);

        assert!(pending.is_empty(), "队列应被 drain 清空");
        assert_eq!(blocks.len(), 2, "每条遗留 pending 补发一个结果块");
        for (i, block) in blocks.iter().enumerate() {
            assert_eq!(block["type"], "web_search_tool_result");
            assert_eq!(
                block["content"],
                serde_json::json!([]),
                "降级路径搜索未执行，结果块应为空数组"
            );
            let _ = i;
        }
    }
}
