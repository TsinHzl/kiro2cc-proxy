//! converter 测试（自 tests.rs 拆分，纯代码搬移）
#![cfg(test)]

use super::super::convert::convert_request;
use super::super::history::{convert_assistant_message, merge_assistant_messages};
use super::super::tools::{remove_orphaned_tool_uses, validate_tool_pairing};
use super::super::websearch::{collect_history_tool_names, create_placeholder_tool};
#[allow(unused_imports)]
use crate::anthropic::types::ContentBlock as _;
use crate::anthropic::types::MessagesRequest;
#[allow(unused_imports)]
use crate::kiro::model::requests::conversation::Message;
use crate::kiro::model::requests::conversation::{
    AssistantMessage, HistoryAssistantMessage, HistoryUserMessage, UserInputMessageContext,
    UserMessage,
};
use crate::kiro::model::requests::tool::ToolResult;

#[test]
fn test_collect_history_tool_names() {
    use crate::kiro::model::requests::tool::ToolUseEntry;

    // 创建包含工具使用的历史消息
    let mut assistant_msg = AssistantMessage::new("I'll read the file.");
    assistant_msg = assistant_msg.with_tool_uses(vec![
        ToolUseEntry::new("tool-1", "read").with_input(serde_json::json!({"path": "/test.txt"})),
        ToolUseEntry::new("tool-2", "write").with_input(serde_json::json!({"path": "/out.txt"})),
    ]);

    let history = vec![
        Message::User(HistoryUserMessage::new(
            "Read the file",
            "claude-sonnet-4.5",
        )),
        Message::Assistant(HistoryAssistantMessage {
            assistant_response_message: assistant_msg,
        }),
    ];

    let tool_names = collect_history_tool_names(&history);
    assert_eq!(tool_names.len(), 2);
    assert!(tool_names.contains(&"read".to_string()));
    assert!(tool_names.contains(&"write".to_string()));
}

#[test]
fn test_create_placeholder_tool() {
    let tool = create_placeholder_tool("my_custom_tool");

    assert_eq!(tool.tool_specification.name, "my_custom_tool");
    assert!(!tool.tool_specification.description.is_empty());

    // 验证 JSON 序列化正确
    let json = serde_json::to_string(&tool).unwrap();
    assert!(json.contains("\"name\":\"my_custom_tool\""));
}

#[test]
fn test_history_tools_added_to_tools_list() {
    use crate::anthropic::types::Message as AnthropicMessage;

    // 创建一个请求，历史中有工具使用，但 tools 列表为空
    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Read the file"),
            },
            AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::json!([
                    {"type": "text", "text": "I'll read the file."},
                    {"type": "tool_use", "id": "tool-1", "name": "read", "input": {"path": "/test.txt"}}
                ]),
            },
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!([
                    {"type": "tool_result", "tool_use_id": "tool-1", "content": "file content"}
                ]),
            },
        ],
        stream: false,
        system: None,
        tools: None, // 没有提供工具定义
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };

    let result = convert_request(&req).unwrap();

    // 验证 tools 列表中包含了历史中使用的工具的占位符定义
    let tools = &result
        .conversation_state
        .current_message
        .user_input_message
        .user_input_message_context
        .tools;

    assert!(!tools.is_empty(), "tools 列表不应为空");
    assert!(
        tools.iter().any(|t| t.tool_specification.name == "read"),
        "tools 列表应包含 'read' 工具的占位符定义"
    );
}

#[test]
fn test_validate_tool_pairing_orphaned_result() {
    // 测试孤立的 tool_result 被过滤
    // 历史中没有 tool_use，但 tool_results 中有 tool_result
    let history = vec![
        Message::User(HistoryUserMessage::new("Hello", "claude-sonnet-4.5")),
        Message::Assistant(HistoryAssistantMessage::new("Hi there!")),
    ];

    let tool_results = vec![ToolResult::success("orphan-123", "some result")];

    let (filtered, _) = validate_tool_pairing(&history, &tool_results);

    // 孤立的 tool_result 应该被过滤掉
    assert!(filtered.is_empty(), "孤立的 tool_result 应该被过滤");
}

#[test]
fn test_validate_tool_pairing_orphaned_use() {
    use crate::kiro::model::requests::tool::ToolUseEntry;

    // 测试孤立的 tool_use（有 tool_use 但没有对应的 tool_result）
    let mut assistant_msg = AssistantMessage::new("I'll read the file.");
    assistant_msg = assistant_msg.with_tool_uses(vec![
        ToolUseEntry::new("tool-orphan", "read")
            .with_input(serde_json::json!({"path": "/test.txt"})),
    ]);

    let history = vec![
        Message::User(HistoryUserMessage::new(
            "Read the file",
            "claude-sonnet-4.5",
        )),
        Message::Assistant(HistoryAssistantMessage {
            assistant_response_message: assistant_msg,
        }),
    ];

    // 没有 tool_result
    let tool_results: Vec<ToolResult> = vec![];

    let (filtered, orphaned) = validate_tool_pairing(&history, &tool_results);

    // 结果应该为空（因为没有 tool_result）
    // 同时应该返回孤立的 tool_use_id
    assert!(filtered.is_empty());
    assert!(orphaned.contains("tool-orphan"));
}

