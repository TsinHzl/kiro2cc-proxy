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
fn test_reminders_stay_in_original_messages() {
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

    assert_eq!(content, "Follow the user request.");
    let Message::User(original_user) = &result.conversation_state.history[2] else {
        panic!("原始 user 消息应保留在历史中");
    };
    assert_eq!(
        original_user.user_input_message.content,
        "<system-reminder>old reminder</system-reminder>"
    );
    assert_eq!(
        result
            .conversation_state
            .current_message
            .user_input_message
            .content,
        "<system-reminder>current reminder</system-reminder>Continue."
    );
}

fn reminder_request(system: Option<&str>, messages: serde_json::Value) -> MessagesRequest {
    let request = serde_json::from_value(serde_json::json!({
        "model": "claude-sonnet-4",
        "max_tokens": 1024,
        "messages": messages,
        "metadata": {
            "user_id": "user_account__session_23083fc0-9423-4a8b-9f54-28667ec939fe"
        }
    }))
    .unwrap();
    MessagesRequest {
        system: system.map(|text| {
            vec![crate::anthropic::types::SystemMessage {
                text: text.to_string(),
            }]
        }),
        ..request
    }
}

fn system_history(result: &ConversionResult) -> &str {
    let Message::User(user) = &result.conversation_state.history[0] else {
        panic!("系统指令应进入历史 user 消息");
    };
    &user.user_input_message.content
}

#[test]
fn test_inline_system_preserves_order_duplicates_and_current_user() {
    let req = reminder_request(
        Some("TOP"),
        serde_json::json!([
            {"role": "system", "content": " A "},
            {"role": "user", "content": "first"},
            {"role": "system", "content": [{"type": "text", "text": "B"}]},
            {"role": "assistant", "content": "reply"},
            {"role": "user", "content": "current"},
            {"role": "system", "content": [
                {"type": "text", "text": " A "}, {"type": "text", "text": ""}
            ]}
        ]),
    );
    let result = convert_request(&req).unwrap();
    assert_eq!(system_history(&result), "TOP\n A \nB\n A \n");
    assert_eq!(result.conversation_state.history.len(), 4);
    assert_eq!(
        result
            .conversation_state
            .current_message
            .user_input_message
            .content,
        "current"
    );
}

#[test]
fn test_inline_system_after_user_survives_without_top_level_system() {
    for content in [
        serde_json::json!("技能与 agent 说明"),
        serde_json::json!([{"type": "text", "text": "技能与 agent 说明"}]),
    ] {
        let req = reminder_request(
            None,
            serde_json::json!([
                {"role": "user", "content": "任务"},
                {"role": "system", "content": content}
            ]),
        );
        let result = convert_request(&req).unwrap();
        assert_eq!(system_history(&result), "技能与 agent 说明");
        assert_eq!(
            result
                .conversation_state
                .current_message
                .user_input_message
                .content,
            "任务"
        );
    }
}

#[test]
fn test_inline_system_survives_assistant_prefill_trimming() {
    let req = reminder_request(
        None,
        serde_json::json!([
            {"role": "user", "content": "first"},
            {"role": "assistant", "content": "kept reply"},
            {"role": "user", "content": "current"},
            {"role": "assistant", "content": "discarded prefill 1"},
            {"role": "system", "content": "instructions"},
            {"role": "assistant", "content": "discarded prefill 2"}
        ]),
    );
    let result = convert_request(&req).unwrap();
    assert_eq!(system_history(&result), "instructions");
    let serialized = serde_json::to_string(&result.conversation_state).unwrap();
    assert!(serialized.contains("kept reply"));
    assert!(!serialized.contains("discarded prefill"));
}

#[test]
fn test_inline_system_rejects_unknown_roles_before_trimming() {
    for index in 0..=3 {
        let mut messages = serde_json::json!([
            {"role": "user", "content": "task"},
            {"role": "system", "content": "rules"},
            {"role": "assistant", "content": "prefill"}
        ])
        .as_array()
        .unwrap()
        .clone();
        messages.insert(
            index,
            serde_json::json!({"role": "developer", "content": "private"}),
        );
        let req = reminder_request(None, serde_json::json!(messages));
        let error = convert_request(&req).expect_err("未知角色不能被裁剪或忽略");
        assert!(error.to_string().contains(&format!("messages[{index}]")));
        assert!(!error.to_string().contains("private"));
    }
}

#[test]
fn test_inline_system_rejects_non_text_content() {
    for content in [
        serde_json::json!(null),
        serde_json::json!(42),
        serde_json::json!({"text": "private"}),
        serde_json::json!([{"type": "image", "source": {}}]),
        serde_json::json!([{"type": "tool_use", "name": "Read"}]),
        serde_json::json!([{"type": "text"}]),
        serde_json::json!([{"type": "text", "text": 42}]),
        serde_json::json!([{"type": "text", "text": "private"}, {"type": "image"}]),
    ] {
        let req = reminder_request(
            None,
            serde_json::json!([
                {"role": "user", "content": "task"},
                {"role": "system", "content": content},
                {"role": "assistant", "content": "prefill"}
            ]),
        );
        let error = convert_request(&req).expect_err("无效 system 不能被静默丢弃");
        assert!(error.to_string().contains("messages[1]"));
        assert!(!error.to_string().contains("private"));
    }
}

