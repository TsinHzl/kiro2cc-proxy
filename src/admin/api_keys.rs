// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin API Key 管理端点：服务器信息查询、子 Key 的列表/创建/更新/删除。

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use super::{
    middleware::AdminState,
    types::{
        AdminErrorResponse, AdminModelsResponse, CreateApiKeyRequest, SuccessResponse,
        UpdateApiKeyRequest,
    },
};
use crate::common::download::csv_attachment;
use crate::model::usage::{ExportAudience, ExportScope};

/// GET /api/admin/server-info
/// 获取服务器信息
pub async fn get_server_info(State(_state): State<AdminState>) -> impl IntoResponse {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ServerInfo {
        version: String,
    }
    Json(ServerInfo {
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

/// GET /api/admin/api-keys/:id/usage/range?days=30
/// 按 CST 自然日聚合单个 API Key 最近 N 天（含今日）的用量
///
/// 数据源为日汇总，不受明细分页与单 Key 留存上限影响；`days` 缺省 30、
/// 取值 1..=366，与 `/credentials/{id}/usage/7days|30days` 同口径。
pub async fn get_key_usage_range(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let days = params
        .get("days")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(30);
    Json(tracker.get_range_summary_for_key(id, days)).into_response()
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

/// GET /api/admin/credentials/:id/usage/summary
/// 获取单个账号全部留存记录的用量汇总（不限日期）
pub async fn get_credential_usage_summary(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_summary_for_credential(id)).into_response()
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

/// GET /api/admin/credentials/:id/usage/7days
/// 获取单个账号在 CST 最近 7 天（含今日）的用量汇总
pub async fn get_credential_7days_summary(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_range_summary_for_credential(id, 7)).into_response()
}

/// GET /api/admin/credentials/:id/usage/30days
/// 获取单个账号在 CST 最近 30 天（含今日）的用量汇总
pub async fn get_credential_30days_summary(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_range_summary_for_credential(id, 30)).into_response()
}

/// GET /api/admin/models
/// 获取当前代理支持的完整模型列表（admin 鉴权），直接来源于上游 ListAvailableModels
/// 实时响应；上游调用失败时回退到本地静态模型表
pub async fn get_admin_models(State(state): State<AdminState>) -> impl IntoResponse {
    Json(AdminModelsResponse {
        object: "list".to_string(),
        data: state.service.list_admin_models().await,
    })
}

/// GET /api/admin/credentials/:id/models
/// 获取指定账号支持的模型列表（admin 鉴权）
///
/// 与 /api/admin/models（取任意可用账号）不同，此接口按 id 指定账号实时查询上游
/// ListAvailableModels，失败时直接返回错误（不回退静态表）。
pub async fn get_credential_models(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match state.service.list_admin_models_for(id).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => (e.status_code(), Json(e.into_response())).into_response(),
    }
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

/// GET /api/admin/usage/records?from=YYYY-MM-DD&to=YYYY-MM-DD&page=1&page_size=50
/// 分页获取 CST 闭区间 [from, to] 的原始请求记录（降序保留最新 10000 条）
pub async fn get_usage_records(
    State(state): State<AdminState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let (from, to) = match parse_cst_date_range(&params) {
        Ok(range) => range,
        Err(message) => return bad_request(message),
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
    Json(tracker.get_records_paged_by_date_range(from, to, page, page_size, &labels))
        .into_response()
}

/// 400 响应（invalid_request）
fn bad_request(message: &str) -> Response {
    let error = AdminErrorResponse::invalid_request(message);
    (StatusCode::BAD_REQUEST, Json(error)).into_response()
}

/// 解析并校验 `from` / `to` 查询参数：严格 YYYY-MM-DD（零填充、合法日期）且 from <= to。
/// 失败时返回对应的错误文案（调用方转 400）。
fn parse_cst_date_range(
    params: &std::collections::HashMap<String, String>,
) -> Result<(&str, &str), &'static str> {
    let Some(from) = params.get("from") else {
        return Err("缺少 from 参数");
    };
    let Some(to) = params.get("to") else {
        return Err("缺少 to 参数");
    };
    // 格式校验：严格 YYYY-MM-DD（零填充、合法日期；2026-13-01、2026-1-1 均拒绝。
    // parse_from_str 的 %Y-%m-%d 接受非零填充，故先验长度/分隔符再 parse）；
    // 合法格式前提下字符串比较等价日期比较
    let is_valid_date = |s: &str| {
        s.len() == 10
            && s.as_bytes()[4] == b'-'
            && s.as_bytes()[7] == b'-'
            && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
    };
    if !is_valid_date(from) || !is_valid_date(to) {
        return Err("from/to 须为 YYYY-MM-DD 格式");
    }
    if from > to {
        return Err("from 不能晚于 to");
    }
    Ok((from.as_str(), to.as_str()))
}

/// 导出文件名里的时间戳
fn export_timestamp() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

/// GET /api/admin/usage/records/download?from=YYYY-MM-DD&to=YYYY-MM-DD
/// 下载 CST 闭区间 [from, to] 内全部 Key 的请求记录（CSV，不分页、不截断）
pub async fn download_usage_records(
    State(state): State<AdminState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let (from, to) = match parse_cst_date_range(&params) {
        Ok(range) => range,
        Err(message) => return bad_request(message),
    };
    let emails = state.service.credential_emails();
    let csv = tracker.export_records_csv(
        ExportScope::DateRange { from, to },
        ExportAudience::Admin,
        &emails,
    );
    csv_attachment(
        &format!("usage-logs-{from}_{to}-{}.csv", export_timestamp()),
        csv,
    )
}

/// GET /api/admin/credentials/:id/usage/records/download
/// 下载单个账号全部留存的请求记录（CSV）
pub async fn download_credential_usage_records(
    State(state): State<AdminState>,
    Path(id): Path<u64>,
) -> Response {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let emails = state.service.credential_emails();
    let csv =
        tracker.export_records_csv(ExportScope::Credential(id), ExportAudience::Admin, &emails);
    csv_attachment(
        &format!("usage-logs-credential-{id}-{}.csv", export_timestamp()),
        csv,
    )
}

/// GET /api/admin/api-keys/:id/usage/records/download
/// 下载单个 API Key 全部留存的请求记录（CSV）
pub async fn download_key_usage_records(
    State(state): State<AdminState>,
    Path(id): Path<u32>,
) -> Response {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let emails = state.service.credential_emails();
    let csv = tracker.export_records_csv(ExportScope::ApiKey(id), ExportAudience::Admin, &emails);
    csv_attachment(
        &format!("usage-logs-key-{id}-{}.csv", export_timestamp()),
        csv,
    )
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

        // 零账号场景下 list_available_models 必然失败，触发回退到本地静态模型表
        assert!(
            parsed
                .data
                .iter()
                .all(|item| item.rate_multiplier.is_none()),
            "无可用账号时所有模型的 rate_multiplier 应为 None"
        );
    }

    // ============ usage_records（区间明细端点）测试 ============

    /// 测试专用临时目录：Drop 时清理，即使中途 assert! panic 也不残留
    /// （标准库 unwind 会执行 Drop），避免 CI 机器上堆积垂悬目录。
    struct TempDirGuard(std::path::PathBuf);

    impl TempDirGuard {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "kiro2cc_admin_usage_records_{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 构造带用量追踪器的 AdminState（临时目录隔离明细与累计文件）
    fn admin_state_with_usage_tracker() -> (
        AdminState,
        std::sync::Arc<crate::model::usage::UsageTracker>,
        TempDirGuard,
    ) {
        let guard = TempDirGuard::new();
        let tracker = std::sync::Arc::new(
            crate::model::usage::UsageTracker::load(guard.0.join("api_key_usage.json")).unwrap(),
        );
        let state = empty_admin_state().with_usage_tracker(tracker.clone());
        (state, tracker, guard)
    }

    /// 直接注入指定时间戳的记录（绕过 record() 的 Utc::now()，控制 CST 归属日）
    fn inject_usage_record(
        tracker: &crate::model::usage::UsageTracker,
        created_at: chrono::DateTime<chrono::Utc>,
        credential_id: Option<u64>,
        user_agent: Option<&str>,
    ) {
        use crate::model::usage::UsageRecord;
        tracker.records.write().push(UsageRecord {
            api_key_id: 1,
            credential_id,
            model: "claude-opus-4.6".to_string(),
            input_tokens: 100,
            output_tokens: 10,
            estimated_cost: 0.01,
            credits_used: Some(0.02),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            cache_creation_5m_input_tokens: 0,
            cache_creation_1h_input_tokens: 0,
            created_at,
            client_ip: None,
            effort: None,
            first_token_ms: None,
            duration_ms: None,
            thinking_token_count: None,
            is_stream: None,
            user_agent: user_agent.map(str::to_string),
            outcome: None,
            error_message: None,
        });
    }

    fn utc_at(s: &str) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    async fn call_usage_records(
        state: AdminState,
        query: &str,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        let params: std::collections::HashMap<String, String> = query
            .split('&')
            .filter(|kv| !kv.is_empty())
            .map(|kv| {
                let (k, v) = kv.split_once('=').expect("查询串须为 k=v 形式");
                (k.to_string(), v.to_string())
            })
            .collect();
        let response = get_usage_records(State(state), axum::extract::Query(params))
            .await
            .into_response();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("读取响应体失败");
        (
            status,
            serde_json::from_slice(&body).expect("解析响应体失败"),
        )
    }

    /// usage_records：缺 from / 缺 to / 非法格式 / from > to 均 400，错误信息明确
    #[tokio::test]
    async fn test_usage_records_invalid_params_rejected() {
        let (state, _tracker, _guard) = admin_state_with_usage_tracker();
        for (query, why) in [
            ("to=2026-10-07", "缺 from"),
            ("from=2026-10-01", "缺 to"),
            ("from=2026-1-1&to=2026-10-07", "非零填充格式"),
            ("from=2026-13-01&to=2026-10-07", "非法日期"),
            ("from=2026-10-08&to=2026-10-07", "from > to"),
        ] {
            let (status, body) = call_usage_records(state.clone(), query).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "应拒绝：{why}");
            assert_eq!(body["error"]["type"], "invalid_request", "{why}");
        }
    }

    /// usage_records：合法闭区间 200，边界日含入、降序排列、
    /// credential_label 查表与 user_agent 透传
    #[tokio::test]
    async fn test_usage_records_range_query_and_transparent_fields() {
        let (state, tracker, _guard) = admin_state_with_usage_tracker();
        inject_usage_record(
            &tracker,
            utc_at("2026-10-01T18:00:00Z"), // CST 10-02 02:00（区间首日）
            Some(7),
            Some("claude-cli/2.1.5 (external, cli)"),
        );
        inject_usage_record(&tracker, utc_at("2026-10-03T18:00:00Z"), None, None); // CST 10-04（区间末日）
        inject_usage_record(&tracker, utc_at("2026-10-05T02:00:00Z"), None, None); // CST 10-05（区间外）

        let (status, body) =
            call_usage_records(state, "from=2026-10-02&to=2026-10-04&page=1&page_size=10").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["total"], 2);
        assert_eq!(body["totalPages"], 1);
        let records = body["records"].as_array().expect("records 应为数组");
        assert_eq!(records.len(), 2);
        // 降序：CST 10-04 记录在前
        assert_eq!(records[0]["createdAt"], "2026-10-03T18:00:00Z");
        assert_eq!(records[1]["createdAt"], "2026-10-01T18:00:00Z");
        // credential_label：label 表为空（无账号）→ null；credential_id 透传
        assert_eq!(records[0]["credentialId"], serde_json::Value::Null);
        assert_eq!(records[1]["credentialId"], 7);
        assert_eq!(records[1]["credentialLabel"], serde_json::Value::Null);
        // user_agent 透传
        assert_eq!(records[1]["userAgent"], "claude-cli/2.1.5 (external, cli)");
    }

    /// usage_records：page/page_size 缺省默认 1/50；越界 page 钳制到末页
    #[tokio::test]
    async fn test_usage_records_pagination_defaults_and_clamp() {
        let (state, tracker, _guard) = admin_state_with_usage_tracker();
        for i in 0..3 {
            inject_usage_record(
                &tracker,
                utc_at("2026-10-02T02:00:00Z") + chrono::Duration::seconds(i),
                None,
                None,
            );
        }
        let (status, body) =
            call_usage_records(state.clone(), "from=2026-10-02&to=2026-10-02").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["page"], 1);
        assert_eq!(body["pageSize"], 50); // 缺省默认

        let (status, body) =
            call_usage_records(state, "from=2026-10-02&to=2026-10-02&page=99&page_size=2").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["totalPages"], 2);
        assert_eq!(body["page"], 2); // 越界钳制到末页
        assert_eq!(body["records"].as_array().unwrap().len(), 1); // 末页仅剩最旧 1 条
    }

    /// usage_records：用量追踪未启用（无 usage_tracker）时 503
    #[tokio::test]
    async fn test_usage_records_tracker_disabled_503() {
        let (status, body) =
            call_usage_records(empty_admin_state(), "from=2026-10-01&to=2026-10-07").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"]["type"], "internal_error");
    }

    // ============ usage records CSV 下载端点测试 ============

    async fn call_download(
        state: AdminState,
        query: &str,
    ) -> (axum::http::HeaderMap, StatusCode, String) {
        let params: std::collections::HashMap<String, String> = query
            .split('&')
            .filter(|kv| !kv.is_empty())
            .map(|kv| {
                let (k, v) = kv.split_once('=').expect("查询串须为 k=v 形式");
                (k.to_string(), v.to_string())
            })
            .collect();
        let response = download_usage_records(State(state), axum::extract::Query(params)).await;
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("读取响应体失败");
        (headers, status, String::from_utf8(body.to_vec()).unwrap())
    }

    /// 下载：参数校验与分页接口同口径（共用解析函数）
    #[tokio::test]
    async fn test_download_usage_records_invalid_params_rejected() {
        let (state, _tracker, _guard) = admin_state_with_usage_tracker();
        for query in [
            "to=2026-10-07",
            "from=2026-10-01",
            "from=2026-1-1&to=2026-10-07",
            "from=2026-10-08&to=2026-10-07",
        ] {
            let (_, status, _) = call_download(state.clone(), query).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "应拒绝：{query}");
        }
    }

    /// 下载：返回 CSV 附件，只含所选 CST 区间内的记录，且不受分页参数影响
    #[tokio::test]
    async fn test_download_usage_records_returns_csv_for_range() {
        let (state, tracker, _guard) = admin_state_with_usage_tracker();
        inject_usage_record(&tracker, utc_at("2026-10-01T18:00:00Z"), Some(7), None); // CST 10-02
        inject_usage_record(&tracker, utc_at("2026-10-03T18:00:00Z"), None, None); // CST 10-04
        inject_usage_record(&tracker, utc_at("2026-10-05T02:00:00Z"), None, None); // CST 10-05（区间外）

        let (headers, status, body) =
            call_download(state, "from=2026-10-02&to=2026-10-04&page=1&page_size=1").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["content-type"], "text/csv; charset=utf-8");
        let disposition = headers["content-disposition"].to_str().unwrap();
        assert!(
            disposition.starts_with("attachment; filename=\"usage-logs-2026-10-02_2026-10-04-"),
            "文件名应含所选区间：{disposition}"
        );
        assert!(disposition.ends_with(".csv\""));
        // 表头 + 区间内 2 条（忽略 page_size=1）
        assert_eq!(body.trim_end().split("\r\n").count(), 3);
        assert!(body.contains("2026-10-04 02:00:00"));
        assert!(body.contains("2026-10-02 02:00:00"));
        assert!(!body.contains("2026-10-05"));
    }

    /// 下载：用量追踪未启用时 503
    #[tokio::test]
    async fn test_download_usage_records_tracker_disabled_503() {
        let (_, status, _) =
            call_download(empty_admin_state(), "from=2026-10-01&to=2026-10-07").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    /// 账号 / Key 下载只含各自的记录，文件名带 ID
    #[tokio::test]
    async fn test_download_credential_and_key_scopes() {
        let (state, tracker, _guard) = admin_state_with_usage_tracker();
        inject_usage_record(&tracker, utc_at("2026-10-01T00:00:00Z"), Some(7), None);
        inject_usage_record(&tracker, utc_at("2026-10-02T00:00:00Z"), Some(8), None);

        let response = download_credential_usage_records(State(state.clone()), Path(7)).await;
        let disposition = response.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            disposition.contains("usage-logs-credential-7-"),
            "{disposition}"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert_eq!(
            body.trim_end().split("\r\n").count(),
            2,
            "表头 + 账号 7 的 1 条"
        );
        assert!(body.contains("2026-10-01 08:00:00"));

        // inject_usage_record 固定 api_key_id=1
        let response = download_key_usage_records(State(state.clone()), Path(1)).await;
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8(body.to_vec())
                .unwrap()
                .trim_end()
                .split("\r\n")
                .count(),
            3
        );

        let response = download_key_usage_records(State(state), Path(99)).await;
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8(body.to_vec())
                .unwrap()
                .trim_end()
                .split("\r\n")
                .count(),
            1,
            "无记录的 Key 只有表头"
        );
    }
}
