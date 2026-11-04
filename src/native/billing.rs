// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 原生推理流的计费用 token 估算与 `metadataEvent.tokenUsage` 注入。
//!
//! # 背景
//!
//! 原生 Kiro EventStream 只带 `meteringEvent`（credits）与 `contextUsageEvent`（百分比），
//! `metadataEvent` 只有 stopReason，没有 tokenUsage；下游 sub2api 无法按 token 记账。
//! Anthropic 路由的 token 口径来自 tiktoken 估算（`crate::token::count_tokens`），
//! 本模块对原生直通做同样口径的估算，并在**唯一需要修改的帧**（`metadataEvent`）里
//! 注入 tokenUsage，其余帧逐字节原样转发。
//!
//! # 语义
//!
//! - 输入 tokens：请求 JSON 中的 systemPrompt / history / currentMessage / tools
//!   schema / toolResults / additionalModelRequestFields（tiktoken cl100k_base 估算，
//!   上限 1,000,000；图片 base64 不参与估算）。
//! - 输出 tokens：上游 `assistantResponseEvent.content` + `reasoningContentEvent.text`
//!   + `toolUseEvent.input` 片段拼接后的 tiktoken 估算。
//! - cache 字段：原生流没有真实 cache token，除非上游 `meteringEvent` 同时给出
//!   `cacheReadInputTokens`/`cacheCreationInputTokens`（camelCase 或 snake_case 均兼容）。
//! - 注入帧格式（仅当 metadataEvent payload 里没有 `tokenUsage` 时）：
//!   `{"uncachedInputTokens": input-cache, "outputTokens": out, "totalTokens": input+out,
//!     "cacheReadInputTokens": cache_read, "cacheWriteInputTokens": cache_write}`
//! - 若上游没有 `metadataEvent`：整条流原样转发（下游会继续看到 usage_missing）。
//!
//! # 流式实现
//!
//! 增量解析 AWS EventStream 帧，**不缓冲整条响应**：buffer 只保留未成帧的尾部字节；
//! 每帧要么原样 yield（byte-identical），要么仅在 metadataEvent 上重建该帧
//! （复用原 header 字节，重算长度 + CRC），随后继续。上游读错误以 io::Error 结束流
//! （与直连上游时客户端看到的行为一致），并记录 warn 日志。

use std::io;
use std::pin::Pin;

use bytes::BytesMut;
use futures::stream::{self, Stream, StreamExt};
use serde_json::Value;

use crate::kiro::parser::crc::crc32;

/// 输入 token 估算上限（与 Anthropic 路由的上下文上限同量级，防止异常请求撑爆计数）。
pub(crate) const MAX_INPUT_TOKEN_ESTIMATE: u64 = 1_000_000;

/// AWS EventStream 单帧上限（与 `kiro::parser::frame::MAX_MESSAGE_SIZE` 一致）。
const MAX_EVENTSTREAM_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// 头部最小值：prelude(12) + 最小 header(0) + message crc(4)。
const MIN_EVENTSTREAM_FRAME_SIZE: usize = 16;

/// AWS EventStream prelude 固定长度。
const PRELUDE_SIZE: usize = 12;

/// 引擎事件类型（`x-amz-target` 之外的 `:event-type` header 值）。
pub(crate) const EVENT_METADATA: &str = "metadataEvent";

// ============================================================================
// 输入 token 估算
// ============================================================================

/// 从原生推理请求体估算输入 token 数。
///
/// 解析失败或字段缺失都不会报错（返回 0/部分和）；结果被
/// [`MAX_INPUT_TOKEN_ESTIMATE`] 截断。
pub(crate) fn estimate_input_tokens(body: &str) -> u64 {
    let Ok(root) = serde_json::from_str::<Value>(body) else {
        return 0;
    };
    let mut total = 0u64;

    // 顶层 systemPrompt（部分客户端形态）与 conversationState.systemPrompt
    if let Some(system_prompt) = root.get("systemPrompt").and_then(Value::as_str) {
        add_text_tokens(&mut total, system_prompt);
    }
    if let Some(conversation_state) = root.get("conversationState") {
        if let Some(system_prompt) = conversation_state
            .get("systemPrompt")
            .and_then(Value::as_str)
        {
            add_text_tokens(&mut total, system_prompt);
        }

        // 历史消息
        if let Some(history) = conversation_state.get("history").and_then(Value::as_array) {
            for message in history {
                add_message_tokens(&mut total, message);
            }
        }

        // 当前消息
        if let Some(message) = conversation_state.get("currentMessage") {
            add_message_tokens(&mut total, message);
            if let Some(user_input) = message.get("userInputMessage") {
                add_tools_tokens(&mut total, user_input);
            }
        }
    }

    // 模型专属请求参数（thinking budget / max_tokens / output_config 等）
    if let Some(extra) = root.get("additionalModelRequestFields") {
        add_json_tokens(&mut total, extra);
    }

    cap_input_tokens(total)
}

