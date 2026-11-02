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
    // 无时长行（流被中断）无法判定思考范围：降级为仅剥离「💭 Thinking」标记行，正文保留
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
    // 标记行已剥；无时长行时正文保守保留（完整思考块的整块丢弃见下方专门测试）
    assert!(!hist.contains("💭 Thinking"), "{hist}");
    assert!(hist.contains("secret reasoning"), "{hist}");
}

#[test]
fn test_credits_footer_stripped_from_assistant_history() {
    // showSessionCredits 开启时，客户端会把末尾的汇总行当正文回传；
    // 回传上游前必须剥掉（ANSI 与 Markdown 两种样式），且保持历史逐字节等同于没有汇总行
    use crate::anthropic::types::{Message as AnthropicMessage, MessagesRequest};
    let build = |assistant_text: &str| MessagesRequest {
        model: "claude-sonnet-4-6".to_string(),
        max_tokens: 2048,
        messages: vec![
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("q1"),
            },
            AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::json!([{"type": "text", "text": assistant_text}]),
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
    let history_of = |text: &str| {
        let r = convert_request(&build(text)).unwrap();
        serde_json::to_string(&r.conversation_state.history).unwrap()
    };

    let clean = history_of("最终答案");
    let ansi =
        history_of("最终答案\n\n\u{1b}[2m▸ Credits: turn 0.86 • session 8.73 | Time: 12s\u{1b}[0m");
    let plain = history_of("最终答案\n\n*▸ Credits: turn 0.86 • session 8.73 | Time: 12s*");

    assert!(clean.contains("最终答案"));
    assert!(!ansi.contains("Credits:"), "{ansi}");
    assert!(!plain.contains("Credits:"), "{plain}");
    // 历史逐字节稳定：prompt cache 不会因汇总行失效
    assert_eq!(ansi, clean);
    assert_eq!(plain, clean);
}

#[test]
fn test_rendered_thinking_block_fully_dropped_from_assistant_history() {
    // 完整的文本化思考块（标记行 + 正文 + 时长行）整块不回传上游；
    // 思考块后的答案与工具调用保留，纯思考 + 工具调用的消息不会留下空 content
    use crate::anthropic::types::{Message as AnthropicMessage, MessagesRequest};
    let thinking = "💭 Thinking\nsecret reasoning\n💭 Thought for 3s (12 tokens)\n";
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
                    {"type": "text", "text": thinking},
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
            AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::json!([
                    {"type": "text", "text": thinking},
                    {"type": "tool_use", "id": "t2", "name": "Read", "input": {}}
                ]),
            },
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!([
                    {"type": "tool_result", "tool_use_id": "t2", "content": "ok"}
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
    assert!(!hist.contains("secret reasoning"), "{hist}");
    assert!(
        !hist.contains("Thinking") && !hist.contains("Thought for"),
        "{hist}"
    );
    assert!(hist.contains("visible answer"), "{hist}");
    // 仅含思考块 + tool_use 的 assistant 消息：content 为占位符，不为空
    for m in &r.conversation_state.history {
        if let crate::kiro::model::requests::conversation::Message::Assistant(a) = m {
            assert!(
                !a.assistant_response_message.content.is_empty(),
                "assistant content 不得为空"
            );
        }
    }
}

#[test]
fn test_consecutive_thinking_only_assistants_merge_to_non_empty_content() {
    // 连续 assistant 消息（Issue #79 合并路径）均只含被丢弃的思考（文本化完整思考块 /
    // 原生 thinking 块）且无 tool_use：合并后 content 必须非空（Kiro 要求）
    use crate::anthropic::types::{Message as AnthropicMessage, MessagesRequest};
    let rendered = "💭 Thinking\nsecret reasoning\n💭 Thought for 3s (12 tokens)\n";
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
                content: serde_json::json!([{"type": "text", "text": rendered}]),
            },
            AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::json!([{"type": "text", "text": rendered}]),
            },
            AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::json!([
                    {"type": "thinking", "thinking": "native secret", "signature": "s"}
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
    assert!(!hist.contains("secret"), "{hist}");
    let mut assistants = 0;
    for m in &r.conversation_state.history {
        if let crate::kiro::model::requests::conversation::Message::Assistant(a) = m {
            assistants += 1;
            assert!(
                !a.assistant_response_message.content.is_empty(),
                "合并后的 assistant content 不得为空: {hist}"
            );
        }
    }
    assert_eq!(assistants, 1, "三条连续 assistant 应合并为一条: {hist}");
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
    // 带时长行的完整思考块整块丢弃（上游不含思考内容）；其余内容（含模型模仿内容）保留
    assert!(!hist.contains("real reasoning"), "{hist}");
    assert!(!hist.contains("Thought for"), "{hist}");
    for kept in ["imitated reasoning", "visible answer"] {
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

/// 带 tools 的请求：复用 reminder_request 的基础字段，再换上指定的 tools
fn request_with_tools(messages: serde_json::Value, tools: serde_json::Value) -> MessagesRequest {
    MessagesRequest {
        tools: Some(serde_json::from_value(tools).unwrap()),
        ..reminder_request(Some("TOP"), messages)
    }
}

#[test]
fn test_inline_system_skips_tool_change_events_and_keeps_text() {
    // advisor 开启后 Claude Code 会在内联 system 里放 tool_addition / tool_removal：
    // 事件被跳过，同一条消息里的 text 照常保留，不再整次请求 400
    let req = reminder_request(
        Some("TOP"),
        serde_json::json!([
            {"role": "user", "content": "first"},
            {"role": "system", "content": [
                {"type": "text", "text": "原有系统说明"},
                {"type": "tool_addition",
                 "tool": {"type": "tool_reference", "name": "advisor"},
                 "cache_control": {"type": "ephemeral"}},
                {"type": "tool_removal", "tool": {"type": "tool_reference", "name": "advisor"}}
            ]},
            {"role": "assistant", "content": "reply"},
            {"role": "user", "content": "current"}
        ]),
    );
    let result = convert_request(&req).expect("工具变更事件不应导致转换失败");
    assert_eq!(system_history(&result), "TOP\n原有系统说明");
}

#[test]
fn test_inline_system_with_only_tool_change_event_adds_no_text() {
    let with_event = reminder_request(
        Some("TOP"),
        serde_json::json!([
            {"role": "user", "content": "first"},
            {"role": "system", "content": [{"type": "tool_addition", "tool": {"name": "x"}}]},
            {"role": "assistant", "content": "reply"},
            {"role": "user", "content": "current"}
        ]),
    );
    let without_event = reminder_request(
        Some("TOP"),
        serde_json::json!([
            {"role": "user", "content": "first"},
            {"role": "assistant", "content": "reply"},
            {"role": "user", "content": "current"}
        ]),
    );
    let a = convert_request(&with_event).unwrap();
    let b = convert_request(&without_event).unwrap();
    assert_eq!(system_history(&a), system_history(&b));
    assert_eq!(
        a.conversation_state.history.len(),
        b.conversation_state.history.len()
    );
}

#[test]
fn test_advisor_server_tool_is_not_forwarded_but_same_named_user_tool_is() {
    let messages = serde_json::json!([{"role": "user", "content": "hi"}]);
    // 带 advisor_* type 的服务端工具被剔除，普通工具保留
    let req = request_with_tools(
        messages.clone(),
        serde_json::json!([
            {"type": "advisor_20260301", "name": "advisor", "defer_loading": true,
             "model": "claude-opus-4-6"},
            {"name": "Read", "description": "read", "input_schema": {"type": "object"}}
        ]),
    );
    let result = convert_request(&req).unwrap();
    let names: Vec<_> = result
        .conversation_state
        .current_message
        .user_input_message
        .user_input_message_context
        .tools
        .iter()
        .map(|t| t.tool_specification.name.as_str())
        .collect();
    assert_eq!(names, ["Read"]);

    // 用户自己定义、恰好叫 advisor 的普通工具（无 advisor_* type）不能被误伤
    let req = request_with_tools(
        messages,
        serde_json::json!([
            {"name": "advisor", "description": "my own", "input_schema": {"type": "object"}}
        ]),
    );
    let result = convert_request(&req).unwrap();
    assert_eq!(
        result
            .conversation_state
            .current_message
            .user_input_message
            .user_input_message_context
            .tools
            .len(),
        1
    );
}

#[test]
fn test_tool_change_event_only_inline_system_keeps_thinking_prefix_without_top_level_system() {
    // 没有顶层 system、内联 system 只有被跳过的工具变更事件时，应与「没有 system」一致：
    // thinking 前缀照常注入，而不是因为空的 system 区被漏掉
    let build = |with_event: bool| {
        let mut messages = vec![serde_json::json!({"role": "user", "content": "first"})];
        if with_event {
            messages.push(serde_json::json!({
                "role": "system",
                "content": [{"type": "tool_addition", "tool": {"name": "advisor"}}]
            }));
        }
        messages.push(serde_json::json!({"role": "assistant", "content": "reply"}));
        messages.push(serde_json::json!({"role": "user", "content": "current"}));
        let mut req = reminder_request(None, serde_json::json!(messages));
        req.thinking = Some(crate::anthropic::types::Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 4096,
        });
        convert_request(&req).unwrap()
    };
    let with_event = build(true);
    let without_event = build(false);
    assert!(
        system_history(&without_event).contains("thinking"),
        "基线应注入 thinking 前缀"
    );
    assert_eq!(system_history(&with_event), system_history(&without_event));
    assert_eq!(
        with_event.conversation_state.history.len(),
        without_event.conversation_state.history.len()
    );
}

/// 会话中途新增系统块（如按路径加载的 rules）不得改变 history[0]，内容改放当前消息；
/// 文本不匹配任何已知动态前缀也一样。
#[test]
fn test_system_block_added_mid_session_is_diverted_from_history_zero() {
    use crate::anthropic::types::{Message as AnthropicMessage, Metadata, SystemMessage};

    let base = [
        format!("base-identity {}", "i".repeat(2000)),
        format!("base-rules {}", "r".repeat(2000)),
        format!("base-env {}", "e".repeat(2000)),
    ];
    let late_rule = format!(
        "Contents of /home/u/.claude/rules/ts/style.md:\n\n{}",
        "s".repeat(400)
    );

    let make_req = |blocks: Vec<&str>| MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("go"),
        }],
        stream: false,
        system: Some(
            blocks
                .into_iter()
                .map(|text| SystemMessage {
                    text: text.to_string(),
                })
                .collect(),
        ),
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: Some(Metadata {
            user_id: Some("user_account__session_5a1c0e22-7d3b-4f6a-8c19-0b2d4e6f8a10".to_string()),
        }),
    };
    let current_text = |r: &ConversionResult| {
        r.conversation_state
            .current_message
            .user_input_message
            .content
            .clone()
    };

    let first = convert_request(&make_req(base.iter().map(String::as_str).collect())).unwrap();
    // 新块插在中间，而不是末尾
    let with_rule =
        convert_request(&make_req(vec![&base[0], &base[1], &late_rule, &base[2]])).unwrap();

    assert_eq!(system_history(&first), system_history(&with_rule));
    assert!(!system_history(&with_rule).contains("Contents of /home"));
    assert!(current_text(&with_rule).contains("Contents of /home/u/.claude/rules/ts/style.md"));
    assert!(!current_text(&first).contains("Contents of"));
}

