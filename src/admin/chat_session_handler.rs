// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin「对话」页会话管理端点：会话列表、读取、创建、保存、删除。
//!
//! 存储细节见 `model::chat_session`。文件读写是阻塞 IO（含 fsync），统一放进 spawn_blocking。

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    middleware::AdminState,
    types::{AdminErrorResponse, SuccessResponse},
};
use crate::model::chat_session::{
    ChatSession, ChatSessionError, ChatSessionMeta, ChatSessionStore,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateChatSessionRequest {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub messages: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChatSessionRequest {
    /// 不传则保留原标题
    pub title: Option<String>,
    pub messages: Vec<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearResponse {
    success: bool,
    removed: usize,
}

/// 端点错误：存储未启用，或存储层错误
pub enum ChatApiError {
    Disabled,
    Store(ChatSessionError),
}

impl From<ChatSessionError> for ChatApiError {
    fn from(e: ChatSessionError) -> Self {
        Self::Store(e)
    }
}

impl IntoResponse for ChatApiError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            Self::Disabled => (
                StatusCode::SERVICE_UNAVAILABLE,
                AdminErrorResponse::internal_error("对话会话存储未启用"),
            ),
            Self::Store(err @ ChatSessionError::NotFound) => (
                StatusCode::NOT_FOUND,
                AdminErrorResponse::not_found(err.to_string()),
            ),
            Self::Store(err @ ChatSessionError::Invalid(_)) => (
                StatusCode::BAD_REQUEST,
                AdminErrorResponse::invalid_request(err.to_string()),
            ),
            Self::Store(err @ ChatSessionError::Io(_)) => {
                tracing::error!("对话会话存储错误: {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    AdminErrorResponse::internal_error(err.to_string()),
                )
            }
        };
        (status, Json(body)).into_response()
    }
}

/// 在阻塞线程池里对会话存储执行操作
async fn with_store<T, F>(state: &AdminState, f: F) -> Result<T, ChatApiError>
where
    T: Send + 'static,
    F: FnOnce(&ChatSessionStore) -> Result<T, ChatSessionError> + Send + 'static,
{
    let store: Arc<ChatSessionStore> = state
        .chat_session_store
        .clone()
        .ok_or(ChatApiError::Disabled)?;
    tokio::task::spawn_blocking(move || f(&store))
        .await
        .map_err(|e| ChatSessionError::Io(e.to_string()))?
        .map_err(Into::into)
}

/// GET /api/admin/chat/sessions
pub async fn list_chat_sessions(
    State(state): State<AdminState>,
) -> Result<Json<Vec<ChatSessionMeta>>, ChatApiError> {
    with_store(&state, |s| Ok(s.list())).await.map(Json)
}

/// POST /api/admin/chat/sessions
pub async fn create_chat_session(
    State(state): State<AdminState>,
    Json(payload): Json<CreateChatSessionRequest>,
) -> Result<(StatusCode, Json<ChatSessionMeta>), ChatApiError> {
    let meta = with_store(&state, move |s| s.create(&payload.title, payload.messages)).await?;
    Ok((StatusCode::CREATED, Json(meta)))
}

/// DELETE /api/admin/chat/sessions
pub async fn clear_chat_sessions(
    State(state): State<AdminState>,
) -> Result<Json<ClearResponse>, ChatApiError> {
    let removed = with_store(&state, |s| s.clear()).await?;
    Ok(Json(ClearResponse {
        success: true,
        removed,
    }))
}

/// GET /api/admin/chat/sessions/:id
pub async fn get_chat_session(
    State(state): State<AdminState>,
    Path(id): Path<String>,
) -> Result<Json<ChatSession>, ChatApiError> {
    with_store(&state, move |s| s.get(&id)).await.map(Json)
}

/// PUT /api/admin/chat/sessions/:id
pub async fn update_chat_session(
    State(state): State<AdminState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateChatSessionRequest>,
) -> Result<Json<ChatSessionMeta>, ChatApiError> {
    with_store(&state, move |s| {
        s.update(&id, payload.title.as_deref(), payload.messages)
    })
    .await
    .map(Json)
}

/// DELETE /api/admin/chat/sessions/:id
pub async fn delete_chat_session(
    State(state): State<AdminState>,
    Path(id): Path<String>,
) -> Result<Json<SuccessResponse>, ChatApiError> {
    with_store(&state, move |s| s.delete(&id)).await?;
    Ok(Json(SuccessResponse::new("会话已删除")))
}