#[test]
fn test_inline_system_requires_user_message() {
    for messages in [
        serde_json::json!([]),
        serde_json::json!([{"role": "system", "content": "rules"}]),
        serde_json::json!([{"role": "assistant", "content": "prefill"}]),
        serde_json::json!([
            {"role": "system", "content": "rules"},
            {"role": "assistant", "content": "prefill"}
        ]),
    ] {
        assert!(convert_request(&reminder_request(None, messages)).is_err());
    }
}

#[test]
fn test_inline_system_cache_and_session_identity() {
    use super::super::session::derive_fallback_conversation_id;

    for metadata in [false, true] {
        let make_request = |rules: &str, extended: bool| {
            let mut messages = vec![
                serde_json::json!({"role": "system", "content": "initial"}),
                serde_json::json!({"role": "user", "content": "first"}),
                serde_json::json!({"role": "system", "content": rules}),
            ];
            if extended {
                messages.extend([
                    serde_json::json!({"role": "assistant", "content": "reply"}),
                    serde_json::json!({"role": "user", "content": "next"}),
                ]);
            }
            let req = reminder_request(Some("TOP"), serde_json::json!(messages));
            MessagesRequest {
                metadata: if metadata { req.metadata.clone() } else { None },
                ..req
            }
        };
        let first_req = make_request("rules-v1", false);
        let original_messages = serde_json::to_value(&first_req.messages).unwrap();
        let expected_id = if metadata {
            "23083fc0-9423-4a8b-9f54-28667ec939fe".to_string()
        } else {
            derive_fallback_conversation_id(&first_req).unwrap()
        };
        let first = convert_request(&first_req).unwrap();
        let grown = convert_request(&make_request("rules-v1", true)).unwrap();
        let updated = convert_request(&make_request("rules-v2", true)).unwrap();
        assert_eq!(system_history(&first), "TOP\ninitial\nrules-v1");
        assert_eq!(system_history(&first), system_history(&grown));
        assert_eq!(system_history(&updated), "TOP\ninitial\nrules-v2");
        for result in [&first, &grown, &updated] {
            assert_eq!(result.conversation_state.conversation_id, expected_id);
        }
        assert_eq!(
            serde_json::to_value(&first_req.messages).unwrap(),
            original_messages
        );
    }
}

#[test]
fn test_current_reminders_preserve_text_with_or_without_system() {
    let texts = [
        "<system-reminder>z: 修改前确认</system-reminder>\n\
         <system-reminder>a: 修改后调用 Agent 做 CR</system-reminder>\n\
         <system-reminder>z: 修改前确认</system-reminder>\n请修复代码。",
        "<system-reminder>未闭合提醒也不能删除后续内容",
    ];
    for system in [None, Some(""), Some("Follow the user request.")] {
        for text in texts {
            for content in [
                serde_json::json!(text),
                serde_json::json!([{"type": "text", "text": text}]),
            ] {
                let req = reminder_request(
                    system,
                    serde_json::json!([{"role": "user", "content": content}]),
                );
                let result = convert_request(&req).unwrap();
                assert_eq!(
                    result
                        .conversation_state
                        .current_message
                        .user_input_message
                        .content,
                    text
                );
            }
        }
    }
}

#[test]
fn test_rules_survive_read_and_edit_results_without_changing_system_history() {
    let rules =
        "<system-reminder>修改前必须 AskUserQuestion；修改后必须 Agent CR。</system-reminder>";
    for content in [
        serde_json::json!(rules),
        serde_json::json!([{"type": "text", "text": rules}]),
    ] {
        let messages = serde_json::json!([
            {"role": "user", "content": content},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "read_1", "name": "Read",
                 "input": {"file_path": "/tmp/example.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "read_1", "content": "file contents"}
            ]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "edit_1", "name": "Edit",
                 "input": {"file_path": "/tmp/example.rs", "old_string": "old", "new_string": "new"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "edit_1", "content": "edit completed"}
            ]}
        ]);
        for count in [1, 3, 5] {
            let req = reminder_request(
                Some("Follow the user request."),
                serde_json::json!(&messages.as_array().unwrap()[..count]),
            );
            let state = convert_request(&req).unwrap().conversation_state;
            let Message::User(system_user) = &state.history[0] else {
                panic!("系统历史应保留");
            };
            assert_eq!(
                system_user.user_input_message.content,
                "Follow the user request."
            );
            let serialized = serde_json::to_string(&state).unwrap();
            assert_eq!(serialized.matches(rules).count(), 1);
            if count > 1 {
                let results = &state
                    .current_message
                    .user_input_message
                    .user_input_message_context
                    .tool_results;
                assert_eq!(results.len(), 1);
                assert_eq!(
                    results[0].tool_use_id,
                    if count == 3 { "read_1" } else { "edit_1" }
                );
            }
        }
    }
    let compacted = reminder_request(
        Some("Follow the user request."),
        serde_json::json!([{
            "role": "user",
            "content": "<system-reminder>新的规则</system-reminder>压缩后的上下文"
        }]),
    );
    let state = convert_request(&compacted).unwrap().conversation_state;
    let serialized = serde_json::to_string(&state).unwrap();
    assert!(!serialized.contains(rules));
    assert!(serialized.contains("<system-reminder>新的规则</system-reminder>"));
}

