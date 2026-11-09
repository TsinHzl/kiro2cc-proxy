// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin 凭据管理端点：凭据列表/启停/优先级调整/重置启用/余额查询等 REST 处理函数。

use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};

use super::{
    middleware::AdminState,
    types::{
        AddCredentialRequest, SetDisabledRequest, SetLoadBalancingModeRequest, SetPriorityRequest,
        SuccessResponse, UpdateCredentialRequest,
    },
};

/// GET /api/admin/credentials
/// 获取所有账号状态
pub async fn get_all_credentials(State(state): State<AdminState>) -> impl IntoResponse {
    let response = state.service.get_all_credentials();
    Json(response)
}

/// POST /api/admin/credentials/:id/disabled
/// 设置账号禁用状态
pub async fn set_credential_disabled(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetDisabledRequest>,
) -> impl IntoResponse {
    match state.service.set_disabled(id, payload.disabled).await {
        Ok(_) => {
            let action = if payload.disabled { "禁用" } else { "启用" };
            Json(SuccessResponse::new(format!("账号 #{} 已{}", id, action))).into_response()
        }
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/priority
/// 设置账号优先级
pub async fn set_credential_priority(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetPriorityRequest>,
) -> impl IntoResponse {
    match state.service.set_priority(id, payload.priority) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "账号 #{} 优先级已设置为 {}",
            id, payload.priority
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/reset
/// 重置失败计数并重新启用
pub async fn reset_failure_count(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.reset_and_enable(id).await {
        Ok(_) => Json(SuccessResponse::new(format!(
            "账号 #{} 失败计数已重置并重新启用",
            id
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/credentials/:id/balance
/// 获取指定账号的余额
pub async fn get_credential_balance(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.get_balance(id).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials
/// 添加新账号
pub async fn add_credential(
    State(state): State<AdminState>,
    Json(payload): Json<AddCredentialRequest>,
) -> impl IntoResponse {
    match state.service.add_credential(payload).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// DELETE /api/admin/credentials/:id
/// 删除账号
pub async fn delete_credential(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.delete_credential(id).await {
        Ok(_) => Json(SuccessResponse::new(format!("账号 #{} 已删除", id))).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// PUT /api/admin/credentials/:id
/// 更新账号配置
pub async fn update_credential(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdateCredentialRequest>,
) -> impl IntoResponse {
    match state.service.update_credential(id, payload).await {
        Ok(_) => Json(SuccessResponse::new(format!("账号 #{} 已更新", id))).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/config/load-balancing
/// 获取负载均衡模式
pub async fn get_load_balancing_mode(State(state): State<AdminState>) -> impl IntoResponse {
    let response = state.service.get_load_balancing_mode();
    Json(response)
}

/// PUT /api/admin/config/load-balancing
/// 设置负载均衡模式
pub async fn set_load_balancing_mode(
    State(state): State<AdminState>,
    Json(payload): Json<SetLoadBalancingModeRequest>,
) -> impl IntoResponse {
    match state.service.set_load_balancing_mode(payload) {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// 将 API Key 脱敏显示（保留前半部分 + ***）
fn mask_key(key: &str) -> String {
    let visible = key.chars().count() / 2;
    let masked: String = key.chars().take(visible).collect();
    format!("{}***", masked)
}

/// GET /api/admin/config/auth-keys
/// 获取当前认证密钥（脱敏显示）
pub async fn get_auth_keys(State(state): State<AdminState>) -> impl IntoResponse {
    let admin_psw = mask_key(&state.admin_psw.read());

    Json(super::types::AuthKeysResponse { admin_psw })
}

/// PUT /api/admin/config/auth-keys
/// 修改认证密钥（运行时生效并持久化到 config.json）
pub async fn set_auth_keys(
    State(state): State<AdminState>,
    Json(payload): Json<super::types::SetAuthKeysRequest>,
) -> impl IntoResponse {
    // 验证输入
    if let Some(ref key) = payload.admin_psw
        && key.trim().is_empty()
    {
        let error = super::types::AdminErrorResponse::invalid_request(
            "adminPsw 不能为空（Admin Password）",
        );
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!(error)),
        )
            .into_response();
    }

    // 更新运行时值
    if let Some(ref new_admin_psw) = payload.admin_psw {
        *state.admin_psw.write() = new_admin_psw.clone();
    }

    // 持久化到 config.json（persist_lock 防止与其他 PUT 并发覆盖）
    let _guard = state.persist_lock.lock();
    if let Some(ref config_path) = state.config_path
        && let Err(e) = persist_auth_keys(config_path, &payload.admin_psw)
    {
        tracing::error!("持久化认证密钥失败: {}", e);
        let error = super::types::AdminErrorResponse::internal_error("持久化失败，但运行时已生效");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!(error)),
        )
            .into_response();
    }

    Json(SuccessResponse::new("认证密钥已更新")).into_response()
}

/// GET /api/admin/config/suggestion-mode
/// 获取 Suggestion Mode 输入建议请求放行开关
pub async fn get_suggestion_mode(State(state): State<AdminState>) -> impl IntoResponse {
    let enabled = state
        .suggestion_mode
        .as_ref()
        .map(|f| f.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(false);
    Json(super::types::SuggestionModeResponse { enabled })
}

/// PUT /api/admin/config/suggestion-mode
/// 设置 Suggestion Mode 输入建议请求放行开关（运行时热切换并持久化到 config.json）
pub async fn set_suggestion_mode(
    State(state): State<AdminState>,
    Json(payload): Json<super::types::SetSuggestionModeRequest>,
) -> impl IntoResponse {
    let Some(flag) = &state.suggestion_mode else {
        let error = super::types::AdminErrorResponse::internal_error("Suggestion Mode 开关未启用");
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!(error)),
        )
            .into_response();
    };

    // 持久化到 config.json（重启后保持；persist_lock 防止与其他 PUT 并发覆盖）。
    // 热更新在锁内执行：保证「内存顺序与磁盘顺序一致」，避免并发请求出现
    // 内存为后值、磁盘为先值 → 重启后设置回退。
    let _guard = state.persist_lock.lock();
    flag.store(payload.enabled, std::sync::atomic::Ordering::Relaxed);
    if let Some(ref config_path) = state.config_path
        && let Err(e) = persist_suggestion_mode(config_path, payload.enabled)
    {
        tracing::error!("持久化 Suggestion Mode 开关失败: {}", e);
        let error = super::types::AdminErrorResponse::internal_error("持久化失败，但运行时已生效");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!(error)),
        )
            .into_response();
    }

    Json(SuccessResponse::new(if payload.enabled {
        "Suggestion Mode 已放行"
    } else {
        "Suggestion Mode 已拦截"
    }))
    .into_response()
}

/// GET /api/admin/config/client-token-passthrough
/// 获取客户端 token 直通开关
pub async fn get_client_token_passthrough() -> impl IntoResponse {
    Json(super::types::ClientTokenPassthroughResponse {
        enabled: crate::anthropic::client_token_passthrough_enabled(),
    })
}

/// PUT /api/admin/config/client-token-passthrough
/// 设置客户端 token 直通开关（运行时热切换并持久化到 config.json）
pub async fn set_client_token_passthrough(
    State(state): State<AdminState>,
    Json(payload): Json<super::types::SetClientTokenPassthroughRequest>,
) -> impl IntoResponse {
    // 持久化到 config.json（重启后保持；persist_lock 防止与其他 PUT 并发覆盖）。
    // 热更新在锁内执行，保证内存与磁盘写入顺序一致（见 set_suggestion_mode 注释）。
    let _guard = state.persist_lock.lock();
    crate::anthropic::set_client_token_passthrough(payload.enabled);
    if let Some(ref config_path) = state.config_path
        && let Err(e) = persist_client_token_passthrough(config_path, payload.enabled)
    {
        tracing::error!("持久化客户端 token 直通开关失败: {}", e);
        let error = super::types::AdminErrorResponse::internal_error("持久化失败，但运行时已生效");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!(error)),
        )
            .into_response();
    }

    Json(SuccessResponse::new(if payload.enabled {
        "客户端 token 直通已开启"
    } else {
        "客户端 token 直通已关闭"
    }))
    .into_response()
}

/// GET /api/admin/config/thinking-as-text
/// 获取思考文本化开关
pub async fn get_thinking_as_text() -> impl IntoResponse {
    Json(super::types::ThinkingAsTextResponse {
        enabled: crate::anthropic::thinking_as_text_enabled(),
    })
}

/// PUT /api/admin/config/thinking-as-text
/// 设置思考文本化开关（运行时热切换并持久化到 config.json；仅影响之后的新请求）
pub async fn set_thinking_as_text(
    State(state): State<AdminState>,
    Json(payload): Json<super::types::SetThinkingAsTextRequest>,
) -> impl IntoResponse {
    // 热更新在锁内执行，保证内存与磁盘写入顺序一致（见 set_suggestion_mode 注释）
    let _guard = state.persist_lock.lock();
    crate::anthropic::set_thinking_as_text(payload.enabled);
    if let Some(ref config_path) = state.config_path
        && let Err(e) = persist_thinking_as_text(config_path, payload.enabled)
    {
        tracing::error!("持久化思考文本化开关失败: {}", e);
        let error = super::types::AdminErrorResponse::internal_error("持久化失败，但运行时已生效");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!(error)),
        )
            .into_response();
    }

    Json(SuccessResponse::new(if payload.enabled {
        "思考文本化已开启"
    } else {
        "思考文本化已关闭"
    }))
    .into_response()
}

/// 将思考文本化开关写回 config.json
fn persist_thinking_as_text(config_path: &std::path::Path, enabled: bool) -> anyhow::Result<()> {
    persist_config_field(config_path, |json| {
        json["thinkingAsText"] = serde_json::Value::Bool(enabled);
    })
}

/// GET /api/admin/config/show-session-credits
/// 获取会话 credits 汇总行开关
pub async fn get_show_session_credits() -> impl IntoResponse {
    Json(super::types::ShowSessionCreditsResponse {
        enabled: crate::anthropic::show_session_credits_enabled(),
    })
}

/// PUT /api/admin/config/show-session-credits
/// 设置会话 credits 汇总行开关（运行时热切换并持久化到 config.json；仅影响之后的新请求）
pub async fn set_show_session_credits(
    State(state): State<AdminState>,
    Json(payload): Json<super::types::SetShowSessionCreditsRequest>,
) -> impl IntoResponse {
    // 热更新在锁内执行，保证内存与磁盘写入顺序一致（见 set_suggestion_mode 注释）
    let _guard = state.persist_lock.lock();
    crate::anthropic::set_show_session_credits(payload.enabled);
    if let Some(ref config_path) = state.config_path
        && let Err(e) = persist_show_session_credits(config_path, payload.enabled)
    {
        tracing::error!("持久化会话 credits 汇总行开关失败: {}", e);
        let error = super::types::AdminErrorResponse::internal_error("持久化失败，但运行时已生效");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!(error)),
        )
            .into_response();
    }

    Json(SuccessResponse::new(if payload.enabled {
        "会话 credits 汇总行已开启"
    } else {
        "会话 credits 汇总行已关闭"
    }))
    .into_response()
}

/// 将会话 credits 汇总行开关写回 config.json
fn persist_show_session_credits(
    config_path: &std::path::Path,
    enabled: bool,
) -> anyhow::Result<()> {
    persist_config_field(config_path, |json| {
        json["showSessionCredits"] = serde_json::Value::Bool(enabled);
    })
}

/// GET /api/admin/config/dump-kiro-body
/// 获取 Kiro 请求体落盘取证开关
pub async fn get_dump_kiro_body() -> impl IntoResponse {
    Json(super::types::DumpKiroBodyResponse {
        enabled: crate::anthropic::kiro_body_dump_enabled(),
    })
}

/// PUT /api/admin/config/dump-kiro-body
/// 设置 Kiro 请求体落盘取证开关（运行时热切换并持久化到 config.json；仅影响之后的新请求）
pub async fn set_dump_kiro_body(
    State(state): State<AdminState>,
    Json(payload): Json<super::types::SetDumpKiroBodyRequest>,
) -> impl IntoResponse {
    // 热更新在锁内执行，保证内存与磁盘写入顺序一致（见 set_suggestion_mode 注释）
    let _guard = state.persist_lock.lock();
    crate::anthropic::set_kiro_body_dump_enabled(payload.enabled);
    if payload.enabled {
        // 与 main.rs 启动时开启的告警保持一致：落盘含完整请求内容（可能含敏感信息），定位后应关闭
        tracing::warn!(
            "已开启 Kiro 请求体落盘取证（热切换）：请求体将写入 app/config/kiro-body-dump/，包含完整对话内容与凭据信息，定位问题后请及时关闭"
        );
    }
    if let Some(ref config_path) = state.config_path
        && let Err(e) = persist_dump_kiro_body(config_path, payload.enabled)
    {
        tracing::error!("持久化 Kiro 请求体落盘开关失败: {}", e);
        // 显式区分运行时生效与持久化结果，避免客户端仅凭 500 误判开关未生效
        let error = serde_json::json!({
            "success": false,
            "message": "持久化失败，但运行时已生效",
            "runtimeEnabled": payload.enabled,
            "persisted": false,
        });
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response();
    }

    Json(SuccessResponse::new(if payload.enabled {
        "Kiro 请求体落盘已开启"
    } else {
        "Kiro 请求体落盘已关闭"
    }))
    .into_response()
}

/// 将 Kiro 请求体落盘取证开关写回 config.json
fn persist_dump_kiro_body(config_path: &std::path::Path, enabled: bool) -> anyhow::Result<()> {
    persist_config_field(config_path, |json| {
        json["dumpKiroBody"] = serde_json::Value::Bool(enabled);
    })
}

/// 将 Suggestion Mode 开关写回 config.json
fn persist_suggestion_mode(config_path: &std::path::Path, enabled: bool) -> anyhow::Result<()> {
    persist_config_field(config_path, |json| {
        json["forwardSuggestionMode"] = serde_json::Value::Bool(enabled);
    })
}

/// GET /api/admin/config/debug-logs
/// 获取 DEBUG 日志开关
pub async fn get_debug_logs() -> impl IntoResponse {
    Json(super::types::DebugLogsResponse {
        enabled: crate::logging::debug_logs_enabled(),
    })
}

/// PUT /api/admin/config/debug-logs
/// 设置 DEBUG 日志开关（全局过滤器热切换并持久化到 config.json）
pub async fn set_debug_logs(
    State(state): State<AdminState>,
    Json(payload): Json<super::types::SetDebugLogsRequest>,
) -> impl IntoResponse {
    // 热更新在锁内执行，保证内存与磁盘写入顺序一致（见 set_suggestion_mode 注释）
    let _guard = state.persist_lock.lock();
    if let Err(msg) = crate::logging::set_debug_enabled(payload.enabled) {
        tracing::error!("热切换 DEBUG 日志开关失败: {}", msg);
        let error = super::types::AdminErrorResponse::internal_error(&msg);
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response();
    }
    if let Some(ref config_path) = state.config_path
        && let Err(e) = persist_debug_logs(config_path, payload.enabled)
    {
        tracing::error!("持久化 DEBUG 日志开关失败: {}", e);
        let error = super::types::AdminErrorResponse::internal_error("持久化失败，但运行时已生效");
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response();
    }

    Json(SuccessResponse::new(if payload.enabled {
        "DEBUG 日志已开启（热生效，控制台 / 日志文件 / 日志页一致）"
    } else {
        "DEBUG 日志已关闭（热生效，已捕获的 DEBUG 记录保留）"
    }))
    .into_response()
}

/// 将 DEBUG 日志开关写回 config.json
fn persist_debug_logs(config_path: &std::path::Path, enabled: bool) -> anyhow::Result<()> {
    persist_config_field(config_path, |json| {
        json["debugLogs"] = serde_json::Value::Bool(enabled);
    })
}

/// GET /api/admin/config/runtime
/// 获取运行时配置（maxRpm / port / proxyUrl / writeChunkingMaxLines）
pub async fn get_runtime_config(State(state): State<AdminState>) -> impl IntoResponse {
    let max_rpm = state.service.max_rpm_per_credential();

    // port / proxyUrl 无运行时热值概念，读取 config.json 持久化值；
    // 文件缺失或字段缺失时回退默认值（与 Config serde default 一致）
    let (mut port, mut proxy_url) = (0u16, None);
    if let Some(ref config_path) = state.config_path
        && let Ok(content) = std::fs::read_to_string(config_path)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
    {
        port = json["port"].as_u64().unwrap_or(8080) as u16;
        proxy_url = json["proxyUrl"].as_str().map(str::to_string);
    }
    if port == 0 {
        port = 8080;
    }

    Json(super::types::RuntimeConfigResponse {
        max_rpm_per_credential: max_rpm,
        port,
        proxy_url,
        write_chunking_max_lines: crate::anthropic::write_chunking_max_lines(),
    })
}

/// 校验上游代理地址：空 = 清除，或 http/https/socks5 URL（须可解析且含主机）
fn validate_proxy_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Ok(());
    }
    let parsed = reqwest::Url::parse(url).map_err(|_| {
        "proxyUrl 必须是有效的 URL（支持 http://、https://、socks5://，空串表示清除）".to_string()
    })?;
    if !matches!(parsed.scheme(), "http" | "https" | "socks5") || parsed.host_str().is_none() {
        return Err("proxyUrl 必须使用 http/https/socks5 协议且包含主机名".to_string());
    }
    Ok(())
}

/// PUT /api/admin/config/runtime
/// 设置运行时配置：maxRpm / writeChunkingMaxLines 热切换 + 各字段写回 config.json（port/proxyUrl 重启后生效）
pub async fn set_runtime_config(
    State(state): State<AdminState>,
    Json(payload): Json<super::types::SetRuntimeConfigRequest>,
) -> impl IntoResponse {
    // 输入校验（先于任何状态修改）
    if let Some(port) = payload.port
        && port == 0
    {
        let error = super::types::AdminErrorResponse::invalid_request("port 必须在 1–65535 范围内");
        return (axum::http::StatusCode::BAD_REQUEST, Json(error)).into_response();
    }
    if let Some(ref url) = payload.proxy_url
        && let Err(msg) = validate_proxy_url(url)
    {
        let error = super::types::AdminErrorResponse::invalid_request(msg);
        return (axum::http::StatusCode::BAD_REQUEST, Json(error)).into_response();
    }

    // 持久化写回 config.json（重启后保持；persist_lock 防止与其他 PUT 并发覆盖）。
    // maxRpm 热切换同样在锁内执行，保证内存与磁盘写入顺序一致
    // （wait_for_rpm_gate 每次请求读取，即时生效）。
    let _guard = state.persist_lock.lock();
    if let Some(max_rpm) = payload.max_rpm_per_credential {
        state.service.set_max_rpm_per_credential(max_rpm);
    }
    if let Some(lines) = payload.write_chunking_max_lines {
        crate::anthropic::set_write_chunking_max_lines(lines);
    }
    if let Some(ref config_path) = state.config_path {
        let result = persist_config_field(config_path, |json| {
            if let Some(max_rpm) = payload.max_rpm_per_credential {
                json["maxRpmPerCredential"] = serde_json::json!(max_rpm);
            }
            if let Some(lines) = payload.write_chunking_max_lines {
                json["writeChunkingMaxLines"] = serde_json::json!(lines);
            }
            if let Some(port) = payload.port {
                json["port"] = serde_json::json!(port);
            }
            if let Some(ref url) = payload.proxy_url {
                if url.is_empty() {
                    if let Some(map) = json.as_object_mut() {
                        map.remove("proxyUrl");
                    }
                } else {
                    json["proxyUrl"] = serde_json::Value::String(url.clone());
                }
            }
        });
        if let Err(e) = result {
            tracing::error!("持久化运行时配置失败: {}", e);
            let error =
                super::types::AdminErrorResponse::internal_error("持久化失败，但运行时已生效");
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response();
        }
    }

    // 响应消息按字段区分生效方式
    let mut parts = Vec::new();
    if payload.max_rpm_per_credential.is_some() {
        parts.push("maxRpmPerCredential 已热生效");
    }
    if payload.write_chunking_max_lines.is_some() {
        parts.push("writeChunkingMaxLines 已热生效（仅影响之后的新请求）");
    }
    if payload.port.is_some() {
        parts.push("port 已保存（重启后生效）");
    }
    if payload.proxy_url.is_some() {
        parts.push("proxyUrl 已保存（重启后生效）");
    }
    Json(SuccessResponse::new(parts.join("，"))).into_response()
}

/// 将客户端 token 直通开关写回 config.json
fn persist_client_token_passthrough(
    config_path: &std::path::Path,
    enabled: bool,
) -> anyhow::Result<()> {
    persist_config_field(config_path, |json| {
        json["clientTokenPassthrough"] = serde_json::Value::Bool(enabled);
    })
}

/// config.json 通用读改写：修改字段后经临时文件 + rename 原子替换，
/// 避免进程在写入中途崩溃产生半截文件。调用方须先持有 `AdminState.persist_lock`。
fn persist_config_field(
    config_path: &std::path::Path,
    mutate: impl FnOnce(&mut serde_json::Value),
) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(config_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)?;
    mutate(&mut json);

    let output = serde_json::to_string_pretty(&json)?;
    let tmp_path = config_path.with_extension("json.tmp");

    // 临时文件创建即限制权限并继承原文件权限：默认 write() 受 umask 影响，
    // 0600 的 config.json（含 adminPsw）经替换后会扩大为 0644，泄露密码可读范围
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mode = std::fs::metadata(config_path)
            .map(|m| m.permissions().mode())
            .unwrap_or(0o600);
        // 崩溃可能残留 .tmp 文件，create_new 会永久失败，先清理
        let _ = std::fs::remove_file(&tmp_path);
        let mut tmp = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode & 0o777)
            .open(&tmp_path)?;
        tmp.write_all(output.as_bytes())?;
        tmp.sync_all().ok();
    }
    #[cfg(not(unix))]
    std::fs::write(&tmp_path, &output)?;

    if let Err(e) = std::fs::rename(&tmp_path, config_path) {
        let _ = std::fs::remove_file(&tmp_path);
        // config.json 是单文件挂载点（Docker bind mount / 平台配置文件挂载）时，
        // rename 覆盖会返回 EBUSY（Resource busy, os error 16）。降级为原地写入：
        // 非原子，但保留 inode 与权限，且比持久化失败更可取。
        if crate::common::fs::is_replace_blocked(&e) {
            tracing::warn!(
                "rename 覆盖 {} 失败（{}），降级为原地写入",
                config_path.display(),
                e
            );
            crate::common::fs::write_in_place(config_path, output.as_bytes())?;
            return Ok(());
        }
        return Err(e.into());
    }
    Ok(())
}

