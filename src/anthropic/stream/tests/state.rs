// Copyright (c) 2026 Harllan He. Licensed under MIT.
// SSE 状态管理与 token 口径测试（自 stream/tests.rs 拆出，纯代码搬移）
#[cfg(test)]
mod tests {
    use crate::anthropic::stream::{
        CLIENT_ASSUMED_CONTEXT_WINDOW, SseEvent, SseStateManager, StreamContext,
        context_window_for_model, count_token_chars, find_real_thinking_end_tag,
        find_real_thinking_end_tag_at_buffer_end, find_real_thinking_start_tag, scale_for_client,
        split_thinking_and_visible, tokens_from_chars,
    };
    use crate::cache::PromptCacheUsage;
    use serde_json::json;

    /// 测试辅助：单段文本的 token 估算 —— 生产路径已改为分桶累加 + 收尾取整，
    /// 不再需要这个封装，只在断言里用来算「若单次估算会得到多少」。
    fn est(text: &str) -> i32 {
        let (cn, other) = count_token_chars(text);
        tokens_from_chars(cn, other)
    }

    /// 非流式整段分离：thinking 与可见文本必须彻底分开，标签原文不得残留
    #[test]
    fn test_split_thinking_and_visible() {
        let (t, v) = split_thinking_and_visible("<thinking>推理过程</thinking>\n\n可见回答");
        assert_eq!(t, "推理过程");
        assert_eq!(v, "可见回答");

        // 无 thinking：原样落到可见内容
        let (t, v) = split_thinking_and_visible("纯文本回答");
        assert!(t.is_empty());
        assert_eq!(v, "纯文本回答");

        // 结束标签后无 `\n\n`（流末尾场景）
        let (t, v) = split_thinking_and_visible("<thinking>abc</thinking>");
        assert_eq!(t, "abc");
        assert!(v.is_empty());

        // 未闭合：其后全部归 thinking，可见内容只保留标签之前的部分
        let (t, v) = split_thinking_and_visible("前置<thinking>没有闭合");
        assert_eq!(t, "没有闭合");
        assert_eq!(v, "前置");

        // 被引用字符包裹的标签不算真标签
        let (t, v) = split_thinking_and_visible("讨论 `<thinking>` 标签本身");
        assert!(t.is_empty());
        assert_eq!(v, "讨论 `<thinking>` 标签本身");
    }

    /// tool_use 的两个 token 口径分工：计费口径无条件累加（与 text 分支一致，
    /// 上游已生成即已计费），上报口径只统计真正发出的 delta。
    #[test]
    fn test_tool_use_tokens_split_billing_and_reported() {
        use crate::kiro::model::events::ToolUseEvent;

        let mut ctx = StreamContext::new_with_thinking("claude-sonnet-5", 100, false);
        let ev = ToolUseEvent {
            name: "Read".to_string(),
            tool_use_id: "tu_1".to_string(),
            input: r#"{"file_path":"/tmp/a.txt"}"#.to_string(),
            stop: true,
        };

        let events = ctx.process_tool_use(&ev);
        assert!(events.iter().any(|e| e.event == "content_block_delta"));
        let expected = (ev.input.len() as i32 + 3) / 4;
        assert_eq!(ctx.visible_output_tokens(), expected);
        assert_eq!(ctx.output_tokens(), expected);

        // 同一 tool_use_id 在 stop 之后再来一帧：delta 被状态机拒绝，
        // 上报口径不增长，但计费口径照记（上游成本已产生）
        let late = ctx.process_tool_use(&ev);
        assert!(!late.iter().any(|e| e.event == "content_block_delta"));
        assert_eq!(ctx.visible_output_tokens(), expected);
        // 两帧的计费口径由累计字符数一次派生，不是 expected * 2 ——
        // 差掉的 1 token 正是被消除的逐次取整噪音
        let billed_two = tokens_from_chars(0, ev.input.len() as i64 * 2);
        assert_eq!(ctx.output_tokens(), billed_two);
        assert!(billed_two < expected * 2);
    }

