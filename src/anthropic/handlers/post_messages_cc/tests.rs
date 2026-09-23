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