/// 将修改后的密钥写回 config.json
fn persist_auth_keys(
    config_path: &std::path::Path,
    new_admin_psw: &Option<String>,
) -> anyhow::Result<()> {
    persist_config_field(config_path, |json| {
        if let Some(key) = new_admin_psw {
            json["adminPsw"] = serde_json::Value::String(key.clone());
            if let Some(map) = json.as_object_mut() {
                map.remove("adminApiKey");
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个测试用独立子目录，避免并行测试共享同一 temp 目录互删
    fn temp_config_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("kiro-admin-test-{}-{}", tag, std::process::id()))
    }

    #[test]
    fn test_persist_suggestion_mode_roundtrip() {
        let dir = temp_config_dir("sugg");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"host":"127.0.0.1","port":8080}"#).unwrap();

        persist_suggestion_mode(&path, true).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["forwardSuggestionMode"], serde_json::Value::Bool(true));
        // 原有字段不被破坏
        assert_eq!(json["host"], "127.0.0.1");

        persist_suggestion_mode(&path, false).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            json["forwardSuggestionMode"],
            serde_json::Value::Bool(false)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_persist_client_token_passthrough_roundtrip() {
        let dir = temp_config_dir("ctp");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"host":"127.0.0.1"}"#).unwrap();

        persist_client_token_passthrough(&path, true).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            json["clientTokenPassthrough"],
            serde_json::Value::Bool(true)
        );
        assert_eq!(json["host"], "127.0.0.1");

        persist_client_token_passthrough(&path, false).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            json["clientTokenPassthrough"],
            serde_json::Value::Bool(false)
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_persist_dump_kiro_body_roundtrip() {
        let dir = temp_config_dir("dkb");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"host":"127.0.0.1"}"#).unwrap();

        persist_dump_kiro_body(&path, true).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["dumpKiroBody"], serde_json::Value::Bool(true));
        assert_eq!(json["host"], "127.0.0.1");

        persist_dump_kiro_body(&path, false).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["dumpKiroBody"], serde_json::Value::Bool(false));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_persist_show_session_credits_roundtrip() {
        let dir = temp_config_dir("ssc");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"host":"127.0.0.1"}"#).unwrap();

        persist_show_session_credits(&path, true).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["showSessionCredits"], serde_json::Value::Bool(true));
        assert_eq!(json["host"], "127.0.0.1");

        persist_show_session_credits(&path, false).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["showSessionCredits"], serde_json::Value::Bool(false));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_persist_thinking_as_text_roundtrip() {
        let dir = temp_config_dir("tat");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"host":"127.0.0.1"}"#).unwrap();

        persist_thinking_as_text(&path, true).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["thinkingAsText"], serde_json::Value::Bool(true));
        assert_eq!(json["host"], "127.0.0.1");

        persist_thinking_as_text(&path, false).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["thinkingAsText"], serde_json::Value::Bool(false));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_persist_debug_logs_roundtrip() {
        let dir = temp_config_dir("dbg");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"host":"127.0.0.1"}"#).unwrap();

        persist_debug_logs(&path, true).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["debugLogs"], serde_json::Value::Bool(true));
        assert_eq!(json["host"], "127.0.0.1");

        persist_debug_logs(&path, false).unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["debugLogs"], serde_json::Value::Bool(false));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_persist_replaces_tmp_file_atomically() {
        let dir = temp_config_dir("tmp");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"a":1}"#).unwrap();

        persist_config_field(&path, |json| {
            json["a"] = serde_json::json!(2);
        })
        .unwrap();

        // 原子替换后临时文件应不存在，目标文件内容为新值
        assert!(!path.with_extension("json.tmp").exists());
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["a"], serde_json::json!(2));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_validate_proxy_url() {
        assert!(validate_proxy_url("").is_ok());
        assert!(validate_proxy_url("http://127.0.0.1:10089").is_ok());
        assert!(validate_proxy_url("https://proxy.example.com").is_ok());
        assert!(validate_proxy_url("socks5://127.0.0.1:1080").is_ok());
        assert!(validate_proxy_url("ftp://x").is_err());
        assert!(validate_proxy_url("127.0.0.1:1080").is_err());
        // 非法端口 / 缺主机 / 空主机均应拒绝（前缀校验无法覆盖的形态）
        assert!(validate_proxy_url("http://proxy.example.com:abc").is_err());
        assert!(validate_proxy_url("http://").is_err());
        assert!(validate_proxy_url("socks5://:1080").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_persist_preserves_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_config_dir("perm");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"a":1}"#).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        persist_config_field(&path, |json| {
            json["a"] = serde_json::json!(2);
        })
        .unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "替换后应保留原文件权限，不得因 umask 扩大");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_persist_runtime_config_roundtrip() {
        let dir = temp_config_dir("runtime");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(
            &path,
            r#"{"host":"127.0.0.1","port":5678,"proxyUrl":"http://127.0.0.1:10089"}"#,
        )
        .unwrap();

        // 修改 maxRpm + port
        persist_config_field(&path, |json| {
            json["maxRpmPerCredential"] = serde_json::json!(16);
            json["port"] = serde_json::json!(9090);
        })
        .unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["maxRpmPerCredential"], serde_json::json!(16));
        assert_eq!(json["port"], serde_json::json!(9090));
        // 未触及的 proxyUrl 保持不变
        assert_eq!(json["proxyUrl"], "http://127.0.0.1:10089");

        // 清除 proxyUrl（空串语义）
        persist_config_field(&path, |json| {
            if let Some(map) = json.as_object_mut() {
                map.remove("proxyUrl");
            }
        })
        .unwrap();
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(json.get("proxyUrl").is_none());
        assert_eq!(json["port"], serde_json::json!(9090));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_get_runtime_config_fallback_defaults() {
        // config.json 缺失字段时 GET 的兜底逻辑与 serde default 一致：
        // port 缺失 → 8080，proxyUrl 缺失 → None
        let json: serde_json::Value = serde_json::from_str(r#"{"host":"127.0.0.1"}"#).unwrap();
        let port = json["port"].as_u64().unwrap_or(8080) as u16;
        let proxy_url = json["proxyUrl"].as_str().map(str::to_string);
        assert_eq!(port, 8080);
        assert_eq!(proxy_url, None);
    }
}

/// 单批次最多允许查询的 IP 数量
const MAX_GEO_BATCH_IPS: usize = 200;

/// GET /api/admin/geo/batch?ips=ip1,ip2,...
/// 批量查询 IP 归属地
pub async fn get_geo_batch(
    State(state): State<AdminState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(resolver) = &state.geo_resolver else {
        let error = super::types::AdminErrorResponse::internal_error("归属地解析未启用");
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let ips: Vec<&str> = params
        .get("ips")
        .map(|v| v.split(',').filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    if ips.len() > MAX_GEO_BATCH_IPS {
        let error = super::types::AdminErrorResponse::invalid_request(format!(
            "单批次最多查询 {MAX_GEO_BATCH_IPS} 个 IP"
        ));
        return (axum::http::StatusCode::BAD_REQUEST, Json(error)).into_response();
    }
    let result: std::collections::HashMap<String, Option<crate::model::geo::GeoInfo>> = ips
        .into_iter()
        .map(|ip| (ip.to_string(), resolver.resolve(ip)))
        .collect();
    Json(result).into_response()
}
