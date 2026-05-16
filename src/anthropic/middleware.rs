//! Anthropic API 中间件

use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use parking_lot::RwLock;

use crate::common::auth;
use crate::kiro::provider::KiroProvider;

use super::types::ErrorResponse;

/// 已认证的 API Key 上下文（注入到 request extensions）
#[derive(Clone, Debug)]
pub struct ApiKeyContext {
    pub id: u32,
}

/// 应用共享状态
#[derive(Clone)]
pub struct AppState {
    pub api_key: Arc<RwLock<String>>,
    pub kiro_provider: Option<Arc<KiroProvider>>,
    pub profile_arn: Option<String>,
}

impl AppState {
    pub fn new(api_key: Arc<RwLock<String>>) -> Self {
        Self {
            api_key,
            kiro_provider: None,
            profile_arn: None,
        }
    }

    pub fn with_kiro_provider(mut self, provider: KiroProvider) -> Self {
        self.kiro_provider = Some(Arc::new(provider));
        self
    }

    pub fn with_profile_arn(mut self, arn: impl Into<String>) -> Self {
        self.profile_arn = Some(arn.into());
        self
    }
}

/// API Key 认证中间件
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let Some(key) = auth::extract_api_key(&request) else {
        let error = ErrorResponse::authentication_error();
        return (StatusCode::UNAUTHORIZED, Json(error)).into_response();
    };

    if auth::constant_time_eq(&key, &state.api_key.read()) {
        request.extensions_mut().insert(ApiKeyContext { id: 0 });
        return next.run(request).await;
    }

    let error = ErrorResponse::authentication_error();
    (StatusCode::UNAUTHORIZED, Json(error)).into_response()
}

/// CORS 中间件层
pub fn cors_layer() -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};

    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}
