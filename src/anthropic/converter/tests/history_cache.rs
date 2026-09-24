//! converter 测试（自 tests.rs 拆分，纯代码搬移）
#![cfg(test)]

use super::super::convert::convert_request;
use super::super::result::ConversionResult;
#[allow(unused_imports)]
use crate::anthropic::types::ContentBlock as _;
use crate::anthropic::types::MessagesRequest;
#[allow(unused_imports)]
use crate::kiro::model::requests::conversation::Message;

#[test]
fn test_system_history_refreshes_when_content_changes() {
    use crate::anthropic::types::{Message as AnthropicMessage, Metadata, SystemMessage};

    let session = Some(Metadata {
        user_id: Some("user_account__session_7b2e9c4d-1a6f-4b8e-9d3c-5f0a2e7b6c11".to_string()),
    });
    let make_req = |system_text: &str| MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: Some(vec![SystemMessage {
            text: system_text.to_string(),
        }]),
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: session.clone(),
    };

    let first = convert_request(&make_req("original system prompt")).unwrap();
    let second = convert_request(&make_req("compacted system prompt")).unwrap();

    let history_content = |result: &ConversionResult| -> String {
        let Message::User(history_user) = &result.conversation_state.history[0] else {
            panic!("系统提示应转换为 history user 消息");
        };
        history_user.user_input_message.content.clone()
    };

    assert!(history_content(&first).contains("original system prompt"));
    assert!(history_content(&second).contains("compacted system prompt"));
    assert!(!history_content(&second).contains("original system prompt"));
}

#[test]
#[test]
fn test_system_history_uses_only_current_reminder() {
    use crate::anthropic::types::{Message as AnthropicMessage, Metadata, SystemMessage};

    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("<system-reminder>old reminder</system-reminder>"),
            },
            AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::json!("Acknowledged."),
            },
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!(
                    "<system-reminder>current reminder</system-reminder>Continue."
                ),
            },
        ],
        stream: false,
        system: Some(vec![SystemMessage {
            text: "Follow the user request.".to_string(),
        }]),
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: Some(Metadata {
            user_id: Some("user_account__session_5c8e1d2a-7f4b-4a9c-8e6d-1b3f0a2c9d44".to_string()),
        }),
    };

    let result = convert_request(&req).unwrap();
    let Message::User(history_user) = &result.conversation_state.history[0] else {
        panic!("系统提示应转换为 history user 消息");
    };
    let content = &history_user.user_input_message.content;

    assert!(content.contains("current reminder"));
    assert!(!content.contains("old reminder"));
}
