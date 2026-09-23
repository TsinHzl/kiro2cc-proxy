// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 流式部分：Anthropic SSE → OpenAI `chat.completion.chunk` SSE 转换状态机

use serde_json::{Value, json};

use super::nonstream::{convert_usage, map_finish_reason, new_completion_id, unix_now};

/// Anthropic `content_block_delta` 中允许透传的 `delta.type` 白名单
///
/// **必须按 `delta.type` 过滤，而不是"落在 thinking 块上的 delta 都算推理增量"**：
/// `src/anthropic/stream.rs` 会在 thinking 块的 `content_block_stop` 之前注入一个
/// `signature_delta`，其内容是为通过下游检测伪造的 ≥100 字符无语义串。若按块类型
/// 归类，这串签名会被拼进 `reasoning_content` 变成乱码。
pub(crate) const TEXT_DELTA: &str = "text_delta";
pub(crate) const THINKING_DELTA: &str = "thinking_delta";
pub(crate) const INPUT_JSON_DELTA: &str = "input_json_delta";
/// 常规注入、无需留痕的 delta 类型（丢弃且不记 WARN，避免每轮 thinking 产生噪声）
pub(crate) const SIGNATURE_DELTA: &str = "signature_delta";

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
        let (message, error_type, code) = crate::openai::error::extract_stream_error(data);
        tracing::warn!(error_type = %error_type, "上游流式响应报错，已下发错误帧并终止");

        let mut frames = vec![format!(
            "data: {}\n\n",
            crate::openai::error::error_body(error_type, message, code)
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

        let (Some(id), Some(name)) = (
            block.get("id").and_then(Value::as_str),
            block.get("name").and_then(Value::as_str),
        ) else {
            tracing::warn!("流式 tool_use 块缺少 id 或 name，已跳过");
            return Vec::new();
        };

        let block_index = data.get("index").and_then(Value::as_i64).unwrap_or(0);
        let tool_index = self.assign_tool_index(block_index);

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
                // 对应 tool_use 块的 start 因缺 id/name 被跳过时不会注册 index，这里也跳过，
                // 避免下发一条客户端从未见过 id/name 的孤儿 tool_call delta
                let Some(&tool_index) = self.tool_index_by_block.get(&block_index) else {
                    return Vec::new();
                };
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
