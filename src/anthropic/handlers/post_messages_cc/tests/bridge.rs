// Copyright (c) 2026 Harllan He. Licensed under MIT.
// /cc/v1/messages 端点测试（错误映射 / bridge / 非流式，自 post_messages_cc/tests.rs 拆出）
// bridge 流式拦截与错误映射测试（自 post_messages_cc/tests.rs 拆出，纯代码搬移）

#[cfg(test)]
mod tests {
    #[test]
    fn test_stream_interrupted_error_event_signals_failure_not_success() {
        // 流中断（已有部分内容）必须报错重试，不能是伪装成功的 message_delta/message_stop
        let event = stream_interrupted_error_event();
        assert_eq!(event.event, "error");
        assert_eq!(event.data["type"], "error");
        assert_eq!(event.data["error"]["type"], "overloaded_error");
        assert!(
            event.data["error"]["message"]
                .as_str()
                .unwrap()
                .contains("interrupted"),
            "错误信息应说明是连接中断导致，而非正常结束"
        );
    }

    async fn response_body_text(resp: Response) -> String {
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(body.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_map_provider_error_quota_marker_returns_402() {
        let err = anyhow::anyhow!("绑定的账号本月请求额度已用尽（共 1 个）[QUOTA_EXHAUSTED_ALL]");
        let resp = map_provider_error_with_context(err, "claude-sonnet-4-6", 100);
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn test_map_provider_error_mixed_marker_and_429_prefers_402() {
        // H2 回归：402 分支必须排在 429 分支之前，混合错误串不能被 429 抢先命中
        let err = anyhow::anyhow!("账号A: 429 Too Many Requests；账号B: [QUOTA_EXHAUSTED_ALL]");
        let resp = map_provider_error_with_context(err, "claude-sonnet-4-6", 100);
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
    }

    #[tokio::test]
    async fn test_map_provider_error_bare_monthly_request_count_does_not_trigger_402() {
        // H1 回归：裸串 "MONTHLY_REQUEST_COUNT" 不再单独触发 402——必须要有
        // describe_unavailable 产出的、已确认"scope 内 100% 耗尽"的机器标记，
        // 否则"单账号耗尽、其余账号可用"的场景会被误判为不可重试
        let err = anyhow::anyhow!("账号 #1 Token 刷新失败，尝试下一个账号: MONTHLY_REQUEST_COUNT");
        let resp = map_provider_error_with_context(err, "claude-sonnet-4-6", 100);
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn test_map_provider_error_429_without_marker_returns_429() {
        let err = anyhow::anyhow!("上游限流：429 Too Many Requests");
        let resp = map_provider_error_with_context(err, "claude-sonnet-4-6", 100);
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn test_map_provider_error_default_branch_does_not_leak_internal_detail() {
        // M3 回归：502 兜底分支不应把账号池内部细节（数量/禁用原因拆解）
        // 透传给客户端，完整信息只应进 tracing 日志
        let err = anyhow::anyhow!(
            "绑定的账号均不可用（共 3 个：1 个额度用尽，1 个连续认证失败，1 个手动禁用）"
        );
        let resp = map_provider_error_with_context(err, "claude-sonnet-4-6", 100);
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        let text = response_body_text(resp).await;
        assert!(
            !text.contains("额度用尽") && !text.contains("连续认证失败"),
            "502 响应体不应回显内部账号池细节: {}",
            text
        );
    }

    #[tokio::test]
    async fn test_map_provider_error_context_length_uses_official_too_long_format() {
        // #25 回归：自造文案不被客户端识别为超窗，只会硬报错中断。必须对齐
        // Anthropic 官方 `prompt is too long: N tokens > M maximum`，且 N > M 才成立。
        let err = anyhow::anyhow!(
            r#"流式 API 请求失败: 400 Bad Request {{"message":"Input content length exceeds threshold.","reason":"CONTENT_LENGTH_EXCEEDS_THRESHOLD"}}"#
        );
        let resp = map_provider_error_with_context(err, "claude-opus-5", 754_234);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // 期望值动态取自缩放函数：系数是校准量，改它不应弄红这条错误映射测试
        let n = scale_for_client(754_234, "claude-opus-5");
        assert!(
            n > CLIENT_ASSUMED_CONTEXT_WINDOW,
            "N({}) 必须大于 M({}) 才构成超窗语义",
            n,
            CLIENT_ASSUMED_CONTEXT_WINDOW
        );
        let text = response_body_text(resp).await;
        assert!(
            text.contains(&format_prompt_too_long(754_234, "claude-opus-5")),
            "超窗文案未对齐官方格式: {}",
            text
        );
    }

    #[tokio::test]
    async fn test_map_provider_error_context_length_never_emits_n_le_m() {
        // 上游报超窗但本地估算异常偏小（远程 count_tokens 返回 0）时，照实填会产出
        // `0 tokens > 200000 maximum` —— N ≤ M 自相矛盾，正是本次修复要消除的形态。
        let err = anyhow::anyhow!("CONTENT_LENGTH_EXCEEDS_THRESHOLD");
        let resp = map_provider_error_with_context(err, "claude-opus-5", 0);
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let text = response_body_text(resp).await;
        // 期望值不复用被测函数，避免同义反复
        assert!(
            text.contains(&format!(
                "prompt is too long: {} tokens > {} maximum",
                CLIENT_ASSUMED_CONTEXT_WINDOW + 1,
                CLIENT_ASSUMED_CONTEXT_WINDOW
            )),
            "N 未兜底到 M+1: {}",
            text
        );
    }

    // deadline 为 None 时 wait_deadline 必须永不就绪 —— 这是 /v1 行为零变化的前提：
    // create_sse_stream 的 select! 里该分支等价于不存在。
    #[tokio::test]
    async fn test_wait_deadline_none_never_resolves() {
        let r = tokio::time::timeout(Duration::from_millis(50), wait_deadline(None)).await;
        assert!(r.is_err(), "deadline 为 None 时 wait_deadline 不应就绪");
    }

    // deadline 已过期时立即就绪，保证撞线后 select! 当轮即可选中该分支。
    #[tokio::test]
    async fn test_wait_deadline_past_instant_resolves_immediately() {
        let past = Instant::now() - Duration::from_secs(1);
        let r = tokio::time::timeout(Duration::from_millis(50), wait_deadline(Some(past))).await;
        assert!(r.is_ok(), "deadline 已过期时 wait_deadline 应立即就绪");
    }

    /// 非流式 content 组装：块顺序 thinking → text → tool_use；
    /// 只有 thinking 的退化响应补占位空格并回报 thinking_only。
    #[test]
    fn test_build_non_stream_content() {
        // thinking + text：thinking 块在前，可见文本原样保留
        let (content, thinking_only) = build_non_stream_content("推理过程", "最终回答", Vec::new());
        assert!(!thinking_only);
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["thinking"], "推理过程");
        assert!(content[0]["signature"].as_str().unwrap().len() >= 100);
        assert_eq!(content[1]["type"], "text");
        assert_eq!(content[1]["text"], "最终回答");

        // 只有 thinking：补一个占位空格 text 块，避免客户端判定空响应
        let (content, thinking_only) = build_non_stream_content("只有推理", "", Vec::new());
        assert!(thinking_only);
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[1]["text"], " ");

        // 无 thinking：只有一个 text 块
        let (content, thinking_only) = build_non_stream_content("", "普通回答", Vec::new());
        assert!(!thinking_only);
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");

        // thinking + tool_use（无可见文本）：不是退化响应，不补占位空格
        let tool = json!({"type": "tool_use", "id": "tu_1", "name": "Read", "input": {}});
        let (content, thinking_only) = build_non_stream_content("推理", "", vec![tool]);
        assert!(!thinking_only);
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[1]["type"], "tool_use");
    }

    #[test]
    fn test_resolve_thinking_enabled_luna_forced_off_even_when_requested() {
        // 收窄后的核心断言：只有 gpt-5.6-luna 即使客户端显式请求了 thinking，也必须
        // 强制返回 false —— 该模型上游恒返回 thinking=0（已知限制），若仍按请求启用，
        // 流式状态机会持续寻找永不出现的 `<thinking>` 标签，导致模型自造的伪标签
        // （<analysis>/<summary>）原样泄漏。
        let thinking = Some(Thinking {
            thinking_type: "adaptive".to_string(),
            budget_tokens: 20000,
        });
        assert!(!resolve_thinking_enabled("gpt-5.6-luna", &thinking));

        let thinking = Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 20000,
        });
        assert!(!resolve_thinking_enabled("gpt-5.6-luna", &thinking));
    }

