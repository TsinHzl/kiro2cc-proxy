//! User API 中间件

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::common::auth;
use crate::model::api_key::{ApiKeyAuthResult, ApiKeyManager};
use crate::model::usage::UsageTracker;

/// User API 共享状态
#[derive(Clone)]
pub struct UserState {
    pub api_key_manager: Arc<ApiKeyManager>,
    pub usage_tracker: Arc<UsageTracker>,
}

/// 用户认证上下文（注入到请求 extensions）
#[derive(Clone)]
pub struct UserContext {
    pub key_id: u32,
    pub key_name: String,
    pub spending_limit: Option<f64>,
}

/// 错误响应
#[derive(Serialize)]
pub struct UserErrorResponse {
    pub error: String,
}

/// User API 认证中间件
/// 通过 API Key（sk-*）鉴权，验证后注入 UserContext
pub async fn user_auth_middleware(
    State(state): State<UserState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let api_key = auth::extract_api_key(&request);

    let Some(key) = api_key else {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(UserErrorResponse {
                error: "缺少 API Key".into(),
            }),
        )
            .into_response();
    };

    match state.api_key_manager.authenticate_readonly(&key) {
        ApiKeyAuthResult::Valid { id, name, spending_limit } => {
            request.extensions_mut().insert(UserContext {
                key_id: id,
                key_name: name,
                spending_limit,
            });
            next.run(request).await
        }
        ApiKeyAuthResult::Disabled => (
            StatusCode::FORBIDDEN,
            axum::Json(UserErrorResponse {
                error: "API Key 已被禁用".into(),
            }),
        )
            .into_response(),
        ApiKeyAuthResult::Expired => (
            StatusCode::FORBIDDEN,
            axum::Json(UserErrorResponse {
                error: "API Key 已过期".into(),
            }),
        )
            .into_response(),
        ApiKeyAuthResult::NotFound => (
            StatusCode::UNAUTHORIZED,
            axum::Json(UserErrorResponse {
                error: "无效的 API Key".into(),
            }),
        )
            .into_response(),
    }
}
