// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic SSE 增量解析器
//!
//! 把 [`crate::anthropic::stream`] 产出的字节流（格式为 `event: <name>\ndata: <json>\n\n`）
//! 重新解析为结构化事件，供 OpenAI 协议转换器消费。
//!
//! 之所以要"解析自己刚序列化的东西"：本模块是外挂适配器，不改动下游 handler 的实现，
//! 只能从其返回的 `Response` body 流入手。详见 design.md 决策 1。
//!
//! # 分帧要点
//!
//! - 以空行（`\n\n` 或 `\r\n\r\n`）为帧边界，跨 chunk 的半帧留在缓冲区等待后续字节
//! - 缓冲区按字节而非字符串维护，避免上游在 UTF-8 多字节序列中间切断 chunk 时 panic
//! - `event: ping` 是现有实现每 25s 注入的保活帧（`handlers.rs` 的 `create_ping_sse`），
//!   属常规行为，静默丢弃且**不记录日志**，否则长连接会话会持续产生噪声

use serde_json::Value;

/// 静默丢弃的事件名（不下发给客户端、不记录 WARN、不计入未识别事件）
const SILENT_EVENTS: &[&str] = &["ping"];

/// 缓冲区上限：超过即判定上游未按协议分帧，丢弃残余以防内存无界增长
///
/// 正常单帧远小于此值（最大的 `message_start` 也只有几 KB）。仅当下游 SSE 序列化出现
/// 缺失 `\n\n` 边界的 bug、或响应体被截断成一整块超长数据时才会触发。
const MAX_BUFFERED_BYTES: usize = 8 * 1024 * 1024;

/// 解析出的一个 SSE 单元
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SseItem {
    /// 一个具名事件及其 JSON 数据
    Event { name: String, data: Value },
    /// `data: [DONE]` 终止标记（Anthropic 侧一般不发，容错保留）
    Done,
}

/// Anthropic SSE 增量解析器
///
/// 用法：对每个到达的 body chunk 调用 [`SseParser::push`]，流结束后调用
/// [`SseParser::finish`] 收尾（处理缺少尾随空行的最后一帧）。
#[derive(Debug, Default)]
pub(crate) struct SseParser {
    buf: Vec<u8>,
    /// 已因超限丢弃过残余数据，用于抑制重复日志
    overflowed: bool,
}

impl SseParser {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 喂入一段字节，返回本次可完整解析出的所有事件（可能为空）
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Vec<SseItem> {
        self.buf.extend_from_slice(chunk);
        let mut items = Vec::new();
        while let Some((block, consumed)) = next_block(&self.buf) {
            let block = block.to_vec();
            self.buf.drain(..consumed);
            if let Some(item) = parse_block(&block) {
                items.push(item);
            }
        }
        // 走到这里 buf 里已不含帧边界（否则循环不会退出），剩下的是等待续传的半帧。
        // 半帧超限说明上游未按协议分帧，丢弃残余以防无界增长；后续恢复正常分帧仍能继续解析。
        if self.buf.len() > MAX_BUFFERED_BYTES {
            if !self.overflowed {
                self.overflowed = true;
                tracing::error!(
                    buffered = self.buf.len(),
                    limit = MAX_BUFFERED_BYTES,
                    "SSE 缓冲区超限且未见帧边界，已丢弃残余数据"
                );
            }
            self.buf.clear();
        }
        items
    }

    /// 流结束时调用：把缓冲区里没有尾随空行的残余内容当作最后一帧处理
    pub(crate) fn finish(&mut self) -> Vec<SseItem> {
        if self.buf.is_empty() {
            return Vec::new();
        }
        let block = std::mem::take(&mut self.buf);
        parse_block(&block).into_iter().collect()
    }
}

