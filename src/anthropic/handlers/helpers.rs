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

/// 提取客户端 User-Agent（截断至 256 字符）
///
/// UA 由客户端可控输入，超长值会撑爆落库 JSON 与明细文件，统一截断；
/// 常见 UA（claude-cli/codex 等）远短于该上限，截断不影响正常展示。
/// 非 UTF-8 字节经 lossy 替换（U+FFFD）降级保留可读部分，避免含非 ASCII
/// 的 UA 被整体丢弃、与"未携带"不可区分；chars 截断最坏 4 字节/char
/// （256 char ≤ 1024B），落库体积仍可控。
pub(crate) fn extract_user_agent(headers: &axum::http::HeaderMap) -> Option<String> {
    const MAX_UA_CHARS: usize = 256;
    let value = headers.get(axum::http::header::USER_AGENT)?;
    let ua = String::from_utf8_lossy(value.as_bytes());
    let ua = ua.trim();
    if ua.is_empty() {
        return None;
    }
    Some(ua.chars().take(MAX_UA_CHARS).collect())
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

/// 取文本前 `max_chars` 个字符作预览：控制字符（换行、制表等）替换为空格，
/// 超长时追加 `…`，保证单行日志可读且体积有界。
fn preview_text(s: &str, max_chars: usize) -> String {
    let mut out: String = s
        .chars()
        .take(max_chars)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if s.chars().nth(max_chars).is_some() {
        out.push('…');
    }
    out
}

/// 提取消息内容的预览：返回 (块类型列表, 首个文本的前 N 字符)。
///
/// content 为字符串时视为单个 text 块；为数组时按块顺序列出 type。
fn message_content_preview(content: &serde_json::Value, max_chars: usize) -> (String, String) {
    match content {
        serde_json::Value::String(s) => ("text".to_string(), preview_text(s, max_chars)),
        serde_json::Value::Array(blocks) => {
            let types = blocks
                .iter()
                .map(|b| b.get("type").and_then(|v| v.as_str()).unwrap_or("?"))
                .collect::<Vec<_>>()
                .join(",");
            let text = blocks
                .iter()
                .find_map(|b| b.get("text").and_then(|t| t.as_str()))
                .map(|t| preview_text(t, max_chars))
                .unwrap_or_default();
            (types, text)
        }
        _ => ("?".to_string(), String::new()),
    }
}

/// 小请求诊断：对 `max_tokens <= 8192 && !stream` 的请求输出一行特征日志。
///
/// 背景：部分客户端（如 VS Code 扩展）会发出体积很小、输出几个 token 的辅助请求
/// （标题 / 摘要 / 输入建议等），未命中 Suggestion Mode 拦截时会被全价转发上游。
/// 为确认其真实类型，这里仅记录识别所需的最少特征，不改变任何转发行为。
///
/// 隐私：只截取 system 首块与首/末条消息各前 80 字符，控制字符折叠为空格；
/// 仅小请求触发，主对话请求不受影响。
pub(crate) fn log_small_request_fingerprint(
    payload: &MessagesRequest,
    headers: &axum::http::HeaderMap,
) {
    const PREVIEW_CHARS: usize = 80;
    // 8192 同时覆盖 issue #64 观察到的 max_tokens=64 与 8192 两种变体；
    // Claude Code 主对话恒为流式，非流式小上限请求基本只来自辅助调用
    const MAX_TOKENS_LIMIT: i32 = 8192;
    if payload.max_tokens > MAX_TOKENS_LIMIT || payload.stream {
        return;
    }

    let ua = extract_user_agent(headers).unwrap_or_default();
    let beta = headers
        .get("anthropic-beta")
        .map(|v| {
            String::from_utf8_lossy(v.as_bytes())
                .chars()
                .take(200)
                .collect::<String>()
        })
        .unwrap_or_default();
    let system_head = payload
        .system
        .as_ref()
        .and_then(|s| s.first())
        .map(|s| preview_text(&s.text, PREVIEW_CHARS))
        .unwrap_or_default();
    let (first_types, first_text, first_role) = payload
        .messages
        .first()
        .map(|m| {
            let (t, p) = message_content_preview(&m.content, PREVIEW_CHARS);
            (t, p, m.role.as_str())
        })
        .unwrap_or_default();
    let (last_types, last_text, last_role) = payload
        .messages
        .last()
        .map(|m| {
            let (t, p) = message_content_preview(&m.content, PREVIEW_CHARS);
            (t, p, m.role.as_str())
        })
        .unwrap_or_default();

    tracing::info!(
        user_agent = %ua,
        anthropic_beta = %beta,
        model = %payload.model,
        max_tokens = %payload.max_tokens,
        message_count = %payload.messages.len(),
        tools = %payload.tools.as_ref().map_or(0, |t| t.len()),
        system_blocks = %payload.system.as_ref().map_or(0, |s| s.len()),
        has_thinking = %payload.thinking.is_some(),
        system_head = %system_head,
        first_role = %first_role,
        first_blocks = %first_types,
        first_text = %first_text,
        last_role = %last_role,
        last_blocks = %last_types,
        last_text = %last_text,
        "[small-req] 小请求特征（max_tokens<=8192 非流式，用于识别未被拦截的辅助请求）"
    );
}

/// 估算缓存前缀 tokens 并按 `prefix_cached` 门控 cache_read 口径。
///
/// 返回 `(raw_prefix_tokens, prefix_estimated_tokens)`：
/// - `raw_prefix_tokens`：system + tools + 除最后一条消息外的全部历史（`count_prefix_tokens`
///   口径），**始终**供 `count_all_tokens_with_prefix` 累加总输入，不受门控影响
/// - `prefix_estimated_tokens`：仅当会话前缀已命中冻结缓存（`ConversionResult::prefix_cached`，
///   详见其文档）时等于 `raw_prefix_tokens`，否则为 0 —— 新窗口首条消息、单条消息重发、
///   history[0] 首次出现/漂移重置的轮次不计入 cache_read
///
/// `/v1/messages` 与 `/cc/v1/messages` 共用，保证两端点口径一致。
pub(crate) fn estimate_prefix_tokens(payload: &MessagesRequest, prefix_cached: bool) -> (u64, i32) {
    let n = payload.messages.len();
    let prior: &[_] = if n > 0 {
        &payload.messages[..n - 1]
    } else {
        &[]
    };
    let raw =
        token::count_prefix_tokens(payload.system.as_deref(), prior, payload.tools.as_deref());
    let gated = if prefix_cached { raw as i32 } else { 0 };
    (raw, gated)
}

/// 为建议请求构造空响应（按 stream 分流）。
///
/// 流式：返回完整 SSE 事件序列（message_start + 空 text 块 + end_turn 收尾）。
/// 非流式：返回等价的 JSON message 对象。
///
/// Claude Code 对该响应只取候选文本，空文本等价于"无建议"；返回结构完整的
/// 正常响应而非错误，避免客户端把建议失败当主对话故障重试。
///
/// `model` 回显请求模型、usage 全部为 0：下游计费网关（如 sub2api）按响应的
/// model / usage 记账，回显假模型名会被标记"模型不一致"，非零 output 会被计费。
pub(crate) fn suggestion_mode_response(stream: bool, model: &str) -> axum::response::Response {
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
                    "model": model,
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                    "usage": {
                        "input_tokens": 0,
                        "output_tokens": 0,
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
                    "model": model,
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": {
                        "input_tokens": 0,
                        "output_tokens": 0,
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
                "usage": { "input_tokens": 0, "output_tokens": 0 }
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

    async fn body_string(resp: axum::response::Response) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn suggestion_response_echoes_model_with_zero_usage() {
        let v: serde_json::Value = serde_json::from_str(
            &body_string(suggestion_mode_response(false, "claude-opus-5-5")).await,
        )
        .unwrap();
        assert_eq!(v["model"], "claude-opus-5-5");
        assert_eq!(v["usage"]["output_tokens"], 0);
        assert_eq!(v["usage"]["input_tokens"], 0);

        let sse = body_string(suggestion_mode_response(true, "claude-opus-5-5")).await;
        assert!(sse.contains("\"model\":\"claude-opus-5-5\""));
        assert!(!sse.contains("\"suggestion\""));
        assert!(!sse.contains("\"output_tokens\":1"));
    }

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

    #[test]
    fn preview_text_folds_control_chars_and_truncates() {
        assert_eq!(preview_text("ab\ncd\te", 10), "ab cd e");
        assert_eq!(preview_text("abcdef", 3), "abc…");
        // 恰好等于上限不追加省略号
        assert_eq!(preview_text("abc", 3), "abc");
        // 按字符而非字节截断，多字节不应 panic
        assert_eq!(preview_text("你好世界", 2), "你好…");
    }

    #[test]
    fn message_content_preview_handles_string_and_blocks() {
        let (t, p) = message_content_preview(&json!("hello"), 80);
        assert_eq!((t.as_str(), p.as_str()), ("text", "hello"));

        let content = json!([
            {"type": "tool_result", "content": "x"},
            {"type": "text", "text": "first text"},
            {"type": "text", "text": "second"}
        ]);
        let (t, p) = message_content_preview(&content, 80);
        assert_eq!(t, "tool_result,text,text");
        assert_eq!(p, "first text");

        let (t, p) = message_content_preview(&json!(null), 80);
        assert_eq!((t.as_str(), p.as_str()), ("?", ""));
    }

    #[test]
    fn small_request_fingerprint_does_not_panic_on_edge_payloads() {
        // 无 system / 无消息 / 流式 / 大 max_tokens：均不应 panic
        let mk = |max_tokens: i32, stream: bool, msgs: serde_json::Value| -> MessagesRequest {
            serde_json::from_value(json!({
                "model": "claude-sonnet-5",
                "max_tokens": max_tokens,
                "stream": stream,
                "messages": msgs
            }))
            .unwrap()
        };
        let headers = axum::http::HeaderMap::new();
        log_small_request_fingerprint(&mk(64, false, json!([])), &headers);
        log_small_request_fingerprint(
            &mk(
                64,
                false,
                json!([{"role": "user", "content": "你好\n世界"}]),
            ),
            &headers,
        );
        log_small_request_fingerprint(&mk(64, true, json!([])), &headers);
        log_small_request_fingerprint(&mk(8192, false, json!([])), &headers);
        log_small_request_fingerprint(&mk(128000, false, json!([])), &headers);
    }

    fn headers_with_ua(ua: &str) -> axum::http::HeaderMap {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::USER_AGENT,
            axum::http::HeaderValue::from_str(ua).unwrap(),
        );
        headers
    }

    #[test]
    fn extract_user_agent_normal_value_passthrough() {
        let headers = headers_with_ua("claude-cli/2.1.5 (external, cli)");
        assert_eq!(
            extract_user_agent(&headers).as_deref(),
            Some("claude-cli/2.1.5 (external, cli)")
        );
    }

    #[test]
    fn extract_user_agent_missing_or_blank_is_none() {
        // 头缺失 → None
        assert_eq!(extract_user_agent(&axum::http::HeaderMap::new()), None);
        // 仅空白字符（trim 后为空）→ None
        assert_eq!(extract_user_agent(&headers_with_ua("   ")), None);
    }

    #[test]
    fn extract_user_agent_truncated_at_256_chars() {
        let long = "a".repeat(300);
        let result = extract_user_agent(&headers_with_ua(&long)).unwrap();
        assert_eq!(result.len(), 256);
    }

    #[test]
    fn extract_user_agent_surrounding_whitespace_trimmed() {
        let headers = headers_with_ua("  claude-cli/2.1.5  ");
        assert_eq!(
            extract_user_agent(&headers).as_deref(),
            Some("claude-cli/2.1.5")
        );
    }

    #[test]
    fn extract_user_agent_non_ascii_lossy_preserved() {
        // 含非法 UTF-8 字节的 UA：降级保留可读部分（非法字节 → U+FFFD），
        // 不应整体返回 None 与"未携带"混淆
        let value =
            axum::http::HeaderValue::from_bytes(b"claude-cli/2.1.5 (\xFF\xFE, cli)").unwrap();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(axum::http::header::USER_AGENT, value);
        let result = extract_user_agent(&headers).unwrap();
        assert!(result.starts_with("claude-cli/2.1.5 ("));
        assert!(result.contains('\u{FFFD}'));
        assert!(result.ends_with(", cli)"));
    }

    #[test]
    fn extract_user_agent_valid_utf8_multibyte_preserved() {
        // 合法多字节 UTF-8 UA（如中文标注）应原样保留而非被丢弃
        let headers = headers_with_ua("MyClient/1.0 (中文客户端)");
        assert_eq!(
            extract_user_agent(&headers).as_deref(),
            Some("MyClient/1.0 (中文客户端)")
        );
    }
}