#[test]
fn prefix_cached_is_false_on_first_turn_and_true_on_subsequent_turns() {
    use crate::anthropic::types::{Message as AnthropicMessage, Metadata, SystemMessage};

    let session = Some(Metadata {
        user_id: Some("user_account__session_9e4a1b2c-3d5f-4a7e-8b9c-1d2e3f4a5b6c".to_string()),
    });
    let make_req = |sys: &str, msgs: Vec<(&str, &str)>| MessagesRequest {
        model: "claude-haiku-5-5".to_string(),
        max_tokens: 1024,
        messages: msgs
            .into_iter()
            .map(|(role, text)| AnthropicMessage {
                role: role.to_string(),
                content: serde_json::json!(text),
            })
            .collect(),
        stream: true,
        system: Some(vec![SystemMessage {
            text: sys.to_string(),
        }]),
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: session.clone(),
    };

    // 1. 新窗口第一条消息：history_end_index == 0 且 history[0] first → prefix_cached 必为 false
    let turn1 = convert_request(&make_req("system prompt v1", vec![("user", "hi")])).unwrap();
    assert!(!turn1.prefix_cached);

    // 2. 单条消息重发：虽然 PREV_H0 已登记，但无历史对话轮次 → 仍为 false
    let turn1_retry = convert_request(&make_req("system prompt v1", vec![("user", "hi")])).unwrap();
    assert!(!turn1_retry.prefix_cached);

    // 3. 第二轮对话：存在历史轮次且 history[0] 命中 frozen → prefix_cached 为 true
    let turn2 = convert_request(&make_req(
        "system prompt v1",
        vec![("user", "hi"), ("assistant", "hello"), ("user", "next")],
    ))
    .unwrap();
    assert!(turn2.prefix_cached);

    // 4. 系统提示被改写导致 history[0] 漂移重置（first）→ 当轮 prefix_cached 回落为 false
    let turn3_reset = convert_request(&make_req(
        "system prompt v2 rewritten",
        vec![
            ("user", "hi"),
            ("assistant", "hello"),
            ("user", "next"),
            ("assistant", "ok"),
            ("user", "after reset"),
        ],
    ))
    .unwrap();
    assert!(!turn3_reset.prefix_cached);
}

