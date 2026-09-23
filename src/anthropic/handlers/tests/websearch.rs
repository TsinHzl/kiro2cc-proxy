// Copyright (c) 2026 Harllan He. Licensed under MIT.
// Web 搜索结果与轮次收割测试（自 handlers/tests.rs 拆出，纯代码搬移）
#[cfg(test)]
mod tests {

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
    use super::super::bridge::tests::{
        bridge_ctx_for_continuation, bridge_stream_context, bridge_test_request,
        sample_search_results, tool_use_event, ws_tool_for_bridge,
    };
    use crate::anthropic::middleware::AppState;
    use crate::anthropic::stream::{CLIENT_ASSUMED_CONTEXT_WINDOW, scale_for_client};
    use crate::anthropic::stream::{SseEvent, StreamContext};
    use crate::anthropic::types::{Model, Thinking};
    use crate::kiro::model::requests::conversation::ConversationState;
    use crate::kiro::parser::decoder::EventStreamDecoder;
    use axum::response::Response;
    use axum::{extract::State, http::StatusCode};
    use serde_json::json;
    use std::collections::VecDeque;
    use std::time::Duration;
    use tokio::time::Instant;

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