#[test]
fn test_validate_tool_pairing_valid() {
    use crate::kiro::model::requests::tool::ToolUseEntry;

    // 测试正常配对的情况
    let mut assistant_msg = AssistantMessage::new("I'll read the file.");
    assistant_msg = assistant_msg.with_tool_uses(vec![
        ToolUseEntry::new("tool-1", "read").with_input(serde_json::json!({"path": "/test.txt"})),
    ]);

    let history = vec![
        Message::User(HistoryUserMessage::new(
            "Read the file",
            "claude-sonnet-4.5",
        )),
        Message::Assistant(HistoryAssistantMessage {
            assistant_response_message: assistant_msg,
        }),
    ];

    let tool_results = vec![ToolResult::success("tool-1", "file content")];

    let (filtered, orphaned) = validate_tool_pairing(&history, &tool_results);

    // 配对成功，应该保留，无孤立
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].tool_use_id, "tool-1");
    assert!(orphaned.is_empty());
}

#[test]
fn test_validate_tool_pairing_mixed() {
    use crate::kiro::model::requests::tool::ToolUseEntry;

    // 测试混合情况：部分配对成功，部分孤立
    let mut assistant_msg = AssistantMessage::new("I'll use two tools.");
    assistant_msg = assistant_msg.with_tool_uses(vec![
        ToolUseEntry::new("tool-1", "read").with_input(serde_json::json!({})),
        ToolUseEntry::new("tool-2", "write").with_input(serde_json::json!({})),
    ]);

    let history = vec![
        Message::User(HistoryUserMessage::new("Do something", "claude-sonnet-4.5")),
        Message::Assistant(HistoryAssistantMessage {
            assistant_response_message: assistant_msg,
        }),
    ];

    // tool_results: tool-1 配对，tool-3 孤立
    let tool_results = vec![
        ToolResult::success("tool-1", "result 1"),
        ToolResult::success("tool-3", "orphan result"), // 孤立
    ];

    let (filtered, orphaned) = validate_tool_pairing(&history, &tool_results);

    // 只有 tool-1 应该保留
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].tool_use_id, "tool-1");
    // tool-2 是孤立的 tool_use（无 result），tool-3 是孤立的 tool_result
    assert!(orphaned.contains("tool-2"));
}

#[test]
fn test_validate_tool_pairing_history_already_paired() {
    use crate::kiro::model::requests::tool::ToolUseEntry;

    // 测试历史中已配对的 tool_use 不应该被报告为孤立
    // 场景：多轮对话中，之前的 tool_use 已经在历史中有对应的 tool_result
    let mut assistant_msg1 = AssistantMessage::new("I'll read the file.");
    assistant_msg1 = assistant_msg1.with_tool_uses(vec![
        ToolUseEntry::new("tool-1", "read").with_input(serde_json::json!({"path": "/test.txt"})),
    ]);

    // 构建历史中的 user 消息，包含 tool_result
    let mut user_msg_with_result = UserMessage::new("", "claude-sonnet-4.5");
    let mut ctx = UserInputMessageContext::new();
    ctx = ctx.with_tool_results(vec![ToolResult::success("tool-1", "file content")]);
    user_msg_with_result = user_msg_with_result.with_context(ctx);

    let history = vec![
        // 第一轮：用户请求
        Message::User(HistoryUserMessage::new(
            "Read the file",
            "claude-sonnet-4.5",
        )),
        // 第一轮：assistant 使用工具
        Message::Assistant(HistoryAssistantMessage {
            assistant_response_message: assistant_msg1,
        }),
        // 第二轮：用户返回工具结果（历史中已配对）
        Message::User(HistoryUserMessage {
            user_input_message: user_msg_with_result,
        }),
        // 第二轮：assistant 响应
        Message::Assistant(HistoryAssistantMessage::new("The file contains...")),
    ];

    // 当前消息没有 tool_results（用户只是继续对话）
    let tool_results: Vec<ToolResult> = vec![];

    let (filtered, orphaned) = validate_tool_pairing(&history, &tool_results);

    // 结果应该为空，且不应该有孤立 tool_use
    // 因为 tool-1 已经在历史中配对了
    assert!(filtered.is_empty());
    assert!(orphaned.is_empty());
}