/// 截断输入估算值到 [`MAX_INPUT_TOKEN_ESTIMATE`]。
fn cap_input_tokens(tokens: u64) -> u64 {
    tokens.min(MAX_INPUT_TOKEN_ESTIMATE)
}

/// 客户端消息（`{userInputMessage}` / `{assistantResponseMessage}` 二选一）。
fn add_message_tokens(total: &mut u64, message: &Value) {
    if let Some(user_input) = message.get("userInputMessage") {
        if let Some(content) = user_input.get("content").and_then(Value::as_str) {
            add_text_tokens(total, content);
        }
        add_tool_results_tokens(total, user_input);
        add_tools_tokens(total, user_input);
    }
    if let Some(assistant) = message.get("assistantResponseMessage") {
        if let Some(content) = assistant.get("content").and_then(Value::as_str) {
            add_text_tokens(total, content);
        }
        // 历史 toolUses 的 input 也是模型已生成的输出上下文
        if let Some(tool_uses) = assistant.get("toolUses").and_then(Value::as_array) {
            for tool_use in tool_uses {
                if let Some(name) = tool_use.get("name").and_then(Value::as_str) {
                    add_text_tokens(total, name);
                }
                if let Some(input) = tool_use.get("input") {
                    add_json_tokens(total, input);
                }
            }
        }
        // 历史 thinking 文本（若有）
        if let Some(reasoning) = assistant.get("reasoningContent")
            && let Some(text) = reasoning.get("text").and_then(Value::as_str)
        {
            add_text_tokens(total, text);
        }
    }
}

