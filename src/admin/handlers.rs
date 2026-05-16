//! Admin API HTTP 处理器

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
/// 获取所有凭据状态
pub async fn get_all_credentials(State(state): State<AdminState>) -> impl IntoResponse {
    let response = state.service.get_all_credentials();
    Json(response)
}

/// POST /api/admin/credentials/:id/disabled
/// 设置凭据禁用状态
pub async fn set_credential_disabled(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetDisabledRequest>,
) -> impl IntoResponse {
    match state.service.set_disabled(id, payload.disabled) {
        Ok(_) => {
            let action = if payload.disabled { "禁用" } else { "启用" };
            Json(SuccessResponse::new(format!("凭据 #{} 已{}", id, action))).into_response()
        }
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// POST /api/admin/credentials/:id/priority
/// 设置凭据优先级
pub async fn set_credential_priority(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<SetPriorityRequest>,
) -> impl IntoResponse {
    match state.service.set_priority(id, payload.priority) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "凭据 #{} 优先级已设置为 {}",
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
    match state.service.reset_and_enable(id) {
        Ok(_) => Json(SuccessResponse::new(format!(
            "凭据 #{} 失败计数已重置并重新启用",
            id
        )))
        .into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// GET /api/admin/credentials/:id/balance
/// 获取指定凭据的余额
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
/// 添加新凭据
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
/// 删除凭据
pub async fn delete_credential(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.delete_credential(id) {
        Ok(_) => Json(SuccessResponse::new(format!("凭据 #{} 已删除", id))).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
}

/// PUT /api/admin/credentials/:id
/// 更新凭据配置
pub async fn update_credential(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdateCredentialRequest>,
) -> impl IntoResponse {
    match state.service.update_credential(id, payload).await {
        Ok(_) => Json(SuccessResponse::new(format!("凭据 #{} 已更新", id))).into_response(),
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
    let visible = key.len() / 2;
    format!("{}***", &key[..visible])
}

/// GET /api/admin/config/auth-keys
/// 获取当前认证密钥（脱敏显示）
pub async fn get_auth_keys(State(state): State<AdminState>) -> impl IntoResponse {
    let api_key = state
        .master_api_key
        .as_ref()
        .map(|k| mask_key(&k.read()))
        .unwrap_or_default();
    let admin_api_key = mask_key(&state.admin_api_key.read());

    Json(super::types::AuthKeysResponse {
        api_key,
        admin_api_key,
    })
}

/// PUT /api/admin/config/auth-keys
/// 修改认证密钥（运行时生效并持久化到 config.json）
pub async fn set_auth_keys(
    State(state): State<AdminState>,
    Json(payload): Json<super::types::SetAuthKeysRequest>,
) -> impl IntoResponse {
    // 验证输入
    if let Some(ref key) = payload.api_key {
        if key.trim().is_empty() {
            let error = super::types::AdminErrorResponse::invalid_request("apiKey 不能为空");
            return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!(error))).into_response();
        }
    }
    if let Some(ref key) = payload.admin_api_key {
        if key.trim().is_empty() {
            let error = super::types::AdminErrorResponse::invalid_request("adminApiKey 不能为空");
            return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!(error))).into_response();
        }
    }

    // 更新运行时值
    if let Some(ref new_api_key) = payload.api_key {
        if let Some(ref master_key) = state.master_api_key {
            *master_key.write() = new_api_key.clone();
        }
    }
    if let Some(ref new_admin_key) = payload.admin_api_key {
        *state.admin_api_key.write() = new_admin_key.clone();
    }

    // 持久化到 config.json
    if let Some(ref config_path) = state.config_path {
        if let Err(e) = persist_auth_keys(config_path, &payload.api_key, &payload.admin_api_key) {
            tracing::error!("持久化认证密钥失败: {}", e);
            let error = super::types::AdminErrorResponse::internal_error("持久化失败，但运行时已生效");
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!(error))).into_response();
        }
    }

    Json(SuccessResponse::new("认证密钥已更新")).into_response()
}

/// 将修改后的密钥写回 config.json
fn persist_auth_keys(
    config_path: &std::path::Path,
    new_api_key: &Option<String>,
    new_admin_api_key: &Option<String>,
) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(config_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)?;

    if let Some(key) = new_api_key {
        json["apiKey"] = serde_json::Value::String(key.clone());
    }
    if let Some(key) = new_admin_api_key {
        json["adminApiKey"] = serde_json::Value::String(key.clone());
    }

    let output = serde_json::to_string_pretty(&json)?;
    std::fs::write(config_path, output)?;
    Ok(())
}
