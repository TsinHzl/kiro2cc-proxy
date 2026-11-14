// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin UI 界面偏好读写端点（侧边栏折叠/隐藏状态持久化）

use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use super::{middleware::AdminState, types::AdminErrorResponse};
use crate::model::ui_preferences::{UiPreferences, UiPreferencesStore};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUiPreferencesRequest {
    pub admin_sidebar_collapsed: Option<bool>,
    pub chat_sidebar_open: Option<bool>,
}

/// GET /api/admin/ui-preferences
pub async fn get_ui_preferences(State(state): State<AdminState>) -> Json<UiPreferences> {
    let prefs = state
        .ui_preferences_store
        .as_ref()
        .map(|s| s.get())
        .unwrap_or_default();
    Json(prefs)
}

/// PUT /api/admin/ui-preferences
pub async fn update_ui_preferences(
    State(state): State<AdminState>,
    Json(payload): Json<UpdateUiPreferencesRequest>,
) -> Response {
    let Some(store): Option<Arc<UiPreferencesStore>> = state.ui_preferences_store.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(AdminErrorResponse::internal_error("UI 偏好存储未启用")),
        )
            .into_response();
    };

    let admin_sidebar_collapsed = payload.admin_sidebar_collapsed;
    let chat_sidebar_open = payload.chat_sidebar_open;
    let result = tokio::task::spawn_blocking(move || {
        store.update(admin_sidebar_collapsed, chat_sidebar_open)
    })
    .await;

    match result {
        Ok(Ok(updated)) => Json(updated).into_response(),
        Ok(Err(e)) => {
            tracing::error!("保存 UI 偏好失败: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AdminErrorResponse::internal_error(format!(
                    "保存 UI 偏好失败: {}",
                    e
                ))),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("保存 UI 偏好任务异常: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AdminErrorResponse::internal_error("保存 UI 偏好任务异常")),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin::service::AdminService;
    use crate::kiro::token_manager::MultiTokenManager;
    use crate::model::config::Config;
    use parking_lot::RwLock;

    fn mock_state(store: Option<Arc<UiPreferencesStore>>) -> AdminState {
        let tm =
            Arc::new(MultiTokenManager::new(Config::default(), vec![], None, None, false).unwrap());
        let service = AdminService::new(tm);
        let mut state = AdminState::new(Arc::new(RwLock::new("secret".into())), service);
        if let Some(s) = store {
            state = state.with_ui_preferences_store(s);
        }
        state
    }

    #[tokio::test]
    async fn get_returns_default_without_store() {
        let state = mock_state(None);
        let Json(prefs) = get_ui_preferences(State(state)).await;
        assert_eq!(prefs, UiPreferences::default());
    }

    #[tokio::test]
    async fn update_returns_503_without_store() {
        let state = mock_state(None);
        let resp = update_ui_preferences(
            State(state),
            Json(UpdateUiPreferencesRequest {
                admin_sidebar_collapsed: Some(true),
                chat_sidebar_open: None,
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn update_persists_and_reads_back_with_store() {
        let dir =
            std::env::temp_dir().join(format!("kiro-ui-prefs-handler-{}", uuid::Uuid::new_v4()));
        let path = dir.join("ui_preferences.json");
        let store = Arc::new(UiPreferencesStore::load(&path));
        let state = mock_state(Some(store));

        let resp = update_ui_preferences(
            State(state.clone()),
            Json(UpdateUiPreferencesRequest {
                admin_sidebar_collapsed: Some(true),
                chat_sidebar_open: Some(false),
            }),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let Json(prefs) = get_ui_preferences(State(state)).await;
        assert!(prefs.admin_sidebar_collapsed);
        assert!(!prefs.chat_sidebar_open);

        std::fs::remove_dir_all(dir).ok();
    }
}
