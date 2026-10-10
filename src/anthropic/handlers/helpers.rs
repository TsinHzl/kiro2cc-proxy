// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Handler 杂项工具：剥离 JSON 代码围栏、提取客户端 IP、按模型名覆盖 thinking 开关、/v1/messages/count_tokens 计数端点、suggestion 模式请求识别与快速响应。

use crate::anthropic::converter::is_luna_model;
use crate::anthropic::types::{
    CountTokensRequest, CountTokensResponse, MessagesRequest, OutputConfig, Thinking,
};

use crate::token;
use axum::{
    Json as JsonExtractor,
    response::{IntoResponse, Json},
};

/// 去除 JSON 响应中模型可能添加的 Markdown 代码围栏
///
/// 当请求 JSON schema 结构化输出时，部分模型仍会将结果包裹在 ```json...``` 中。
/// 此函数识别并剥离这些围栏，返回纯 JSON 文本。
pub(crate) fn strip_json_fences(text: String) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return text;
    }
    let after_fence = if let Some(rest) = trimmed.strip_prefix("```json\n") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("```json\r\n") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("```\n") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("```\r\n") {
        rest
    } else {
        return text;
    };
    let result = after_fence
        .strip_suffix("\n```")
        .or_else(|| after_fence.strip_suffix("\r\n```"))
        .or_else(|| after_fence.strip_suffix("```"))
        .unwrap_or(after_fence);
    result.to_string()
}

/// 从请求头或连接信息提取客户端真实 IP
pub(crate) fn extract_client_ip(
    headers: &axum::http::HeaderMap,
    connect_info: Option<&std::net::SocketAddr>,
) -> Option<String> {
    if let Some(val) = headers.get("x-forwarded-for")
        && let Ok(s) = val.to_str()
    {
        let ip = s.split(',').next().unwrap_or("").trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }
    if let Some(val) = headers.get("x-real-ip")
        && let Ok(s) = val.to_str()
    {
        let ip = s.trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }
    connect_info.map(|addr| addr.ip().to_string())
}

/// 检测模型名是否包含 "thinking" 后缀，若包含则覆写 thinking 配置
///
/// - Opus 4.6/4.7/4.8/5：覆写为 adaptive 类型
/// - 其他模型：覆写为 enabled 类型
/// - budget_tokens 固定为 20000
pub(crate) fn override_thinking_from_model_name(payload: &mut MessagesRequest) {
    let model_lower = payload.model.to_lowercase();
    if !model_lower.contains("thinking") {
        return;
    }

    let is_opus_adaptive = model_lower.contains("opus")
        && (model_lower.contains("4-6")
            || model_lower.contains("4.6")
            || model_lower.contains("4-7")
            || model_lower.contains("4.7")
            || model_lower.contains("4-8")
            || model_lower.contains("4.8")
            || model_lower.contains("opus-5")
            || model_lower.contains("opus.5")
            || model_lower.contains("opus 5"));

    let thinking_type = if is_opus_adaptive {
        "adaptive"
    } else {
        "enabled"
    };

    tracing::info!(
        model = %payload.model,
        thinking_type = thinking_type,
        "模型名包含 thinking 后缀，覆写 thinking 配置"
    );

    payload.thinking = Some(Thinking {
        thinking_type: thinking_type.to_string(),
        budget_tokens: 20000,
    });

    if is_opus_adaptive {
        payload.output_config = Some(OutputConfig {
            effort: "high".to_string(),
            format: None,
        });
    }
}

