// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin 路由装配：create_admin_router 将凭据、API Key、模型、日志、负载均衡等端点挂载到 /api/admin。

use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{delete, get, post, put},
};

use super::{
    api_keys::{
        create_api_key, delete_api_key, get_admin_models, get_all_usage,
        get_credential_7days_summary, get_credential_30days_summary, get_credential_models,
        get_credential_today_summary, get_credential_usage_records, get_credential_usage_summary,
        get_daily_usage, get_daily_usage_records, get_failure_logs, get_key_usage,
        get_key_usage_records, get_rpm, get_server_info, get_throttle_logs, get_usage_records,
        list_api_keys, reset_key_usage, update_api_key,
    },
    changelog::get_changelog,
    chat_session_handler::{
        clear_chat_sessions, create_chat_session, delete_chat_session, get_chat_session,
        list_chat_sessions, update_chat_session,
    },
    dump_handler::{download_dump_archive, download_dump_file, list_dump_files_handler},
    handlers::{
        add_credential, delete_credential, get_all_credentials, get_auth_keys,
        get_client_token_passthrough, get_credential_balance, get_credential_keep_alive,
        get_debug_logs, get_dump_kiro_body, get_geo_batch, get_load_balancing_mode,
        get_runtime_config, get_show_session_credits, get_suggestion_mode, get_thinking_as_text,
        reset_failure_count, set_auth_keys, set_client_token_passthrough, set_credential_disabled,
        set_credential_keep_alive, set_credential_priority, set_debug_logs, set_dump_kiro_body,
        set_load_balancing_mode, set_runtime_config, set_show_session_credits, set_suggestion_mode,
        set_thinking_as_text, update_credential,
    },
    log_handler::{download_logs, snapshot_logs, stream_logs},
    middleware::{AdminState, admin_auth_middleware},
};

/// 对话会话接口的请求体上限（32MB）
const CHAT_SESSION_BODY_LIMIT: usize = 32 * 1024 * 1024;

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
        .route("/credentials/{id}/models", get(get_credential_models))
        .route(
            "/credentials/{id}/usage/records",
            get(get_credential_usage_records),
        )
        .route(
            "/credentials/{id}/usage/today",
            get(get_credential_today_summary),
        )
        .route(
            "/credentials/{id}/usage/7days",
            get(get_credential_7days_summary),
        )
        .route(
            "/credentials/{id}/usage/30days",
            get(get_credential_30days_summary),
        )
        .route(
            "/credentials/{id}/usage/summary",
            get(get_credential_usage_summary),
        )
        .route("/credentials/{id}/throttle-logs", get(get_throttle_logs))
        .route("/credentials/{id}/failure-logs", get(get_failure_logs))
        .route(
            "/config/load-balancing",
            get(get_load_balancing_mode).put(set_load_balancing_mode),
        )
        .route("/config/auth-keys", get(get_auth_keys).put(set_auth_keys))
        .route(
            "/config/suggestion-mode",
            get(get_suggestion_mode).put(set_suggestion_mode),
        )
        .route(
            "/config/client-token-passthrough",
            get(get_client_token_passthrough).put(set_client_token_passthrough),
        )
        .route(
            "/config/thinking-as-text",
            get(get_thinking_as_text).put(set_thinking_as_text),
        )
        .route(
            "/config/show-session-credits",
            get(get_show_session_credits).put(set_show_session_credits),
        )
        .route(
            "/config/credential-keep-alive",
            get(get_credential_keep_alive).put(set_credential_keep_alive),
        )
        .route(
            "/config/dump-kiro-body",
            get(get_dump_kiro_body).put(set_dump_kiro_body),
        )
        .route(
            "/config/debug-logs",
            get(get_debug_logs).put(set_debug_logs),
        )
        .route(
            "/config/runtime",
            get(get_runtime_config).put(set_runtime_config),
        )
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
        .route("/changelog", get(get_changelog))
        .route("/rpm", get(get_rpm))
        .route("/usage/daily", get(get_daily_usage))
        .route("/usage/daily/{date}/records", get(get_daily_usage_records))
        .route("/usage/records", get(get_usage_records))
        .route("/geo/batch", get(get_geo_batch))
        // dump 落盘文件的浏览与下载（列表 / 单文件 / 全量 ZIP）
        .route("/dump-kiro-body/files", get(list_dump_files_handler))
        .route("/dump-kiro-body/files/{name}", get(download_dump_file))
        .route("/dump-kiro-body/archive", get(download_dump_archive))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ))
        .with_state(state.clone());

    // 日志路由使用内联认证：Query Param（EventSource 不支持自定义 Header）或 x-api-key / Bearer 头
    let log_routes = Router::new()
        .route("/logs/stream", get(stream_logs))
        .route("/logs/snapshot", get(snapshot_logs))
        .route("/logs/download", get(download_logs))
        .with_state(state.clone());

    // 对话会话：消息正文（含思考内容）可能较大，单独放宽请求体上限（axum 默认 2MB）
    let chat_routes = Router::new()
        .route(
            "/chat/sessions",
            get(list_chat_sessions)
                .post(create_chat_session)
                .delete(clear_chat_sessions),
        )
        .route(
            "/chat/sessions/{id}",
            get(get_chat_session)
                .put(update_chat_session)
                .delete(delete_chat_session),
        )
        .layer(DefaultBodyLimit::max(CHAT_SESSION_BODY_LIMIT))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ))
        .with_state(state.clone());

    protected.merge(chat_routes).merge(log_routes)
}
