//! Admin API 中间件

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use parking_lot::RwLock;

use super::service::AdminService;
use super::types::AdminErrorResponse;
use crate::common::auth;
use crate::model::api_key::ApiKeyManager;
use crate::model::rpm::RpmTracker;
use crate::model::usage::UsageTracker;

/// Admin API 共享状态
#[derive(Clone)]
pub struct AdminState {
    /// Admin API 密钥（运行时可修改）
    pub admin_api_key: Arc<RwLock<String>>,
    /// 主 API 密钥（用于前端展示，运行时可修改）
    pub master_api_key: Option<Arc<RwLock<String>>>,
    /// Admin 服务
    pub service: Arc<AdminService>,
    /// API Key 管理器（可选）
    pub api_key_manager: Option<Arc<ApiKeyManager>>,
    /// 用量追踪器（可选）
    pub usage_tracker: Option<Arc<UsageTracker>>,
    /// RPM 追踪器（可选）
    pub rpm_tracker: Option<Arc<RpmTracker>>,
    /// 配置文件路径（用于持久化修改）
    pub config_path: Option<PathBuf>,
}

impl AdminState {
    pub fn new(admin_api_key: Arc<RwLock<String>>, service: AdminService) -> Self {
        Self {
            admin_api_key,
            master_api_key: None,
            service: Arc::new(service),
            api_key_manager: None,
            usage_tracker: None,
            rpm_tracker: None,
            config_path: None,
        }
    }

    pub fn with_master_api_key(mut self, key: Arc<RwLock<String>>) -> Self {
        self.master_api_key = Some(key);
        self
    }

    pub fn with_api_key_manager(mut self, manager: Arc<ApiKeyManager>) -> Self {
        self.api_key_manager = Some(manager);
        self
    }

    pub fn with_usage_tracker(mut self, tracker: Arc<UsageTracker>) -> Self {
        self.usage_tracker = Some(tracker);
        self
    }

    pub fn with_rpm_tracker(mut self, tracker: Arc<RpmTracker>) -> Self {
        self.rpm_tracker = Some(tracker);
        self
    }

    pub fn with_config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }
}

/// Admin API 认证中间件
pub async fn admin_auth_middleware(
    State(state): State<AdminState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let api_key = auth::extract_api_key(&request);

    match api_key {
        Some(key) if auth::constant_time_eq(&key, &state.admin_api_key.read()) => next.run(request).await,
        _ => {
            let error = AdminErrorResponse::authentication_error();
            (StatusCode::UNAUTHORIZED, Json(error)).into_response()
        }
    }
}
