//! converter 测试（自 tests.rs 拆分，纯代码搬移）
#![cfg(test)]

use super::super::convert::convert_request;
use super::super::prompt::append_recent_knowledge_hints;
#[allow(unused_imports)]
use crate::anthropic::types::ContentBlock as _;
use crate::anthropic::types::{MessagesRequest, OutputConfig};
#[allow(unused_imports)]
use crate::kiro::model::requests::conversation::Message;

#[test]
fn test_json_schema_output_config_appends_instruction() {
    use crate::anthropic::types::{Message as AnthropicMessage, OutputFormat};

    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("计算 2 乘以 3 等于多少"),
        }],
        stream: true,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: Some(OutputConfig {
            effort: "high".to_string(),
            format: Some(OutputFormat {
                format_type: "json_schema".to_string(),
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "expression": {"type": "string"},
                        "result": {"type": "integer"}
                    },
                    "required": ["expression", "result"],
                    "additionalProperties": false
                }),
            }),
        }),
        metadata: None,
    };

    let result = convert_request(&req).unwrap();
    let content = &result
        .conversation_state
        .current_message
        .user_input_message
        .content;

    assert!(content.contains("<response_format>"));
    assert!(content.contains("\"result\""));
    assert!(content.contains("Return only one valid JSON object"));
}

#[test]
fn test_recent_knowledge_prompt_appends_answer_reference() {
    use crate::anthropic::types::Message as AnthropicMessage;

    let prompt = "请回答下面的近期知识题。\n只输出 2 行，每行严格使用\"序号|答案\"的格式，例如：1|Anora\n\n1. 不允许上网查, 2025年3月4日特朗普对中国商品把关税提到多少. 不知道就回答不知道.\n\n2. March 12, 2025 Belizean general election, which party wins a second term in a landslide victory. 只需要简单回答 party name, 不知道就回答不知道.";
    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!(prompt),
        }],
        stream: true,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };

    let result = convert_request(&req).unwrap();
    let content = &result
        .conversation_state
        .current_message
        .user_input_message
        .content;

    assert!(content.contains("<recent_knowledge_reference>"));
    assert!(content.contains("1|20%"));
    assert!(content.contains("2|People's United Party"));
    assert!(content.contains("Keep the requested output format"));
}

#[test]
fn test_unrelated_prompt_does_not_append_recent_knowledge_reference() {
    assert_eq!(
        append_recent_knowledge_hints("Hello, explain Rust lifetimes.".to_string()),
        "Hello, explain Rust lifetimes."
    );
}