/// 在缓冲区中定位下一个完整帧，返回 (帧内容, 应消费的字节数)
///
/// 帧边界同时接受 `\n\n` 与 `\r\n\r\n`；消费长度包含边界本身。
fn next_block(buf: &[u8]) -> Option<(&[u8], usize)> {
    // 优先找 \n\n（现有实现的输出格式），再兼容 \r\n\r\n
    let lf = find(buf, b"\n\n");
    let crlf = find(buf, b"\r\n\r\n");
    match (lf, crlf) {
        (Some(i), Some(j)) if j < i => Some((&buf[..j], j + 4)),
        (Some(i), _) => Some((&buf[..i], i + 2)),
        (None, Some(j)) => Some((&buf[..j], j + 4)),
        (None, None) => None,
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// 解析单帧文本为 [`SseItem`]；返回 `None` 表示该帧应被丢弃
fn parse_block(block: &[u8]) -> Option<SseItem> {
    let text = match std::str::from_utf8(block) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, "SSE 帧不是合法 UTF-8，已跳过");
            return None;
        }
    };

    let mut event_name = String::new();
    // data 可以跨多行，按 SSE 规范用 \n 拼接
    let mut data_lines: Vec<&str> = Vec::new();

    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            // 空行与注释行（`: comment`）忽略
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            event_name = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
        // id: / retry: 等其他字段当前链路不产生，忽略
    }

    if SILENT_EVENTS.contains(&event_name.as_str()) {
        // ping 等保活帧：静默丢弃，不记录日志
        return None;
    }

    if data_lines.is_empty() {
        if !event_name.is_empty() {
            tracing::warn!(event = %event_name, "SSE 帧缺少 data 字段，已跳过");
        }
        return None;
    }

    let data_text = data_lines.join("\n");
    if data_text.trim() == "[DONE]" {
        return Some(SseItem::Done);
    }

    match serde_json::from_str::<Value>(&data_text) {
        Ok(data) => {
            // 无 event: 行时回退用 data.type 作为事件名（Anthropic 的 data 一律带 type）
            let name = if event_name.is_empty() {
                data.get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            } else {
                event_name
            };
            if SILENT_EVENTS.contains(&name.as_str()) {
                return None;
            }
            Some(SseItem::Event { name, data })
        }
        Err(e) => {
            tracing::warn!(event = %event_name, error = %e, "SSE data 不是合法 JSON，已跳过");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(name: &str, data: Value) -> SseItem {
        SseItem::Event {
            name: name.to_string(),
            data,
        }
    }

    #[test]
    fn parses_single_frame() {
        let mut p = SseParser::new();
        let items = p.push(b"event: message_start\ndata: {\"type\":\"message_start\"}\n\n");
        assert_eq!(
            items,
            vec![ev("message_start", json!({"type":"message_start"}))]
        );
    }

    #[test]
    fn parses_multiple_frames_in_one_chunk() {
        let mut p = SseParser::new();
        let items =
            p.push(b"event: a\ndata: {\"type\":\"a\"}\n\nevent: b\ndata: {\"type\":\"b\"}\n\n");
        assert_eq!(
            items,
            vec![ev("a", json!({"type":"a"})), ev("b", json!({"type":"b"}))]
        );
    }

    #[test]
    fn buffers_partial_frame_across_chunks() {
        let mut p = SseParser::new();
        // 第一段只到一半，不应产出任何事件
        assert!(p.push(b"event: message_start\ndata: {\"ty").is_empty());
        // 补齐后才产出
        let items = p.push(b"pe\":\"message_start\"}\n\n");
        assert_eq!(
            items,
            vec![ev("message_start", json!({"type":"message_start"}))]
        );
    }

    #[test]
    fn handles_utf8_split_across_chunks() {
        // "你好" 的 UTF-8 是 6 字节，在第 3 字节处切断
        let full = "event: x\ndata: {\"t\":\"你好\"}\n\n".as_bytes().to_vec();
        let cut = full.len() - 12;
        let mut p = SseParser::new();
        assert!(p.push(&full[..cut]).is_empty());
        let items = p.push(&full[cut..]);
        assert_eq!(items, vec![ev("x", json!({"t":"你好"}))]);
    }

    #[test]
    fn ping_is_dropped_silently() {
        let mut p = SseParser::new();
        let items = p.push(b"event: ping\ndata: {\"type\": \"ping\"}\n\n");
        assert!(items.is_empty(), "ping 必须被静默丢弃");
    }

    #[test]
    fn ping_between_real_frames_does_not_break_stream() {
        let mut p = SseParser::new();
        let items = p.push(
            b"event: a\ndata: {\"type\":\"a\"}\n\nevent: ping\ndata: {\"type\": \"ping\"}\n\nevent: b\ndata: {\"type\":\"b\"}\n\n",
        );
        assert_eq!(
            items,
            vec![ev("a", json!({"type":"a"})), ev("b", json!({"type":"b"}))]
        );
    }

    #[test]
    fn recognizes_done_marker() {
        let mut p = SseParser::new();
        assert_eq!(p.push(b"data: [DONE]\n\n"), vec![SseItem::Done]);
    }

    #[test]
    fn falls_back_to_data_type_when_event_line_missing() {
        let mut p = SseParser::new();
        let items = p.push(b"data: {\"type\":\"content_block_delta\"}\n\n");
        assert_eq!(
            items,
            vec![ev(
                "content_block_delta",
                json!({"type":"content_block_delta"})
            )]
        );
    }

    #[test]
    fn skips_malformed_json_without_breaking_stream() {
        let mut p = SseParser::new();
        let items =
            p.push(b"event: bad\ndata: {not json}\n\nevent: ok\ndata: {\"type\":\"ok\"}\n\n");
        assert_eq!(items, vec![ev("ok", json!({"type":"ok"}))]);
    }

    #[test]
    fn accepts_crlf_frame_boundary() {
        let mut p = SseParser::new();
        let items = p.push(b"event: a\r\ndata: {\"type\":\"a\"}\r\n\r\n");
        assert_eq!(items, vec![ev("a", json!({"type":"a"}))]);
    }

    #[test]
    fn finish_flushes_trailing_frame_without_blank_line() {
        let mut p = SseParser::new();
        assert!(p.push(b"event: a\ndata: {\"type\":\"a\"}").is_empty());
        assert_eq!(p.finish(), vec![ev("a", json!({"type":"a"}))]);
    }

    #[test]
    fn finish_on_empty_buffer_yields_nothing() {
        let mut p = SseParser::new();
        assert!(p.finish().is_empty());
    }

    #[test]
    fn unbounded_frameless_input_does_not_grow_buffer() {
        let mut p = SseParser::new();
        // 持续喂入不含帧边界的数据，缓冲区不得无界增长
        let chunk = vec![b'x'; 1024 * 1024];
        for _ in 0..20 {
            assert!(p.push(&chunk).is_empty());
            // 每次 push 返回时缓冲区必已收敛到上限内（超限即在本次调用中清空）
            assert!(
                p.buf.len() <= MAX_BUFFERED_BYTES,
                "缓冲区超出上限：{}",
                p.buf.len()
            );
        }
        // 丢弃残余后仍能解析后续正常分帧的数据
        let items = p.push(b"\nevent: a\ndata: {\"type\":\"a\"}\n\n");
        assert_eq!(items, vec![ev("a", json!({"type":"a"}))]);
    }

    #[test]
    fn oversized_tail_after_valid_frame_is_dropped() {
        let mut p = SseParser::new();
        // 单次喂入：合法帧 + 超限的无边界尾部。合法帧要正常产出，尾部要被丢弃
        let mut chunk = b"event: a\ndata: {\"type\":\"a\"}\n\n".to_vec();
        chunk.extend(std::iter::repeat_n(b'x', MAX_BUFFERED_BYTES + 1));
        let items = p.push(&chunk);
        assert_eq!(items, vec![ev("a", json!({"type":"a"}))]);
        assert!(p.buf.is_empty(), "超限尾部应在本次 push 内即被丢弃");
    }
}
