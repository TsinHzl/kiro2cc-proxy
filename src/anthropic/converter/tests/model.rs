//! converter 测试（自 tests.rs 拆分，纯代码搬移）
#![cfg(test)]

use super::super::cache;
use super::super::convert::{
    convert_request, determine_agent_task_type, determine_chat_trigger_type,
};
use super::super::fields::model_max_output_tokens;
use super::super::history::{convert_assistant_message, merge_assistant_messages};
use super::super::model::map_model;
use super::super::pdf::extract_pdf_text_from_base64;
use super::super::prompt::append_recent_knowledge_hints;
use super::super::schema::normalize_json_schema;
use super::super::session::{
    derive_fallback_conversation_id, extract_session_id, is_compact_request, is_valid_uuid,
};
use super::super::thinking::generate_thinking_prefix;
use super::super::tools::{remove_orphaned_tool_uses, validate_tool_pairing};
use super::super::websearch::{
    collect_history_tool_names, create_placeholder_tool, is_web_search_server_tool,
    split_web_search_tool,
};
#[allow(unused_imports)]
use crate::anthropic::types::ContentBlock as _;
use crate::anthropic::types::{
    Message as AnthropicMessage, MessagesRequest, OutputConfig, Tool as AnthropicTool2,
};
#[allow(unused_imports)]
use crate::kiro::model::requests::conversation::Message;
use crate::kiro::model::requests::conversation::{
    AssistantMessage, HistoryAssistantMessage, HistoryUserMessage, UserInputMessageContext,
    UserMessage,
};
use crate::kiro::model::requests::tool::ToolResult;

#[test]
fn test_map_model_sonnet() {
    assert_eq!(map_model("claude-sonnet-4").unwrap(), "claude-sonnet-4");
    assert_eq!(
        map_model("claude-sonnet-4-20250514").unwrap(),
        "claude-sonnet-4"
    );
    assert_eq!(
        map_model("claude-sonnet-4-5-20250929").unwrap(),
        "claude-sonnet-4.5"
    );
    // claude-3-5-sonnet 含日期中有 "4"，但不含 sonnet-4，应兜底到 4.5
    assert_eq!(
        map_model("claude-3-5-sonnet-20241022").unwrap(),
        "claude-sonnet-4.5"
    );
}

#[test]
#[test]
fn test_map_model_opus() {
    assert!(
        map_model("claude-opus-4-20250514")
            .unwrap()
            .contains("opus")
    );
}

#[test]
#[test]
fn test_map_model_haiku() {
    assert!(
        map_model("claude-haiku-4-20250514")
            .unwrap()
            .contains("haiku")
    );
}

#[test]
#[test]
fn test_map_model_passthrough_unknown() {
    // 开放透传：未命中内置规则的非空模型 ID 原样透传
    assert_eq!(map_model("gpt-4").unwrap(), "gpt-4");
    assert_eq!(
        map_model("some-brand-new-model").unwrap(),
        "some-brand-new-model"
    );
}

#[test]
#[test]
fn test_map_model_passthrough_strips_thinking() {
    // 透传时剥离 -thinking 标记（thinking 由 req.thinking 单独控制）
    assert_eq!(map_model("gpt-4-thinking").unwrap(), "gpt-4");
    assert_eq!(
        map_model("brand-new-thinking-model").unwrap(),
        "brand-new-model"
    );
}

#[test]
#[test]
fn test_map_model_empty_rejected() {
    // 空字符串仍拒绝
    assert!(map_model("").is_none());
    assert!(map_model("   ").is_none());
    assert!(map_model("-thinking").is_none());
}

#[test]
#[test]
fn test_map_model_builtin_rules_unaffected() {
    // 内置规则优先级不变
    assert_eq!(map_model("claude-sonnet-4-5").unwrap(), "claude-sonnet-4.5");
    assert_eq!(map_model("gpt-5.6-sol").unwrap(), "gpt-5.6-sol");
    assert_eq!(map_model("glm-4.6").unwrap(), "glm-5");
}

#[test]
#[test]
fn test_map_model_gpt_5_6_variants() {
    assert_eq!(map_model("gpt-5.6-sol").unwrap(), "gpt-5.6-sol");
    assert_eq!(map_model("gpt-5.6-terra").unwrap(), "gpt-5.6-terra");
    assert_eq!(map_model("gpt-5.6-luna").unwrap(), "gpt-5.6-luna");
    assert_eq!(map_model("gpt-5.6").unwrap(), "gpt-5.6-sol");
}

#[test]
#[test]
fn test_map_model_thinking_suffix_sonnet() {
    // thinking 后缀不应影响 sonnet 模型映射
    let result = map_model("claude-sonnet-4-5-20250929-thinking");
    assert_eq!(result, Some("claude-sonnet-4.5".to_string()));
}

#[test]
#[test]
fn test_map_model_opus_5_aliases() {
    assert_eq!(map_model("claude-opus-5").unwrap(), "claude-opus-5");
    assert_eq!(
        map_model("claude-opus-5-thinking").unwrap(),
        "claude-opus-5"
    );
    assert_eq!(map_model("Claude Opus 5").unwrap(), "claude-opus-5");
    assert_eq!(
        map_model("claude-opus-5-20260101").unwrap(),
        "claude-opus-5"
    );
    assert_eq!(
        map_model("claude-opus-5-thinking-20260101").unwrap(),
        "claude-opus-5"
    );
    assert_eq!(map_model("claude-Opus-5").unwrap(), "claude-opus-5");

    // 回归：现有 opus-4.7/4.8 不被 opus-5 分支误命中
    assert_eq!(
        map_model("claude-opus-4-7-20251115").unwrap(),
        "claude-opus-4.7"
    );
    assert_eq!(map_model("claude-opus-4.8").unwrap(), "claude-opus-4.8");
    assert_eq!(map_model("claude-opus-4.6").unwrap(), "claude-opus-4.6");
    assert_eq!(map_model("claude-opus-4.5").unwrap(), "claude-opus-4.5");
    assert_eq!(map_model("claude-sonnet-5").unwrap(), "claude-sonnet-5");
}

#[test]
#[test]
fn test_map_model_thinking_suffix_opus_4_5() {
    // thinking 后缀不应影响 opus 4.5 模型映射
    let result = map_model("claude-opus-4-5-20251101-thinking");
    assert_eq!(result, Some("claude-opus-4.5".to_string()));
}

#[test]
#[test]
fn test_map_model_thinking_suffix_opus_4_6() {
    // thinking 后缀不应影响 opus 4.6 模型映射
    let result = map_model("claude-opus-4-6-thinking");
    assert_eq!(result, Some("claude-opus-4.6".to_string()));
}

#[test]
#[test]
fn test_map_model_thinking_suffix_haiku() {
    // thinking 后缀不应影响 haiku 模型映射
    let result = map_model("claude-haiku-4-5-20251001-thinking");
    assert_eq!(result, Some("claude-haiku-4.5".to_string()));
}

#[test]
#[test]
fn test_map_model_fable_routes_to_kiro_fable() {
    assert_eq!(
        map_model("claude-fable-5"),
        Some("claude-fable-5".to_string())
    );
    assert_eq!(
        map_model("claude-fable-5-thinking"),
        Some("claude-fable-5".to_string())
    );
}

#[test]
#[test]
fn test_map_model_opus_4_6_unchanged() {
    // 回归：opus-4-6 默认走 claude-opus-4.6
    assert_eq!(
        map_model("claude-opus-4-6"),
        Some("claude-opus-4.6".to_string())
    );
}
