// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 错误映射与辅助函数测试（自 handlers/tests.rs 拆出，纯代码搬移）
#[cfg(test)]
mod tests {

    use super::super::super::error::{format_prompt_too_long, map_provider_error_with_context};
    use super::super::super::helpers::resolve_thinking_enabled;

    use super::super::super::nonstream::build_non_stream_content;
    use super::super::super::stream::{stream_interrupted_error_event, wait_deadline};

    use crate::anthropic::stream::{CLIENT_ASSUMED_CONTEXT_WINDOW, scale_for_client};

    use crate::anthropic::types::Thinking;

    use axum::http::StatusCode;
    use axum::response::Response;
    use serde_json::json;

    use std::time::Duration;
    use tokio::time::Instant;

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
}
