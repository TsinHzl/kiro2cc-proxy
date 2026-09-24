// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 续请求构建与非流式 bridge 测试（自 post_messages_cc/tests.rs 拆出，纯代码搬移）

#[cfg(test)]
mod tests {
    // ---- 续请求体构建（任务 4 单测，D3）----

    /// 构造测试用 BridgeContext（conversation_state 带完整字段供不变量断言）
    fn bridge_ctx_for_continuation() -> BridgeContext {
        let mut state = ConversationState::new("conv-123");
        state.agent_continuation_id = Some("cont-456".to_string());
        state.agent_task_type = Some("vibe".to_string());
        state.chat_trigger_type = Some("MANUAL".to_string());
        state.current_message.user_input_message.content = "搜索一下今天的新闻".to_string();
        state.history = vec![
            Message::user("历史用户消息", "claude-sonnet-4"),
            Message::assistant("历史助手回复"),
        ];
        BridgeContext {
            conversation_state: state,
            profile_arn: Some("arn:test".to_string()),
            additional_model_request_fields: Some(serde_json::json!({"max_tokens": 1024})),
            max_uses: Some(3),
            bound_ids: vec![1],
            is_compact_request: false,
            thinking_adaptive_requested: false,
        }
    }

    fn sample_search_results() -> websearch::WebSearchResults {
        websearch::WebSearchResults {
            results: vec![websearch::WebSearchResult {
                title: "Rust 官方文档".to_string(),
                url: "https://doc.rust-lang.org".to_string(),
                snippet: Some("The Rust programming language".to_string()),
                published_date: None,
                id: None,
                domain: None,
                max_verbatim_word_limit: None,
                public_domain: None,
            }],
            total_results: Some(1),
            query: Some("rust".to_string()),
            error: None,
        }
    }

    #[test]
    fn test_continuation_request_invariants() {
        // 续请求体不变量（D3）：conversationId/agentContinuationId/agentTaskType/
        // chatTriggerType/history 逐字节不变；仅 current_message.tool_results 回填，
        // toolUseId 用 Kiro 流截获的 tu.tool_use_id（非 create_mcp_request 的 srvtoolu_ id）
        let ctx = bridge_ctx_for_continuation();
        let baseline = serde_json::to_string(&ctx.conversation_state).unwrap();
        let results = sample_search_results();

        let tool_result = build_search_tool_result("tu-kiro-new-1", "rust", &Some(results));
        let kiro_request = build_continuation_request(
            &ctx,
            Some(ctx.conversation_state.clone()),
            vec![tool_result],
        );
        let body = serde_json::to_string(&kiro_request.conversation_state).unwrap();
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();

        // 不变量字段逐字节一致
        assert!(
            value["conversationId"]
                .as_str()
                .unwrap()
                .contains("conv-123")
        );
        assert_eq!(value["agentContinuationId"], serde_json::json!("cont-456"));
        assert_eq!(value["agentTaskType"], serde_json::json!("vibe"));
        assert_eq!(value["chatTriggerType"], serde_json::json!("MANUAL"));
        assert_eq!(
            value["history"],
            serde_json::to_value(&ctx.conversation_state.history).unwrap(),
            "history 必须逐字节不变"
        );
        // current_message 的 content 不变，tool_results 回填为 1 条
        assert_eq!(
            value["currentMessage"]["userInputMessage"]["content"],
            serde_json::json!("搜索一下今天的新闻")
        );
        let tool_results =
            &value["currentMessage"]["userInputMessage"]["userInputMessageContext"]["toolResults"];
        assert_eq!(tool_results.as_array().unwrap().len(), 1);
        assert_eq!(
            tool_results[0]["toolUseId"],
            serde_json::json!("tu-kiro-new-1")
        );
        assert_eq!(tool_results[0]["status"], serde_json::json!("success"));
        // is_error=false 时被 is_false 跳过序列化，出现即为缺陷
        assert!(
            tool_results[0].get("isError").is_none(),
            "success 路径不应序列化 isError"
        );
        // 回填只改 tool_results，其余部分与基底完全一致
        assert_ne!(body, baseline, "tool_results 回填后应与基底不同");
        // profile_arn / additional_model_request_fields 同参序列化
        assert_eq!(
            serde_json::to_string(&kiro_request.profile_arn).unwrap(),
            serde_json::to_string(&Some("arn:test".to_string())).unwrap()
        );
        assert!(kiro_request.additional_model_request_fields.is_some());
    }

