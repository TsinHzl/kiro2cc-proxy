// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 日志查看 API Handler — SSE 流 + 下载接口

use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::{StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures::stream::{self, StreamExt};
use tokio::sync::broadcast::error::RecvError;

use super::middleware::AdminState;
use crate::common::auth;

/// GET /api/admin/logs/stream?api_key=<key>
///
/// EventSource 不支持自定义 Header，因此通过 Query Param 认证。
/// 连接后先发送 history 事件（全量快照），随后持续推送 log 事件。
///
/// # 安全说明
/// API Key 以 Query Param 形式传输，会出现在 access log 中。
/// 生产环境部署时必须在 HTTPS 反向代理后方使用本端点。
pub async fn stream_logs(
    State(state): State<AdminState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if !check_api_key(&state, &params) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let Some(log_capture) = &state.log_capture else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Log capture not enabled").into_response();
    };
    let log_capture = log_capture.clone();

    // 先订阅，再取快照，避免遗漏订阅后、快照前的新事件
    let rx = log_capture.subscribe();
    let history = log_capture.snapshot();
    let history_json = serde_json::to_string(&history).unwrap_or_else(|_| "[]".to_string());

    let history_stream = stream::once(async move {
        Ok::<Event, Infallible>(Event::default().event("history").data(history_json))
    });

    let live_stream = stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(entry) => {
                    let Ok(json) = serde_json::to_string(&entry) else {
                        continue;
                    };
                    return Some((Ok(Event::default().event("log").data(json)), rx));
                }
                Err(RecvError::Lagged(n)) => {
                    // 广播通道溢出，跳过丢失的消息继续
                    tracing::warn!("log SSE stream lagged by {} messages", n);
                    continue;
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });

    Sse::new(history_stream.chain(live_stream))
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(30))
                .text("ping"),
        )
        .into_response()
}

/// GET /api/admin/logs/download?api_key=<key>
///
/// 返回当前 ring buffer 内全部日志，以 .txt 文件形式下载。
///
/// # 安全说明
/// API Key 以 Query Param 形式传输，必须在 HTTPS 后方使用。
pub async fn download_logs(
    State(state): State<AdminState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if !check_api_key(&state, &params) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let Some(log_capture) = &state.log_capture else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Log capture not enabled").into_response();
    };

    let snapshot = log_capture.snapshot();
    let mut content = String::with_capacity(snapshot.len() * 120);
    for entry in &snapshot {
        content.push_str(&format!(
            "{} [{}] {} {}\n",
            entry.timestamp, entry.level, entry.target, entry.message
        ));
    }

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let disposition = format!(
        "attachment; filename=\"kiro2cc-proxy-logs-{}.txt\"",
        timestamp
    );

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_str(&disposition)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("attachment")),
    );

    (headers, content).into_response()
}

fn check_api_key(state: &AdminState, params: &HashMap<String, String>) -> bool {
    let key = params.get("api_key").map(|s| s.as_str()).unwrap_or("");
    auth::constant_time_eq(key, &state.admin_api_key.read())
}
