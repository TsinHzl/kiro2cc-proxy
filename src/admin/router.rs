//! Admin API 路由配置

use axum::{
    Router, middleware,
    routing::{delete, get, post, put},
};

use super::{
    api_keys::{
        create_api_key, delete_api_key, get_all_usage, get_key_usage, get_rpm, get_server_info,
        list_api_keys, reset_key_usage, update_api_key,
    },
    handlers::{
        add_credential, delete_credential, get_all_credentials, get_auth_keys,
        get_credential_balance, get_load_balancing_mode, reset_failure_count,
        set_auth_keys, set_credential_disabled, set_credential_priority,
        set_load_balancing_mode, update_credential,
    },
    middleware::{AdminState, admin_auth_middleware},
};

/// 创建 Admin API 路由
pub fn create_admin_router(state: AdminState) -> Router {
    Router::new()
        // 凭据管理
        .route(
            "/credentials",
            get(get_all_credentials).post(add_credential),
        )
        .route("/credentials/{id}", delete(delete_credential).put(update_credential))
        .route("/credentials/{id}/disabled", post(set_credential_disabled))
        .route("/credentials/{id}/priority", post(set_credential_priority))
        .route("/credentials/{id}/reset", post(reset_failure_count))
        .route("/credentials/{id}/balance", get(get_credential_balance))
        .route(
            "/config/load-balancing",
            get(get_load_balancing_mode).put(set_load_balancing_mode),
        )
        .route(
            "/config/auth-keys",
            get(get_auth_keys).put(set_auth_keys),
        )
        // API Key 管理
        .route("/server-info", get(get_server_info))
        .route("/api-keys", get(list_api_keys).post(create_api_key))
        .route("/api-keys/usage", get(get_all_usage))
        .route("/api-keys/{id}", put(update_api_key).delete(delete_api_key))
        .route("/api-keys/{id}/usage", get(get_key_usage).delete(reset_key_usage))
        // RPM 监控
        .route("/rpm", get(get_rpm))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ))
        .with_state(state)
}
