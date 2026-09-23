// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 非流式部分：Anthropic Messages 响应 → `chat.completion` 对象

use serde_json::{Map, Value, json};
use uuid::Uuid;

/// 生成 `chatcmpl-<32位hex>` 形式的响应 id
pub(crate) fn new_completion_id() -> String {
    format!("chatcmpl-{}", Uuid::new_v4().simple())
}

/// 当前 Unix 秒
pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// Anthropic `stop_reason` → OpenAI `finish_reason`
///
/// 上游可能产出的四种取值均显式覆盖（`src/anthropic/handlers.rs` 中
/// `end_turn` / `tool_use` / `max_tokens` / `model_context_window_exceeded`）。
/// 出现未枚举值时回退 `stop` 并留痕，便于发现上游新增状态。
pub(crate) fn map_finish_reason(stop_reason: Option<&str>) -> &'static str {
    match stop_reason {
        Some("end_turn") | Some("stop_sequence") | None => "stop",
        Some("tool_use") => "tool_calls",
        Some("max_tokens") | Some("model_context_window_exceeded") => "length",
        Some(other) => {
            tracing::warn!(stop_reason = %other, "未识别的上游 stop_reason，finish_reason 回退为 stop");
            "stop"
        }
    }
}

/// 从 Anthropic content 块数组中抽取的可见内容
#[derive(Debug, Default)]
struct ExtractedContent {
    text: String,
    reasoning: String,
    tool_calls: Vec<Value>,
}

/// 遍历 Anthropic content 数组，按块类型分拣
///
/// `thinking` 块的 `signature` 字段是为通过下游检测伪造的无语义串
/// （`src/anthropic/stream.rs` 的 `generate_fake_signature`），只取 `thinking` 文本，
/// 绝不把签名混进任何面向客户端的字段。
fn extract_content(content: Option<&Value>) -> ExtractedContent {
    let mut out = ExtractedContent::default();
    let Some(blocks) = content.and_then(Value::as_array) else {
        return out;
    };

    for block in blocks {
        match block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "text" => {
                if let Some(t) = block.get("text").and_then(Value::as_str) {
                    out.text.push_str(t);
                }
            }
            "thinking" => {
                if let Some(t) = block.get("thinking").and_then(Value::as_str) {
                    out.reasoning.push_str(t);
                }
            }
            "tool_use" => {
                let index = out.tool_calls.len();
                if let Some(call) = tool_use_to_call(block, index) {
                    out.tool_calls.push(call);
                }
            }
            other => {
                tracing::warn!(block_type = %other, "未识别的上游 content 块类型，已跳过");
            }
        }
    }

    out
}

/// Anthropic `tool_use` block → OpenAI `tool_calls[]` 项
///
/// `index` 仅在流式 chunk 中是必需字段，非流式响应里 OpenAI 也会带上，保持一致。
///
/// 缺 `id` 或 `name` 的块无法构造合法 `tool_calls` 项，只能跳过；但必须留痕，否则客户端
/// 收到的响应会莫名少一次工具调用而无从排查。
fn tool_use_to_call(block: &Value, index: usize) -> Option<Value> {
    let id = block.get("id").and_then(Value::as_str);
    let name = block.get("name").and_then(Value::as_str);
    let (Some(id), Some(name)) = (id, name) else {
        tracing::warn!(
            has_id = id.is_some(),
            has_name = name.is_some(),
            "上游 tool_use 块缺少 id 或 name，已跳过该工具调用"
        );
        return None;
    };
    // OpenAI 的 arguments 是 JSON 字符串，不是对象
    let arguments = block
        .get("input")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string());

    Some(json!({
        "id": id,
        "type": "function",
        "index": index,
        "function": {"name": name, "arguments": arguments},
    }))
}

/// 把 Anthropic 的 usage 换算为 OpenAI usage
///
/// Anthropic 的 `input_tokens` **不含**缓存部分——`crate::cache::select_final_usage` 的每个分支
/// 都会从总量里减掉 `cache_read` 与 `cache_creation`。而 OpenAI 的 `prompt_tokens` 是输入总量，
/// 缓存命中另由 `prompt_tokens_details.cached_tokens` 单列。直接对齐会让 `prompt_tokens` 系统性
/// 偏低（默认比例模拟下约少一半），客户端的成本统计与上下文水位随之失真，所以这里把三部分加回来。
///
/// `cached_tokens` 只取 `cache_read_input_tokens`：OpenAI 的语义是"命中缓存的输入"，
/// cache creation 是写入新缓存，不算命中。
pub(crate) fn convert_usage(usage: Option<&Value>) -> Value {
    let read = |key: &str| {
        usage
            .and_then(|u| u.get(key))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    let cached = read("cache_read_input_tokens");
    let prompt = read("input_tokens") + cached + read("cache_creation_input_tokens");
    let completion = read("output_tokens");
    json!({
        "prompt_tokens": prompt,
        "completion_tokens": completion,
        "total_tokens": prompt + completion,
        "prompt_tokens_details": {"cached_tokens": cached},
    })
}

/// 把 Anthropic 非流式响应转换为 `chat.completion` 对象
pub(crate) fn convert_non_stream(anthropic: &Value, client_model: &str) -> Value {
    let extracted = extract_content(anthropic.get("content"));
    let stop_reason = anthropic.get("stop_reason").and_then(Value::as_str);
    let finish_reason = map_finish_reason(stop_reason);

    let mut message = Map::new();
    message.insert("role".to_string(), json!("assistant"));
    // 只有工具调用、没有可见文本时 content 为 null（OpenAI 的约定）
    message.insert(
        "content".to_string(),
        if extracted.text.is_empty() && !extracted.tool_calls.is_empty() {
            Value::Null
        } else {
            json!(extracted.text)
        },
    );
    if !extracted.reasoning.is_empty() {
        message.insert("reasoning_content".to_string(), json!(extracted.reasoning));
    }
    if !extracted.tool_calls.is_empty() {
        message.insert("tool_calls".to_string(), json!(extracted.tool_calls));
    }

    json!({
        "id": new_completion_id(),
        "object": "chat.completion",
        "created": unix_now(),
        "model": client_model,
        "choices": [{
            "index": 0,
            "message": Value::Object(message),
            "finish_reason": finish_reason,
        }],
        "usage": convert_usage(anthropic.get("usage")),
    })
}