/// userInputMessageContext.toolResults：工具结果正文属于输入 token。
fn add_tool_results_tokens(total: &mut u64, user_input: &Value) {
    let Some(tool_results) = user_input
        .get("userInputMessageContext")
        .and_then(|context| context.get("toolResults"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for result in tool_results {
        let Some(content) = result.get("content").and_then(Value::as_array) else {
            continue;
        };
        for block in content {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                add_text_tokens(total, text);
            } else if let Some(json) = block.get("json") {
                add_json_tokens(total, json);
            }
        }
    }
}

/// userInputMessageContext.tools：工具名 + 描述 + inputSchema.json。
fn add_tools_tokens(total: &mut u64, user_input: &Value) {
    let Some(tools) = user_input
        .get("userInputMessageContext")
        .and_then(|context| context.get("tools"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for tool in tools {
        let Some(spec) = tool.get("toolSpecification") else {
            continue;
        };
        if let Some(name) = spec.get("name").and_then(Value::as_str) {
            add_text_tokens(total, name);
        }
        if let Some(description) = spec.get("description").and_then(Value::as_str) {
            add_text_tokens(total, description);
        }
        if let Some(schema) = spec
            .get("inputSchema")
            .and_then(|input_schema| input_schema.get("json"))
        {
            add_json_tokens(total, schema);
        }
    }
}

fn add_text_tokens(total: &mut u64, text: &str) {
    if !text.is_empty() {
        *total = total.saturating_add(crate::token::count_tokens(text));
    }
}

fn add_json_tokens(total: &mut u64, value: &Value) {
    if let Ok(serialized) = serde_json::to_string(value) {
        add_text_tokens(total, &serialized);
    }
}

// ============================================================================
// 输出 token 统计 + metadataEvent.tokenUsage 注入
// ============================================================================

/// 从上游事件流增量收集输出文本与 metering cache 字段。
#[derive(Debug, Default)]
pub(crate) struct OutputTracker {
    /// assistantResponseEvent.content / reasoningContentEvent.text / toolUseEvent.input
    output_text: String,
    /// meteringEvent 提供的 cache read tokens（缺省 0）
    cache_read_input_tokens: u64,
    /// meteringEvent 提供的 cache creation/write tokens（缺省 0）
    cache_creation_input_tokens: u64,
}

impl OutputTracker {
    /// 观察一个已解码的 JSON payload（未知事件/字段忽略）。
    pub(crate) fn observe(&mut self, event_type: &str, payload: &Value) {
        match event_type {
            "assistantResponseEvent" => {
                if let Some(content) = payload.get("content").and_then(Value::as_str) {
                    self.output_text.push_str(content);
                }
            }
            "reasoningContentEvent" => {
                if let Some(text) = payload.get("text").and_then(Value::as_str) {
                    self.output_text.push_str(text);
                }
            }
            "toolUseEvent" => {
                if let Some(input) = payload.get("input").and_then(Value::as_str) {
                    self.output_text.push_str(input);
                }
            }
            "meteringEvent" => {
                // 兼容 camelCase（KAS/IDE 真实形态）与 snake_case（模型定义字段名）。
                if let Some(value) = number_field(
                    payload,
                    &["cacheReadInputTokens", "cache_read_input_tokens"],
                ) {
                    self.cache_read_input_tokens = value;
                }
                if let Some(value) = number_field(
                    payload,
                    &[
                        "cacheCreationInputTokens",
                        "cache_creation_input_tokens",
                        "cache_write_input_tokens",
                        "cacheWriteInputTokens",
                    ],
                ) {
                    self.cache_creation_input_tokens = value;
                }
            }
            _ => {}
        }
    }

    /// 已累计的输出 token 估算值。
    pub(crate) fn output_tokens(&self) -> u64 {
        crate::token::count_tokens(&self.output_text)
    }

    /// 构造注入 `metadataEvent` 的 tokenUsage 对象。
    ///
    /// `uncachedInputTokens = max(0, input - cache_read - cache_creation)`；
    /// `totalTokens = input + output`（与上游 TokenUsage 字段语义一致）。
    pub(crate) fn token_usage(&self, input_tokens: u64) -> Value {
        let cache_read = self.cache_read_input_tokens;
        let cache_write = self.cache_creation_input_tokens;
        let output_tokens = self.output_tokens();
        let uncached_input = input_tokens
            .saturating_sub(cache_read)
            .saturating_sub(cache_write);
        serde_json::json!({
            "uncachedInputTokens": uncached_input,
            "outputTokens": output_tokens,
            "totalTokens": input_tokens.saturating_add(output_tokens),
            "cacheReadInputTokens": cache_read,
            "cacheWriteInputTokens": cache_write,
        })
    }

    #[cfg(test)]
    pub(crate) fn cache_tokens(&self) -> (u64, u64) {
        (
            self.cache_read_input_tokens,
            self.cache_creation_input_tokens,
        )
    }
}

/// 从 payload 读取第一个存在且非负的整数（兼容 u64/i64/f64 形态）。
fn number_field(payload: &Value, keys: &[&str]) -> Option<u64> {
    for key in keys {
        let Some(value) = payload.get(*key) else {
            continue;
        };
        if let Some(number) = value.as_u64() {
            return Some(number);
        }
        if let Some(number) = value.as_i64() {
            return Some(number.max(0) as u64);
        }
        if let Some(number) = value.as_f64()
            && number.is_finite()
            && number > 0.0
        {
            return Some(number as u64);
        }
    }
    None
}

// ============================================================================
// AWS EventStream 增量帧解析 / 重写
// ============================================================================

/// 把上游 AWS EventStream 字节流包装为"注入 tokenUsage 后"的流。
///
/// - 完整帧：解析 `:event-type`，累积输出文本/cache 字段，除 `metadataEvent`
///   外全部按原字节 yield；
/// - `metadataEvent` 且 payload 无 `tokenUsage`：注入估算值并重建该帧（其余字段
///   原样保留），CRC 重算；
/// - 半帧：留在 buffer 等下一个 chunk；上游 EOF 时残留字节原样 flush；
/// - 结构损坏（长度非法）：之后整条流退化为原样透传，绝不丢字节。
pub(crate) fn rewrite_event_stream<S>(
    upstream: S,
    input_tokens: u64,
) -> impl Stream<Item = Result<bytes::Bytes, io::Error>> + Send + 'static
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
{
    let state = Rewriter {
        upstream: Box::pin(upstream),
        buffer: BytesMut::with_capacity(8192),
        tracker: OutputTracker::default(),
        input_tokens,
        upstream_done: false,
        passthrough: false,
        finished: false,
    };
    stream::unfold(state, |mut state| async move {
        loop {
            if state.finished {
                return None;
            }
            if let Some(result) = state.next_frame() {
                return Some((result, state));
            }
            if state.upstream_done {
                if let Some(result) = state.flush_tail() {
                    return Some((result, state));
                }
                // 流结束：直接返回 None（state 不再被使用）。
                return None;
            }
            match state.upstream.next().await {
                Some(Ok(chunk)) => state.buffer.extend_from_slice(&chunk),
                Some(Err(err)) => {
                    state.finished = true;
                    tracing::warn!(
                        error = %err,
                        buffered_bytes = state.buffer.len(),
                        "native 计费流：读取上游 EventStream 失败，终止客户端流"
                    );
                    return Some((
                        Err(io::Error::other(format!(
                            "upstream event stream error: {err}"
                        ))),
                        state,
                    ));
                }
                None => state.upstream_done = true,
            }
        }
    })
}

struct Rewriter<S> {
    upstream: Pin<Box<S>>,
    buffer: BytesMut,
    tracker: OutputTracker,
    input_tokens: u64,
    upstream_done: bool,
    /// 结构损坏后退化为原样透传（不解析、不注入）。
    passthrough: bool,
    finished: bool,
}

impl<S> Rewriter<S>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
{
    /// 尝试从 buffer 取出一帧/一段可转发字节；数据不足返回 None。
    fn next_frame(&mut self) -> Option<Result<bytes::Bytes, io::Error>> {
        if self.passthrough {
            if self.buffer.is_empty() {
                return None;
            }
            return Some(Ok(std::mem::take(&mut self.buffer).freeze()));
        }
        if self.buffer.len() < PRELUDE_SIZE {
            return None;
        }

        // prelude（BE32）：total_len / headers_len / prelude_crc
        let total_len = u32::from_be_bytes(self.buffer[0..4].try_into().ok()?) as usize;
        if !(MIN_EVENTSTREAM_FRAME_SIZE..=MAX_EVENTSTREAM_FRAME_SIZE).contains(&total_len) {
            tracing::warn!(
                total_len,
                buffered = self.buffer.len(),
                "native 计费流：EventStream 帧长度非法，退化为原样透传"
            );
            self.passthrough = true;
            return Some(Ok(std::mem::take(&mut self.buffer).freeze()));
        }
        if self.buffer.len() < total_len {
            return None;
        }

        let frame = self.buffer.split_to(total_len);
        match rewrite_frame(&frame, &mut self.tracker, self.input_tokens) {
            Ok(Some(rewritten)) => Some(Ok(rewritten)),
            Ok(None) => Some(Ok(frame.freeze())),
            Err(reason) => {
                // 帧结构/CRC 损坏：本帧原样转发，继续解析后续帧（不丢字节）。
                tracing::warn!(reason, "native 计费流：跳过 tokenUsage 注入（帧解析失败）");
                Some(Ok(frame.freeze()))
            }
        }
    }

    /// 上游 EOF 后 flush 半帧残留（可能是不完整的最后一帧或垃圾字节）。
    fn flush_tail(&mut self) -> Option<Result<bytes::Bytes, io::Error>> {
        if self.buffer.is_empty() {
            return None;
        }
        if !self.passthrough {
            tracing::warn!(
                trailing_bytes = self.buffer.len(),
                "native 计费流：上游结束时仍有未成帧字节，原样转发"
            );
        }
        Some(Ok(std::mem::take(&mut self.buffer).freeze()))
    }
}

/// 解析并（必要时）重建单帧。
///
/// 返回 `Ok(None)` = 原样转发；`Ok(Some(bytes))` = 注入 tokenUsage 后的新帧；
/// `Err(reason)` = 帧无效，调用方原样转发。
fn rewrite_frame(
    frame: &[u8],
    tracker: &mut OutputTracker,
    input_tokens: u64,
) -> Result<Option<bytes::Bytes>, &'static str> {
    let total_len = frame.len();
    let header_len =
        u32::from_be_bytes(frame[4..8].try_into().map_err(|_| "short header length")?) as usize;
    let prelude_crc = u32::from_be_bytes(frame[8..12].try_into().map_err(|_| "short prelude crc")?);
    if crc32(&frame[..8]) != prelude_crc {
        return Err("prelude crc mismatch");
    }
    let message_crc = u32::from_be_bytes(
        frame[total_len - 4..]
            .try_into()
            .map_err(|_| "short message crc")?,
    );
    if crc32(&frame[..total_len - 4]) != message_crc {
        return Err("message crc mismatch");
    }
    let header_start = PRELUDE_SIZE;
    let header_end = header_start + header_len;
    if header_end + 4 > total_len {
        return Err("header length out of range");
    }
    let headers = &frame[header_start..header_end];
    let payload = &frame[header_end..total_len - 4];

    let Some(event_type) = find_header_value(headers, ":event-type") else {
        // 没有 :event-type（例如 error 帧）：原样转发。
        return Ok(None);
    };

    let payload_json = serde_json::from_slice::<Value>(payload).ok();
    if let Some(json) = payload_json.as_ref() {
        tracker.observe(event_type, json);
    }

    if !event_type.eq_ignore_ascii_case(EVENT_METADATA) {
        return Ok(None);
    }
    let Some(mut json) = payload_json else {
        return Ok(None);
    };
    let Some(object) = json.as_object_mut() else {
        return Ok(None);
    };
    if object.contains_key("tokenUsage") {
        // 上游已提供真实 usage：不改帧（下游可直接用真实值）。
        return Ok(None);
    }
    object.insert("tokenUsage".to_string(), tracker.token_usage(input_tokens));
    let new_payload = serde_json::to_vec(&json).map_err(|_| "re-encode metadataEvent failed")?;
    Ok(Some(rebuild_frame(headers, &new_payload)))
}

/// 解析 AWS EventStream header 段，返回指定 header 的字符串值。
fn find_header_value<'a>(headers: &'a [u8], wanted: &str) -> Option<&'a str> {
    let mut offset = 0usize;
    let mut result = None;
    while offset < headers.len() {
        if offset + 1 > headers.len() {
            return result;
        }
        let name_len = headers[offset] as usize;
        offset += 1;
        if name_len == 0 || offset + name_len > headers.len() {
            return result;
        }
        let name = &headers[offset..offset + name_len];
        offset += name_len;
        if offset >= headers.len() {
            return result;
        }
        let value_type = headers[offset];
        offset += 1;
        let value_range = match value_type {
            // bool true/false：无 value 字节
            0 | 1 => None,
            2 => {
                offset += 1;
                None
            }
            3 => {
                offset += 2;
                None
            }
            4 => {
                offset += 4;
                None
            }
            5 | 8 => {
                offset += 8;
                None
            }
            9 => {
                offset += 16;
                None
            }
            6 | 7 => {
                if offset + 2 > headers.len() {
                    return result;
                }
                let len = u16::from_be_bytes([headers[offset], headers[offset + 1]]) as usize;
                offset += 2;
                if offset + len > headers.len() {
                    return result;
                }
                let range = offset..offset + len;
                offset += len;
                Some((value_type, range))
            }
            _ => return result,
        };
        if name == wanted.as_bytes()
            && let Some((value_type, range)) = value_range
            && value_type == 7
            && let Ok(value) = std::str::from_utf8(&headers[range])
        {
            result = Some(value);
        }
    }
    result
}