// ---- issue #59：会话中途内联 system 通知就地挂回，而不是每轮重复置顶 ----

const FILE_NOTE: &str = "Note: /tmp/a.rs changed on disk since you last read it.";

/// 足够大的稳定系统提示：基线的「多出块不得超过基线字节数」防护要求基线比新增块大
fn big_system() -> String {
    "You are a coding assistant. Follow the project rules carefully. ".repeat(20)
}

/// 带唯一会话 ID 的请求，避免全局基线表在用例间串扰
fn inline_req(session: &str, system: Option<&str>, messages: serde_json::Value) -> MessagesRequest {
    let mut req = reminder_request(system, messages);
    req.metadata = serde_json::from_value(serde_json::json!({
        "user_id": format!("user_account__session_{session}")
    }))
    .unwrap();
    req
}

fn history_user_texts(result: &ConversionResult) -> Vec<String> {
    result
        .conversation_state
        .history
        .iter()
        .filter_map(|m| match m {
            Message::User(u) => Some(u.user_input_message.content.clone()),
            _ => None,
        })
        .collect()
}

fn current_text(result: &ConversionResult) -> &str {
    &result
        .conversation_state
        .current_message
        .user_input_message
        .content
}

/// 会话里先出现一次通知，之后又过了若干轮：旧通知必须留在它当时的位置，
/// 不能出现在最新一轮的当前消息里。
#[test]
fn mid_session_notice_stays_at_its_original_position() {
    let session = "7c1d0e5a-3b2f-4c8a-9e61-0a1b2c3d4e51";
    // 第 1 轮：建立基线（不含通知）
    let turn1 = inline_req(
        session,
        Some(big_system().as_str()),
        serde_json::json!([{"role": "user", "content": "task one"}]),
    );
    convert_request(&turn1).unwrap();

    // 第 2 轮：通知夹在 assistant 与新 user 之间
    let r2 = convert_request(&inline_req(
        session,
        Some(big_system().as_str()),
        serde_json::json!([
            {"role": "user", "content": "task one"},
            {"role": "assistant", "content": "done one"},
            {"role": "system", "content": FILE_NOTE},
            {"role": "user", "content": "task two"}
        ]),
    ))
    .unwrap();
    assert!(
        current_text(&r2).contains(FILE_NOTE) && current_text(&r2).ends_with("task two"),
        "通知刚出现的那一轮挂在它后面紧跟的 user 消息开头: {}",
        current_text(&r2)
    );

    // 第 3 轮：历史里已有该通知，用户又发了新消息
    let turn3 = inline_req(
        session,
        Some(big_system().as_str()),
        serde_json::json!([
            {"role": "user", "content": "task one"},
            {"role": "assistant", "content": "done one"},
            {"role": "system", "content": FILE_NOTE},
            {"role": "user", "content": "task two"},
            {"role": "assistant", "content": "done two"},
            {"role": "user", "content": "task three"}
        ]),
    );
    let r3 = convert_request(&turn3).unwrap();
    assert_eq!(
        current_text(&r3),
        "task three",
        "旧通知不得再出现在当前消息"
    );
    let users = history_user_texts(&r3);
    assert_eq!(users[0], big_system(), "通知不得进入 history[0]");
    assert!(
        users[2].contains(FILE_NOTE) && users[2].ends_with("task two"),
        "旧通知应仍在它所属的 user 消息里: {users:?}"
    );
    assert!(!users[1].contains(FILE_NOTE));
}

