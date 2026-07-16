// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin API Key 管理处理器

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Serialize;

use super::{
    middleware::AdminState,
    types::{
        AdminErrorResponse, AdminModelItem, AdminModelsResponse, CreateApiKeyRequest,
        SuccessResponse, UpdateApiKeyRequest,
    },
};
use crate::anthropic::{converter::map_model, handlers::build_model_list};

/// GET /api/admin/server-info
/// 获取服务器连接信息（主 API Key）
pub async fn get_server_info(State(state): State<AdminState>) -> impl IntoResponse {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ServerInfo {
        master_api_key: Option<String>,
        version: String,
    }
    Json(ServerInfo {
        master_api_key: state.master_api_key.as_ref().map(|k| k.read().clone()),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// GET /api/admin/api-keys
/// 列出所有 API Key
pub async fn list_api_keys(State(state): State<AdminState>) -> impl IntoResponse {
    match &state.api_key_manager {
        Some(manager) => Json(manager.list()).into_response(),
        None => {
            let error = AdminErrorResponse::internal_error("API Key 管理未启用");
            (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response()
        }
    }
}

/// POST /api/admin/api-keys
/// 创建新 API Key
pub async fn create_api_key(
    State(state): State<AdminState>,
    Json(payload): Json<CreateApiKeyRequest>,
) -> impl IntoResponse {
    let Some(manager) = &state.api_key_manager else {
        let error = AdminErrorResponse::internal_error("API Key 管理未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };

    match manager.create(
        payload.name,
        payload.expires_at,
        payload.spending_limit,
        payload.limit_unit,
        payload.duration_days,
        payload.bound_credential_ids,
    ) {
        Ok(api_key) => (StatusCode::CREATED, Json(api_key)).into_response(),
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// PUT /api/admin/api-keys/:id
/// 更新 API Key
pub async fn update_api_key(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
    Json(payload): Json<UpdateApiKeyRequest>,
) -> impl IntoResponse {
    let Some(manager) = &state.api_key_manager else {
        let error = AdminErrorResponse::internal_error("API Key 管理未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };

    match manager.update(
        id,
        payload.name,
        payload.enabled,
        payload.expires_at,
        payload.spending_limit,
        payload.limit_unit,
        payload.duration_days,
        payload.bound_credential_ids,
    ) {
        Ok(Some(api_key)) => Json(api_key).into_response(),
        Ok(None) => {
            let error = AdminErrorResponse::not_found(format!("API Key #{} 不存在", id));
            (StatusCode::NOT_FOUND, Json(error)).into_response()
        }
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// DELETE /api/admin/api-keys/:id
/// 删除 API Key
pub async fn delete_api_key(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let Some(manager) = &state.api_key_manager else {
        let error = AdminErrorResponse::internal_error("API Key 管理未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };

    match manager.delete(id) {
        Ok(true) => Json(SuccessResponse::new(format!("API Key #{} 已删除", id))).into_response(),
        Ok(false) => {
            let error = AdminErrorResponse::not_found(format!("API Key #{} 不存在", id));
            (StatusCode::NOT_FOUND, Json(error)).into_response()
        }
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// GET /api/admin/api-keys/usage
/// 获取所有 API Key 的用量概览
pub async fn get_all_usage(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_all_summaries()).into_response()
}

/// GET /api/admin/api-keys/:id/usage
/// 获取单个 API Key 的用量汇总
pub async fn get_key_usage(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_summary(id)).into_response()
}

/// DELETE /api/admin/api-keys/:id/usage
/// 重置单个 API Key 的用量记录
pub async fn reset_key_usage(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    match tracker.reset(id) {
        Ok(()) => Json(SuccessResponse::new(format!("API Key #{} 用量已重置", id))).into_response(),
        Err(e) => {
            let error = AdminErrorResponse::internal_error(e.to_string());
            (StatusCode::INTERNAL_SERVER_ERROR, Json(error)).into_response()
        }
    }
}

/// GET /api/admin/api-keys/:id/usage/records?page=1&page_size=50
/// 分页获取单个 API Key 的原始请求记录
pub async fn get_key_usage_records(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    let labels = state.service.credential_labels();
    Json(tracker.get_records_paged(id, page, page_size, &labels)).into_response()
}

/// GET /api/admin/credentials/:id/usage/records?page=1&page_size=50
/// 分页获取单个账号的原始请求记录
pub async fn get_credential_usage_records(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    let labels = state.service.credential_labels();
    Json(tracker.get_records_paged_by_credential(id, page, page_size, &labels)).into_response()
}

/// GET /api/admin/credentials/:id/usage/today
/// 获取单个账号在 CST 今日的用量汇总
pub async fn get_credential_today_summary(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_today_summary_for_credential(id)).into_response()
}

/// 将 `build_model_list()` 的每一项与费率表关联，生成 `AdminModelItem` 列表
///
/// 抽成独立函数以便单测覆盖"归一化命中"路径，无需真实网络调用
fn build_admin_model_items(rates: &std::collections::HashMap<String, f64>) -> Vec<AdminModelItem> {
    build_model_list()
        .into_iter()
        .map(|model| {
            let rate_multiplier =
                map_model(&model.id).and_then(|real_id| rates.get(&real_id).copied());
            AdminModelItem {
                model,
                rate_multiplier,
            }
        })
        .collect()
}

/// GET /api/admin/models
/// 获取当前代理支持的完整模型列表（admin 鉴权，数据源与 /v1/models 共用），
/// 附加实时查询的官方费率倍率
pub async fn get_admin_models(State(state): State<AdminState>) -> impl IntoResponse {
    let rates = state.service.list_model_rates().await;

    Json(AdminModelsResponse {
        object: "list".to_string(),
        data: build_admin_model_items(&rates),
    })
}

/// GET /api/admin/rpm
/// 获取实时 RPM 数据（含 sticky cache 命中/未命中统计）
pub async fn get_rpm(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(rpm_tracker) = &state.rpm_tracker else {
        let error = AdminErrorResponse::internal_error("RPM 监控未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let (sticky_hits, sticky_misses) = state.service.sticky_metrics();
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RpmAndCacheSnapshot {
        #[serde(flatten)]
        rpm: crate::model::rpm::RpmSnapshot,
        sticky_hits: u64,
        sticky_misses: u64,
    }
    Json(RpmAndCacheSnapshot {
        rpm: rpm_tracker.snapshot(),
        sticky_hits,
        sticky_misses,
    })
    .into_response()
}

/// GET /api/admin/usage/daily
/// 获取所有日期的用量汇总（按日期降序）
pub async fn get_daily_usage(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_daily_summaries()).into_response()
}

/// GET /api/admin/usage/daily/{date}/records?page=1&page_size=50
/// 分页获取指定日期的原始请求记录（最多 2000 条）
pub async fn get_daily_usage_records(
    State(state): State<AdminState>,
    Path(date): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50);
    let labels = state.service.credential_labels();
    Json(tracker.get_records_paged_by_date(&date, page, page_size, &labels)).into_response()
}

/// GET /api/admin/credentials/:id/failure-logs?page=1&page_size=50
/// 分页获取指定账号的失败日志
pub async fn get_failure_logs(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(store) = &state.failure_log_store else {
        let error = AdminErrorResponse::internal_error("失败日志未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    Json(store.get_paged(id, page, page_size)).into_response()
}

/// GET /api/admin/credentials/:id/throttle-logs?page=1&page_size=50
/// 分页获取指定账号的限流日志
pub async fn get_throttle_logs(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(store) = &state.throttle_log_store else {
        let error = AdminErrorResponse::internal_error("限流日志未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params
        .get("page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let page_size = params
        .get("page_size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50)
        .min(500);
    Json(store.get_paged(id, page, page_size)).into_response()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::RwLock;

    use super::*;
    use crate::admin::{middleware::AdminState, service::AdminService};
    use crate::anthropic::handlers::build_model_list;
    use crate::kiro::token_manager::MultiTokenManager;
    use crate::model::config::Config;

    fn empty_admin_state() -> AdminState {
        let token_manager =
            MultiTokenManager::new(Config::default(), vec![], None, None, false).unwrap();
        let service = AdminService::new(Arc::new(token_manager));
        AdminState::new(Arc::new(RwLock::new("test-admin-key".to_string())), service)
    }

    #[tokio::test]
    async fn test_get_admin_models_matches_build_model_list() {
        let response = get_admin_models(State(empty_admin_state()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("读取响应体失败");
        let parsed: AdminModelsResponse = serde_json::from_slice(&body).expect("解析响应体失败");

        assert_eq!(parsed.object, "list");

        let expected_ids: std::collections::HashSet<String> =
            build_model_list().into_iter().map(|m| m.id).collect();
        let actual_ids: std::collections::HashSet<String> = parsed
            .data
            .iter()
            .map(|item| item.model.id.clone())
            .collect();
        assert_eq!(actual_ids, expected_ids);

        for id in [
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
            "claude-fable-5",
            "claude-sonnet-5",
        ] {
            assert!(actual_ids.contains(id), "模型列表应包含 {id}");
        }

        // 零账号场景下 list_available_models 必然失败，覆盖优雅降级路径
        assert!(
            parsed
                .data
                .iter()
                .all(|item| item.rate_multiplier.is_none()),
            "无可用账号时所有模型的 rate_multiplier 应为 None"
        );
    }

    #[test]
    fn test_build_admin_model_items_fills_rate_multiplier_on_match() {
        let sample = build_model_list()
            .into_iter()
            .find_map(|m| map_model(&m.id).map(|real_id| (m.id, real_id)))
            .expect("模型列表中应至少存在一个可被 map_model 归一化的模型");
        let (sample_id, sample_real_id) = sample;

        let mut rates = std::collections::HashMap::new();
        rates.insert(sample_real_id.clone(), 1.3);

        let items = build_admin_model_items(&rates);

        let matched = items
            .iter()
            .find(|item| item.model.id == sample_id)
            .expect("应能在结果中找到样本模型");
        assert_eq!(
            matched.rate_multiplier,
            Some(1.3),
            "归一化命中费率表时应正确回填 rate_multiplier"
        );

        // 除样本模型外，只有恰好归一化到同一 real_id 的模型（如 thinking 变体）才应命中；
        // 其余模型的归一化结果不等于 sample_real_id，rate_multiplier 必须为 None
        let unexpected = items.iter().any(|item| {
            item.rate_multiplier.is_some()
                && map_model(&item.model.id).as_deref() != Some(sample_real_id.as_str())
        });
        assert!(
            !unexpected,
            "只有归一化命中费率表 key 的模型才应有 rate_multiplier"
        );
    }
}
