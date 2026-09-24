// Copyright (c) 2026 Harllan He. Licensed under MIT.
// /cc/v1/messages 端点测试（自 post_messages_cc.rs 拆出，纯代码搬移）

#[cfg(test)]
mod tests {
    use super::super::bridge::{
        BridgeContext, BridgePhase, BridgeRoundOutcome, BridgeState, PendingSearch,
        body_dummy_bytes, bridge_execute_round, bridge_handle_event, build_bridge_context,
        build_continuation_request, build_search_tool_result, build_web_search_result_block,
        flush_unpaired_search_blocks, harvest_bridge_round,
    };
    use super::super::error::{format_prompt_too_long, map_provider_error_with_context};
    use super::super::models::{
        ModelCache, available_model_to_model, build_model_list, cached_if_fresh,
        fetch_models_dynamic, get_model, guess_owned_by, resolve_after_refresh,
    };
    use super::super::nonstream::{build_non_stream_content, non_stream_bridge_step};
    use super::super::stream::{
        create_ping_sse, deadline_error_event, stream_interrupted_error_event, wait_deadline,
    };
    use super::*;
    use crate::anthropic::middleware::AppState;
    use crate::anthropic::stream::CLIENT_ASSUMED_CONTEXT_WINDOW;
    use crate::anthropic::stream::scale_for_client;
    use crate::anthropic::stream::{SseEvent, StreamContext};
    use crate::anthropic::types::{Model, Thinking};
    use crate::kiro::model::requests::conversation::ConversationState;
    use crate::kiro::parser::decoder::EventStreamDecoder;
    use axum::http::StatusCode;
    use axum::response::Response;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::time::Duration;
    use tokio::time::Instant;

    fn find_by_id(id: &str) -> Option<Model> {
        build_model_list().into_iter().find(|m| m.id == id)
    }

    #[test]
    fn test_guess_owned_by_known_and_unknown_prefixes() {
        assert_eq!(guess_owned_by("claude-sonnet-4.6"), "anthropic");
        assert_eq!(guess_owned_by("gpt-5.6-sol"), "openai");
        assert_eq!(guess_owned_by("auto"), "kiro");
        assert_eq!(guess_owned_by("deepseek-3.2"), "deepseek");
        assert_eq!(guess_owned_by("minimax-m2.5"), "minimax");
        assert_eq!(guess_owned_by("glm-5"), "glm");
        assert_eq!(guess_owned_by("qwen3-coder-next"), "qwen");
        assert_eq!(guess_owned_by("foo-model"), "unknown");
    }

    fn fake_model(id: &str) -> Model {
        Model {
            id: id.to_string(),
            object: "model".to_string(),
            created: 0,
            owned_by: "test".to_string(),
            display_name: id.to_string(),
            model_type: "chat".to_string(),
            max_tokens: 8192,
        }
    }

    fn new_cache(entry: Option<super::super::super::middleware::CachedModels>) -> ModelCache {
        std::sync::Arc::new(parking_lot::RwLock::new(entry))
    }

    // 分支 1：缓存命中且未过期 → 返回缓存，不触发刷新
    #[test]
    fn test_cached_if_fresh_hit() {
        let cache = new_cache(Some(super::super::super::middleware::CachedModels {
            models: vec![fake_model("cached-a")],
            fetched_at: std::time::Instant::now(),
        }));
        let hit = cached_if_fresh(&cache, Duration::from_secs(3600));
        assert!(hit.is_some());
        assert_eq!(hit.unwrap()[0].id, "cached-a");
    }

    // 分支 1 反例：缓存过期 → 视为未命中
    #[test]
    fn test_cached_if_fresh_expired() {
        let cache = new_cache(Some(super::super::super::middleware::CachedModels {
            models: vec![fake_model("stale")],
            fetched_at: std::time::Instant::now() - Duration::from_secs(10),
        }));
        // TTL 5s，已过 10s
        assert!(cached_if_fresh(&cache, Duration::from_secs(5)).is_none());
        // 空缓存亦未命中
        assert!(cached_if_fresh(&new_cache(None), Duration::from_secs(3600)).is_none());
    }

    // 分支 2：刷新成功 → 写缓存并返回
    #[test]
    fn test_resolve_after_refresh_success_writes_cache() {
        let cache = new_cache(None);
        let out = resolve_after_refresh(&cache, Some(vec![fake_model("fresh")]));
        assert_eq!(out[0].id, "fresh");
        // 缓存已写入
        let guard = cache.read();
        assert_eq!(guard.as_ref().unwrap().models[0].id, "fresh");
    }

    // 分支 3：刷新失败但有旧缓存 → 续用旧缓存
    #[test]
    fn test_resolve_after_refresh_failure_uses_old_cache() {
        let cache = new_cache(Some(super::super::super::middleware::CachedModels {
            models: vec![fake_model("old")],
            fetched_at: std::time::Instant::now(),
        }));
        let out = resolve_after_refresh(&cache, None);
        assert_eq!(out[0].id, "old");
    }

    // 分支 4：刷新失败且无缓存 → 回退静态表
    #[test]
    fn test_resolve_after_refresh_failure_no_cache_falls_back_static() {
        let cache = new_cache(None);
        let out = resolve_after_refresh(&cache, None);
        assert_eq!(out.len(), build_model_list().len());
        assert!(out.iter().any(|m| m.id == "claude-3-5-sonnet-20241022"));
    }

