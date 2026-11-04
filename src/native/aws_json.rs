// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! AWS JSON 1.0 辅助：响应构造、错误分类、响应头过滤与 null 裁剪。
//!
//! 原生（Kiro 客户端）协议是 AWS JSON 1.0 RPC：请求 `POST` + `x-amz-target`，
//! 响应为 `application/json`（控制面）或 `application/vnd.amazon.eventstream`
//! （推理/MCP 流）。这里只提供无网络依赖的纯函数，便于单元测试。

use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::Value;

/// 上游响应中不应转发给客户端的 hop-by-hop / 由本层重新计算的响应头。
///
/// - hop-by-hop 头只对单跳连接有效；
/// - `content-length`/`transfer-encoding` 由 axum 按实际流式输出重新计算；
/// - `content-encoding`：本代理按原始字节转发，不做解压，避免声明错误的编码。
const STRIPPED_RESPONSE_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "trailers",
    "transfer-encoding",
    "upgrade",
    "content-length",
    "content-encoding",
];

/// 判断上游响应头是否应原样转发给原生客户端。
///
/// 采用"黑名单"而不是白名单：`x-amzn-*`（如 conversation id / request id）、
/// `retry-after` 等未来新增头都能透传，只有逐跳/长度/编码类头被丢弃。
pub(crate) fn should_copy_response_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !STRIPPED_RESPONSE_HEADERS.contains(&lower.as_str())
}

/// 把 reqwest 响应头转换为 axum 响应头；非法头值直接跳过（不 panic）。
pub(crate) fn copy_response_headers(
    builder: axum::http::response::Builder,
    headers: &reqwest::header::HeaderMap,
) -> axum::http::response::Builder {
    let mut builder = builder;
    for (name, value) in headers {
        if !should_copy_response_header(name.as_str()) {
            continue;
        }
        let Ok(header_name) = HeaderName::from_bytes(name.as_str().as_bytes()) else {
            continue;
        };
        let Ok(header_value) = HeaderValue::from_bytes(value.as_bytes()) else {
            continue;
        };
        builder = builder.header(header_name, header_value);
    }
    builder
}

/// 构造 `application/json` 响应（AWS JSON 1.0 控制面响应/错误体）。
pub(crate) fn json_response(status: StatusCode, value: &Value) -> Response {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// 成功响应 `200 {}`。
pub(crate) fn ok_empty() -> Response {
    json_response(StatusCode::OK, &serde_json::json!({}))
}

/// AWS JSON 1.0 错误响应体：`{"__type": "...", "message": "..."}`。
pub(crate) fn error(status: StatusCode, error_type: &str, message: &str) -> Response {
    json_response(
        status,
        &serde_json::json!({
            "__type": error_type,
            "message": message,
        }),
    )
}

/// 把任意 `anyhow::Error` 映射为原生客户端可理解的 HTTP 错误。
///
/// 分类依据是 provider 错误链里的稳定文案（额度耗尽 marker、状态码等），
/// 与 Anthropic 路由的 `map_provider_error_with_context` 同源但输出原生 JSON。
pub(crate) fn provider_error(err: &anyhow::Error) -> Response {
    // `{:#}` 保留完整错误链，便于内部日志定位；对外只截取前 1000 字符，
    // 避免把超长上游响应体（可能含请求内容）整体回显给客户端。
    let chain = format!("{err:#}");
    let (status, error_type) = classify_provider_error(&chain);
    let message = truncate_chars(&chain, 1000);
    error(status, error_type, &message)
}

/// 从错误链文本推断原生错误类型与 HTTP 状态。
pub(crate) fn classify_provider_error(message: &str) -> (StatusCode, &'static str) {
    use crate::kiro::token_manager::QUOTA_EXHAUSTED_ALL_MARKER;

    if message.contains(QUOTA_EXHAUSTED_ALL_MARKER)
        || message.contains("MONTHLY_REQUEST_COUNT")
        || message.contains("额度")
    {
        (StatusCode::PAYMENT_REQUIRED, "QuotaExhaustedException")
    } else if message.contains("429")
        || message.contains("限流")
        || message.contains("Too Many")
        || message.contains("throttl")
    {
        (StatusCode::TOO_MANY_REQUESTS, "ThrottlingException")
    } else if message.contains("400")
        || message.contains("Bad Request")
        || message.contains("ValidationException")
    {
        (StatusCode::BAD_REQUEST, "ValidationException")
    } else if message.contains("401") || message.contains("403") || message.contains("认证") {
        // 账号侧鉴权失败属于网关上游问题，对原生客户端统一报 502 可重试
        (StatusCode::BAD_GATEWAY, "UpstreamAuthException")
    } else {
        (StatusCode::BAD_GATEWAY, "UpstreamException")
    }
}

/// 截断字符串到最多 `max` 个字符（按字符而不是字节，保证 UTF-8 边界安全）。
pub(crate) fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max).collect();
    out.push_str("... (truncated)");
    out
}