/// 同一条通知在相邻两轮里挂靠的 user 消息文本必须逐字节一致（前缀缓存稳定）。
#[test]
fn notice_attachment_is_byte_stable_across_turns() {
    let session = "7c1d0e5a-3b2f-4c8a-9e61-0a1b2c3d4e52";
    let base = |tail: serde_json::Value| {
        let mut m = serde_json::json!([
            {"role": "user", "content": "q1"},
            {"role": "assistant", "content": "a1"},
            {"role": "system", "content": FILE_NOTE},
            {"role": "user", "content": "q2"},
            {"role": "assistant", "content": "a2"},
            {"role": "user", "content": "q3"}
        ]);
        m.as_array_mut()
            .unwrap()
            .extend(tail.as_array().unwrap().clone());
        m
    };
    let register = inline_req(
        session,
        Some(big_system().as_str()),
        serde_json::json!([{"role": "user", "content": "q1"}]),
    );
    convert_request(&register).unwrap();

    let a = convert_request(&inline_req(
        session,
        Some(big_system().as_str()),
        base(serde_json::json!([])),
    ))
    .unwrap();
    let b = convert_request(&inline_req(
        session,
        Some(big_system().as_str()),
        base(serde_json::json!([
            {"role": "assistant", "content": "a3"},
            {"role": "user", "content": "q4"}
        ])),
    ))
    .unwrap();
    let (ua, ub) = (history_user_texts(&a), history_user_texts(&b));
    assert_eq!(ua[..3], ub[..3], "已有历史前缀必须逐字节不变");
}

