// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! OpenAI 协议错误映射
//!
//! 把下游 [`crate::anthropic`] 层产出的错误（`{"error":{"type","message"}}`）翻译为
//! OpenAI 的错误结构（`{"error":{"message","type","param","code"}}`），**HTTP 状态码原样保留**。
//!
//! 状态码必须透传的理由与 `handlers.rs` 的注释一致：OpenAI SDK 与 Codex CLI 都按状态码
//! 决定重试策略（429 退避重试、402/400 判定为不可重试硬失败）。把 429 压成 500 会让客户端
//! 反复空转，把 402 抬成 429 会让客户端在额度耗尽后无限重试。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

/// 上游错误体不是 JSON 时，纳入错误消息的原始文本上限（避免把整页 HTML 塞给客户端）
const RAW_BODY_SNIPPET_LIMIT: usize = 500;

/// 构造一个 OpenAI 格式的错误响应
pub(crate) fn error_response(
    status: StatusCode,
    err_type: &str,
    message: impl Into<String>,
    code: Option<&str>,
) -> Response {
    (status, Json(error_body(err_type, message, code))).into_response()
}

/// 构造 OpenAI 错误 JSON 体
///
/// `param` 恒为 `null`：本代理无法把上游错误归因到具体请求字段，编造字段名会误导客户端。
pub(crate) fn error_body(err_type: &str, message: impl Into<String>, code: Option<&str>) -> Value {
    json!({
        "error": {
            "message": message.into(),
            "type": err_type,
            "param": Value::Null,
            "code": code.map(Value::from).unwrap_or(Value::Null),
        }
    })
}