    /// 本次修复的核心等价性：同一段文本无论被切成多少个 chunk 投喂，两个 token
    /// 口径的派生结果都必须完全一致。旧实现每 chunk 各自向上取整（中文桶与非中文桶
    /// 各一次），切得越碎高估越多 —— 实测同一响应计费口径 8703、上报口径 8061，
    /// 差值 642 全部是取整噪音（该响应 thinking 块数为 0）。
    #[test]
    fn test_output_tokens_invariant_to_chunking() {
        let full = "这是一段中英混合的输出 with some ASCII words，用来验证分块不变性。".repeat(20);

        // 一次性投喂
        let mut whole = StreamContext::new_with_thinking("claude-sonnet-5", 100, false);
        let _ = whole.process_assistant_response(&full);

        // 切成小 chunk 逐段投喂（按 char 边界切，避免截断多字节 UTF-8）
        let mut chunked = StreamContext::new_with_thinking("claude-sonnet-5", 100, false);
        let chars: Vec<char> = full.chars().collect();
        for piece in chars.chunks(7) {
            let s: String = piece.iter().collect();
            let _ = chunked.process_assistant_response(&s);
        }

        assert_eq!(
            chunked.output_tokens(),
            whole.output_tokens(),
            "分块 {} vs 整段 {} —— 计费口径必须与分块粒度无关",
            chunked.output_tokens(),
            whole.output_tokens()
        );
        // 上报口径同理（thinking 未启用，所有 delta 都实际发出）
        assert_eq!(
            chunked.visible_output_tokens(),
            whole.visible_output_tokens()
        );
        // 防止未来把 repeat 改小导致本测试失去区分度
        let chunk_count = chars.len().div_ceil(7);
        assert!(
            chunk_count > 50,
            "chunk 数 {chunk_count} 太少，测不出取整噪音的量级"
        );
    }

    /// 对客户端上报的 output_tokens 必须是「可见输出」——排除 thinking，且不再被
    /// 固定上限截断（旧实现把真实 10145 上报成 380）。
    #[test]
    fn test_visible_output_tokens_excludes_thinking_and_is_uncapped() {
        let mut ctx = StreamContext::new_with_thinking("gpt-5.6-luna", 1000, true);
        let thinking = "y".repeat(4000); // ≈1000 token
        let visible = "x".repeat(8000); // ≈2000 token，远超旧的 380 上限
        // 结束标签必须写成 `</thinking>\n\n` —— find_real_thinking_end_tag 要求尾随空行
        let _ = ctx
            .process_assistant_response(&format!("<thinking>{thinking}</thinking>\n\n{visible}"));
        let _ = ctx.generate_final_events(); // flush 掉 lookahead 缓冲

        let visible_est = est(&visible);
        let thinking_est = est(&thinking);
        // 上报量 ≈ 可见文本；分段 delta 的向上取整允许微小上浮，但绝不含 thinking 的量级
        assert!(
            ctx.visible_output_tokens() >= visible_est,
            "visible={} output={} visible_est={} thinking_est={} buf_len={}",
            ctx.visible_output_tokens(),
            ctx.output_tokens(),
            visible_est,
            thinking_est,
            ctx.thinking_buffer.len()
        );
        assert!(ctx.visible_output_tokens() < visible_est + thinking_est);
        // 旧的 380 固定上限已不再截断
        assert!(ctx.visible_output_tokens() > 380);
        // 计费口径仍含 thinking，未受本次改动影响
        assert!(ctx.output_tokens() >= visible_est + thinking_est);
    }

    /// message_start 与 message_delta 的 cache_* 必须同口径 —— 防止 message_start
    /// 泄漏 PromptCacheUsage 模拟值，被客户端记成凭空的 cache write。
    #[test]
    fn test_message_start_cache_usage_matches_final() {
        let ctx = StreamContext::new_with_thinking("gpt-5.6-luna", 45409, false)
            // 模拟值刻意给非零 creation，若被泄漏则断言失败
            .with_prompt_cache_usage(PromptCacheUsage {
                input_tokens: 5000,
                cache_creation_input_tokens: 3906,
                cache_read_input_tokens: 36503,
                cache_creation_5m_input_tokens: 3906,
                cache_creation_1h_input_tokens: 0,
            })
            .with_prefix_estimated_tokens(36348);

        let start = ctx.create_message_start_event();
        let start_usage = &start["message"]["usage"];
        assert_eq!(start_usage["cache_creation_input_tokens"], 0);
        assert_eq!(
            start_usage["cache_read_input_tokens"],
            scale_for_client(36348, "gpt-5.6-luna")
        );
        assert_eq!(
            start_usage["input_tokens"],
            scale_for_client(45409 - 36348, "gpt-5.6-luna")
        );

        // 末尾 message_delta 走同一派生逻辑，三元组必须一致
        assert_eq!(ctx.derive_report_cache_usage(45409), (9061, 0, 36348));
    }