    // 分支 5：无 provider → fetch_models_dynamic 直接回退静态表
    #[tokio::test]
    async fn test_fetch_models_dynamic_no_provider_static() {
        let state = AppState::new();
        assert!(state.kiro_provider.is_none());
        let out = fetch_models_dynamic(&state).await;
        assert_eq!(out.len(), build_model_list().len());
    }

    fn test_state() -> AppState {
        AppState::new()
    }

    // get_model 命中（无 provider → 静态表来源）
    #[tokio::test]
    async fn test_get_model_hit() {
        let resp = get_model(
            State(test_state()),
            axum::extract::Path("claude-3-5-sonnet-20241022".to_string()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // get_model 未命中 → 404
    #[tokio::test]
    async fn test_get_model_not_found() {
        let resp = get_model(
            State(test_state()),
            axum::extract::Path("no-such-model-xyz".to_string()),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_available_model_to_model_maps_fields() {
        use crate::kiro::model::available_models::{AvailableModelInfo, TokenLimits};
        let info = AvailableModelInfo {
            model_id: "claude-sonnet-4.6".to_string(),
            model_name: "Claude Sonnet 4.6".to_string(),
            rate_multiplier: Some(1.3),
            token_limits: TokenLimits {
                max_input_tokens: 1_000_000,
                max_output_tokens: 64_000,
            },
            additional_model_request_fields_schema: None,
        };
        let m = available_model_to_model(&info);
        assert_eq!(m.id, "claude-sonnet-4.6");
        assert_eq!(m.display_name, "Claude Sonnet 4.6");
        assert_eq!(m.owned_by, "anthropic");
        assert_eq!(m.max_tokens, 64_000);
        assert_eq!(m.object, "model");
        assert_eq!(m.model_type, "chat");
    }

    #[test]
    fn test_opus_4_6_max_tokens_is_128k() {
        let m = find_by_id("claude-opus-4-6").expect("claude-opus-4-6 缺失");
        assert_eq!(m.max_tokens, 128000);
        let mt = find_by_id("claude-opus-4-6-thinking").expect("claude-opus-4-6-thinking 缺失");
        assert_eq!(mt.max_tokens, 128000);
    }

    #[test]
    fn test_fable_5_present() {
        let m = find_by_id("claude-fable-5").expect("claude-fable-5 应存在");
        assert_eq!(m.max_tokens, 128000);
        assert_eq!(m.owned_by, "anthropic");
        assert_eq!(m.object, "model");
        assert_eq!(m.model_type, "chat");
        assert_eq!(m.display_name, "Claude Fable 5");
    }

    #[test]
    fn test_fable_5_thinking_present() {
        let m = find_by_id("claude-fable-5-thinking").expect("claude-fable-5-thinking 应存在");
        assert_eq!(m.max_tokens, 128000);
        assert_eq!(m.display_name, "Claude Fable 5 (Thinking)");
    }

    #[test]
    fn test_haiku_4_5_max_tokens_unchanged() {
        // 回归：haiku-4-5 max_tokens 维持 64000
        let m = find_by_id("claude-haiku-4-5-20251001").expect("haiku 条目缺失");
        assert_eq!(m.max_tokens, 64000);
    }

    #[test]
    fn test_opus_4_7_4_8_max_tokens_unchanged() {
        // 回归
        assert_eq!(find_by_id("claude-opus-4-7").unwrap().max_tokens, 128000);
        assert_eq!(find_by_id("claude-opus-4-8").unwrap().max_tokens, 128000);
    }

    #[test]
    fn test_build_model_list_includes_opus_5() {
        let list = build_model_list();
        let ids: std::collections::HashSet<&str> = list.iter().map(|m| m.id.as_str()).collect();

        assert!(ids.contains("claude-opus-5"), "缺 claude-opus-5 静态表项");
        assert!(
            ids.contains("claude-opus-5-thinking"),
            "缺 claude-opus-5-thinking 静态表项"
        );

        let opus5 = list.iter().find(|m| m.id == "claude-opus-5").unwrap();
        assert_eq!(opus5.owned_by, "anthropic");
        assert_eq!(opus5.display_name, "Claude Opus 5");
        assert_eq!(opus5.max_tokens, 128000);

        let opus5t = list
            .iter()
            .find(|m| m.id == "claude-opus-5-thinking")
            .unwrap();
        assert_eq!(opus5t.owned_by, "anthropic");
        assert_eq!(opus5t.display_name, "Claude Opus 5 (Thinking)");
        assert_eq!(opus5t.max_tokens, 128000);

        // 回归：sonnet-5 仍在
        assert!(ids.contains("claude-sonnet-5"));
        assert!(ids.contains("claude-sonnet-5-thinking"));
        // 回归：opus-4.7/4.8 仍在
        assert!(ids.contains("claude-opus-4-7"));
        assert!(ids.contains("claude-opus-4-8"));
    }

    #[test]
    fn test_sonnet_4_6_max_tokens_unchanged() {
        // 回归
        assert_eq!(find_by_id("claude-sonnet-4-6").unwrap().max_tokens, 64000);
    }

}