/// 重建单帧：复用原 header 字节，替换 payload，重算长度与两个 CRC。
fn rebuild_frame(headers: &[u8], payload: &[u8]) -> bytes::Bytes {
    let total_len = PRELUDE_SIZE + headers.len() + payload.len() + 4;
    let mut out = BytesMut::with_capacity(total_len);
    out.extend_from_slice(&(total_len as u32).to_be_bytes());
    out.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    let prelude_crc = crc32(&out[..8]);
    out.extend_from_slice(&prelude_crc.to_be_bytes());
    out.extend_from_slice(headers);
    out.extend_from_slice(payload);
    let message_crc = crc32(&out[..]);
    out.extend_from_slice(&message_crc.to_be_bytes());
    out.freeze()
}

/// 解码单帧 payload（测试/诊断用；生产走 [`find_header_value`] + serde_json）。
#[cfg(test)]
pub(crate) fn decode_frame_payload(frame: &[u8]) -> Option<Value> {
    let total_len = frame.len();
    if total_len < MIN_EVENTSTREAM_FRAME_SIZE {
        return None;
    }
    let header_len = u32::from_be_bytes(frame[4..8].try_into().ok()?) as usize;
    let end = PRELUDE_SIZE + header_len;
    if end + 4 > total_len {
        return None;
    }
    serde_json::from_slice(&frame[end..total_len - 4]).ok()
}