/// 判断本次请求是否应启用流式 thinking 处理路径（`<thinking>` 标签的提取与转换）。
///
/// **范围仅限 `gpt-5.6-luna`**，不包括 terra/sol。原因：
/// - `converter::generate_thinking_prefix` / `build_additional_model_request_fields`
///   对全部 GPT 系模型跳过 thinking 注入（有 400 REQUEST_BODY_INVALID 实测依据，
///   范围覆盖整个 gpt-5.6 系列），这一点本函数无需重复处理。
/// - 但"上游是否会真的输出 `<thinking>...</thinking>` 标签、是否具备可用的推理
///   能力"是另一件事，代码库里只有 luna 的实测记录（"上游恒返回 `thinking=0`"，
///   见 README/`openai::model_map` 已知限制）。terra/sol 没有类似证据，不应假定
///   它们也不产出 thinking——若在此处也对它们强制关闭，会误伤其本该具备的、可能
///   仍受客户端 thinking 请求影响的推理能力。
///
/// 因此仅对 luna 强制关闭 `thinking_enabled`：若仍按客户端请求启用，流式状态机会
/// 持续寻找永不出现的 `<thinking>` 标签，而 luna 在缺乏协议约束时有概率自行选择用
/// `<analysis>`/`<summary>` 等自造伪标签组织输出，被当作普通可见文本原样转发给
/// 客户端（配合 `converter::gpt_anti_pseudo_tag_hint` 的提示词引导兜底）。
/// terra/sol 恢复原有行为：`thinking_enabled` 仅取决于客户端是否请求。
pub(crate) fn resolve_thinking_enabled(model: &str, thinking: &Option<Thinking>) -> bool {
    if is_luna_model(model) {
        return false;
    }
    thinking.as_ref().map(|t| t.is_enabled()).unwrap_or(false)
}

/// 综合账号级 thinking 开关，决定响应侧是否按"启用 thinking"处理（纯函数，便于测试）。
///
/// 账号开关是最终裁决，与客户端请求无关：
/// - `Some(false)`：一律关闭——请求侧已剥离 thinking 控制，响应侧同步丢弃上游推理内容；
/// - `Some(true)`：`native_supported`（模型支持原生 adaptive 字段，请求侧会强制注入）
///   时一律开启；否则无法强制，退回客户端请求；
/// - `None`（账号已不存在）：退回客户端请求。
pub(crate) fn resolve_effective_thinking(
    account_switch: Option<bool>,
    native_supported: bool,
    client_enabled: bool,
) -> bool {
    match account_switch {
        Some(false) => false,
        Some(true) => native_supported || client_enabled,
        None => client_enabled,
    }
}

/// effort 入库口径（与 thinking 裁决结果联动，纯函数便于测试）：
/// - thinking 生效：保留客户端携带值；未携带时补记 "high"——与 `types.rs`
///   `default_effort()`（output_config 存在但未写 effort 时的 serde 默认）同口径，
///   仅作徽章展示约定。注意与 `converter/fields.rs` 注入上游的兜底值 "low"
///   （issue #40 TTFB 实测下调）是两个不同口径：wire 上无 effort 字段的模型
///   （如 4.5 代际走文本标签协议）徽章值不回传上游，互不影响；
/// - thinking 未生效（账号开关关闭 / 客户端未请求 / luna）：一律丢弃，徽章不展示。
pub(crate) fn resolve_recorded_effort(
    thinking_effective: bool,
    effort: Option<String>,
) -> Option<String> {
    if thinking_effective {
        Some(effort.unwrap_or_else(|| "high".to_string()))
    } else {
        None
    }
}

/// 按实际选中的账号计算响应侧 thinking 是否启用（见 [`resolve_effective_thinking`]）。
///
/// `model` 为客户端原始模型名，内部经 `map_model` 归一化后判定是否支持原生字段。
pub(crate) fn effective_thinking_enabled(
    provider: &crate::kiro::provider::KiroProvider,
    credential_id: u64,
    model: &str,
    client_enabled: bool,
) -> bool {
    let mapped = crate::anthropic::converter::map_model(model).unwrap_or_else(|| model.to_string());
    resolve_effective_thinking(
        provider.token_manager().thinking_adaptive_of(credential_id),
        crate::anthropic::converter::native_thinking_supported(&mapped),
        client_enabled,
    )
}

