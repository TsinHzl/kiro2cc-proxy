// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin 中间件与状态：AdminState 装配、admin_auth_middleware 管理密码认证，保护 /api/admin 下全部端点。

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
use crate::log_capture::LogCapture;
use crate::model::api_key::ApiKeyManager;
use crate::model::chat_session::ChatSessionStore;
use crate::model::failure_log::FailureLogStore;
use crate::model::geo::GeoResolver;
use crate::model::rpm::RpmTracker;
use crate::model::throttle_log::ThrottleLogStore;
use crate::model::usage::UsageTracker;

/// Admin API 共享状态
#[derive(Clone)]
pub struct AdminState {
    /// Admin Password（运行时可修改）
    pub admin_psw: Arc<RwLock<String>>,
    /// Admin 服务
    pub service: Arc<AdminService>,
    /// API Key 管理器（可选）
    pub api_key_manager: Option<Arc<ApiKeyManager>>,
    /// 用量追踪器（可选）
    pub usage_tracker: Option<Arc<UsageTracker>>,
    /// RPM 追踪器（可选）
    pub rpm_tracker: Option<Arc<RpmTracker>>,
    /// 限流日志存储（可选）
    pub throttle_log_store: Option<Arc<ThrottleLogStore>>,
    /// 失败日志存储（可选）
    pub failure_log_store: Option<Arc<FailureLogStore>>,
    /// 对话页会话存储（可选）
    pub chat_session_store: Option<Arc<ChatSessionStore>>,
    /// 日志捕获器（可选）
    pub log_capture: Option<Arc<LogCapture>>,
    /// 配置文件路径（用于持久化修改）
    pub config_path: Option<PathBuf>,
    /// IP 归属地解析器（可选）
    pub geo_resolver: Option<Arc<GeoResolver>>,
    /// Suggestion Mode 输入建议请求放行开关（与 anthropic AppState 共享同一 Arc 实例）
    pub suggestion_mode: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// config.json 持久化互斥锁：所有 persist_* 写回共享同一把锁，
    /// 防止并发 PUT（如同时改密钥与开关）的读改写互相覆盖导致字段丢失
    pub persist_lock: Arc<parking_lot::Mutex<()>>,
}

impl AdminState {
    pub fn new(admin_psw: Arc<RwLock<String>>, service: AdminService) -> Self {
        Self {
            admin_psw,
            service: Arc::new(service),
            api_key_manager: None,
            usage_tracker: None,
            rpm_tracker: None,
            throttle_log_store: None,
            failure_log_store: None,
            chat_session_store: None,
            log_capture: None,
            config_path: None,
            geo_resolver: None,
            suggestion_mode: None,
            persist_lock: Arc::new(parking_lot::Mutex::new(())),
        }
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

    pub fn with_throttle_log_store(mut self, store: Arc<ThrottleLogStore>) -> Self {
        self.throttle_log_store = Some(store);
        self
    }

    pub fn with_failure_log_store(mut self, store: Arc<FailureLogStore>) -> Self {
        self.failure_log_store = Some(store);
        self
    }

    pub fn with_config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    pub fn with_chat_session_store(mut self, store: Arc<ChatSessionStore>) -> Self {
        self.chat_session_store = Some(store);
        self
    }

    pub fn with_log_capture(mut self, capture: Arc<LogCapture>) -> Self {
        self.log_capture = Some(capture);
        self
    }

    pub fn with_geo_resolver(mut self, resolver: Arc<GeoResolver>) -> Self {
        self.geo_resolver = Some(resolver);
        self
    }

    pub fn with_suggestion_mode(mut self, flag: Arc<std::sync::atomic::AtomicBool>) -> Self {
        self.suggestion_mode = Some(flag);
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
        Some(key) if auth::constant_time_eq(&key, &state.admin_psw.read()) => {
            next.run(request).await
        }
        _ => {
            let error = AdminErrorResponse::authentication_error();
            (StatusCode::UNAUTHORIZED, Json(error)).into_response()
        }
    }
}
