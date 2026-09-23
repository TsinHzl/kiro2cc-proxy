// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 非流式部分：Anthropic Messages 响应 → Responses `response` 对象树
//!
//! 与 Chat Completions 的关键差异：产物是一棵 `response` 对象树（`output[]` 里每个
//! item 有独立 id 与 status），而不是 `choices[]`；截断不体现在 `finish_reason`，
//! 而是 `status: "incomplete"` + `incomplete_details.reason`。
//!
//! 响应的 `model` 字段一律回写**客户端请求的原始模型名**（Codex 会校验一致性）。
//!
//! # custom 工具的还原
//!
//! 请求侧把 `custom` 工具声明降级成 `{"input": string}` 的 JSON schema
//! （[`super::responses_request`]），上游因此以普通 `tool_use` 块回调。客户端只认
//! `custom_tool_call` item（入参是自由文本），所以这里要按请求侧给出的 custom 工具名
//! 集合把它还原回去——否则 Codex 会把调用当成未知工具。
//!
//! # reasoning 的处理
//!
//! 上游 thinking 文本会作为 `type: "reasoning"` item 放在 `output` 首位（与 OpenAI 的
//! 顺序一致），但不带 `encrypted_content`——本代理产不出可回传的加密推理内容，客户端
//! 把它原样回传时会被 [`super::responses_request`] 静默丢弃。

use std::collections::HashSet;

use serde_json::{Value, json};
use uuid::Uuid;

use super::super::chat_response::unix_now;

/// 生成 `<prefix>_<32位hex>` 形式的 id
pub(crate) fn new_id(prefix: &str) -> String {
    format!("{}_{}", prefix, Uuid::new_v4().simple())
}

/// 将摘要文本 base64 编码，用作 compaction item 的 `encrypted_content`
///
/// 这不是真正的 Fernet 加密——本代理用标准 base64 编码摘要文本。
/// Codex 只会将其原样回传，代理在下一轮 [`super::responses_request`] 中解码并注入上下文。
fn encode_compaction_content(text: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
}

/// 构造一个 `type: "compaction"` 的 output item（用于 Codex remote compaction v2 响应）
pub(crate) fn compaction_item(summary_text: &str) -> Value {
    json!({
        "type": "compaction",
        "id": new_id("cmp"),
        "encrypted_content": encode_compaction_content(summary_text),
    })
}

/// 上游 `stop_reason` 是否表示输出被截断
///
/// 两种取值都来自 `src/anthropic/handlers.rs`：`max_tokens` 是命中 `max_tokens` 上限，
/// `model_context_window_exceeded` 是上下文窗口耗尽。对 Responses 客户端而言两者都是
/// "没写完就停了"，统一映射为 `max_output_tokens`。
pub(crate) fn is_truncated(stop_reason: Option<&str>) -> bool {
    matches!(
        stop_reason,
        Some("max_tokens") | Some("model_context_window_exceeded")
    )
}

