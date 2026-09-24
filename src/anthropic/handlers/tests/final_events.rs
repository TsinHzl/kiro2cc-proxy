// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 非流式桥接与最终事件测试（自 handlers/tests.rs 拆出，纯代码搬移）
#[cfg(test)]
mod tests {

    use super::super::super::nonstream::non_stream_bridge_step;

    use super::super::bridge::tests::bridge_stream_context;

    use crate::kiro::model::events::Event;

    #[test]
    fn test_generate_final_events_message_stop_exactly_once() {
        // 桥接收尾恰好一次：generate_final_events 的 message_stop 由 message_ended
        // 门控——首次调用补发 message_stop，重复调用不再产生（防客户端双 message_stop）
        let mut ctx = bridge_stream_context();
        ctx.process_kiro_event(&Event::AssistantResponse(
            serde_json::from_str(r#"{"content":"回答正文"}"#).unwrap(),
        ));
        // 模拟流结束：置位 message_ended 前的最终事件序列
        let final_events = ctx.generate_final_events();
        let stops: Vec<_> = final_events
            .iter()
            .filter(|e| e.event == "message_stop")
            .collect();
        assert_eq!(stops.len(), 1, "首次收尾应恰好发出 1 个 message_stop");
        // 桥接失败兜底路径可能再次调用收尾——message_ended 门控保证不重发
        let repeated = ctx.generate_final_events();
        assert!(
            !repeated.iter().any(|e| e.event == "message_stop"),
            "重复收尾不得再次发出 message_stop"
        );
    }

    // ---- 非流式桥接（任务 5.5 单测，D4 非流式段 / D5 非流式段）----

    fn non_stream_tool_use_event(
        name: &str,
        id: &str,
        input: &str,
        stop: bool,
    ) -> crate::kiro::model::events::ToolUseEvent {
        // input 与上游流一致，是原始 JSON 字符串（未解析），可传分片
        crate::kiro::model::events::ToolUseEvent {
            name: name.to_string(),
            tool_use_id: id.to_string(),
            input: input.to_string(),
            stop,
        }
    }

    #[test]
    fn test_non_stream_bridge_step_intercept_and_aggregate() {
        // 截获聚合：轮次未达上限的 web_search 分片被截获，stop 时完成并解析 query；
        // 期间不产生普通 tool_use 语义（调用方据此不置 has_tool_use）
        let mut collecting: Option<(String, String)> = None;

        // 分片 1（非 stop）→ 截获、未完成
        let (intercepted, completed) = non_stream_bridge_step(
            &mut collecting,
            0,
            5,
            &non_stream_tool_use_event("web_search", "tu1", r#"{"query":"rus"#, false),
        );
        assert!(intercepted);
        assert!(completed.is_none());
        assert!(collecting.is_some());

        // 分片 2（stop）→ 完成截获，query 聚合完整
        let (intercepted, completed) = non_stream_bridge_step(
            &mut collecting,
            0,
            5,
            &non_stream_tool_use_event("web_search", "tu1", r#"t"}"#, true),
        );
        assert!(intercepted);
        let pending = completed.expect("stop 分片应完成截获");
        assert_eq!(pending.tool_use_id, "tu1");
        assert_eq!(pending.query, "rust");
        assert!(collecting.is_none());
    }

    #[test]
    fn test_non_stream_bridge_step_query_parse_failure_yields_empty() {
        // input 非 JSON（query 解析失败）→ parse_bridge_query 兜底空串，仍完成截获
        let (intercepted, completed) = non_stream_bridge_step(
            &mut None,
            0,
            5,
            &non_stream_tool_use_event("web_search", "tu-bad", "not-json", true),
        );
        assert!(intercepted);
        assert_eq!(completed.unwrap().query, "");
    }

    #[test]
    fn test_non_stream_bridge_step_limit_passthrough() {
        // 轮次耗尽（rounds_used >= max_rounds）→ 不截获，按普通 tool_use 透传；
        // 非桥接请求（max_rounds=0）同样不截获
        let (intercepted, completed) = non_stream_bridge_step(
            &mut None,
            3,
            3,
            &non_stream_tool_use_event("web_search", "tu1", r#"{"query":"a"}"#, true),
        );
        assert!(!intercepted, "轮次耗尽后应按普通 tool_use 透传");
        assert!(completed.is_none());

        let (intercepted, _) = non_stream_bridge_step(
            &mut None,
            0,
            0,
            &non_stream_tool_use_event("web_search", "tu2", r#"{"query":"b"}"#, true),
        );
        assert!(!intercepted, "非桥接请求（max_rounds=0）不应截获");
    }

    #[test]
    fn test_non_stream_bridge_step_other_tool_passthrough() {
        // 非目标工具不截获、不污染聚合状态
        let mut collecting: Option<(String, String)> = None;
        let (intercepted, _) = non_stream_bridge_step(
            &mut collecting,
            0,
            5,
            &non_stream_tool_use_event("Read", "tu-file", r#"{"path":"a.rs"}"#, true),
        );
        assert!(!intercepted);
        assert!(collecting.is_none(), "非 web_search 不得进入聚合状态");
    }
}