#[test]
fn test_client_workflow_tool_descriptions_are_not_augmented() {
    let description = "修改前必须确认；修改后必须执行独立审查。";
    let req: MessagesRequest = serde_json::from_value(serde_json::json!({
        "model": "claude-sonnet-4",
        "max_tokens": 1024,
        "system": "Follow the user request.",
        "messages": [{"role": "user", "content": "请修复代码。"}],
        "tools": (["Write", "Edit", "Agent", "AskUserQuestion"].map(|name| {
            serde_json::json!({
                "name": name,
                "description": description,
                "input_schema": {"type": "object", "properties": {}}
            })
        }))
    }))
    .unwrap();
    let state = convert_request(&req).unwrap().conversation_state;
    let tools = &state
        .current_message
        .user_input_message
        .user_input_message_context
        .tools;
    assert_eq!(tools.len(), 4);
    for tool in tools {
        assert_eq!(tool.tool_specification.description, description);
    }
}

#[test]
fn test_rendered_thinking_header_stripped_from_assistant_history() {
    // thinkingAsText 开启时，客户端会把渲染出的思考 text 块当正文回传；
    // 历史转换仅剥离「💭 Thinking」标记行，思考正文按普通助手文本保留回传上游
    use crate::anthropic::types::{Message as AnthropicMessage, MessagesRequest};
    let req = MessagesRequest {
        model: "claude-sonnet-4-6".to_string(),
        max_tokens: 2048,
        messages: vec![
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("q1"),
            },
            AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::json!([
                    {"type": "text", "text": "💭 Thinking\nsecret reasoning\n\nmore"},
                    {"type": "text", "text": "visible answer"},
                    {"type": "tool_use", "id": "t1", "name": "Read", "input": {}}
                ]),
            },
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!([
                    {"type": "tool_result", "tool_use_id": "t1", "content": "ok"}
                ]),
            },
        ],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };
    let r = convert_request(&req).unwrap();
    let hist = serde_json::to_string(&r.conversation_state.history).unwrap();
    assert!(hist.contains("visible answer"));
    // 标记行已剥；正文保留（用户已确认的语义：上下文略增，显示最干净）
    assert!(!hist.contains("💭 Thinking"), "{hist}");
    assert!(hist.contains("secret reasoning"), "{hist}");
}

#[test]
fn test_dim_markers_stripped_from_assistant_history() {
    // 文本化思考正文每行带 dim 转义，客户端原样回传；模型模仿产生的字面 `[2m…[0m` 整行包裹
    // 同样不应留在上游上下文里（否则会强化模型继续输出该格式）
    use crate::anthropic::types::{Message as AnthropicMessage, MessagesRequest};
    let req = MessagesRequest {
        model: "claude-sonnet-4-5".to_string(),
        max_tokens: 2048,
        messages: vec![
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("q1"),
            },
            AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::json!([
                    {"type": "text", "text": "\u{1b}[2m💭 Thinking\u{1b}[0m\n\u{1b}[2mreal reasoning\u{1b}[0m\n\n\u{1b}[2m💭 Thought for 2s (9 tokens)\u{1b}[0m\n"},
                    {"type": "text", "text": "[2mimitated reasoning[0m\n[2m[0m\nvisible answer"}
                ]),
            },
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("q2"),
            },
        ],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };
    let r = convert_request(&req).unwrap();
    let hist = serde_json::to_string(&r.conversation_state.history).unwrap();
    assert!(
        !hist.contains("[2m") && !hist.contains("[0m") && !hist.contains("\\u001b"),
        "{hist}"
    );
    assert!(!hist.contains("💭 Thinking"), "{hist}");
    for kept in ["real reasoning", "imitated reasoning", "visible answer"] {
        assert!(hist.contains(kept), "{kept}: {hist}");
    }
}

#[test]
fn test_code_example_with_literal_dim_line_survives_history() {
    // 普通回答里讲解 ANSI 的 fenced 代码示例（无任何思考标记）不得被改写
    use crate::anthropic::types::{Message as AnthropicMessage, MessagesRequest};
    let answer = "示例：\n```\n[2mhello[0m\n[2mworld[0m\n```\n以上。";
    let req = MessagesRequest {
        model: "claude-sonnet-4-5".to_string(),
        max_tokens: 2048,
        messages: vec![
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("q1"),
            },
            AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::json!([{"type": "text", "text": answer}]),
            },
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("q2"),
            },
        ],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };
    let r = convert_request(&req).unwrap();
    let hist = serde_json::to_string(&r.conversation_state.history).unwrap();
    let expected = serde_json::to_string(answer).unwrap();
    let expected = &expected[1..expected.len() - 1];
    assert!(hist.contains(expected), "{hist}");
}