/// POST /v1/messages/count_tokens
///
/// 计算消息的 token 数量
pub async fn count_tokens(
    JsonExtractor(payload): JsonExtractor<CountTokensRequest>,
) -> impl IntoResponse {
    tracing::info!(
        model = %payload.model,
        message_count = %payload.messages.len(),
        "Received POST /v1/messages/count_tokens request"
    );

    let total_tokens = token::count_all_tokens(
        payload.model,
        payload.system,
        payload.messages,
        payload.tools,
    ) as i32;

    Json(CountTokensResponse {
        input_tokens: total_tokens.max(1),
    })
}

/// 检测 Claude Code 的输入建议请求（Suggestion Mode）。
///
/// 此类请求是客户端在每轮主对话结束后自动发起的"预测用户下一条输入"辅助请求，
/// 携带完整 50k+ 上下文却只产出几条候选短语。经 Kiro 全量转发会按全价计费
/// （metering ~0.37 credits/次），且其末条消息会导致 history[0] 前缀指纹漂移、
/// 干扰主对话的 prompt cache。这里识别后直接返回空文本响应，不转发上游。
///
/// 判据：最后一条消息为 user 且文本以 `[SUGGESTION MODE` 标记开头（该标记
/// 固定位于消息首部，用 starts_with 而非 contains 降低误伤面）。
pub(crate) fn is_suggestion_mode_request(payload: &MessagesRequest) -> bool {
    payload
        .messages
        .last()
        .filter(|m| m.role == "user")
        .is_some_and(|m| match &m.content {
            serde_json::Value::String(s) => s.starts_with("[SUGGESTION MODE"),
            // 标记固定位于消息首部：仅检查首块，避免首块为普通文本、
            // 后续块恰好以标记开头时误吞整条正常消息
            serde_json::Value::Array(blocks) => blocks.first().is_some_and(|b| {
                b.get("type").and_then(|v| v.as_str()) == Some("text")
                    && b.get("text")
                        .and_then(|t| t.as_str())
                        .is_some_and(|t| t.starts_with("[SUGGESTION MODE"))
            }),
            _ => false,
        })
}

