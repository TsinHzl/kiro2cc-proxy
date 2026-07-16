// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin API 路由配置

use axum::{
    Router, middleware,
    routing::{delete, get, post, put},
};

use super::{
    api_keys::{
        create_api_key, delete_api_key, get_admin_models, get_all_usage,
        get_credential_today_summary, get_credential_usage_records, get_daily_usage,
        get_daily_usage_records, get_failure_logs, get_key_usage, get_key_usage_records, get_rpm,
        get_server_info, get_throttle_logs, list_api_keys, reset_key_usage, update_api_key,
    },
    handlers::{
        add_credential, delete_credential, get_all_credentials, get_auth_keys,
        get_credential_balance, get_load_balancing_mode, reset_failure_count, set_auth_keys,
        set_credential_disabled, set_credential_priority, set_load_balancing_mode,
        update_credential,
    },
    log_handler::{download_logs, snapshot_logs, stream_logs},
    middleware::{AdminState, admin_auth_middleware},
};

/// 创建 Admin API 路由
pub fn create_admin_router(state: AdminState) -> Router {
    // 受 header 认证中间件保护的路由
    let protected = Router::new()
        .route(
            "/credentials",
            get(get_all_credentials).post(add_credential),
        )
        .route(
            "/credentials/{id}",
            delete(delete_credential).put(update_credential),
        )
        .route("/credentials/{id}/disabled", post(set_credential_disabled))
        .route("/credentials/{id}/priority", post(set_credential_priority))
        .route("/credentials/{id}/reset", post(reset_failure_count))
        .route("/credentials/{id}/balance", get(get_credential_balance))
        .route(
            "/credentials/{id}/usage/records",
            get(get_credential_usage_records),
        )
        .route(
            "/credentials/{id}/usage/today",
            get(get_credential_today_summary),
        )
        .route("/credentials/{id}/throttle-logs", get(get_throttle_logs))
        .route("/credentials/{id}/failure-logs", get(get_failure_logs))
        .route(
            "/config/load-balancing",
            get(get_load_balancing_mode).put(set_load_balancing_mode),
        )
        .route("/config/auth-keys", get(get_auth_keys).put(set_auth_keys))
        .route("/server-info", get(get_server_info))
        .route("/api-keys", get(list_api_keys).post(create_api_key))
        .route("/api-keys/usage", get(get_all_usage))
        .route("/api-keys/{id}", put(update_api_key).delete(delete_api_key))
        .route(
            "/api-keys/{id}/usage",
            get(get_key_usage).delete(reset_key_usage),
        )
        .route("/api-keys/{id}/usage/records", get(get_key_usage_records))
        .route("/models", get(get_admin_models))
        .route("/rpm", get(get_rpm))
        .route("/usage/daily", get(get_daily_usage))
        .route("/usage/daily/{date}/records", get(get_daily_usage_records))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ))
        .with_state(state.clone());

    // 日志路由使用 Query Param 内联认证（EventSource API 不支持自定义 Header）
    let log_routes = Router::new()
        .route("/logs/stream", get(stream_logs))
        .route("/logs/snapshot", get(snapshot_logs))
        .route("/logs/download", get(download_logs))
        .with_state(state);

    protected.merge(log_routes)
}