    #[test]
    fn test_scale_for_client_basic() {
        // 默认模型（非 4.7/4.8）：× 0.6657
        assert_eq!(scale_for_client(100_000, "claude-opus-4-6"), 66_570);
        assert_eq!(scale_for_client(85_000, "claude-sonnet-4-6"), 56_585);
        // sonnet-5 与 sonnet-4.6 同档，不归入大窗口分支
        assert_eq!(scale_for_client(100_000, "claude-sonnet-5"), 66_570);
        assert_eq!(scale_for_client(0, "claude-opus-4-6"), 0);
        assert_eq!(scale_for_client(1, "claude-opus-4-6"), 1);
        assert_eq!(scale_for_client(-100, "claude-opus-4-6"), 0);
    }

    #[test]
    fn test_scale_for_client_opus_4_7_4_8_unified() {
        // opus-4.7/4.8 已与其他模型统一为 × 0.6657（不再区分大窗口分支）
        assert_eq!(scale_for_client(100_000, "claude-opus-4-7"), 66_570);
        assert_eq!(scale_for_client(100_000, "claude-opus-4-8"), 66_570);
        assert_eq!(scale_for_client(200_000, "claude-opus-4-7"), 133_140);
        assert_eq!(scale_for_client(1, "claude-opus-4-8"), 1);
        assert_eq!(scale_for_client(0, "claude-opus-4-7"), 0);
    }

    #[test]
    fn test_scale_for_client_opus_5_unified() {
        // opus-5 系列同样统一为 × 0.6657
        assert_eq!(scale_for_client(100_000, "claude-opus-5"), 66_570);
        assert_eq!(scale_for_client(100_000, "claude-opus-5-thinking"), 66_570);
        assert_eq!(scale_for_client(200_000, "Claude-Opus-5"), 133_140);
        assert_eq!(scale_for_client(1, "claude-opus-5"), 1);
        // opus-5 空格别名（如客户端发送 "Claude Opus 5"）
        assert_eq!(scale_for_client(100_000, "Claude Opus 5"), 66_570);
        // opus-5 点号别名（如客户端发送 "claude-opus.5"）
        assert_eq!(scale_for_client(100_000, "claude-opus.5"), 66_570);

        // 回归：sonnet-5 / opus-4.5 / opus-4.6 / opus-4.7 / opus-4.8 均同档
        assert_eq!(scale_for_client(100_000, "claude-sonnet-5"), 66_570);
        assert_eq!(scale_for_client(100_000, "claude-opus-4-5"), 66_570);
        assert_eq!(scale_for_client(100_000, "claude-opus-4-6"), 66_570);
        assert_eq!(scale_for_client(100_000, "claude-opus-4-7"), 66_570);
        assert_eq!(scale_for_client(100_000, "claude-opus-4-8"), 66_570);
    }

    #[test]
    fn test_scale_for_client_non_round() {
        // 11 × 0.6657 = ceil(7.3227) = 8
        assert_eq!(scale_for_client(11, "claude-opus-4-6"), 8);
        assert_eq!(scale_for_client(11, "claude-opus-4-7"), 8);
        // i32::MAX 不溢出
        let r = scale_for_client(i32::MAX, "claude-opus-4-6");
        assert!(r > 0 && r < i32::MAX);
        let r2 = scale_for_client(i32::MAX, "claude-opus-4-8");
        assert!(r2 > 0 && r2 < i32::MAX);
    }

    #[test]
    fn test_sse_event_format() {
        let event = SseEvent::new("message_start", json!({"type": "message_start"}));
        let sse_str = event.to_sse_string();

        assert!(sse_str.starts_with("event: message_start\n"));
        assert!(sse_str.contains("data: "));
        assert!(sse_str.ends_with("\n\n"));
    }

    #[test]
    fn test_sse_state_manager_message_start() {
        let mut manager = SseStateManager::new();

        // 第一次应该成功
        let event = manager.handle_message_start(json!({"type": "message_start"}));
        assert!(event.is_some());

        // 第二次应该被跳过
        let event = manager.handle_message_start(json!({"type": "message_start"}));
        assert!(event.is_none());
    }

    #[test]
    fn test_sse_state_manager_block_lifecycle() {
        let mut manager = SseStateManager::new();

        // 创建块
        let events = manager.handle_content_block_start(0, "text", json!({}));
        assert_eq!(events.len(), 1);

        // delta
        let event = manager.handle_content_block_delta(0, json!({}));
        assert!(event.is_some());

        // stop
        let event = manager.handle_content_block_stop(0);
        assert!(event.is_some());

        // 重复 stop 应该被跳过
        let event = manager.handle_content_block_stop(0);
        assert!(event.is_none());
    }

    #[test]
    fn test_text_delta_after_tool_use_restarts_text_block() {
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false);