/// 把下游返回的错误响应体转换为 OpenAI 错误 JSON
///
/// `body` 解析失败时不丢弃信息：截取一段原文作为 message，类型按状态码兜底。
pub(crate) fn convert_error_body(status: StatusCode, body: &[u8]) -> Value {
    let parsed: Option<Value> = serde_json::from_slice(body).ok();
    let detail = parsed.as_ref().and_then(|v| v.get("error"));

    let anthropic_type = detail
        .and_then(|d| d.get("type"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let (err_type, code) = map_type_and_code(status.as_u16(), anthropic_type);

    let message = detail
        .and_then(|d| d.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| fallback_message(status, body));

    error_body(err_type, message, code)
}

/// 上游错误体缺少 `error.message` 时的兜底文案
fn fallback_message(status: StatusCode, body: &[u8]) -> String {
    let raw = String::from_utf8_lossy(body);
    let snippet = raw.trim();
    if snippet.is_empty() {
        return format!(
            "Upstream returned HTTP {} with an empty body.",
            status.as_u16()
        );
    }
    // 按字符边界截断（字节切片会在多字节 UTF-8 中间 panic）
    let truncated: String = snippet.chars().take(RAW_BODY_SNIPPET_LIMIT).collect();
    if truncated.chars().count() < snippet.chars().count() {
        format!("{}…", truncated)
    } else {
        truncated
    }
}

/// Anthropic 错误类型 / HTTP 状态码 → OpenAI 的 `type` 与 `code`
///
/// 优先信任上游给出的语义类型；类型缺失或不认识时才按状态码兜底。
fn map_type_and_code(
    status: u16,
    anthropic_type: Option<&str>,
) -> (&'static str, Option<&'static str>) {
    match anthropic_type {
        Some("authentication_error") => ("invalid_request_error", Some("invalid_api_key")),
        Some("permission_error") => ("invalid_request_error", Some("permission_denied")),
        Some("not_found_error") => ("invalid_request_error", Some("model_not_found")),
        Some("rate_limit_error") => ("rate_limit_error", Some("rate_limit_exceeded")),
        // 额度耗尽当月不恢复，用 OpenAI 的 insufficient_quota 让客户端判定为不可重试
        Some("quota_exceeded_error") => ("insufficient_quota", Some("insufficient_quota")),
        Some("invalid_request_error") => ("invalid_request_error", None),
        Some("overloaded_error") => ("server_error", Some("overloaded")),
        Some("api_error") => ("server_error", None),
        other => {
            if let Some(t) = other {
                tracing::warn!(
                    anthropic_error_type = %t,
                    status = status,
                    "未识别的上游错误类型，按 HTTP 状态码映射"
                );
            }
            match status {
                401 => ("invalid_request_error", Some("invalid_api_key")),
                403 => ("invalid_request_error", Some("permission_denied")),
                404 => ("invalid_request_error", Some("model_not_found")),
                402 => ("insufficient_quota", Some("insufficient_quota")),
                429 => ("rate_limit_error", Some("rate_limit_exceeded")),
                400..=499 => ("invalid_request_error", None),
                _ => ("server_error", None),
            }
        }
    }
}

/// 从流式 `error` 事件的 data 中提取 (message, openai_type, code)
///
/// 流式场景 HTTP 头已发出，状态码无法再改，只能按 `error.type` 判定；未知类型退化为
/// `server_error`（对应 500 分支）。
pub(crate) fn extract_stream_error(data: &Value) -> (String, &'static str, Option<&'static str>) {
    let detail = data.get("error");
    let anthropic_type = detail
        .and_then(|d| d.get("type"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let (err_type, code) = map_type_and_code(500, anthropic_type);
    let message = detail
        .and_then(|d| d.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("Upstream stream failed without a message.")
        .to_string();
    (message, err_type, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_rate_limit_error_keeping_status() {
        let body = br#"{"error":{"type":"rate_limit_error","message":"Upstream rate limit reached on all accounts. Please retry shortly."}}"#;
        let out = convert_error_body(StatusCode::TOO_MANY_REQUESTS, body);
        assert_eq!(out["error"]["type"], "rate_limit_error");
        assert_eq!(out["error"]["code"], "rate_limit_exceeded");
        assert_eq!(
            out["error"]["message"],
            "Upstream rate limit reached on all accounts. Please retry shortly."
        );
        assert_eq!(out["error"]["param"], Value::Null);
    }

    #[test]
    fn maps_invalid_request_error() {
        let body = br#"{"error":{"type":"invalid_request_error","message":"prompt is too long: 754234 tokens > 200000 maximum"}}"#;
        let out = convert_error_body(StatusCode::BAD_REQUEST, body);
        assert_eq!(out["error"]["type"], "invalid_request_error");
        assert_eq!(out["error"]["code"], Value::Null);
        assert!(
            out["error"]["message"]
                .as_str()
                .unwrap()
                .starts_with("prompt is too long")
        );
    }

    #[test]
    fn maps_server_error() {
        let body = r#"{"error":{"type":"api_error","message":"上游转发失败"}}"#;
        let out = convert_error_body(StatusCode::INTERNAL_SERVER_ERROR, body.as_bytes());
        assert_eq!(out["error"]["type"], "server_error");
        assert_eq!(out["error"]["code"], Value::Null);
        assert_eq!(out["error"]["message"], "上游转发失败");
    }

    #[test]
    fn maps_authentication_error_to_invalid_api_key() {
        let body = br#"{"error":{"type":"authentication_error","message":"Invalid API key"}}"#;
        let out = convert_error_body(StatusCode::UNAUTHORIZED, body);
        assert_eq!(out["error"]["type"], "invalid_request_error");
        assert_eq!(out["error"]["code"], "invalid_api_key");
    }

    #[test]
    fn maps_quota_exhausted_to_insufficient_quota() {
        let body = br#"{"error":{"type":"quota_exceeded_error","message":"All bound Kiro accounts have exhausted their monthly request quota."}}"#;
        let out = convert_error_body(StatusCode::PAYMENT_REQUIRED, body);
        assert_eq!(out["error"]["type"], "insufficient_quota");
        assert_eq!(out["error"]["code"], "insufficient_quota");
    }

    #[test]
    fn unknown_type_falls_back_to_status_code() {
        let body = br#"{"error":{"type":"brand_new_error","message":"x"}}"#;
        let out = convert_error_body(StatusCode::TOO_MANY_REQUESTS, body);
        assert_eq!(out["error"]["type"], "rate_limit_error");
    }

    #[test]
    fn non_json_body_keeps_snippet_and_status_mapping() {
        let out = convert_error_body(StatusCode::BAD_GATEWAY, b"<html>502 Bad Gateway</html>");
        assert_eq!(out["error"]["type"], "server_error");
        assert_eq!(out["error"]["message"], "<html>502 Bad Gateway</html>");
    }

    #[test]
    fn empty_body_yields_descriptive_message() {
        let out = convert_error_body(StatusCode::INTERNAL_SERVER_ERROR, b"");
        assert_eq!(
            out["error"]["message"],
            "Upstream returned HTTP 500 with an empty body."
        );
    }

    #[test]
    fn oversized_non_json_body_is_truncated_on_char_boundary() {
        // 全中文，字节数远大于字符数：按字节截断会 panic，按字符截断才安全
        let raw = "错".repeat(RAW_BODY_SNIPPET_LIMIT + 50);
        let out = convert_error_body(StatusCode::BAD_GATEWAY, raw.as_bytes());
        let msg = out["error"]["message"].as_str().unwrap();
        assert!(msg.ends_with('…'));
        assert_eq!(msg.chars().count(), RAW_BODY_SNIPPET_LIMIT + 1);
    }

    #[test]
    fn stream_error_extracts_message_and_type() {
        let data = json!({
            "type": "error",
            "error": {"type": "overloaded_error", "message": "Upstream returned an empty response. Please retry."}
        });
        let (msg, t, code) = extract_stream_error(&data);
        assert_eq!(msg, "Upstream returned an empty response. Please retry.");
        assert_eq!(t, "server_error");
        assert_eq!(code, Some("overloaded"));
    }

    #[test]
    fn stream_error_without_detail_has_fallback_message() {
        let (msg, t, code) = extract_stream_error(&json!({"type": "error"}));
        assert_eq!(msg, "Upstream stream failed without a message.");
        assert_eq!(t, "server_error");
        assert_eq!(code, None);
    }

    #[test]
    fn error_response_preserves_status() {
        let resp = error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "previous_response_id is not supported",
            Some("unsupported_parameter"),
        );
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
