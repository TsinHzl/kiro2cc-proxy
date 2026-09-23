// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic API Handler 函数

use crate::anthropic::stream::{CLIENT_ASSUMED_CONTEXT_WINDOW, scale_for_client};
use crate::anthropic::types::{ErrorResponse, MessagesRequest};

use crate::kiro::token_manager::QUOTA_EXHAUSTED_ALL_MARKER;
use anyhow::Error;
use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Json, Response},
};

/// 超窗错误文案（对齐 Anthropic 官方 `prompt is too long: N tokens > M maximum`）。
///
/// N 取客户端展示口径（`scale_for_client`）、M 取 `CLIENT_ASSUMED_CONTEXT_WINDOW`，
/// 与同一会话中 usage 字段口径一致。N 兜底为 M+1：上游报超窗但本地估算异常偏小
/// （远程 count_tokens 返回 0 等）时，照实填会产出 `0 tokens > 200000 maximum`
/// 这种 N ≤ M 的自相矛盾文案 —— 正是本函数要消除的形态。
pub(crate) fn format_prompt_too_long(estimated_input_tokens: i32, model: &str) -> String {
    let n = scale_for_client(estimated_input_tokens, model).max(CLIENT_ASSUMED_CONTEXT_WINDOW + 1);
    format!(
        "prompt is too long: {} tokens > {} maximum",
        n, CLIENT_ASSUMED_CONTEXT_WINDOW
    )
}

pub(crate) fn map_provider_error_with_context(
    err: Error,
    model: &str,
    estimated_input_tokens: i32,
) -> Response {
    let err_str = err.to_string();

    // 上下文窗口满了（对话历史累积超出模型上下文窗口限制）
    if err_str.contains("CONTENT_LENGTH_EXCEEDS_THRESHOLD") {
        tracing::warn!(
            error = %err,
            model = %model,
            estimated_input_tokens = estimated_input_tokens,
            "上游拒绝请求：上下文窗口已满（不应重试）— 请检查是否真正达到 1M 上下文限制"
        );
        // 文案对齐 Anthropic 官方超窗格式 `prompt is too long: N tokens > M maximum`。
        // 原自造文案不匹配任何客户端识别模式，Claude Code 收到后只会硬报错中断（#25）；
        // 官方格式才有机会被识别为「压缩后重试」。两个数字统一用客户端展示口径
        // （N 经 scale_for_client 缩放、M 取客户端假设的 200K 窗口），与同一会话中
        // usage 字段的口径一致，客户端自算 Ctx% 不会与这段文案矛盾。
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "invalid_request_error",
                format_prompt_too_long(estimated_input_tokens, model),
            )),
        )
            .into_response();
    }

    // 单次输入太长（请求体本身超出上游限制）
    if err_str.contains("Input is too long") {
        tracing::warn!(error = %err, "上游拒绝请求：输入过长（不应重试）");
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "invalid_request_error",
                "Input is too long. Reduce the size of your messages.",
            )),
        )
            .into_response();
    }
    // 额度耗尽（402）：range 内所有（模型过滤后）账号本月额度均已用尽。
    // 必须优先于 429 判断，且只信任 describe_unavailable 产出的机器可识别标记——
    // 该标记只在"scope 内 100% 账号确认为 QuotaExceeded"时才会写入，裸匹配
    // "MONTHLY_REQUEST_COUNT" 会在"单账号耗尽、其余账号仍可用"时被误判为全部耗尽。
    // 额度当月不会恢复，必须返回 402 而非 5xx/429 —— 否则 Claude Code 等客户端会
    // 判定为 temporary 故障并反复重试，定时任务会整轮空转。
    if err_str.contains(QUOTA_EXHAUSTED_ALL_MARKER) {
        tracing::error!(error = %err, "上游额度耗尽：返回 402 告知客户端不可重试");
        return (
            StatusCode::PAYMENT_REQUIRED,
            Json(ErrorResponse::new(
                "quota_exceeded_error",
                "All bound Kiro accounts have exhausted their monthly request quota. \
                 Quota resets at the start of next month, or add/enable another account \
                 in the admin panel.",
            )),
        )
            .into_response();
    }

    // 上游限流（429 Too Many Requests）：所有账号重试后仍被限流。
    // 必须把 429 透传给客户端（而非转成 502），让 Claude Code 等客户端的
    // 内置指数退避重试接管 —— 502 会被客户端判定为硬失败，导致"请求那一轮直接废掉"
    // （表现为工具调用不执行 / 卡住），而 429 会触发客户端自动等待重试。
    if err_str.contains("429") || err_str.contains("Too Many Requests") {
        tracing::warn!(error = %err, "上游限流（所有账号 429 耗尽）：透传 429 给客户端以触发其退避重试");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "5")],
            Json(ErrorResponse::new(
                "rate_limit_error",
                "Upstream rate limit reached on all accounts. Please retry shortly.",
            )),
        )
            .into_response();
    }

    // 兜底：完整错误详情只进日志，不回显给客户端——describe_unavailable 等诊断文案
    // 含账号数量/禁用原因拆解，属内部状态，不应通过客户端可见的响应体外泄。
    tracing::error!(error = %err, "Kiro API 调用失败");
    (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse::new(
            "api_error",
            "Upstream API call failed. Please retry shortly.",
        )),
    )
        .into_response()
}

/// 从原始请求体反序列化 MessagesRequest，失败时记录详细的 serde 错误用于诊断。
///
/// 替代 axum 的 `Json<MessagesRequest>` 提取器——后者反序列化失败时直接返回 400
/// 且不记录任何信息，导致无法定位是哪个字段/格式导致客户端请求被拒。
/// 此函数在失败时打印 serde 错误（行列+字段路径）、body 长度、出错位置附近的片段。
#[allow(clippy::result_large_err)]
pub(crate) fn parse_messages_request(body: &[u8]) -> Result<MessagesRequest, Response> {
    match serde_json::from_slice::<MessagesRequest>(body) {
        Ok(req) => Ok(req),
        Err(e) => {
            // serde_json 错误自带行列号；定位出错字节附近的片段辅助判断
            let line = e.line();
            let col = e.column();
            // 估算出错字节偏移附近的上下文（按行列粗略定位，取该行附近 200 字节）
            let body_str = String::from_utf8_lossy(body);
            let snippet: String = body_str
                .lines()
                .nth(line.saturating_sub(1))
                .map(|l| {
                    let start = col.saturating_sub(80);
                    l.chars().skip(start).take(200).collect()
                })
                .unwrap_or_default();
            tracing::error!(
                error = %e,
                serde_line = line,
                serde_col = col,
                body_len = body.len(),
                snippet = %snippet,
                "[REQ-DIAG] /v1/messages 请求体反序列化失败（导致 400，客户端那轮中断）"
            );
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "invalid_request_error",
                    format!("Request body could not be parsed: {}", e),
                )),
            )
                .into_response())
        }
    }
}