/// 为建议请求构造空响应（按 stream 分流）。
///
/// 流式：返回完整 SSE 事件序列（message_start + 空 text 块 + end_turn 收尾）。
/// 非流式：返回等价的 JSON message 对象。
///
/// Claude Code 对该响应只取候选文本，空文本等价于"无建议"；返回结构完整的
/// 正常响应而非错误，避免客户端把建议失败当主对话故障重试。
pub(crate) fn suggestion_mode_response(stream: bool) -> axum::response::Response {
    if !stream {
        return axum::response::Response::builder()
            .status(axum::http::StatusCode::OK)
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Body::from(
                serde_json::json!({
                    "id": "msg_suggestion",
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "text", "text": "" }],
                    "model": "suggestion",
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                    "usage": {
                        "input_tokens": 0,
                        "output_tokens": 1,
                        "cache_creation_input_tokens": 0,
                        "cache_read_input_tokens": 0
                    }
                })
                .to_string(),
            ))
            .unwrap();
    }

    use crate::anthropic::stream::SseEvent;

    let events = [
        SseEvent::new(
            "message_start",
            serde_json::json!({
                "type": "message_start",
                "message": {
                    "id": format!("msg_suggestion_{}", std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or_default()),
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "model": "suggestion",
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": {
                        "input_tokens": 0,
                        "output_tokens": 1,
                        "cache_creation_input_tokens": 0,
                        "cache_read_input_tokens": 0
                    }
                }
            }),
        ),
        SseEvent::new(
            "content_block_start",
            serde_json::json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "text", "text": "" }
            }),
        ),
        SseEvent::new(
            "content_block_stop",
            serde_json::json!({ "type": "content_block_stop", "index": 0 }),
        ),
        SseEvent::new(
            "message_delta",
            serde_json::json!({
                "type": "message_delta",
                "delta": { "stop_reason": "end_turn", "stop_sequence": null },
                "usage": { "input_tokens": 0, "output_tokens": 1 }
            }),
        ),
        SseEvent::new(
            "message_stop",
            serde_json::json!({ "type": "message_stop" }),
        ),
    ];

    let body = events
        .iter()
        .map(|e| e.to_sse_string())
        .collect::<Vec<_>>()
        .join("");

    axum::http::Response::builder()
        .status(axum::http::StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
        .header(axum::http::header::CACHE_CONTROL, "no-cache")
        .body(axum::body::Body::from(body))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request_with_last_message(role: &str, content: serde_json::Value) -> MessagesRequest {
        serde_json::from_value(json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 1024,
            "messages": [
                { "role": "user", "content": "hi" },
                { "role": "assistant", "content": "hello" },
                { "role": role, "content": content }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn suggestion_string_content_hit() {
        let p = request_with_last_message(
            "user",
            json!("[SUGGESTION MODE: Suggest what the user might naturally type next.]"),
        );
        assert!(is_suggestion_mode_request(&p));
    }

    #[test]
    fn suggestion_block_content_hit() {
        let p = request_with_last_message(
            "user",
            json!([{ "type": "text", "text": "[SUGGESTION MODE: suggest next input]" }]),
        );
        assert!(is_suggestion_mode_request(&p));
    }

    #[test]
    fn suggestion_marker_not_at_start_misses() {
        let p = request_with_last_message("user", json!("请问 [SUGGESTION MODE 是什么协议？"));
        assert!(!is_suggestion_mode_request(&p));
    }

    #[test]
    fn assistant_role_misses() {
        let p = request_with_last_message("assistant", json!("[SUGGESTION MODE: ...]"));
        assert!(!is_suggestion_mode_request(&p));
    }

    #[test]
    fn empty_messages_miss() {
        let p: MessagesRequest = serde_json::from_value(json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 1024,
            "messages": []
        }))
        .unwrap();
        assert!(!is_suggestion_mode_request(&p));
    }

    #[test]
    fn normal_user_message_misses() {
        let p = request_with_last_message("user", json!("帮我看看这个 bug"));
        assert!(!is_suggestion_mode_request(&p));
    }

    #[test]
    fn normal_string_with_marker_inside_misses() {
        // 字符串形态：标记出现在文本中部而非开头，不应误判
        let p =
            request_with_last_message("user", json!("请问 [SUGGESTION MODE] 这个标记是什么意思？"));
        assert!(!is_suggestion_mode_request(&p));
    }

    #[test]
    fn suggestion_marker_in_later_block_misses() {
        // 首块为普通文本、后续块以标记开头：标记不在消息首部，不应误判
        let p = request_with_last_message(
            "user",
            json!([
                { "type": "text", "text": "普通内容" },
                { "type": "text", "text": "[SUGGESTION MODE: suggest next input]" }
            ]),
        );
        assert!(!is_suggestion_mode_request(&p));
    }

    #[test]
    fn suggestion_first_block_non_text_misses() {
        // 首块非 text 类型时即使带标记也不判中（标记必须位于消息首部）
        let p = request_with_last_message(
            "user",
            json!([
                { "type": "image", "text": "[SUGGESTION MODE: x]" },
                { "type": "text", "text": "[SUGGESTION MODE: x]" }
            ]),
        );
        assert!(!is_suggestion_mode_request(&p));
    }

    #[test]
    fn recorded_effort_kept_when_thinking_effective() {
        assert_eq!(
            resolve_recorded_effort(true, Some("low".to_string())),
            Some("low".to_string())
        );
    }

    #[test]
    fn recorded_effort_defaults_high_when_thinking_effective_without_value() {
        assert_eq!(
            resolve_recorded_effort(true, None),
            Some("high".to_string())
        );
    }

    #[test]
    fn recorded_effort_dropped_when_thinking_disabled() {
        // 账号开关关闭 / 客户端未请求 thinking：即使请求携带 effort 也不入库
        assert_eq!(
            resolve_recorded_effort(false, Some("high".to_string())),
            None
        );
        assert_eq!(resolve_recorded_effort(false, None), None);
    }
}
