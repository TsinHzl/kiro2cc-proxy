// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 工具使用事件处理与 signature 事件生成（自 context.rs 拆出，纯代码搬移）

use serde_json::json;

use super::StreamContext;
use crate::anthropic::stream::helpers::generate_fake_signature;
use crate::anthropic::stream::state::SseEvent;

impl StreamContext {
    /// 首次调用时生成 signature_delta 事件，后续调用返回空（签名只能在 content_block_stop 之前发送一次）
    pub(super) fn take_signature_events(&mut self) -> Vec<SseEvent> {
        if self.signature_sent {
            return Vec::new();
        }
        self.signature_sent = true;
        self.generate_signature_events()
    }

    /// 生成伪造的 signature_delta 事件
    ///
    /// 检测工具要求 signature_delta 事件的 signature 字段总长度 >= 100 字符才能通过。
    /// 重要：signature_delta 必须附着在 thinking block 上，不能作为独立的 content block。
    /// 只有当流中实际产生了 thinking block 时才注入 signature。
    fn generate_signature_events(&mut self) -> Vec<SseEvent> {
        let thinking_index = match self.thinking_block_index {
            Some(idx) => idx,
            None => return Vec::new(),
        };

        let mut events = Vec::new();
        let signature_content = generate_fake_signature();

        let chunk_size = 40;
        let chunks: Vec<&str> = signature_content
            .as_bytes()
            .chunks(chunk_size)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect();

        for chunk in chunks {
            events.push(SseEvent::new(
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": thinking_index,
                    "delta": {
                        "type": "signature_delta",
                        "signature": chunk
                    }
                }),
            ));
        }

        events
    }
}
