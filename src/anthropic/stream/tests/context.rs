// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 空响应检测与上下文窗口测试（自 stream/tests.rs 拆出，纯代码搬移）
#[cfg(test)]
mod tests {
    use crate::anthropic::stream::{StreamContext, context_window_for_model};

    #[test]
    fn test_empty_response_detected() {
        // 上游完全没发任何内容事件 → is_empty_response 为 true
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false);
        let _ = ctx.generate_initial_events();
        assert!(
            ctx.is_empty_response(),
            "未收到任何内容事件时应判定为空响应"
        );
    }

    #[test]
    fn test_empty_response_oversized_context_by_threshold() {
        // contextUsage 本地化后所有模型按 1M 窗口，阈值 = 1M * 0.28 = 280_000
        // 大输入(>=28万)的空响应 → 判定为上下文过大
        let big = StreamContext::new_with_thinking("test-model", 300_000, true);
        assert!(
            big.empty_response_is_oversized_context(),
            "input 30万应判定为上下文过大"
        );
        // 小输入的空响应 → 视为偶发，可重试
        let small = StreamContext::new_with_thinking("test-model", 100_000, true);
        assert!(
            !small.empty_response_is_oversized_context(),
            "input 10万不应判定为上下文过大"
        );
    }

    #[test]
    fn test_non_empty_response_not_flagged() {
        // 收到了文本内容 → 不应判定为空响应
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false);
        let _ = ctx.generate_initial_events();
        ctx.process_assistant_response("hello");
        assert!(!ctx.is_empty_response(), "已产生文本内容时不应判定为空响应");
    }

    #[test]
    fn test_tool_only_response_not_flagged() {
        // 只有工具调用、无文本 → 不是空响应
        let mut ctx = StreamContext::new_with_thinking("test-model", 1, false);
        let _ = ctx.generate_initial_events();
        ctx.process_tool_use(&crate::kiro::model::events::ToolUseEvent {
            name: "Bash".to_string(),
            tool_use_id: "toolu_1".to_string(),
            input: "{}".to_string(),
            stop: true,
        });
        assert!(!ctx.is_empty_response(), "仅有工具调用时不应判定为空响应");
    }

    #[test]
    fn test_near_empty_response_oversized_context_flagged() {
        // 大上下文（>28万）+ 极短输出（< 30 tokens）+ 无工具调用 → 视为退化空响应
        let mut ctx = StreamContext::new_with_thinking("test-model", 300_000, false);
        let _ = ctx.generate_initial_events();
        // 模拟极短输出（40 个非中文字符 = 10 token）
        ctx.output_chars_other = 40;
        assert!(
            ctx.is_empty_response(),
            "大上下文+极短输出应判定为近似空响应"
        );
    }

    #[test]
    fn test_near_empty_response_small_context_not_flagged() {
        // 小上下文 + 极短输出 → 不应判定为空响应（可能是正常的短回复）
        let mut ctx = StreamContext::new_with_thinking("test-model", 50_000, false);
        let _ = ctx.generate_initial_events();
        ctx.output_chars_other = 40;
        assert!(
            !ctx.is_empty_response(),
            "小上下文+极短输出不应判定为空响应"
        );
    }

    #[test]
    fn test_near_empty_response_with_tool_use_not_flagged() {
        // 大上下文 + 极短输出 + 有工具调用 → 不应判定为空响应（工具调用是有效响应）
        let mut ctx = StreamContext::new_with_thinking("test-model", 300_000, false);
        let _ = ctx.generate_initial_events();
        ctx.output_chars_other = 40;
        ctx.state_manager.set_has_tool_use(true);
        assert!(!ctx.is_empty_response(), "有工具调用时不应判定为空响应");
    }

    #[test]
    fn test_context_window_opus_4_6_is_1m() {
        assert_eq!(context_window_for_model("claude-opus-4-6"), 1_000_000);
        assert_eq!(
            context_window_for_model("claude-opus-4-6-thinking"),
            1_000_000
        );
    }

    #[test]
    fn test_context_window_fable_5_is_1m() {
        assert_eq!(context_window_for_model("claude-fable-5"), 1_000_000);
        assert_eq!(
            context_window_for_model("claude-fable-5-thinking"),
            1_000_000
        );
    }

    #[test]
    fn test_context_window_all_models_unified_to_1m() {
        // contextUsage 本地化后所有模型统一返回 1M
        assert_eq!(
            context_window_for_model("claude-haiku-4-5-20251001"),
            1_000_000
        );
        assert_eq!(context_window_for_model("unknown-model"), 1_000_000);
        assert_eq!(context_window_for_model(""), 1_000_000);
    }

    #[test]
    fn test_context_window_existing_branches_unchanged() {
        // 回归：4-7 / 4-8 / sonnet-4-6 仍为 1M
        assert_eq!(context_window_for_model("claude-opus-4-7"), 1_000_000);
        assert_eq!(context_window_for_model("claude-opus-4-8"), 1_000_000);
        assert_eq!(context_window_for_model("claude-sonnet-4-6"), 1_000_000);
    }
}