    #[test]
    fn test_resolve_thinking_enabled_terra_and_sol_respect_client_request() {
        // 范围收窄：terra/sol 没有 luna 那样的 thinking=0 实测依据，不应被一并强制
        // 关闭，恢复原有行为——thinking_enabled 仅取决于客户端是否请求。
        let thinking = Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 20000,
        });
        assert!(resolve_thinking_enabled("gpt-5.6-terra", &thinking));
        assert!(resolve_thinking_enabled("gpt-5.6-sol", &thinking));

        assert!(!resolve_thinking_enabled("gpt-5.6-terra", &None));
        assert!(!resolve_thinking_enabled("gpt-5.6-sol", &None));
    }

    #[test]
    fn test_resolve_thinking_enabled_non_gpt_model_respects_request() {
        // 非 GPT 模型（Claude 系）走既有 Kiro thinking 协议，行为不变：
        // 请求了就启用，没请求就不启用。
        let thinking = Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 20000,
        });
        assert!(resolve_thinking_enabled("claude-sonnet-4", &thinking));

        assert!(!resolve_thinking_enabled("claude-sonnet-4", &None));
    }

    #[test]
    fn test_resolve_thinking_enabled_luna_without_thinking_request() {
        // luna 且客户端未请求 thinking：本就应为 false，确认无 panic 且结果正确。
        assert!(!resolve_thinking_enabled("gpt-5.6-luna", &None));
    }

    // ---- build_bridge_context 构造条件（任务 2 单测，D5/D7）----

    use crate::anthropic::converter as converter_mod;

    /// 构造只携带一条 user 消息的最小 MessagesRequest（tools 可选）
    fn bridge_test_request(
        tools: Option<Vec<super::super::super::types::Tool>>,
    ) -> super::super::super::types::MessagesRequest {
        super::super::super::types::MessagesRequest {
            model: "claude-sonnet-4".to_string(),
            max_tokens: 1024,
            messages: vec![super::super::super::types::Message {
                role: "user".to_string(),
                content: serde_json::json!("搜索一下今天的新闻"),
            }],
            stream: false,
            system: None,
            tools,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        }
    }

    fn ws_tool_for_bridge(
        tool_type: Option<&str>,
        name: &str,
        max_uses: Option<i32>,
    ) -> super::super::super::types::Tool {
        super::super::super::types::Tool {
            tool_type: tool_type.map(|s| s.to_string()),
            name: name.to_string(),
            description: String::new(),
            input_schema: Default::default(),
            max_uses,
            defer_loading: None,
        }
    }

    #[test]
    fn test_build_bridge_context_hit_constructs() {
        // 携带 web_search server tool → Some(BridgeContext)，字段取自 conversion_result
        let req = bridge_test_request(Some(vec![ws_tool_for_bridge(
            Some("web_search_20250305"),
            "web_search",
            Some(3),
        )]));
        let conversion = converter_mod::convert_request(&req).unwrap();

        let ctx = build_bridge_context(&conversion, Some("arn:test".to_string()), vec![1, 2]);
        let ctx = ctx.expect("命中 server tool 应构造 BridgeContext");
        assert_eq!(ctx.max_uses, Some(3));
        assert_eq!(ctx.profile_arn, Some("arn:test".to_string()));
        assert_eq!(ctx.bound_ids, vec![1, 2]);
        assert_eq!(ctx.is_compact_request, conversion.is_compact_request);
        // conversation_state 是首次转换结果的 clone（D3 演进基底）
        assert_eq!(
            serde_json::to_string(&ctx.conversation_state).unwrap(),
            serde_json::to_string(&conversion.conversation_state).unwrap()
        );
    }

    #[test]
    fn test_build_bridge_context_no_hit_returns_none() {
        // 未携带 web_search server tool → None（零行为变化路径）
        let req = bridge_test_request(Some(vec![ws_tool_for_bridge(None, "Read", None)]));
        let conversion = converter_mod::convert_request(&req).unwrap();

        assert!(build_bridge_context(&conversion, None, Vec::new()).is_none());
    }

    #[test]
    fn test_build_bridge_context_hit_without_max_uses_still_constructs() {
        // 命中但未声明 max_uses（内层 None）→ 仍构造，max_uses 为 None（上限由桥接层兜底 5）
        let req = bridge_test_request(Some(vec![ws_tool_for_bridge(
            Some("web_search_20250305"),
            "web_search",
            None,
        )]));
        let conversion = converter_mod::convert_request(&req).unwrap();

        let ctx = build_bridge_context(&conversion, None, Vec::new())
            .expect("命中但未声明 max_uses 仍应构造");
        assert_eq!(ctx.max_uses, None);
    }

    #[test]
    fn test_build_bridge_context_mixed_list_hit() {
        // 混合工具列表（普通工具 + web_search server tool）→ 命中构造；
        // 且构造条件与 stream 无关（D7：流式/非流式共用同一判定，构造函数不接收 stream 字段）
        let req = bridge_test_request(Some(vec![
            ws_tool_for_bridge(None, "Bash", None),
            ws_tool_for_bridge(Some("web_search_20250305"), "web_search", Some(5)),
        ]));
        let conversion = converter_mod::convert_request(&req).unwrap();

        let ctx = build_bridge_context(&conversion, None, Vec::new())
            .expect("混合列表命中 server tool 应构造");
        assert_eq!(ctx.max_uses, Some(5));

        // 对照：同一请求 stream=true 的转换结果构造条件一致
        let mut req_stream = req;
        req_stream.stream = true;
        let conversion_stream = converter_mod::convert_request(&req_stream).unwrap();
        assert!(build_bridge_context(&conversion_stream, None, Vec::new()).is_some());
    }

    // ---- 桥接状态机截获与聚合（任务 3 单测，D4/D8）----

    use crate::kiro::model::events::Event;
    use crate::kiro::model::events::ToolUseEvent;
    use crate::kiro::model::requests::conversation::Message;

    /// 构造最小 StreamContext（thinking 关闭；字段无外部依赖）
    fn bridge_stream_context() -> StreamContext {
        StreamContext::new_with_thinking("claude-sonnet-4", 1000, false)
    }

    /// 构造 Kiro ToolUse 事件
    fn tool_use_event(name: &str, id: &str, input: &str, stop: bool) -> Event {
        Event::ToolUse(ToolUseEvent {
            name: name.to_string(),
            tool_use_id: id.to_string(),
            input: input.to_string(),
            stop,
        })
    }

    #[test]
    fn test_bridge_input_fragments_aggregated() {
        // 分片到达（stop=false）→ Collecting 聚合，不透传也不发块；
        // stop=true → 截获完成，发 server_tool_use + web_search_tool_result 可见性块
        let mut ctx = bridge_stream_context();
        let mut bridge = Some(BridgeState::new(Some(3)));

        // 分片 1：不透传、无可见性块
        let (consumed, events) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"que"#, false),
        );
        assert!(consumed, "web_search toolUse 分片应被截获");
        assert!(events.is_empty(), "聚合期间不应发任何 SSE 块");
        assert!(matches!(
            bridge.as_ref().unwrap().phase,
            BridgePhase::Collecting { .. }
        ));

        // 分片 2 + stop：截获完成，发出可见性块
        let (consumed, events) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"ry":"rust programming"}"#, true),
        );
        assert!(consumed, "stop 分片仍属于同一 toolUse，应被截获");
        // 截获完成只发 server_tool_use(start/delta/stop) 共 3 个事件；
        // web_search_tool_result 结果块由续流阶段（unfold None 分支）携带真实 MCP 结果发出
        assert_eq!(
            events.len(),
            3,
            "应发出 server_tool_use(start/delta/stop) 共 3 个事件"
        );
        let types: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();
        assert!(
            types.contains(&"content_block_start"),
            "应含 content_block_start"
        );
        assert!(
            types.contains(&"content_block_stop"),
            "应含 content_block_stop"
        );
        // 回到 PassThrough 且轮次计数 +1
        assert!(matches!(
            bridge.as_ref().unwrap().phase,
            BridgePhase::PassThrough
        ));
        assert_eq!(bridge.as_ref().unwrap().rounds_used, 1);
    }

    #[test]
    fn test_bridge_non_target_tool_passthrough() {
        // 非目标工具（Read）与 Collecting 期间的 AssistantResponse 均正常透传（D8）
        let mut ctx = bridge_stream_context();
        let mut bridge = Some(BridgeState::new(Some(3)));

        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("Read", "tu0", r#"{"file_path":"a.rs"}"#, true),
        );
        assert!(!consumed, "非 web_search 工具应走现有透传路径");
        // 透传 SSE 由 unfold 调用方调用 process_kiro_event 产生（bridge_handle_event 返回空 Vec）
        let sse = ctx.process_kiro_event(&tool_use_event(
            "Read",
            "tu0",
            r#"{"file_path":"a.rs"}"#,
            true,
        ));
        assert!(
            !sse.is_empty(),
            "透传时 process_kiro_event 应产生 tool_use SSE"
        );
        assert!(matches!(
            bridge.as_ref().unwrap().phase,
            BridgePhase::PassThrough
        ));
        // 透传路径应分配 tool_use 块
        assert!(
            !ctx.tool_block_indices.is_empty(),
            "透传应分配 tool_use 块索引"
        );

        // 进入 Collecting 后，AssistantResponse 说明文字仍透传
        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"q"#, false),
        );
        assert!(consumed);
        // extra 字段私有，走 serde 反序列化构造
        let resp: crate::kiro::model::events::AssistantResponseEvent =
            serde_json::from_str(r#"{"content":"让我搜索一下"}"#).unwrap();
        let resp_event = Event::AssistantResponse(resp);
        let (consumed, _) = bridge_handle_event(&mut ctx, &mut bridge, &resp_event);
        assert!(!consumed, "Collecting 期间 AssistantResponse 应透传");
        let sse = ctx.process_kiro_event(&resp_event);
        assert!(!sse.is_empty(), "AssistantResponse 透传应产生 text SSE");
    }

    #[test]
    fn test_bridge_no_tool_use_sse_leak() {
        // 截获的 web_search 不产生普通 tool_use 块：state_manager 未分配 tool_use 块，
        // 客户端可见的是 server_tool_use 块（索引由 next_block_index 单调分配）
        let mut ctx = bridge_stream_context();
        let mut bridge = Some(BridgeState::new(None));

        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"query":"rust"}"#, true),
        );
        assert!(consumed);
        assert!(
            ctx.tool_block_indices.is_empty(),
            "截获路径不得分配普通 tool_use 块索引"
        );
        // 截获完成时 server_tool_use 块消耗了 1 个块索引；
        // web_search_tool_result 结果块由续流阶段发出（届时再消耗 1 个）
        let next = ctx.state_manager.next_block_index();
        assert!(
            next >= 1,
            "server_tool_use 块应已占用至少 1 个块索引，实际 next={next}"
        );
        // max_uses 未声明（内层 None）时兜底上限 5
        assert_eq!(bridge.as_ref().unwrap().max_rounds, 5);
    }

    #[test]
    fn test_bridge_rounds_exhausted_passthrough() {
        // 轮次耗尽后 web_search 不再截获，按普通 tool_use 透传（D8 上限语义）
        let mut ctx = bridge_stream_context();
        // 上限 0（max_uses=0 时 clamp 到 0）
        let mut bridge = Some(BridgeState::new(Some(0)));

        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"query":"x"}"#, true),
        );
        assert!(!consumed, "轮次耗尽后应透传为普通 tool_use");
        // 透传 SSE 由 unfold 调用方调用 process_kiro_event 产生
        ctx.process_kiro_event(&tool_use_event(
            "web_search",
            "tu1",
            r#"{"query":"x"}"#,
            true,
        ));
        assert!(
            !ctx.tool_block_indices.is_empty(),
            "透传路径应分配普通 tool_use 块"
        );
    }

}