/// 通知跟在 tool_result 之后（最新一轮，后面没有 user）：挂在该 tool_result 消息末尾，
/// 不能让通知变成「当前消息」而把 tool_result 挤进历史。
#[test]
fn trailing_notice_does_not_displace_tool_result_message() {
    let session = "7c1d0e5a-3b2f-4c8a-9e61-0a1b2c3d4e53";
    let register = inline_req(
        session,
        Some(big_system().as_str()),
        serde_json::json!([{"role": "user", "content": "go"}]),
    );
    convert_request(&register).unwrap();

    let req = inline_req(
        session,
        Some(big_system().as_str()),
        serde_json::json!([
            {"role": "user", "content": "go"},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"file_path": "/a"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "toolu_1", "content": "file body"}
            ]},
            {"role": "system", "content": FILE_NOTE}
        ]),
    );
    let r = convert_request(&req).unwrap();
    let cur = &r.conversation_state.current_message.user_input_message;
    assert_eq!(cur.user_input_message_context.tool_results.len(), 1);
    assert_eq!(
        cur.user_input_message_context.tool_results[0].tool_use_id,
        "toolu_1"
    );
    assert!(
        cur.content.starts_with("(tool result above)"),
        "{}",
        cur.content
    );
    assert!(cur.content.contains(FILE_NOTE));
    // 历史里仍是 user / assistant 成对，tool_use 紧邻当前消息之前
    assert!(matches!(
        r.conversation_state.history.last(),
        Some(Message::Assistant(_))
    ));
}

/// token 预算提示是逐轮变化的状态，行为不变：仍放当前消息开头，不进历史。
#[test]
fn token_budget_notice_still_goes_to_current_head() {
    let session = "7c1d0e5a-3b2f-4c8a-9e61-0a1b2c3d4e54";
    let req = inline_req(
        session,
        Some(big_system().as_str()),
        serde_json::json!([
            {"role": "user", "content": "a"},
            {"role": "assistant", "content": "b"},
            {"role": "system", "content": "<total_tokens>100 tokens left</total_tokens>"},
            {"role": "user", "content": "c"}
        ]),
    );
    // 首个请求即建立基线；预算提示按既有规则走开头路径，不挂靠历史
    let r = convert_request(&req).unwrap();
    let hist = serde_json::to_string(&r.conversation_state.history).unwrap();
    assert!(!hist.contains("total_tokens"), "{hist}");
    assert!(current_text(&r).contains("total_tokens"));
}

#[test]
fn resolve_inline_anchor_prefers_next_user_then_previous_user() {
    use super::super::convert::resolve_inline_anchor as resolve;
    use super::super::history::InlineAnchor;
    // user, assistant, [S], user  -> 挂后面 user 开头
    assert_eq!(
        resolve(&["user", "assistant", "user"], 2),
        Some(InlineAnchor {
            msg: 2,
            after: false
        })
    );
    // user, [S], assistant        -> 挂前面 user 末尾
    assert_eq!(
        resolve(&["user", "assistant"], 1),
        Some(InlineAnchor {
            msg: 0,
            after: true
        })
    );
    // user, assistant, [S], assistant, user -> 夹在两条 assistant 之间，就近取后面的 user
    assert_eq!(
        resolve(&["user", "assistant", "assistant", "user"], 2),
        Some(InlineAnchor {
            msg: 3,
            after: false
        })
    );
    // 会话开头不挂靠
    assert_eq!(resolve(&["user"], 0), None);
}
