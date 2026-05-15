//! User API 处理器

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use super::middleware::{UserContext, UserErrorResponse, UserState};
use crate::model::api_key::ApiKeyAuthResult;

/// POST /api/user/login
/// 验证 API Key，返回 key 基本信息
pub async fn login(
    State(state): State<UserState>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    match state.api_key_manager.authenticate_readonly(&payload.api_key) {
        ApiKeyAuthResult::Valid { id, name, spending_limit } => {
            // 查询用量
            let summary = state.usage_tracker.get_summary(id);
            // 查询 key 详情（过期时间等）
            let keys = state.api_key_manager.list();
            let key_info = keys.iter().find(|k| k.id == id);

            let response = LoginResponse {
                id,
                name,
                spending_limit,
                total_cost: summary.total_cost,
                expires_at: key_info.and_then(|k| k.expires_at.map(|t| t.to_rfc3339())),
                duration_days: key_info.and_then(|k| k.duration_days),
                activated_at: key_info.and_then(|k| k.activated_at.map(|t| t.to_rfc3339())),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        ApiKeyAuthResult::Disabled => (
            StatusCode::FORBIDDEN,
            Json(UserErrorResponse { error: "API Key 已被禁用".into() }),
        ).into_response(),
        ApiKeyAuthResult::Expired => (
            StatusCode::FORBIDDEN,
            Json(UserErrorResponse { error: "API Key 已过期".into() }),
        ).into_response(),
        ApiKeyAuthResult::NotFound => (
            StatusCode::UNAUTHORIZED,
            Json(UserErrorResponse { error: "无效的 API Key".into() }),
        ).into_response(),
    }
}

/// GET /api/user/usage
/// 获取当前用户的用量汇总（需要通过中间件鉴权）
pub async fn get_usage(
    State(state): State<UserState>,
    Extension(ctx): Extension<UserContext>,
) -> impl IntoResponse {
    let summary = state.usage_tracker.get_summary(ctx.key_id);

    // 查询 key 详情
    let keys = state.api_key_manager.list();
    let key_info = keys.iter().find(|k| k.id == ctx.key_id);

    let response = UsageResponse {
        id: ctx.key_id,
        name: ctx.key_name,
        spending_limit: ctx.spending_limit,
        expires_at: key_info.and_then(|k| k.expires_at.map(|t| t.to_rfc3339())),
        duration_days: key_info.and_then(|k| k.duration_days),
        activated_at: key_info.and_then(|k| k.activated_at.map(|t| t.to_rfc3339())),
        total_requests: summary.total_requests,
        total_input_tokens: summary.total_input_tokens,
        total_output_tokens: summary.total_output_tokens,
        total_cost: summary.total_cost,
        by_model: summary.by_model,
    };
    Json(response)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub api_key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub id: u32,
    pub name: String,
    pub spending_limit: Option<f64>,
    pub total_cost: f64,
    pub expires_at: Option<String>,
    pub duration_days: Option<f64>,
    pub activated_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageResponse {
    pub id: u32,
    pub name: String,
    pub spending_limit: Option<f64>,
    pub expires_at: Option<String>,
    pub duration_days: Option<f64>,
    pub activated_at: Option<String>,
    pub total_requests: u64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost: f64,
    pub by_model: Vec<crate::model::usage::ModelUsage>,
}
