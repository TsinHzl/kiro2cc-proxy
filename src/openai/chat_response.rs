// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic Messages 响应 → OpenAI Chat Completions 响应
//!
//! 非流式部分把 `handle_non_stream_request` 产出的 JSON（`src/anthropic/handlers.rs`
//! 末尾的 `response_body`）重新组装为 `chat.completion` 对象。
//!
//! 响应的 `model` 字段一律回写**客户端请求的原始模型名**，而非映射后的 Kiro 模型名 ——
//! Codex 会校验请求与响应的模型名一致性。

use serde_json::{Map, Value, json};
use uuid::Uuid;

/// 生成 `chatcmpl-<32位hex>` 形式的响应 id
fn new_completion_id() -> String {
    format!("chatcmpl-{}", Uuid::new_v4().simple())
}

/// 当前 Unix 秒
pub(super) fn unix_now() -> i64 {
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
fn convert_usage(usage: Option<&Value>) -> Value {
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

// === 流式 ===

/// Anthropic `content_block_delta` 中允许透传的 `delta.type` 白名单
///
/// **必须按 `delta.type` 过滤，而不是"落在 thinking 块上的 delta 都算推理增量"**：
/// `src/anthropic/stream.rs` 会在 thinking 块的 `content_block_stop` 之前注入一个
/// `signature_delta`，其内容是为通过下游检测伪造的 ≥100 字符无语义串。若按块类型
/// 归类，这串签名会被拼进 `reasoning_content` 变成乱码。
pub(super) const TEXT_DELTA: &str = "text_delta";
pub(super) const THINKING_DELTA: &str = "thinking_delta";
pub(super) const INPUT_JSON_DELTA: &str = "input_json_delta";
/// 常规注入、无需留痕的 delta 类型（丢弃且不记 WARN，避免每轮 thinking 产生噪声）
pub(super) const SIGNATURE_DELTA: &str = "signature_delta";

/// Anthropic SSE → OpenAI `chat.completion.chunk` SSE 转换状态机
///
/// 逐事件喂入 [`ChatStreamConverter::on_event`]，返回若干条待下发的 SSE 帧文本
/// （已含 `data: ` 前缀与结尾空行）。上游流结束后调用 [`ChatStreamConverter::finish`]
/// 补齐收尾帧，保证即使上游中途断开也不会缺 `[DONE]`。
pub(crate) struct ChatStreamConverter {
    id: String,
    created: i64,
    /// 客户端请求的原始模型名
    model: String,
    include_usage: bool,
    role_sent: bool,
    finish_sent: bool,
    done_sent: bool,
    /// Anthropic block index → OpenAI tool_calls index
    tool_index_by_block: std::collections::HashMap<i64, usize>,
    next_tool_index: usize,
    finish_reason: &'static str,
    usage: Option<Value>,
}

impl ChatStreamConverter {
    pub(crate) fn new(client_model: &str, include_usage: bool) -> Self {
        Self {
            id: new_completion_id(),
            created: unix_now(),
            model: client_model.to_string(),
            include_usage,
            role_sent: false,
            finish_sent: false,
            done_sent: false,
            tool_index_by_block: std::collections::HashMap::new(),
            next_tool_index: 0,
            finish_reason: "stop",
            usage: None,
        }
    }

    /// 处理一个上游事件，返回待下发的 SSE 帧
    pub(crate) fn on_event(&mut self, name: &str, data: &Value) -> Vec<String> {
        match name {
            "message_start" => self.ensure_role_frame(),
            "content_block_start" => self.on_block_start(data),
            "content_block_delta" => self.on_block_delta(data),
            // 块级收尾在 OpenAI 协议里没有对应事件
            "content_block_stop" => Vec::new(),
            "message_delta" => {
                if let Some(reason) = data
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(Value::as_str)
                {
                    self.finish_reason = map_finish_reason(Some(reason));
                }
                if let Some(usage) = data.get("usage") {
                    self.usage = Some(convert_usage(Some(usage)));
                }
                Vec::new()
            }
            "message_stop" => self.finish(),
            "error" => self.on_error(data),
            other => {
                tracing::warn!(event = %other, "未识别的上游 SSE 事件，已跳过");
                Vec::new()
            }
        }
    }

    /// 上游流结束时收尾：补发 finish_reason 帧、可选 usage 帧与 `[DONE]`
    pub(crate) fn finish(&mut self) -> Vec<String> {
        let mut frames = Vec::new();
        if self.done_sent {
            return frames;
        }
        // 极端情况下上游一个内容块都没给，仍要让客户端看到合法的 chunk 序列
        frames.extend(self.ensure_role_frame());

        if !self.finish_sent {
            self.finish_sent = true;
            frames.push(self.frame(json!([{
                "index": 0,
                "delta": {},
                "finish_reason": self.finish_reason,
            }])));
        }

        if self.include_usage {
            let usage = self.usage.clone().unwrap_or_else(|| convert_usage(None));
            frames.push(self.frame_with_usage(usage));
        }

        self.done_sent = true;
        frames.push("data: [DONE]\n\n".to_string());
        frames
    }

    /// 流式过程中上游报错：下发一个错误帧后终止，不静默结束
    fn on_error(&mut self, data: &Value) -> Vec<String> {
        // 已收尾过就不再补帧：否则会下发第二个 `[DONE]`
        if self.done_sent {
            return Vec::new();
        }
        let (message, error_type, code) = super::error::extract_stream_error(data);
        tracing::warn!(error_type = %error_type, "上游流式响应报错，已下发错误帧并终止");

        let mut frames = vec![format!(
            "data: {}\n\n",
            super::error::error_body(error_type, message, code)
        )];
        self.done_sent = true;
        frames.push("data: [DONE]\n\n".to_string());
        frames
    }

    /// 首帧必须携带 `delta.role`
    fn ensure_role_frame(&mut self) -> Vec<String> {
        if self.role_sent {
            return Vec::new();
        }
        self.role_sent = true;
        vec![self.frame(json!([{
            "index": 0,
            "delta": {"role": "assistant", "content": ""},
            "finish_reason": Value::Null,
        }]))]
    }

    fn on_block_start(&mut self, data: &Value) -> Vec<String> {
        let block = match data.get("content_block") {
            Some(b) => b,
            None => return Vec::new(),
        };
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            // text / thinking 块的开始不产生 OpenAI 帧，增量到来时再下发
            return Vec::new();
        }

        let block_index = data.get("index").and_then(Value::as_i64).unwrap_or(0);
        let tool_index = self.assign_tool_index(block_index);
        let id = block.get("id").and_then(Value::as_str).unwrap_or_default();
        let name = block
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();

        let mut frames = self.ensure_role_frame();
        frames.push(self.frame(json!([{
            "index": 0,
            "delta": {"tool_calls": [{
                "index": tool_index,
                "id": id,
                "type": "function",
                "function": {"name": name, "arguments": ""},
            }]},
            "finish_reason": Value::Null,
        }])));
        frames
    }

    fn on_block_delta(&mut self, data: &Value) -> Vec<String> {
        let delta = match data.get("delta") {
            Some(d) => d,
            None => return Vec::new(),
        };
        let delta_type = delta
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match delta_type {
            TEXT_DELTA => {
                let text = delta
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if text.is_empty() {
                    return Vec::new();
                }
                let mut frames = self.ensure_role_frame();
                frames.push(self.frame(json!([{
                    "index": 0,
                    "delta": {"content": text},
                    "finish_reason": Value::Null,
                }])));
                frames
            }
            THINKING_DELTA => {
                let text = delta
                    .get("thinking")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if text.is_empty() {
                    return Vec::new();
                }
                let mut frames = self.ensure_role_frame();
                frames.push(self.frame(json!([{
                    "index": 0,
                    "delta": {"reasoning_content": text},
                    "finish_reason": Value::Null,
                }])));
                frames
            }
            INPUT_JSON_DELTA => {
                let partial = delta
                    .get("partial_json")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if partial.is_empty() {
                    return Vec::new();
                }
                let block_index = data.get("index").and_then(Value::as_i64).unwrap_or(0);
                let tool_index = self.assign_tool_index(block_index);
                let mut frames = self.ensure_role_frame();
                frames.push(self.frame(json!([{
                    "index": 0,
                    "delta": {"tool_calls": [{
                        "index": tool_index,
                        "function": {"arguments": partial},
                    }]},
                    "finish_reason": Value::Null,
                }])));
                frames
            }
            // 伪造签名：丢弃且不留痕（每个 thinking 块都会注入一次）
            SIGNATURE_DELTA => Vec::new(),
            other => {
                tracing::warn!(delta_type = %other, "未识别的 content_block_delta 类型，已跳过");
                Vec::new()
            }
        }
    }

    /// 取或分配某个上游 block 对应的 `tool_calls` index，保证同一块在整个流中稳定
    fn assign_tool_index(&mut self, block_index: i64) -> usize {
        if let Some(i) = self.tool_index_by_block.get(&block_index) {
            return *i;
        }
        let i = self.next_tool_index;
        self.next_tool_index += 1;
        self.tool_index_by_block.insert(block_index, i);
        i
    }

    fn frame(&self, choices: Value) -> String {
        format!(
            "data: {}\n\n",
            json!({
                "id": self.id,
                "object": "chat.completion.chunk",
                "created": self.created,
                "model": self.model,
                "choices": choices,
            })
        )
    }

    fn frame_with_usage(&self, usage: Value) -> String {
        format!(
            "data: {}\n\n",
            json!({
                "id": self.id,
                "object": "chat.completion.chunk",
                "created": self.created,
                "model": self.model,
                "choices": [],
                "usage": usage,
            })
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anthropic_text_response() -> Value {
        json!({
            "id": "msg_abc",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "你好"}],
            "model": "gpt-5.6-terra",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 12,
                "output_tokens": 3,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
            },
        })
    }

    #[test]
    fn converts_plain_text_response() {
        let out = convert_non_stream(&anthropic_text_response(), "gpt-5-codex");
        assert!(
            out["id"].as_str().unwrap().starts_with("chatcmpl-"),
            "id 前缀应为 chatcmpl-"
        );
        assert_eq!(out["object"], "chat.completion");
        assert!(out["created"].as_i64().unwrap() > 0);
        // 必须回写客户端原始模型名
        assert_eq!(out["model"], "gpt-5-codex");
        assert_eq!(out["choices"][0]["index"], 0);
        assert_eq!(out["choices"][0]["message"]["role"], "assistant");
        assert_eq!(out["choices"][0]["message"]["content"], "你好");
        assert_eq!(out["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn usage_matches_upstream() {
        let out = convert_non_stream(&anthropic_text_response(), "gpt-5-codex");
        assert_eq!(out["usage"]["prompt_tokens"], 12);
        assert_eq!(out["usage"]["completion_tokens"], 3);
        assert_eq!(out["usage"]["total_tokens"], 15);
    }

    #[test]
    fn missing_usage_yields_zeros() {
        let out = convert_non_stream(&json!({"content": []}), "gpt-5-codex");
        assert_eq!(out["usage"]["prompt_tokens"], 0);
        assert_eq!(out["usage"]["completion_tokens"], 0);
        assert_eq!(out["usage"]["total_tokens"], 0);
    }

    #[test]
    fn converts_tool_use_response() {
        let anthropic = json!({
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "get_weather",
                "input": {"city": "SH"},
            }],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 5, "output_tokens": 7},
        });
        let out = convert_non_stream(&anthropic, "gpt-5-codex");
        let msg = &out["choices"][0]["message"];
        assert_eq!(msg["content"], Value::Null);
        assert_eq!(out["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(msg["tool_calls"][0]["id"], "toolu_1");
        assert_eq!(msg["tool_calls"][0]["type"], "function");
        assert_eq!(msg["tool_calls"][0]["index"], 0);
        assert_eq!(msg["tool_calls"][0]["function"]["name"], "get_weather");
        // arguments 必须是 JSON 字符串而不是对象
        let args = msg["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .expect("arguments 应为字符串");
        assert_eq!(
            serde_json::from_str::<Value>(args).unwrap(),
            json!({"city": "SH"})
        );
    }

    #[test]
    fn text_plus_tool_use_keeps_both() {
        let anthropic = json!({
            "content": [
                {"type": "text", "text": "让我查一下"},
                {"type": "tool_use", "id": "t1", "name": "f", "input": {}},
            ],
            "stop_reason": "tool_use",
        });
        let out = convert_non_stream(&anthropic, "m");
        assert_eq!(out["choices"][0]["message"]["content"], "让我查一下");
        assert_eq!(
            out["choices"][0]["message"]["tool_calls"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn multiple_tool_uses_get_distinct_indexes() {
        let anthropic = json!({
            "content": [
                {"type": "tool_use", "id": "a", "name": "f", "input": {}},
                {"type": "tool_use", "id": "b", "name": "g", "input": {}},
            ],
            "stop_reason": "tool_use",
        });
        let out = convert_non_stream(&anthropic, "m");
        let calls = out["choices"][0]["message"]["tool_calls"]
            .as_array()
            .unwrap();
        assert_eq!(calls[0]["index"], 0);
        assert_eq!(calls[1]["index"], 1);
    }

    #[test]
    fn prompt_tokens_include_cached_input() {
        // Anthropic 的 input_tokens 已扣掉缓存，OpenAI 的 prompt_tokens 要求输入总量
        let out = convert_non_stream(
            &json!({
                "content": [{"type": "text", "text": "hi"}],
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 100, "output_tokens": 7,
                    "cache_creation_input_tokens": 300,
                    "cache_read_input_tokens": 600,
                },
            }),
            "m",
        );
        assert_eq!(out["usage"]["prompt_tokens"], 1000);
        assert_eq!(out["usage"]["total_tokens"], 1007);
        // cached_tokens 只计命中，不含 creation
        assert_eq!(out["usage"]["prompt_tokens_details"]["cached_tokens"], 600);
    }

    #[test]
    fn malformed_tool_use_is_skipped_without_shifting_indexes() {
        // 缺 id / 缺 name 的块无法构造合法 tool_calls 项，跳过后剩下的 index 仍须连续
        let anthropic = json!({
            "content": [
                {"type": "tool_use", "name": "no_id", "input": {}},
                {"type": "tool_use", "id": "ok1", "name": "f", "input": {}},
                {"type": "tool_use", "id": "no_name", "input": {}},
                {"type": "tool_use", "id": "ok2", "name": "g", "input": {}},
            ],
            "stop_reason": "tool_use",
        });
        let out = convert_non_stream(&anthropic, "m");
        let calls = out["choices"][0]["message"]["tool_calls"]
            .as_array()
            .unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["id"], "ok1");
        assert_eq!(calls[0]["index"], 0);
        assert_eq!(calls[1]["id"], "ok2");
        assert_eq!(calls[1]["index"], 1);
    }

    #[test]
    fn thinking_goes_to_reasoning_content_without_signature() {
        let anthropic = json!({
            "content": [
                {"type": "thinking", "thinking": "先看天气", "signature": "FAKESIGNATURE".repeat(10)},
                {"type": "text", "text": "晴"},
            ],
            "stop_reason": "end_turn",
        });
        let out = convert_non_stream(&anthropic, "m");
        let msg = &out["choices"][0]["message"];
        assert_eq!(msg["reasoning_content"], "先看天气");
        assert_eq!(msg["content"], "晴");
        // 伪造签名不得出现在任何字段中
        assert!(!out.to_string().contains("FAKESIGNATURE"));
    }

    #[test]
    fn finish_reason_covers_all_upstream_stop_reasons() {
        assert_eq!(map_finish_reason(Some("end_turn")), "stop");
        assert_eq!(map_finish_reason(Some("tool_use")), "tool_calls");
        assert_eq!(map_finish_reason(Some("max_tokens")), "length");
        assert_eq!(
            map_finish_reason(Some("model_context_window_exceeded")),
            "length"
        );
        assert_eq!(map_finish_reason(Some("stop_sequence")), "stop");
        // 未枚举值回退
        assert_eq!(map_finish_reason(Some("brand_new_reason")), "stop");
        assert_eq!(map_finish_reason(None), "stop");
    }

    #[test]
    fn unknown_content_block_is_skipped() {
        let anthropic = json!({
            "content": [
                {"type": "redacted_thinking", "data": "xxx"},
                {"type": "text", "text": "ok"},
            ],
            "stop_reason": "end_turn",
        });
        let out = convert_non_stream(&anthropic, "m");
        assert_eq!(out["choices"][0]["message"]["content"], "ok");
    }

    #[test]
    fn empty_content_yields_empty_string_not_null() {
        // 无 tool_calls 时 content 为空串（null 会让部分 SDK 报错）
        let out = convert_non_stream(&json!({"content": [], "stop_reason": "end_turn"}), "m");
        assert_eq!(out["choices"][0]["message"]["content"], "");
        assert!(out["choices"][0]["message"].get("tool_calls").is_none());
    }

    // === 流式 ===

    /// 把帧文本还原为 JSON（`[DONE]` 保持原样返回 `None`）
    fn parse_frame(frame: &str) -> Option<Value> {
        let payload = frame
            .strip_prefix("data: ")
            .and_then(|s| s.strip_suffix("\n\n"))
            .expect("帧格式应为 'data: <payload>\\n\\n'");
        if payload == "[DONE]" {
            return None;
        }
        Some(serde_json::from_str(payload).expect("帧内容应为合法 JSON"))
    }

    fn text_delta(index: i64, text: &str) -> Value {
        json!({
            "type": "content_block_delta",
            "index": index,
            "delta": {"type": "text_delta", "text": text},
        })
    }

    /// 驱动一串上游事件，返回所有下发帧
    fn run_stream(events: &[(&str, Value)], include_usage: bool) -> Vec<String> {
        let mut conv = ChatStreamConverter::new("gpt-5-codex", include_usage);
        let mut frames = Vec::new();
        for (name, data) in events {
            frames.extend(conv.on_event(name, data));
        }
        frames.extend(conv.finish());
        frames
    }

    #[test]
    fn plain_text_stream_has_role_first_and_done_last() {
        let frames = run_stream(
            &[
                ("message_start", json!({"type": "message_start"})),
                (
                    "content_block_start",
                    json!({"index": 0, "content_block": {"type": "text", "text": ""}}),
                ),
                ("content_block_delta", text_delta(0, "你")),
                ("content_block_delta", text_delta(0, "好")),
                ("content_block_stop", json!({"index": 0})),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": "end_turn"}, "usage": {"input_tokens": 4, "output_tokens": 2}}),
                ),
                ("message_stop", json!({})),
            ],
            false,
        );

        // 首帧：role
        let first = parse_frame(&frames[0]).unwrap();
        assert_eq!(first["object"], "chat.completion.chunk");
        assert_eq!(first["model"], "gpt-5-codex");
        assert_eq!(first["choices"][0]["delta"]["role"], "assistant");

        // 文本增量
        assert_eq!(
            parse_frame(&frames[1]).unwrap()["choices"][0]["delta"]["content"],
            "你"
        );
        assert_eq!(
            parse_frame(&frames[2]).unwrap()["choices"][0]["delta"]["content"],
            "好"
        );

        // 倒数第二帧：finish_reason 非 null 且 delta 为空对象
        let finish = parse_frame(&frames[frames.len() - 2]).unwrap();
        assert_eq!(finish["choices"][0]["finish_reason"], "stop");
        assert_eq!(finish["choices"][0]["delta"], json!({}));

        // 末帧：[DONE]
        assert_eq!(frames.last().unwrap(), "data: [DONE]\n\n");
        // 未开启 include_usage 时不得出现 usage 帧
        assert!(frames.iter().all(|f| !f.contains("\"usage\"")));
        // 所有 chunk 共用同一个 id
        let id = first["id"].as_str().unwrap().to_string();
        for f in &frames {
            if let Some(v) = parse_frame(f) {
                assert_eq!(v["id"], id);
            }
        }
    }

    #[test]
    fn include_usage_appends_usage_frame_before_done() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                ("content_block_delta", text_delta(0, "x")),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": "end_turn"}, "usage": {"input_tokens": 10, "output_tokens": 5}}),
                ),
                ("message_stop", json!({})),
            ],
            true,
        );

        let usage_frame = parse_frame(&frames[frames.len() - 2]).unwrap();
        assert_eq!(usage_frame["choices"], json!([]));
        assert_eq!(usage_frame["usage"]["prompt_tokens"], 10);
        assert_eq!(usage_frame["usage"]["completion_tokens"], 5);
        assert_eq!(usage_frame["usage"]["total_tokens"], 15);
        assert_eq!(frames.last().unwrap(), "data: [DONE]\n\n");
    }

    #[test]
    fn tool_call_stream_emits_id_then_argument_deltas() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                (
                    "content_block_start",
                    json!({"index": 1, "content_block": {
                        "type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {},
                    }}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 1, "delta": {"type": "input_json_delta", "partial_json": "{\"ci"}}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 1, "delta": {"type": "input_json_delta", "partial_json": "ty\":\"SH\"}"}}),
                ),
                ("content_block_stop", json!({"index": 1})),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": "tool_use"}}),
                ),
                ("message_stop", json!({})),
            ],
            false,
        );

        // frames[0] = role 帧，frames[1] = 工具首帧
        let head = parse_frame(&frames[1]).unwrap();
        let call = &head["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(call["index"], 0);
        assert_eq!(call["id"], "toolu_1");
        assert_eq!(call["type"], "function");
        assert_eq!(call["function"]["name"], "get_weather");

        // 后续帧只带 index 与 arguments 增量，不重复 id / name
        let arg1 = parse_frame(&frames[2]).unwrap();
        let c1 = &arg1["choices"][0]["delta"]["tool_calls"][0];
        assert_eq!(c1["index"], 0);
        assert_eq!(c1["function"]["arguments"], "{\"ci");
        assert!(c1.get("id").is_none());
        assert!(c1["function"].get("name").is_none());

        let finish = parse_frame(&frames[frames.len() - 2]).unwrap();
        assert_eq!(finish["choices"][0]["finish_reason"], "tool_calls");
    }

    #[test]
    fn concurrent_tool_calls_get_stable_distinct_indexes() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                (
                    "content_block_start",
                    json!({"index": 1, "content_block": {"type": "tool_use", "id": "a", "name": "f"}}),
                ),
                (
                    "content_block_start",
                    json!({"index": 2, "content_block": {"type": "tool_use", "id": "b", "name": "g"}}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 2, "delta": {"type": "input_json_delta", "partial_json": "{}"}}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 1, "delta": {"type": "input_json_delta", "partial_json": "{}"}}),
                ),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": "tool_use"}}),
                ),
                ("message_stop", json!({})),
            ],
            false,
        );

        let idx_of = |frame: &str| -> i64 {
            parse_frame(frame).unwrap()["choices"][0]["delta"]["tool_calls"][0]["index"]
                .as_i64()
                .unwrap()
        };
        // 两个块拿到不同 index
        assert_eq!(idx_of(&frames[1]), 0);
        assert_eq!(idx_of(&frames[2]), 1);
        // 参数增量按块归属回到各自 index（顺序交错也不串号）
        assert_eq!(idx_of(&frames[3]), 1);
        assert_eq!(idx_of(&frames[4]), 0);
    }

    #[test]
    fn thinking_delta_goes_to_reasoning_content() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                (
                    "content_block_delta",
                    json!({"index": 0, "delta": {"type": "thinking_delta", "thinking": "推理中"}}),
                ),
                ("content_block_delta", text_delta(1, "答案")),
                ("message_stop", json!({})),
            ],
            false,
        );

        let reasoning = parse_frame(&frames[1]).unwrap();
        assert_eq!(
            reasoning["choices"][0]["delta"]["reasoning_content"],
            "推理中"
        );
        // 推理内容不得混入 content
        assert!(reasoning["choices"][0]["delta"].get("content").is_none());
        assert_eq!(
            parse_frame(&frames[2]).unwrap()["choices"][0]["delta"]["content"],
            "答案"
        );
    }

    #[test]
    fn signature_delta_never_reaches_client() {
        let fake_signature = "A".repeat(120);
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                (
                    "content_block_delta",
                    json!({"index": 0, "delta": {"type": "thinking_delta", "thinking": "想"}}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 0, "delta": {"type": "signature_delta", "signature": fake_signature}}),
                ),
                ("content_block_stop", json!({"index": 0})),
                ("message_stop", json!({})),
            ],
            false,
        );

        let all = frames.concat();
        assert!(
            !all.contains(&"A".repeat(120)),
            "伪造签名串不得出现在任何下发帧中"
        );
        assert!(!all.contains("signature"));
        // 签名帧不产生任何输出：role + reasoning + finish + DONE 共 4 帧
        assert_eq!(frames.len(), 4);
    }

    #[test]
    fn error_after_done_emits_no_second_done() {
        let mut conv = ChatStreamConverter::new("gpt-5.6-terra", false);
        let mut frames = conv.on_event("message_start", &json!({}));
        frames.extend(conv.finish());
        // message_stop 之后上游又异常下发 error，不得再产出第二个 [DONE]
        let extra = conv.on_event(
            "error",
            &json!({"error": {"type": "overloaded_error", "message": "迟到的错误"}}),
        );
        assert!(extra.is_empty(), "收尾后不应再下发任何帧");
        assert_eq!(
            frames.iter().filter(|f| f.contains("[DONE]")).count(),
            1,
            "整条流只应有一个 [DONE]"
        );
    }

    #[test]
    fn max_tokens_stop_reason_maps_to_length() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                ("content_block_delta", text_delta(0, "x")),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": "model_context_window_exceeded"}}),
                ),
                ("message_stop", json!({})),
            ],
            false,
        );
        let finish = parse_frame(&frames[frames.len() - 2]).unwrap();
        assert_eq!(finish["choices"][0]["finish_reason"], "length");
    }

    #[test]
    fn finish_is_idempotent_after_message_stop() {
        let mut conv = ChatStreamConverter::new("m", true);
        let mut frames = conv.on_event("message_start", &json!({}));
        frames.extend(conv.on_event("message_stop", &json!({})));
        let extra = conv.finish();
        // message_stop 已收尾，重复 finish 不得再产生帧（否则客户端会看到两个 [DONE]）
        assert!(extra.is_empty());
        assert_eq!(frames.iter().filter(|f| f.contains("[DONE]")).count(), 1);
    }

    #[test]
    fn truncated_stream_still_gets_finish_and_done() {
        // 上游只发了 message_start 就断开
        let frames = run_stream(&[("message_start", json!({}))], false);
        assert_eq!(frames.len(), 3);
        assert_eq!(
            parse_frame(&frames[0]).unwrap()["choices"][0]["delta"]["role"],
            "assistant"
        );
        assert_eq!(
            parse_frame(&frames[1]).unwrap()["choices"][0]["finish_reason"],
            "stop"
        );
        assert_eq!(frames[2], "data: [DONE]\n\n");
    }

    #[test]
    fn upstream_error_event_emits_error_frame_then_done() {
        let mut conv = ChatStreamConverter::new("m", false);
        let mut frames = conv.on_event("message_start", &json!({}));
        frames.extend(conv.on_event(
            "error",
            &json!({"type": "error", "error": {"type": "overloaded_error", "message": "上游过载"}}),
        ));
        // 错误帧之后不得再补发正常收尾帧
        frames.extend(conv.finish());

        let err = parse_frame(&frames[1]).unwrap();
        assert_eq!(err["error"]["message"], "上游过载");
        // overloaded_error 映射为 OpenAI 的 server_error / overloaded（见 openai::error）
        assert_eq!(err["error"]["type"], "server_error");
        assert_eq!(err["error"]["code"], "overloaded");
        assert_eq!(frames.last().unwrap(), "data: [DONE]\n\n");
        assert_eq!(frames.iter().filter(|f| f.contains("[DONE]")).count(), 1);
    }

    #[test]
    fn unknown_event_and_unknown_delta_are_skipped() {
        let frames = run_stream(
            &[
                ("message_start", json!({})),
                ("brand_new_event", json!({"foo": 1})),
                (
                    "content_block_delta",
                    json!({"index": 0, "delta": {"type": "brand_new_delta", "x": 1}}),
                ),
                ("message_stop", json!({})),
            ],
            false,
        );
        // 只剩 role + finish + DONE
        assert_eq!(frames.len(), 3);
    }
}