/// 测试辅助：读取帧的 event type。
#[cfg(test)]
pub(crate) fn decode_frame_event_type(frame: &[u8]) -> Option<String> {
    let header_len = u32::from_be_bytes(frame[4..8].try_into().ok()?) as usize;
    let end = PRELUDE_SIZE + header_len;
    if end + 4 > frame.len() {
        return None;
    }
    find_header_value(&frame[PRELUDE_SIZE..end], ":event-type").map(str::to_string)
}

/// 测试辅助：校验帧的 prelude/message CRC。
#[cfg(test)]
pub(crate) fn frame_crc_valid(frame: &[u8]) -> bool {
    if frame.len() < MIN_EVENTSTREAM_FRAME_SIZE {
        return false;
    }
    let prelude_crc = u32::from_be_bytes(match frame[8..12].try_into() {
        Ok(bytes) => bytes,
        Err(_) => return false,
    });
    if crc32(&frame[..8]) != prelude_crc {
        return false;
    }
    let message_crc = u32::from_be_bytes(match frame[frame.len() - 4..].try_into() {
        Ok(bytes) => bytes,
        Err(_) => return false,
    });
    crc32(&frame[..frame.len() - 4]) == message_crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    // ---- 测试帧构造 ------------------------------------------------------

    fn header_bytes(pairs: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, value) in pairs {
            out.push(name.len() as u8);
            out.extend_from_slice(name.as_bytes());
            out.push(7); // value type: string
            out.extend_from_slice(&(value.len() as u16).to_be_bytes());
            out.extend_from_slice(value.as_bytes());
        }
        out
    }

    fn build_test_frame(event_type: &str, payload: &Value) -> Vec<u8> {
        let headers = header_bytes(&[
            (":message-type", "event"),
            (":event-type", event_type),
            (":content-type", "application/json"),
        ]);
        rebuild_frame(&headers, &serde_json::to_vec(payload).unwrap()).to_vec()
    }

    fn stream_from_chunks(
        chunks: Vec<Vec<u8>>,
    ) -> impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> {
        stream::iter(
            chunks
                .into_iter()
                .map(|chunk| Ok(bytes::Bytes::from(chunk))),
        )
    }

    async fn collect_frames(chunks: Vec<Vec<u8>>, input_tokens: u64) -> Vec<u8> {
        let mut out = Vec::new();
        let mut stream = Box::pin(rewrite_event_stream(
            stream_from_chunks(chunks),
            input_tokens,
        ));
        while let Some(item) = stream.next().await {
            out.extend_from_slice(&item.expect("stream item"));
        }
        out
    }

    /// 按帧边界切分输出（测试用；帧来自被测实现，长度可信）。
    fn split_frames(bytes: &[u8]) -> Vec<&[u8]> {
        let mut frames = Vec::new();
        let mut offset = 0;
        while offset + 4 <= bytes.len() {
            let total_len =
                u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
            if total_len < MIN_EVENTSTREAM_FRAME_SIZE || offset + total_len > bytes.len() {
                break;
            }
            frames.push(&bytes[offset..offset + total_len]);
            offset += total_len;
        }
        frames
    }

    // ---- 输入估算 --------------------------------------------------------

    #[test]
    fn estimates_input_from_native_request_history_tools_and_system() {
        let body = serde_json::json!({
            "conversationState": {
                "systemPrompt": "You are Kiro, an agentic AI software engineer.",
                "history": [
                    {"userInputMessage": {"content": "hello there, please inspect the repository"}},
                    {"assistantResponseMessage": {"content": "I will follow these instructions."}},
                    {"userInputMessage": {"content": "list files", "userInputMessageContext": {
                        "toolResults": [{"toolUseId": "t1", "content": [{"text": "a.txt\nb.txt"}]}]
                    }}},
                    {"assistantResponseMessage": {"content": "", "toolUses": [
                        {"toolUseId": "t1", "name": "list_directory", "input": {"path": "/tmp"}}
                    ]}}
                ],
                "currentMessage": {"userInputMessage": {"content": "continue please",
                    "userInputMessageContext": {"tools": [
                        {"toolSpecification": {"name": "list_directory",
                         "description": "Tool for listing directory contents in long format",
                         "inputSchema": {"json": {"type": "object", "properties": {"path": {"type": "string"}}}}}}
                    ]}}}
            },
            "additionalModelRequestFields": {"max_tokens": 64000, "thinking": {"type": "enabled", "budget_tokens": 20000}}
        });
        let tokens = estimate_input_tokens(&body.to_string());
        // 只断言"明显包含了各段文本"，不做 tiktoken 精确值断言（编码器版本差异）。
        assert!(tokens > 60, "estimated={tokens}");
        assert!(tokens < 2_000, "estimated={tokens}");
    }

    #[test]
    fn input_estimate_is_robust_for_missing_or_invalid_fields() {
        assert_eq!(estimate_input_tokens("not json"), 0);
        assert_eq!(estimate_input_tokens("{}"), 0);
        assert_eq!(
            estimate_input_tokens(r#"{"conversationState":{"history":[{"userInputMessage":{}}]}}"#),
            0
        );
        // 图片 base64 不计入 token 估算（避免严重高估）
        let with_image = serde_json::json!({
            "conversationState": {"currentMessage": {"userInputMessage": {
                "content": "what is in this image?",
                "images": [{"format": "png", "source": {"bytes": "AAAA".repeat(50_000)}}]
            }}}
        });
        let tokens = estimate_input_tokens(&with_image.to_string());
        assert!(
            tokens < 100,
            "image base64 must not be counted, got {tokens}"
        );
    }

    #[test]
    fn input_estimate_is_capped_at_one_million() {
        // 截断逻辑用纯函数验证（避免构造 4MB+ 文本来触发 tiktoken 编码）。
        assert_eq!(cap_input_tokens(0), 0);
        assert_eq!(cap_input_tokens(1_000_000), 1_000_000);
        assert_eq!(
            cap_input_tokens(MAX_INPUT_TOKEN_ESTIMATE + 1),
            MAX_INPUT_TOKEN_ESTIMATE
        );
        assert_eq!(cap_input_tokens(u64::MAX), MAX_INPUT_TOKEN_ESTIMATE);
    }

    // ---- tokenUsage 注入 -------------------------------------------------

    #[tokio::test]
    async fn injects_token_usage_into_metadata_event_and_keeps_other_frames_identical() {
        let text_frame = build_test_frame(
            "assistantResponseEvent",
            &serde_json::json!({"content": "Hello from native stream."}),
        );
        let tool_frame = build_test_frame(
            "toolUseEvent",
            &serde_json::json!({"toolUseId": "t1", "name": "list_directory", "input": "{\"path\":\"/tmp\"}", "stop": true}),
        );
        let metering_frame = build_test_frame(
            "meteringEvent",
            &serde_json::json!({"usage": 0.01, "unit": "credit", "unitPlural": "credits", "cacheReadInputTokens": 120, "cacheCreationInputTokens": 30}),
        );
        let metadata_frame = build_test_frame(
            "metadataEvent",
            &serde_json::json!({"stopReason": "END_TURN"}),
        );

        // 各种奇怪的分块边界（含单字节）都必须正确组装。
        let stream_bytes = [
            text_frame.clone(),
            tool_frame.clone(),
            metering_frame.clone(),
            metadata_frame.clone(),
        ]
        .concat();
        let chunks: Vec<Vec<u8>> = stream_bytes.chunks(7).map(|chunk| chunk.to_vec()).collect();

        let output = collect_frames(chunks, 1_000).await;
        let frames = split_frames(&output);
        assert_eq!(frames.len(), 4, "all frames forwarded");

        // 非 metadata 帧逐字节一致。
        assert_eq!(frames[0], text_frame.as_slice());
        assert_eq!(frames[1], tool_frame.as_slice());
        assert_eq!(frames[2], metering_frame.as_slice());

        // metadata 帧被重建：event type 不变、CRC 合法、tokenUsage 注入正确。
        let metadata = frames[3];
        assert_eq!(
            decode_frame_event_type(metadata).as_deref(),
            Some("metadataEvent")
        );
        assert!(frame_crc_valid(metadata), "rebuilt frame CRC must be valid");
        let payload = decode_frame_payload(metadata).unwrap();
        assert_eq!(payload["stopReason"], "END_TURN");
        let usage = &payload["tokenUsage"];
        let expected_output =
            crate::token::count_tokens("Hello from native stream.{\"path\":\"/tmp\"}");
        assert_eq!(usage["outputTokens"], expected_output);
        assert_eq!(usage["cacheReadInputTokens"], 120);
        assert_eq!(usage["cacheWriteInputTokens"], 30);
        assert_eq!(usage["uncachedInputTokens"], 1_000u64 - 120 - 30);
        assert_eq!(usage["totalTokens"], 1_000u64 + expected_output);
    }

    #[tokio::test]
    async fn metadata_event_with_existing_token_usage_is_forwarded_unchanged() {
        let metadata = build_test_frame(
            "metadataEvent",
            &serde_json::json!({"stopReason": "END_TURN", "tokenUsage": {
                "uncachedInputTokens": 1, "outputTokens": 2, "totalTokens": 3,
                "cacheReadInputTokens": 4, "cacheWriteInputTokens": 5
            }}),
        );
        let output = collect_frames(vec![metadata.clone()], 999).await;
        assert_eq!(output, metadata);
    }

    #[tokio::test]
    async fn without_metadata_event_stream_is_byte_identical() {
        let frames = [
            build_test_frame(
                "assistantResponseEvent",
                &serde_json::json!({"content": "abc"}),
            ),
            build_test_frame(
                "meteringEvent",
                &serde_json::json!({"usage": 0.1, "unit": "credit", "unitPlural": "credits"}),
            ),
        ]
        .concat();
        let chunks: Vec<Vec<u8>> = frames.chunks(13).map(|chunk| chunk.to_vec()).collect();
        let output = collect_frames(chunks, 42).await;
        assert_eq!(output, frames);
    }

    #[tokio::test]
    async fn truncated_tail_bytes_are_flushed_unchanged() {
        let frame = build_test_frame(
            "assistantResponseEvent",
            &serde_json::json!({"content": "hi"}),
        );
        let mut bytes = frame.clone();
        bytes.extend_from_slice(&frame[..5]); // 不完整尾帧
        let expected_tail = bytes[frame.len()..].to_vec();
        let output = collect_frames(vec![bytes.clone()], 10).await;
        assert_eq!(&output[..frame.len()], frame.as_slice());
        assert_eq!(&output[frame.len()..], expected_tail.as_slice());
    }

    #[tokio::test]
    async fn garbage_stream_is_passed_through_without_panicking() {
        // 非法 total_len：退化为原样透传。
        let garbage = b"not-an-eventstream-frame".to_vec();
        let output = collect_frames(vec![garbage.clone()], 10).await;
        assert_eq!(output, garbage);
    }

    #[test]
    fn metering_cache_fields_accept_camel_and_snake_case() {
        let mut tracker = OutputTracker::default();
        tracker.observe(
            "meteringEvent",
            &serde_json::json!({"cacheReadInputTokens": 7, "cacheCreationInputTokens": 3}),
        );
        assert_eq!(tracker.cache_tokens(), (7, 3));

        let mut tracker = OutputTracker::default();
        tracker.observe(
            "meteringEvent",
            &serde_json::json!({"cache_read_input_tokens": 11, "cache_creation_input_tokens": 5}),
        );
        assert_eq!(tracker.cache_tokens(), (11, 5));

        // 负数/非法值不写入
        let mut tracker = OutputTracker::default();
        tracker.observe(
            "meteringEvent",
            &serde_json::json!({"cacheReadInputTokens": -5, "cacheCreationInputTokens": "x"}),
        );
        assert_eq!(tracker.cache_tokens(), (0, 0));
    }

    #[test]
    fn token_usage_clamps_uncached_input_at_zero() {
        let mut tracker = OutputTracker::default();
        tracker.observe(
            "meteringEvent",
            &serde_json::json!({"cacheReadInputTokens": 900, "cacheCreationInputTokens": 200}),
        );
        let usage = tracker.token_usage(100);
        assert_eq!(usage["uncachedInputTokens"], 0);
        assert_eq!(usage["cacheReadInputTokens"], 900);
        assert_eq!(usage["cacheWriteInputTokens"], 200);
        assert_eq!(usage["totalTokens"], 100);
    }
}