#[test]
fn test_validate_tool_pairing_duplicate_result() {
    use crate::kiro::model::requests::tool::ToolUseEntry;

    // 测试重复的 tool_result（历史中已配对，当前消息又发送了相同的 tool_result）
    let mut assistant_msg = AssistantMessage::new("I'll read the file.");
    assistant_msg = assistant_msg.with_tool_uses(vec![
        ToolUseEntry::new("tool-1", "read").with_input(serde_json::json!({"path": "/test.txt"})),
    ]);

    // 历史中已有 tool_result
    let mut user_msg_with_result = UserMessage::new("", "claude-sonnet-4.5");
    let mut ctx = UserInputMessageContext::new();
    ctx = ctx.with_tool_results(vec![ToolResult::success("tool-1", "file content")]);
    user_msg_with_result = user_msg_with_result.with_context(ctx);

    let history = vec![
        Message::User(HistoryUserMessage::new(
            "Read the file",
            "claude-sonnet-4.5",
        )),
        Message::Assistant(HistoryAssistantMessage {
            assistant_response_message: assistant_msg,
        }),
        Message::User(HistoryUserMessage {
            user_input_message: user_msg_with_result,
        }),
        Message::Assistant(HistoryAssistantMessage::new("Done")),
    ];

    // 当前消息又发送了相同的 tool_result（重复）
    let tool_results = vec![ToolResult::success("tool-1", "file content again")];

    let (filtered, _) = validate_tool_pairing(&history, &tool_results);

    // 重复的 tool_result 应该被过滤掉
    assert!(filtered.is_empty(), "重复的 tool_result 应该被过滤");
}

#[test]
fn test_convert_assistant_message_tool_use_only() {
    use crate::anthropic::types::Message as AnthropicMessage;

    // 测试仅包含 tool_use 的 assistant 消息（无 text 块）
    // Kiro API 要求 content 字段不能为空
    let msg = AnthropicMessage {
        role: "assistant".to_string(),
        content: serde_json::json!([
            {"type": "tool_use", "id": "toolu_01ABC", "name": "read_file", "input": {"path": "/test.txt"}}
        ]),
    };

    let result = convert_assistant_message(&msg).expect("应该成功转换");

    // 验证 content 不为空（使用占位符）
    assert!(
        !result.assistant_response_message.content.is_empty(),
        "content 不应为空"
    );
    assert_eq!(
        result.assistant_response_message.content, " ",
        "仅 tool_use 时应使用 ' ' 占位符"
    );

    // 验证 tool_uses 被正确保留
    let tool_uses = result
        .assistant_response_message
        .tool_uses
        .expect("应该有 tool_uses");
    assert_eq!(tool_uses.len(), 1);
    assert_eq!(tool_uses[0].tool_use_id, "toolu_01ABC");
    assert_eq!(tool_uses[0].name, "read_file");
}

#[test]
fn test_convert_assistant_message_with_text_and_tool_use() {
    use crate::anthropic::types::Message as AnthropicMessage;

    // 测试同时包含 text 和 tool_use 的 assistant 消息
    let msg = AnthropicMessage {
        role: "assistant".to_string(),
        content: serde_json::json!([
            {"type": "text", "text": "Let me read that file for you."},
            {"type": "tool_use", "id": "toolu_02XYZ", "name": "read_file", "input": {"path": "/data.json"}}
        ]),
    };

    let result = convert_assistant_message(&msg).expect("应该成功转换");

    // 验证 content 使用原始文本（不是占位符）
    assert_eq!(
        result.assistant_response_message.content,
        "Let me read that file for you."
    );

    // 验证 tool_uses 被正确保留
    let tool_uses = result
        .assistant_response_message
        .tool_uses
        .expect("应该有 tool_uses");
    assert_eq!(tool_uses.len(), 1);
    assert_eq!(tool_uses[0].tool_use_id, "toolu_02XYZ");
}

#[test]
fn test_remove_orphaned_tool_uses() {
    use crate::kiro::model::requests::tool::ToolUseEntry;

    // 测试从历史中移除孤立的 tool_use
    let mut assistant_msg = AssistantMessage::new("I'll use multiple tools.");
    assistant_msg = assistant_msg.with_tool_uses(vec![
        ToolUseEntry::new("tool-1", "read").with_input(serde_json::json!({})),
        ToolUseEntry::new("tool-2", "write").with_input(serde_json::json!({})),
        ToolUseEntry::new("tool-3", "delete").with_input(serde_json::json!({})),
    ]);

    let mut history = vec![
        Message::User(HistoryUserMessage::new("Do something", "claude-sonnet-4.5")),
        Message::Assistant(HistoryAssistantMessage {
            assistant_response_message: assistant_msg,
        }),
    ];

    // 移除 tool-1 和 tool-3
    let mut orphaned = std::collections::HashSet::new();
    orphaned.insert("tool-1".to_string());
    orphaned.insert("tool-3".to_string());

    remove_orphaned_tool_uses(&mut history, &orphaned);

    // 验证只剩下 tool-2
    if let Message::Assistant(ref assistant_msg) = history[1] {
        let tool_uses = assistant_msg
            .assistant_response_message
            .tool_uses
            .as_ref()
            .expect("应该还有 tool_uses");
        assert_eq!(tool_uses.len(), 1);
        assert_eq!(tool_uses[0].tool_use_id, "tool-2");
    } else {
        panic!("应该是 Assistant 消息");
    }
}