/// 把 Anthropic 的 usage 换算为 Responses usage（字段名与 Chat Completions 不同）
///
/// 输入总量口径与 `cached_tokens` 取值的理由见 `chat_response::convert_usage` 的说明。
pub(crate) fn convert_usage(usage: Option<&Value>) -> Value {
    let read = |key: &str| {
        usage
            .and_then(|u| u.get(key))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    let cached = read("cache_read_input_tokens");
    let input = read("input_tokens") + cached + read("cache_creation_input_tokens");
    let output = read("output_tokens");
    json!({
        "input_tokens": input,
        "output_tokens": output,
        "total_tokens": input + output,
        "input_tokens_details": {"cached_tokens": cached},
    })
}

/// 从 Anthropic content 块数组中分拣出的内容
#[derive(Debug, Default)]
struct ExtractedContent {
    text: String,
    reasoning: String,
    /// 已转换为 Responses `function_call` / `custom_tool_call` item 的工具调用
    tool_calls: Vec<Value>,
}

/// 遍历 Anthropic content 数组，按块类型分拣
///
/// `thinking` 块的 `signature` 字段是为通过下游检测伪造的无语义串
/// （`src/anthropic/stream.rs` 的 `generate_fake_signature`），只取 `thinking` 文本。
fn extract_content(content: Option<&Value>, custom_tools: &HashSet<String>) -> ExtractedContent {
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
                if let Some(call) = tool_use_to_item(block, custom_tools) {
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

/// Anthropic `tool_use` block → Responses `function_call` / `custom_tool_call` item
///
/// `call_id` 直接用上游的 `tool_use.id`：客户端下一轮会以该值回传
/// `function_call_output`，请求侧再原样还原为 `tool_result.tool_use_id`。
fn tool_use_to_item(block: &Value, custom_tools: &HashSet<String>) -> Option<Value> {
    let id = block.get("id").and_then(Value::as_str);
    let name = block.get("name").and_then(Value::as_str);
    // 缺字段的块只能跳过，但必须留痕：否则客户端收到的响应会莫名少一次工具调用且无从排查
    let (Some(id), Some(name)) = (id, name) else {
        tracing::warn!(
            has_id = id.is_some(),
            has_name = name.is_some(),
            "上游 tool_use 块缺少 id 或 name，已跳过该工具调用"
        );
        return None;
    };

    if custom_tools.contains(name) {
        return Some(json!({
            "type": "custom_tool_call",
            "id": new_id("ctc"),
            "call_id": id,
            "name": name,
            "input": custom_input_from_value(block.get("input")),
            "status": "completed",
        }));
    }

    // Responses 的 arguments 与 Chat Completions 一致，是 JSON 字符串而非对象
    let arguments = block
        .get("input")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string());

    Some(json!({
        "type": "function_call",
        "id": new_id("fc"),
        "call_id": id,
        "name": name,
        "arguments": arguments,
        "status": "completed",
    }))
}

/// 从降级 schema 的入参对象里取回 custom 工具的自由文本
///
/// 正常情况是 `{"input": "<原文>"}`（降级 schema 只有这一个字段）。模型没照 schema 作答时
/// 退化为整段 JSON 原文——宁可让客户端收到多余的包装，也不能给它空串。
fn custom_input_from_value(input: Option<&Value>) -> String {
    match input {
        Some(v) => match v.get("input").and_then(Value::as_str) {
            Some(s) => s.to_string(),
            None => v.to_string(),
        },
        None => String::new(),
    }
}

/// 同 [`custom_input_from_value`]，输入是流式累积出的 JSON 文本
pub(crate) fn custom_input_from_json_text(raw: &str) -> String {
    match serde_json::from_str::<Value>(raw) {
        Ok(v) => custom_input_from_value(Some(&v)),
        // 上游可能在参数发完前就断了，半截 JSON 原样交给客户端，由它决定怎么处理
        Err(_) => raw.to_string(),
    }
}

/// 构造一个 `type: "message"` 的 output item
fn message_item(text: &str) -> Value {
    json!({
        "type": "message",
        "id": new_id("msg"),
        "status": "completed",
        "role": "assistant",
        "content": [{"type": "output_text", "text": text, "annotations": []}],
    })
}

/// 构造一个 `type: "reasoning"` 的 output item
fn reasoning_item(text: &str) -> Value {
    json!({
        "type": "reasoning",
        "id": new_id("rs"),
        "summary": [{"type": "summary_text", "text": text}],
    })
}

/// 把 Anthropic 非流式响应转换为 Responses `response` 对象
pub(crate) fn convert_non_stream(
    anthropic: &Value,
    client_model: &str,
    custom_tools: &HashSet<String>,
) -> Value {
    convert_non_stream_inner(anthropic, client_model, custom_tools, false)
}

/// 把 Anthropic 非流式响应转换为 Responses `response` 对象（压缩模式）
///
/// 当 `is_compaction=true` 时，将模型返回的文本内容包装为 `type: "compaction"` output item，
/// 满足 Codex remote compaction v2 的协议要求（恰好一个 compaction output item）。
pub(crate) fn convert_non_stream_compaction(
    anthropic: &Value,
    client_model: &str,
    custom_tools: &HashSet<String>,
) -> Value {
    convert_non_stream_inner(anthropic, client_model, custom_tools, true)
}

fn convert_non_stream_inner(
    anthropic: &Value,
    client_model: &str,
    custom_tools: &HashSet<String>,
    is_compaction: bool,
) -> Value {
    let extracted = extract_content(anthropic.get("content"), custom_tools);
    let stop_reason = anthropic.get("stop_reason").and_then(Value::as_str);

    let mut output = Vec::new();

    if is_compaction {
        // 压缩模式：忽略 reasoning/tool_calls，只保留文本摘要，包装为 compaction item
        // Codex 要求恰好一个 compaction output item
        let summary = extracted.text.trim();
        if summary.is_empty() {
            tracing::warn!(
                "compaction 模式下模型未返回文本内容（可能为纯 reasoning 或 tool call），\
                 将产出空摘要 item，Codex 可能无法正确恢复上下文"
            );
        } else {
            tracing::info!("生成 compaction output item（摘要 {} 字节）", summary.len());
        }
        output.push(compaction_item(summary));
    } else {
        if !extracted.reasoning.is_empty() {
            output.push(reasoning_item(&extracted.reasoning));
        }
        // 只有工具调用、没有可见文本时不产出空的 message item
        if !extracted.text.is_empty() {
            output.push(message_item(&extracted.text));
        }
        output.extend(extracted.tool_calls);
    }

    let mut response = json!({
        "id": new_id("resp"),
        "object": "response",
        "created_at": unix_now(),
        "status": "completed",
        "model": client_model,
        "output": output,
        "usage": convert_usage(anthropic.get("usage")),
    });

    if is_truncated(stop_reason) {
        response["status"] = json!("incomplete");
        response["incomplete_details"] = json!({"reason": "max_output_tokens"});
    }

    response
}
