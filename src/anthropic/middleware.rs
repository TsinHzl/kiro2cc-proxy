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
use crate::model::api_key::{ApiKeyAuthResult, ApiKeyManager};
use crate::model::rpm::RpmTracker;
use crate::model::usage::UsageTracker;

use super::types::ErrorResponse;

/// 已认证的 API Key 上下文（注入到 request extensions）
#[derive(Clone, Debug)]
pub struct ApiKeyContext {
    /// API Key ID（0 = 主密钥）
    pub id: u32,
    /// 额度限制（美元），None 表示不限额
    pub spending_limit: Option<f64>,
}

/// 应用共享状态
#[derive(Clone)]
pub struct AppState {
    /// 主 API 密钥（始终有效，不可禁用，运行时可修改）
    pub api_key: Arc<RwLock<String>>,
    /// Kiro Provider（可选，用于实际 API 调用）
    pub kiro_provider: Option<Arc<KiroProvider>>,
    /// Profile ARN（可选，用于请求）
    pub profile_arn: Option<String>,
    /// API Key 管理器（可选，启用多用户 API Key）
    pub api_key_manager: Option<Arc<ApiKeyManager>>,
    /// 用量追踪器（可选，启用用量追踪）
    pub usage_tracker: Option<Arc<UsageTracker>>,
    /// RPM 追踪器（可选，启用 RPM 实时监控）
    pub rpm_tracker: Option<Arc<RpmTracker>>,
}

impl AppState {
    /// 创建新的应用状态
    pub fn new(api_key: Arc<RwLock<String>>) -> Self {
        Self {
            api_key,
            kiro_provider: None,
            profile_arn: None,
            api_key_manager: None,
            usage_tracker: None,
            rpm_tracker: None,
        }
    }

    /// 设置 KiroProvider
    pub fn with_kiro_provider(mut self, provider: KiroProvider) -> Self {
        self.kiro_provider = Some(Arc::new(provider));
        self
    }

    /// 设置 Profile ARN
    pub fn with_profile_arn(mut self, arn: impl Into<String>) -> Self {
        self.profile_arn = Some(arn.into());
        self
    }

    /// 设置 API Key 管理器
    pub fn with_api_key_manager(mut self, manager: Arc<ApiKeyManager>) -> Self {
        self.api_key_manager = Some(manager);
        self
    }

    /// 设置用量追踪器
    pub fn with_usage_tracker(mut self, tracker: Arc<UsageTracker>) -> Self {
        self.usage_tracker = Some(tracker);
        self
    }

    /// 设置 RPM 追踪器
    pub fn with_rpm_tracker(mut self, tracker: Arc<RpmTracker>) -> Self {
        self.rpm_tracker = Some(tracker);
        self
    }
}

/// API Key 认证中间件
///
/// 认证优先级：
/// 1. 主密钥（config.apiKey）→ 直接放行，id=0
/// 2. 子 API Key（ApiKeyManager）→ 检查启用/过期/额度
/// 3. 都不匹配 → 401
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let Some(key) = auth::extract_api_key(&request) else {
        let error = ErrorResponse::authentication_error();
        return (StatusCode::UNAUTHORIZED, Json(error)).into_response();
    };

    // 1. 主密钥匹配 → 直接放行
    if auth::constant_time_eq(&key, &state.api_key.read()) {
        request.extensions_mut().insert(ApiKeyContext {
            id: 0,
            spending_limit: None,
        });
        return next.run(request).await;
    }

    // 2. 尝试子 API Key 认证
    if let Some(manager) = &state.api_key_manager {
        match manager.authenticate(&key) {
            ApiKeyAuthResult::Valid { id, name, spending_limit } => {
                // 懒激活：首次使用时激活 key
                if let Err(e) = manager.activate_key(id) {
                    tracing::warn!(api_key_id = id, error = %e, "激活 API Key 失败");
                }

                // 额度检查
                if let (Some(limit), Some(tracker)) = (spending_limit, &state.usage_tracker) {
                    let total_cost = tracker.get_total_cost(id);
                    if total_cost >= limit {
                        tracing::warn!(
                            api_key_id = id,
                            api_key_name = %name,
                            total_cost = total_cost,
                            spending_limit = limit,
                            "API Key 额度已用尽"
                        );
                        let error = ErrorResponse::new(
                            "forbidden",
                            format!(
                                "API key spending limit exceeded. Used: ${:.2}, Limit: ${:.2}",
                                total_cost, limit
                            ),
                        );
                        return (StatusCode::FORBIDDEN, Json(error)).into_response();
                    }
                }

                tracing::debug!(api_key_id = id, api_key_name = %name, "子 API Key 认证通过");
                request.extensions_mut().insert(ApiKeyContext {
                    id,
                    spending_limit,
                });
                return next.run(request).await;
            }
            ApiKeyAuthResult::Expired => {
                let error = ErrorResponse::new(
                    "forbidden",
                    "API key has expired. Please contact the administrator to renew it.",
                );
                return (StatusCode::FORBIDDEN, Json(error)).into_response();
            }
            ApiKeyAuthResult::Disabled => {
                let error = ErrorResponse::new(
                    "forbidden",
                    "API key has been disabled. Please contact the administrator.",
                );
                return (StatusCode::FORBIDDEN, Json(error)).into_response();
            }
            ApiKeyAuthResult::NotFound => {
                // 继续到下面的通用 401
            }
        }
    }

    // 3. 都不匹配
    let error = ErrorResponse::authentication_error();
    (StatusCode::UNAUTHORIZED, Json(error)).into_response()
}

/// CORS 中间件层
///
/// **安全说明**：当前配置允许所有来源（Any），这是为了支持公开 API 服务。
/// 如果需要更严格的安全控制，请根据实际需求配置具体的允许来源、方法和头信息。
///
/// # 配置说明
/// - `allow_origin(Any)`: 允许任何来源的请求
/// - `allow_methods(Any)`: 允许任何 HTTP 方法
/// - `allow_headers(Any)`: 允许任何请求头
pub fn cors_layer() -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};

    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}
