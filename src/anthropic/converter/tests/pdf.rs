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
fn test_extract_pdf_text_from_simple_tj_pdf() {
    use base64::Engine as _;

    let pdf = "%PDF-1.4\n1 0 obj\n<<>>\nendobj\nstream\nBT /F1 14 Tf 10 20 Td (hvoyabcd) Tj ET\nendstream\n%%EOF";
    let data = base64::engine::general_purpose::STANDARD.encode(pdf);

    assert_eq!(
        extract_pdf_text_from_base64(&data),
        Some("hvoyabcd".to_string())
    );
}

#[test]
#[test]
fn test_extract_pdf_text_non_ascii_no_panic() {
    use base64::Engine as _;

    // 回归：'(' 字面量串内含多字节 UTF-8，lookahead 切片旧实现按字符串字节
    // 切片可能落在字符中间 panic，新实现按字节数组匹配 ASCII，应安全
    let pdf = "stream\nBT (你好世界测试内容) Tj ET\nendstream";
    let data = base64::engine::general_purpose::STANDARD.encode(pdf);
    // 不校验具体返回值，只要求不 panic
    let _ = extract_pdf_text_from_base64(&data);
}