        let initial_events = ctx.generate_initial_events();
        assert!(
            initial_events
                .iter()
                .any(|e| e.event == "content_block_start"
                    && e.data["content_block"]["type"] == "text")
        );

        let initial_text_index = ctx
            .text_block_index
            .expect("initial text block index should exist");

        // tool_use 开始会自动关闭现有 text block
        let tool_events = ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "test_tool".to_string(),
            tool_use_id: "tool_1".to_string(),
            input: "{}".to_string(),
            stop: false,
        });
        assert!(
            tool_events.iter().any(|e| {
                e.event == "content_block_stop"
                    && e.data["index"].as_i64() == Some(initial_text_index as i64)
            }),
            "tool_use should stop the previous text block"
        );

        // 之后再来文本增量，应自动创建新的 text block 而不是往已 stop 的块里写 delta
        let text_events = ctx.process_assistant_response("hello");
        let new_text_start_index = text_events.iter().find_map(|e| {
            if e.event == "content_block_start" && e.data["content_block"]["type"] == "text" {
                e.data["index"].as_i64()
            } else {
                None
            }
        });
        assert!(
            new_text_start_index.is_some(),
            "should start a new text block"
        );
        assert_ne!(
            new_text_start_index.unwrap(),
            initial_text_index as i64,
            "new text block index should differ from the stopped one"
        );
        assert!(
            text_events.iter().any(|e| {
                e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "text_delta"
                    && e.data["delta"]["text"] == "hello"
            }),
            "should emit text_delta after restarting text block"
        );
    }

    #[test]
    fn test_tool_use_flushes_pending_thinking_buffer_text_before_tool_block() {
        // thinking 模式下，短文本可能被暂存在 thinking_buffer 以等待 `<thinking>` 的跨 chunk 匹配。
        // 当紧接着出现 tool_use 时，应先 flush 这段文本，再开始 tool_use block。
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, true);
        let _initial_events = ctx.generate_initial_events();

        // 两段短文本（各 2 个中文字符），总长度仍可能不足以满足 safe_len>0 的输出条件，
        // 因而会留在 thinking_buffer 中等待后续 chunk。
        let ev1 = ctx.process_assistant_response("有修");
        assert!(
            ev1.iter().all(|e| e.event != "content_block_delta"),
            "short prefix should be buffered under thinking mode"
        );
        let ev2 = ctx.process_assistant_response("改：");
        assert!(
            ev2.iter().all(|e| e.event != "content_block_delta"),
            "short prefix should still be buffered under thinking mode"
        );

        let events = ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "Write".to_string(),
            tool_use_id: "tool_1".to_string(),
            input: "{}".to_string(),
            stop: false,
        });

        let text_start_index = events.iter().find_map(|e| {
            if e.event == "content_block_start" && e.data["content_block"]["type"] == "text" {
                e.data["index"].as_i64()
            } else {
                None
            }
        });
        let pos_text_delta = events.iter().position(|e| {
            e.event == "content_block_delta" && e.data["delta"]["type"] == "text_delta"
        });
        let pos_text_stop = text_start_index.and_then(|idx| {
            events.iter().position(|e| {
                e.event == "content_block_stop" && e.data["index"].as_i64() == Some(idx)
            })
        });
        let pos_tool_start = events.iter().position(|e| {
            e.event == "content_block_start" && e.data["content_block"]["type"] == "tool_use"
        });

        assert!(
            text_start_index.is_some(),
            "should start a text block to flush buffered text"
        );
        assert!(
            pos_text_delta.is_some(),
            "should flush buffered text as text_delta"
        );
        assert!(
            pos_text_stop.is_some(),
            "should stop text block before tool_use block starts"
        );
        assert!(pos_tool_start.is_some(), "should start tool_use block");

        let pos_text_delta = pos_text_delta.unwrap();
        let pos_text_stop = pos_text_stop.unwrap();
        let pos_tool_start = pos_tool_start.unwrap();

        assert!(
            pos_text_delta < pos_text_stop && pos_text_stop < pos_tool_start,
            "ordering should be: text_delta -> text_stop -> tool_use_start"
        );

        assert!(
            events.iter().any(|e| {
                e.event == "content_block_delta"
                    && e.data["delta"]["type"] == "text_delta"
                    && e.data["delta"]["text"] == "有修改："
            }),
            "flushed text should equal the buffered prefix"
        );
    }

    #[test]
    fn test_estimate_tokens() {
        assert!(est("Hello") > 0);
        assert!(est("你好") > 0);
        assert!(est("Hello 你好") > 0);
        // 两桶皆空返回 0 —— is_empty_response 依赖这个状态
        assert_eq!(est(""), 0);
    }
}