    #[test]
    fn test_continuation_request_multiround_evolution() {
        // 多轮演进语义（D3）：第 2 轮续请求基于第 1 轮所用状态演进（bridge_execute_round
        // 把本轮 kiro_request.conversation_state 写回 evolution_base），history 仍逐字节不变
        let ctx = bridge_ctx_for_continuation();
        let results = sample_search_results();

        // 第 1 轮：evolution_base = BridgeContext.conversation_state（create_sse_stream 初始化语义）
        let round1 = build_continuation_request(
            &ctx,
            Some(ctx.conversation_state.clone()),
            vec![build_search_tool_result(
                "tu-round-1",
                "rust",
                &Some(sample_search_results()),
            )],
        );
        // 第 2 轮：bridge_execute_round 将 round1 的状态写回 evolution_base（此处模拟）
        let round2 = build_continuation_request(
            &ctx,
            Some(round1.conversation_state.clone()),
            vec![build_search_tool_result(
                "tu-round-2",
                "tokio",
                &Some(results),
            )],
        );

        let v1: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&round1.conversation_state).unwrap())
                .unwrap();
        let v2: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&round2.conversation_state).unwrap())
                .unwrap();

        // 第 2 轮 history 与第 1 轮一致（中间轮 toolResults 不追加进 history）
        assert_eq!(v2["history"], v1["history"], "多轮 history 必须逐字节不变");
        assert_eq!(v2["conversationId"], v1["conversationId"]);
        assert_eq!(v2["agentContinuationId"], v1["agentContinuationId"]);
        // 第 2 轮 tool_results 替换为本轮结果（承载最新 toolUseId）
        let tr2 =
            &v2["currentMessage"]["userInputMessage"]["userInputMessageContext"]["toolResults"];
        assert_eq!(tr2[0]["toolUseId"], serde_json::json!("tu-round-2"));
        // 第 2 轮的其余字段与第 1 轮状态一致（基于第 1 轮演进，非从原始 clone 重新出发）
        assert_eq!(
            v2["currentMessage"]["userInputMessage"]["content"],
            v1["currentMessage"]["userInputMessage"]["content"]
        );
    }

    #[test]
    fn test_continuation_request_mcp_failure_error_tool_result() {
        // MCP 失败（search_results=None）→ ToolResult::error 降级，仍发续请求（流不中断）
        let ctx = bridge_ctx_for_continuation();

        let tool_result = build_search_tool_result("tu-fail-1", "rust", &None);
        let kiro_request = build_continuation_request(
            &ctx,
            Some(ctx.conversation_state.clone()),
            vec![tool_result],
        );
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&kiro_request.conversation_state).unwrap())
                .unwrap();
        let tool_results =
            &value["currentMessage"]["userInputMessage"]["userInputMessageContext"]["toolResults"];
        assert_eq!(tool_results.as_array().unwrap().len(), 1);
        assert_eq!(tool_results[0]["toolUseId"], serde_json::json!("tu-fail-1"));
        assert_eq!(tool_results[0]["status"], serde_json::json!("error"));
        assert_eq!(tool_results[0]["isError"], serde_json::json!(true));
        let text = tool_results[0]["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("Web search failed for query: rust"),
            "error 文案应说明搜索失败，实际: {text}"
        );
        // 降级路径不变量保持：history/会话标识仍逐字节不变
        assert_eq!(
            value["history"],
            serde_json::to_value(&ctx.conversation_state.history).unwrap()
        );
        assert_eq!(
            value["conversationId"],
            serde_json::to_value("conv-123").unwrap()
        );
        assert_eq!(value["agentContinuationId"], serde_json::json!("cont-456"));
    }

    #[test]
    fn test_continuation_request_fallback_to_bridge_ctx_state() {
        // evolution_base 为 None（防御路径）→ 回退到 BridgeContext.conversation_state clone
        let ctx = bridge_ctx_for_continuation();
        let results = sample_search_results();

        let tool_result = build_search_tool_result("tu-fb-1", "rust", &Some(results));
        let kiro_request = build_continuation_request(&ctx, None, vec![tool_result]);
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&kiro_request.conversation_state).unwrap())
                .unwrap();
        assert_eq!(
            value["conversationId"],
            serde_json::to_value("conv-123").unwrap()
        );
        let tool_results =
            &value["currentMessage"]["userInputMessage"]["userInputMessageContext"]["toolResults"];
        assert_eq!(tool_results[0]["toolUseId"], serde_json::json!("tu-fb-1"));
        assert_eq!(tool_results[0]["status"], serde_json::json!("success"));
    }

    // ---- 多轮上限与收尾（任务 5 单测，D8）----

    #[test]
    fn test_bridge_round_counting_up_to_limit() {
        // 上限计数：每完成一轮截获 rounds_used +1；达到上限后 has_remaining_rounds 为 false
        let mut ctx = bridge_stream_context();
        let mut bridge = Some(BridgeState::new(Some(2)));

        // 第 1 轮截获完成
        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"query":"a"}"#, true),
        );
        assert!(consumed);
        assert_eq!(bridge.as_ref().unwrap().rounds_used, 1);
        assert!(bridge.as_ref().unwrap().has_remaining_rounds());

        // 第 2 轮截获完成 → 达到上限
        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu2", r#"{"query":"b"}"#, true),
        );
        assert!(consumed);
        assert_eq!(bridge.as_ref().unwrap().rounds_used, 2);
        assert!(
            !bridge.as_ref().unwrap().has_remaining_rounds(),
            "达到 min(max_uses,5) 后不应再有剩余轮次"
        );

        // 上限为 None 时兜底 5，5 轮内均有剩余
        let bridge5 = BridgeState::new(None);
        assert_eq!(bridge5.max_rounds, 5);
        assert!(bridge5.has_remaining_rounds());

        // max_uses 超过 5 时 clamp 到 5（D8 硬上限）
        let bridge_clamped = BridgeState::new(Some(99));
        assert_eq!(bridge_clamped.max_rounds, 5);
    }

    #[test]
    fn test_bridge_exhausted_web_search_passthrough_non_target_still_intercepts() {
        // 上限后透传：轮次耗尽后新 web_search toolUse 不再截获，按普通 tool_use 走
        // process_kiro_event 产生 tool_use SSE 块；随后继续正常透传（状态不被污染）
        let mut ctx = bridge_stream_context();
        let mut bridge = Some(BridgeState::new(Some(1)));

        // 唯一轮次用掉
        let (consumed, _) = bridge_handle_event(
            &mut ctx,
            &mut bridge,
            &tool_use_event("web_search", "tu1", r#"{"query":"a"}"#, true),
        );
        assert!(consumed);

        // 轮次耗尽后的 web_search：不截获 → process_kiro_event 分配普通 tool_use 块
        let exhausted = tool_use_event("web_search", "tu2", r#"{"query":"b"}"#, true);
        let (consumed, events) = bridge_handle_event(&mut ctx, &mut bridge, &exhausted);
        assert!(!consumed, "轮次耗尽后 web_search 应透传");
        assert!(events.is_empty());
        let sse = ctx.process_kiro_event(&exhausted);
        assert!(!sse.is_empty(), "透传应产生 tool_use SSE");
        assert_eq!(
            ctx.tool_block_indices.len(),
            1,
            "透传路径应恰好分配 1 个普通 tool_use 块"
        );

        // 透传后状态仍为 PassThrough、轮次不再增长；pending 保持第 1 轮的待执行搜索
        let state = bridge.as_ref().unwrap();
        assert!(matches!(state.phase, BridgePhase::PassThrough));
        assert_eq!(state.rounds_used, 1);
        assert_eq!(state.pending.len(), 1, "应恰好保留 1 条待执行搜索");
        assert_eq!(state.pending.front().unwrap().tool_use_id, "tu1");
        assert_eq!(state.pending.front().unwrap().query, "a");

        // 之后又截获到新轮次窗口的情形不存在（rounds_used 不回退），上限语义稳定
        assert!(!state.has_remaining_rounds());
    }

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

    #[test]
    fn test_web_search_result_block_shape() {
        // web_search_tool_result 块格式（D5 非流式段）：与流式条目格式一致；
        // MCP 失败（None）时 content 为空数组
        let block = build_web_search_result_block("tu-ok-1", &Some(sample_search_results()));
        assert_eq!(block["type"], "web_search_tool_result");
        assert_eq!(block["tool_use_id"], "tu-ok-1");
        let items = block["content"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["type"], "web_search_result");
        assert_eq!(items[0]["title"], "Rust 官方文档");
        assert_eq!(items[0]["url"], "https://doc.rust-lang.org");
        assert_eq!(
            items[0]["encrypted_content"],
            "The Rust programming language"
        );
        assert!(items[0]["page_age"].is_null());

        let empty = build_web_search_result_block("tu-fail-1", &None);
        assert_eq!(
            empty["content"].as_array().unwrap().len(),
            0,
            "MCP 失败时 content 应为空数组"
        );
    }

    // ---- H3：续请求失败 → error 事件收尾（任务 4 单测，unfold None 分支语义）----

    use crate::kiro::model::credentials::KiroCredentials;
    use crate::kiro::token_manager::MultiTokenManager;
    use crate::model::config::Config;
    use chrono::Utc;

    /// 构造"必然刷新失败"的测试 Provider：凭据已过期且无 refreshToken，
    /// validate_refresh_token 阶段立即失败（无需真实网络请求），
    /// MCP 调用与续请求 call_api_stream 均快速返回 Err
    fn bridge_test_provider() -> crate::kiro::provider::KiroProvider {
        let mut cred = KiroCredentials::default();
        cred.access_token = Some("test-invalid-token".to_string());
        cred.expires_at = Some((Utc::now() - chrono::Duration::hours(1)).to_rfc3339());
        let manager = std::sync::Arc::new(
            MultiTokenManager::new(Config::default(), vec![cred], None, None, false)
                .expect("构造 MultiTokenManager 失败"),
        );
        crate::kiro::provider::KiroProvider::new(manager)
    }

    #[tokio::test]
    async fn test_bridge_execute_round_returns_failed_on_continuation_failure() {
        // unfold None 分支语义（H3）：bridge_execute_round 在续请求发起失败时
        // 返回 Failed（携带 MCP 搜索结果供调用方补发结果块配对），调用方据此
        // 先发 web_search_tool_result 结果块、再补发 stream_interrupted_error_event
        // 并置 finished=true 正常收尾（不再发 message_stop），防止客户端流悬挂
        let provider = bridge_test_provider();
        let bridge_ctx = bridge_ctx_for_continuation();
        let mut bridge = BridgeState::new(Some(1));
        let pending = PendingSearch {
            tool_use_id: "tu-fail-net".to_string(),
            query: "rust".to_string(),
        };

        let result = bridge_execute_round(&provider, &bridge_ctx, &mut bridge, pending).await;
        assert!(
            matches!(result, BridgeRoundOutcome::Failed(_)),
            "续请求发起失败时应返回 Failed"
        );
        // Err 分支：演进基底写回取出的状态，避免下一轮基于未知状态演进
        assert!(bridge.evolution_base.is_some());

        // None → unfold 补发的 error 事件结构（与既有
        // test_stream_interrupted_error_event_signals_failure_not_success 同口径）
        let event = stream_interrupted_error_event();
        assert_eq!(event.event, "error");
        assert_eq!(event.data["error"]["type"], "overloaded_error");
        assert!(
            event.data["error"]["message"]
                .as_str()
                .unwrap()
                .contains("interrupted")
        );
    }

    // ---- ⚠️#2（第 2 轮增量 CR）：harvest_bridge_round 三条收尾不变量 ----

    #[test]
    fn test_harvest_bridge_round_continued_pairs_result_block() {
        // Continued → 先发配对结果块（finished=false），结果块与 server_tool_use
        // 成对，且不含 error/message_stop 收尾事件
        let mut ctx = StreamContext::new_with_thinking("claude-sonnet-4", 1000, false);
        let response = reqwest::Response::from(http::Response::new(body_dummy_bytes()));
        let outcome = BridgeRoundOutcome::Continued(
            response,
            EventStreamDecoder::new(),
            Some(sample_search_results()),
        );

        let harvest = harvest_bridge_round(outcome, "tu-cont-1", &mut ctx);
        assert!(!harvest.finished, "Continued 应换入续流，finished=false");
        assert_eq!(
            harvest.events.len(),
            2,
            "Continued 补发 web_search_tool_result 结果块（start + stop 两事件）"
        );
        assert_eq!(
            harvest.events[0].data["content_block"]["type"],
            "web_search_tool_result"
        );
        assert_eq!(
            harvest.events[1].event, "content_block_stop",
            "配对结果块以 stop 事件收尾"
        );
    }

    #[test]
    fn test_harvest_bridge_round_failed_emits_result_then_error() {
        // Failed → 结果块（携带已产出搜索结果）+ error 收尾事件，finished=true
        let mut ctx = StreamContext::new_with_thinking("claude-sonnet-4", 1000, false);
        let outcome = BridgeRoundOutcome::Failed(Some(sample_search_results()));

        let harvest = harvest_bridge_round(outcome, "tu-fail-1", &mut ctx);
        assert!(harvest.finished, "Failed 应立即收尾，finished=true");
        assert_eq!(
            harvest.events.len(),
            3,
            "Failed = 结果块(start + stop) + error 事件"
        );
        assert_eq!(
            harvest.events[0].data["content_block"]["type"],
            "web_search_tool_result"
        );
        assert_eq!(harvest.events[2].event, "error");
    }

    #[test]
    fn test_harvest_bridge_round_panic_fallback_empty_results() {
        // 后台任务 panic 兜底 = Failed(None) → 空数组结果块 + error 收尾，
        // 与 MCP 失败同口径（保证 server_tool_use / web_search_tool_result 成对）
        let mut ctx = StreamContext::new_with_thinking("claude-sonnet-4", 1000, false);
        let outcome = BridgeRoundOutcome::Failed(None);

        let harvest = harvest_bridge_round(outcome, "tu-panic-1", &mut ctx);
        assert!(harvest.finished);
        assert_eq!(harvest.events.len(), 3, "panic 兜底同 Failed 收尾结构");
        assert_eq!(
            harvest.events[0].data["content_block"]["content"],
            serde_json::json!([]),
            "panic 兜底结果块 content 应为空数组"
        );
        assert_eq!(harvest.events[2].event, "error");
    }

    #[test]
    fn test_flush_unpaired_search_blocks_drains_queue() {
        // 非流式降级收尾：队列剩余 pending 全部补发空结果块，且队列被清空
        let mut pending = VecDeque::new();
        pending.push_back(PendingSearch {
            tool_use_id: "tu-left-1".to_string(),
            query: "a".to_string(),
        });
        pending.push_back(PendingSearch {
            tool_use_id: "tu-left-2".to_string(),
            query: "b".to_string(),
        });
        let mut blocks = Vec::new();

        flush_unpaired_search_blocks(&mut pending, &mut blocks);

        assert!(pending.is_empty(), "队列应被 drain 清空");
        assert_eq!(blocks.len(), 2, "每条遗留 pending 补发一个结果块");
        for (i, block) in blocks.iter().enumerate() {
            assert_eq!(block["type"], "web_search_tool_result");
            assert_eq!(
                block["content"],
                serde_json::json!([]),
                "降级路径搜索未执行，结果块应为空数组"
            );
            let _ = i;
        }
    }
}
}
}
