// Copyright (c) 2026 Harllan He. Licensed under MIT.
// Bridge 网桥流程测试（自 handlers/tests.rs 拆出，纯代码搬移）
#[cfg(test)]
pub(crate) mod tests {

    use super::super::super::bridge::{
        BridgeContext, BridgePhase, BridgeRoundOutcome, BridgeState, PendingSearch,
        body_dummy_bytes, bridge_execute_round, bridge_handle_event, build_bridge_context,
        build_continuation_request, build_search_tool_result, build_web_search_result_block,
        flush_unpaired_search_blocks, harvest_bridge_round,
    };
    use super::super::super::error::{format_prompt_too_long, map_provider_error_with_context};
    use super::super::super::helpers::resolve_thinking_enabled;
    use super::super::super::models::{
        ModelCache, available_model_to_model, build_model_list, cached_if_fresh,
        fetch_models_dynamic, get_model, guess_owned_by, resolve_after_refresh,
    };
    use super::super::super::nonstream::{build_non_stream_content, non_stream_bridge_step};
    use super::super::super::stream::{stream_interrupted_error_event, wait_deadline};
    use crate::anthropic::middleware::AppState;
    use crate::anthropic::stream::{CLIENT_ASSUMED_CONTEXT_WINDOW, scale_for_client};
    use crate::anthropic::stream::{SseEvent, StreamContext};
    use crate::anthropic::types::{Model, Thinking};
    use crate::kiro::model::events::Event;
    use crate::kiro::model::events::ToolUseEvent;
    use crate::kiro::model::requests::conversation::ConversationState;
    use crate::kiro::model::requests::conversation::Message;
    use crate::kiro::parser::decoder::EventStreamDecoder;
    use axum::response::Response;
    use axum::{extract::State, http::StatusCode};
    use serde_json::json;
    use std::collections::VecDeque;
    use std::time::Duration;
    use tokio::time::Instant;

    use crate::anthropic::converter as converter_mod;

    /// 构造只携带一条 user 消息的最小 MessagesRequest（tools 可选）
    pub(crate) fn bridge_test_request(
        tools: Option<Vec<crate::anthropic::types::Tool>>,
    ) -> crate::anthropic::types::MessagesRequest {
        crate::anthropic::types::MessagesRequest {
            model: "claude-sonnet-4".to_string(),
            max_tokens: 1024,
            messages: vec![crate::anthropic::types::Message {
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

    pub(crate) fn ws_tool_for_bridge(
        tool_type: Option<&str>,
        name: &str,
        max_uses: Option<i32>,
    ) -> crate::anthropic::types::Tool {
        crate::anthropic::types::Tool {
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

    /// 构造最小 StreamContext（thinking 关闭；字段无外部依赖）
    pub(crate) fn bridge_stream_context() -> StreamContext {
        StreamContext::new_with_thinking("claude-sonnet-4", 1000, false)
    }

    /// 构造 Kiro ToolUse 事件
    pub(crate) fn tool_use_event(name: &str, id: &str, input: &str, stop: bool) -> Event {
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

    // ---- 续请求体构建（任务 4 单测，D3）----

    /// 构造测试用 BridgeContext（conversation_state 带完整字段供不变量断言）
    pub(crate) fn bridge_ctx_for_continuation() -> BridgeContext {
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

    pub(crate) fn sample_search_results() -> crate::anthropic::websearch::WebSearchResults {
        crate::anthropic::websearch::WebSearchResults {
            results: vec![crate::anthropic::websearch::WebSearchResult {
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
}
