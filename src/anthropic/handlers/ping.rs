// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic API Handler 函数

use crate::anthropic::middleware::AppState;

use axum::{
    body::Body,
    extract::State,
    response::{IntoResponse, Json},
};
use serde_json::json;

use super::models::build_model_list;

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