/// 递归删除 JSON object 中的 null 成员（数组元素保持原位，不删除）。
///
/// 合成控制面响应时用 `json!` 直接写 `Option` 字段会得到 null；AWS JSON 1.0
/// 客户端对可选成员的 null 容忍度不一，统一删除比依赖上游语义更安全。
/// 数组不裁剪 null：数组通常是有序列表，删除元素会改变语义/索引。
pub(crate) fn prune_nulls(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter_map(|(key, value)| {
                    let pruned = prune_nulls(value);
                    (!pruned.is_null()).then_some((key, pruned))
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(prune_nulls).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hop_by_hop_and_length_headers_are_stripped() {
        for name in [
            "connection",
            "Connection",
            "transfer-encoding",
            "content-length",
            "content-encoding",
            "upgrade",
        ] {
            assert!(
                !should_copy_response_header(name),
                "{name} must be stripped"
            );
        }
    }

    #[test]
    fn amazon_and_streaming_headers_are_forwarded() {
        for name in [
            "content-type",
            "x-amzn-requestid",
            "x-amzn-codewhisperer-conversation-id",
            "x-amzn-kiro-conversation-id",
            "retry-after",
        ] {
            assert!(
                should_copy_response_header(name),
                "{name} must be forwarded"
            );
        }
    }

    #[test]
    fn prune_nulls_removes_nested_nulls_and_keeps_empty_containers() {
        let value = serde_json::json!({
            "a": null,
            "b": 1,
            "c": {"d": null, "e": "x"},
            "f": [null, {"g": null, "h": true}],
            "empty": {},
            "emptyList": [],
        });
        let pruned = prune_nulls(value);
        assert_eq!(
            pruned,
            serde_json::json!({
                "b": 1,
                "c": {"e": "x"},
                "f": [null, {"h": true}],
                "empty": {},
                "emptyList": [],
            })
        );
    }

    #[test]
    fn provider_error_classification_covers_quota_throttle_validation_and_default() {
        assert_eq!(
            classify_provider_error("所有账号均无可用额度 QUOTA_EXHAUSTED_ALL_MARKER").0,
            StatusCode::PAYMENT_REQUIRED
        );
        assert_eq!(
            classify_provider_error("upstream returned 429 Too Many Requests").0,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            classify_provider_error("API 请求失败: 400 Bad Request: invalid").0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            classify_provider_error("connection reset by peer").0,
            StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn truncate_chars_is_utf8_safe() {
        let text = "额度查询失败".repeat(300);
        let truncated = truncate_chars(&text, 10);
        assert_eq!(
            truncated.chars().count(),
            10 + "... (truncated)".chars().count()
        );
        assert!(truncated.starts_with("额度查询失败额度查询"));
    }

    #[test]
    fn json_error_shape_matches_aws_json_1_0() {
        let response = error(StatusCode::FORBIDDEN, "AccessDeniedException", "denied");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