#[test]
fn test_remove_orphaned_tool_uses_all_removed() {
    use crate::kiro::model::requests::tool::ToolUseEntry;

    // 测试移除所有 tool_use 后，tool_uses 变为 None
    let mut assistant_msg = AssistantMessage::new("I'll use a tool.");
    assistant_msg = assistant_msg.with_tool_uses(vec![
        ToolUseEntry::new("tool-1", "read").with_input(serde_json::json!({})),
    ]);

    let mut history = vec![
        Message::User(HistoryUserMessage::new("Do something", "claude-sonnet-4.5")),
        Message::Assistant(HistoryAssistantMessage {
            assistant_response_message: assistant_msg,
        }),
    ];

    let mut orphaned = std::collections::HashSet::new();
    orphaned.insert("tool-1".to_string());

    remove_orphaned_tool_uses(&mut history, &orphaned);

    // 验证 tool_uses 变为 None
    if let Message::Assistant(ref assistant_msg) = history[1] {
        assert!(
            assistant_msg.assistant_response_message.tool_uses.is_none(),
            "移除所有 tool_use 后应为 None"
        );
    } else {
        panic!("应该是 Assistant 消息");
    }
}

#[test]
fn test_merge_consecutive_assistant_messages() {
    // 测试连续 assistant 消息被正确合并（Issue #79）
    use crate::anthropic::types::Message as AnthropicMessage;

    let msg1 = AnthropicMessage {
        role: "assistant".to_string(),
        content: serde_json::json!([
            {"type": "thinking", "thinking": "Let me think about this..."},
            {"type": "text", "text": " "}
        ]),
    };

    let msg2 = AnthropicMessage {
        role: "assistant".to_string(),
        content: serde_json::json!([
            {"type": "thinking", "thinking": "I should read the file."},
            {"type": "text", "text": "Let me read that file."},
            {"type": "tool_use", "id": "toolu_01ABC", "name": "read_file", "input": {"path": "/test.txt"}}
        ]),
    };

    let messages: Vec<&AnthropicMessage> = vec![&msg1, &msg2];
    let result = merge_assistant_messages(&messages).expect("合并应成功");

    let content = &result.assistant_response_message.content;
    // thinking 块在 convert_assistant_message 中被有意剥离，不应出现
    assert!(!content.contains("<thinking>"), "thinking 应被剥离");
    assert!(
        content.contains("Let me read that file"),
        "应包含第二条消息的 text 内容"
    );

    let tool_uses = result
        .assistant_response_message
        .tool_uses
        .expect("应有 tool_uses");
    assert_eq!(tool_uses.len(), 1);
    assert_eq!(tool_uses[0].tool_use_id, "toolu_01ABC");
}

#[test]
fn test_consecutive_assistant_with_tool_use_result_pairing() {
    // 测试 Issue #79 的完整场景
    use crate::anthropic::types::Message as AnthropicMessage;

    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Read the config file"),
            },
            AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::json!([
                    {"type": "thinking", "thinking": "I need to read the file..."},
                    {"type": "text", "text": " "}
                ]),
            },
            AnthropicMessage {
                role: "assistant".to_string(),
                content: serde_json::json!([
                    {"type": "thinking", "thinking": "Let me read the config."},
                    {"type": "text", "text": "I'll read the config file for you."},
                    {"type": "tool_use", "id": "toolu_01XYZ", "name": "read_file", "input": {"path": "/config.json"}}
                ]),
            },
            AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!([
                    {"type": "tool_result", "tool_use_id": "toolu_01XYZ", "content": "{\"key\": \"value\"}"}
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

    let result = convert_request(&req);
    assert!(
        result.is_ok(),
        "连续 assistant 消息场景不应报错: {:?}",
        result.err()
    );

    let state = result.unwrap().conversation_state;
    let mut found_tool_use = false;
    for msg in &state.history {
        if let Message::Assistant(assistant_msg) = msg
            && let Some(ref tool_uses) = assistant_msg.assistant_response_message.tool_uses
            && tool_uses.iter().any(|t| t.tool_use_id == "toolu_01XYZ")
        {
            found_tool_use = true;
            break;
        }
    }
    assert!(found_tool_use, "合并后的 assistant 消息应包含 tool_use");
}
