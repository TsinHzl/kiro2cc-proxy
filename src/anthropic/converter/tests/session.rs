//! converter 测试（自 tests.rs 拆分，纯代码搬移）
#![cfg(test)]

use super::super::convert::{
    convert_request, determine_agent_task_type, determine_chat_trigger_type,
};
use super::super::session::{
    derive_fallback_conversation_id, extract_session_id, is_compact_request, is_valid_uuid,
};
use super::fallback_req;
#[allow(unused_imports)]
use crate::anthropic::types::ContentBlock as _;
use crate::anthropic::types::MessagesRequest;
#[allow(unused_imports)]
use crate::kiro::model::requests::conversation::Message;

#[test]
fn test_determine_chat_trigger_type() {
    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };
    assert_eq!(determine_chat_trigger_type(&req), "MANUAL");
}

#[test]
fn test_determine_agent_task_type_no_tools() {
    use crate::anthropic::types::Message as AnthropicMessage;
    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("hi"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };
    assert_eq!(determine_agent_task_type(&req), "vibe");
}

#[test]
fn test_determine_agent_task_type_code_tools() {
    use crate::anthropic::types::{Message as AnthropicMessage, Tool};
    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("hi"),
        }],
        stream: false,
        system: None,
        tools: Some(vec![
            Tool {
                tool_type: None,
                name: "Read".to_string(),
                description: "Read a file".to_string(),
                input_schema: Default::default(),
                max_uses: None,
                defer_loading: None,
            },
            Tool {
                tool_type: None,
                name: "Write".to_string(),
                description: "Write a file".to_string(),
                input_schema: Default::default(),
                max_uses: None,
                defer_loading: None,
            },
        ]),
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };
    assert_eq!(determine_agent_task_type(&req), "spectask");
}

#[test]
fn test_determine_agent_task_type_non_code_tools() {
    use crate::anthropic::types::{Message as AnthropicMessage, Tool};
    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("hi"),
        }],
        stream: false,
        system: None,
        tools: Some(vec![Tool {
            tool_type: None,
            name: "calculator".to_string(),
            description: "Do math".to_string(),
            input_schema: Default::default(),
            max_uses: None,
            defer_loading: None,
        }]),
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };
    assert_eq!(determine_agent_task_type(&req), "spectask");
}

#[test]
fn test_determine_agent_task_type_bash_tool() {
    use crate::anthropic::types::{Message as AnthropicMessage, Tool};
    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("hi"),
        }],
        stream: false,
        system: None,
        tools: Some(vec![Tool {
            tool_type: None,
            name: "Bash".to_string(),
            description: "Run bash".to_string(),
            input_schema: Default::default(),
            max_uses: None,
            defer_loading: None,
        }]),
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };
    assert_eq!(determine_agent_task_type(&req), "spectask");
}

#[test]
fn test_extract_session_id_pure_uuid_passthrough() {
    // 纯 UUID 直通：OpenAI user 字段透传场景，显式声明的会话身份直接采用
    // （旧实现返回 None → fallback 派生，透传形同虚设）
    let user_id = "8bb5523b-ec7c-4540-a9ca-beb6d79f1552";
    assert_eq!(
        extract_session_id(user_id),
        Some("8bb5523b-ec7c-4540-a9ca-beb6d79f1552".to_string())
    );

    // 大写 hex 的 UUID 也应直通（归一为小写 v4 形态）
    let upper = "8BB5523B-EC7C-4540-A9CA-BEB6D79F1552";
    assert_eq!(
        extract_session_id(upper),
        Some("8bb5523b-ec7c-4540-a9ca-beb6d79f1552".to_string())
    );

    // 非 v4 形态 UUID（Version 位非 4）→ 直通且归一为 v4，防上游严格解析器 400
    let v4 = extract_session_id("8bb5523b-ec7c-4540-a9ca-beb6d79f1552").expect("合法 UUID 应直通");
    assert_eq!(v4, "8bb5523b-ec7c-4540-a9ca-beb6d79f1552");
    let mut v1_bytes = *uuid::Uuid::parse_str("8bb5523b-ec7c-4540-a9ca-beb6d79f1552")
        .expect("测试锚点 UUID 应可解析")
        .as_bytes();
    // 构造 v1 形态：Version=1、Variant=RFC4122
    v1_bytes[6] = (v1_bytes[6] & 0x0f) | 0x10;
    v1_bytes[8] = (v1_bytes[8] & 0x3f) | 0x80;
    let v1 = uuid::Uuid::from_bytes(v1_bytes).to_string();
    let normalized = extract_session_id(&v1).expect("v1 形态合法 UUID 应直通");
    let n = uuid::Uuid::parse_str(&normalized).expect("归一结果应可解析");
    assert_eq!(n.get_version_num(), 4, "归一后 Version 应为 4");
    assert_eq!(
        n.get_variant(),
        uuid::Variant::RFC4122,
        "归一后 Variant 应为 RFC4122"
    );

    // 非 UUID 的普通值（OpenAI user-123、邮箱等）仍走 fallback，不误直通
    assert_eq!(extract_session_id("user-123"), None);
    assert_eq!(extract_session_id("user@example.com"), None);
}

#[test]
fn test_extract_session_id_valid() {
    // 标准格式: user_xxx_account__session_UUID
    let user_id = "user_0dede55c6dcc4a11a30bbb5e7f22e6fdf86cdeba3820019cc27612af4e1243cd_account__session_8bb5523b-ec7c-4540-a9ca-beb6d79f1552";
    let session_id = extract_session_id(user_id);
    assert_eq!(
        session_id,
        Some("8bb5523b-ec7c-4540-a9ca-beb6d79f1552".to_string())
    );
}

#[test]
fn test_extract_session_id_json_format() {
    // JSON 格式: {"session_id":"UUID"} — Claude Code 2.1.128+ 实际发送的格式
    let user_id = r#"{"session_id":"3d69af26-0a80-483f-baa0-b4ccaaa07e81"}"#;
    let session_id = extract_session_id(user_id);
    assert_eq!(
        session_id,
        Some("3d69af26-0a80-483f-baa0-b4ccaaa07e81".to_string())
    );
}

#[test]
fn test_extract_session_id_json_id_field() {
    // JSON 格式: {"id":"UUID"} — 备用字段名
    let user_id = r#"{"id":"3d69af26-0a80-483f-baa0-b4ccaaa07e81"}"#;
    let session_id = extract_session_id(user_id);
    assert_eq!(
        session_id,
        Some("3d69af26-0a80-483f-baa0-b4ccaaa07e81".to_string())
    );
}

#[test]
fn test_extract_session_id_json_pollution_rejected() {
    // 旧版 bug：session_id":"xxx 被误识别为合法 UUID，现在应该被拒绝
    // 因为 "id\":" 包含非 hex 字符 '"' 和 ':'
    let user_id = r#"{"session_id":"3d69af26-0a80-483f-baa0-b4ccaaa07e81"}"#;
    let result = extract_session_id(user_id);
    // 应该通过 JSON 路径正确提取，而不是通过污染路径
    assert_eq!(
        result,
        Some("3d69af26-0a80-483f-baa0-b4ccaaa07e81".to_string())
    );
    // 验证污染值本身不是合法 UUID
    assert!(!is_valid_uuid(r#"id":"3d69af26-0a80-483f-baa0-b4ccaaa"#));
}

#[test]
fn test_extract_session_id_no_session() {
    // 没有 session 的 user_id
    let user_id = "user_0dede55c6dcc4a11a30bbb5e7f22e6fdf86cdeba3820019cc27612af4e1243cd";
    let session_id = extract_session_id(user_id);
    assert_eq!(session_id, None);
}

#[test]
fn test_extract_session_id_invalid_uuid() {
    // 无效的 UUID 格式
    let user_id = "user_xxx_session_invalid-uuid";
    let session_id = extract_session_id(user_id);
    assert_eq!(session_id, None);
}

#[test]
fn test_extract_session_id_non_ascii_no_panic() {
    // 回归：session_ 之后第 36 字节落在多字节 UTF-8 字符中间，
    // 旧实现 &session_part[..36] 会 panic，新实现应安全返回 None
    let user_id = format!("session_{}", "中".repeat(20));
    assert_eq!(extract_session_id(&user_id), None);

    // session_ 后内容短于 36 字节也不应 panic
    assert_eq!(extract_session_id("session_短"), None);
}

/// 构造用于 fallback 派生测试的最小请求（system + 工具名 + 消息序列）

#[test]
fn test_derive_fallback_distinguishes_different_sessions() {
    // 回归 issue #27：system 与工具集完全相同的两个会话，
    // 旧实现会折叠成同一个 conversationId，导致 sticky 把全部流量钉在同一账号上
    let a = fallback_req(
        Some("You are Claude Code."),
        &["Read", "Write"],
        &[("user", "帮我看下 main.rs")],
    );
    let b = fallback_req(
        Some("You are Claude Code."),
        &["Read", "Write"],
        &[("user", "帮我重构 token_manager")],
    );
    let id_a = derive_fallback_conversation_id(&a).expect("应派生出 ID");
    let id_b = derive_fallback_conversation_id(&b).expect("应派生出 ID");
    assert_ne!(id_a, id_b, "不同会话必须派生出不同的 conversationId");
}

#[test]
fn test_derive_fallback_stable_across_turns() {
    // 同一会话的后续轮次追加历史消息，首条消息不变 → conversationId 必须保持稳定，
    // 否则每轮都会重新绑定账号，sticky 与上游 prompt cache 全部失效
    let turn1 = fallback_req(
        Some("You are Claude Code."),
        &["Read", "Write"],
        &[("user", "帮我看下 main.rs")],
    );
    let turn3 = fallback_req(
        Some("You are Claude Code."),
        &["Read", "Write"],
        &[
            ("user", "帮我看下 main.rs"),
            ("assistant", "已读取"),
            ("user", "再看下 lib.rs"),
        ],
    );
    assert_eq!(
        derive_fallback_conversation_id(&turn1),
        derive_fallback_conversation_id(&turn3),
        "同一会话跨轮次的 conversationId 必须一致"
    );
}

#[test]
fn test_derive_fallback_array_content_ignores_binary_blocks() {
    // 数组型 content：只有顶层 text 块参与 seed，image 的 base64 数据不参与
    let with_image = |data: &str, text: &str| {
        let mut req = fallback_req(Some("You are Claude Code."), &["Read"], &[("user", text)]);
        req.messages[0].content = serde_json::json!([
            {"type": "text", "text": text},
            {"type": "image", "source": {
                "type": "base64", "media_type": "image/png", "data": data
            }},
        ]);
        req
    };
    // 文本相同、仅图片数据不同 → 判为同一会话
    assert_eq!(
        derive_fallback_conversation_id(&with_image("AAAA", "看这张图")),
        derive_fallback_conversation_id(&with_image("BBBB", "看这张图")),
        "仅附件不同不应改变会话身份"
    );
    // 文本不同 → 判为不同会话
    assert_ne!(
        derive_fallback_conversation_id(&with_image("AAAA", "看这张图")),
        derive_fallback_conversation_id(&with_image("AAAA", "换个问题")),
        "数组型 content 的文本变化必须体现在会话 ID 上"
    );
}

#[test]
fn test_derive_fallback_bare_request_uses_first_message() {
    // 无 system 也无工具的裸请求改用首条消息派生稳定 ID：
    // 上游 prompt cache 依赖跨轮会话身份稳定，实测可省约 38% credits
    // （旧实现返回 None 退化为随机 UUID，导致裸请求每轮全价）
    let req = fallback_req(None, &[], &[("user", "Hello")]);
    let id = derive_fallback_conversation_id(&req).expect("裸请求应派生出 ID");
    // 派生结果必须是合法 UUID v4（供上游严格解析器接受）
    assert!(is_valid_uuid(&id), "派生 ID 应为合法 UUID: {id}");
    // 与带 system 的派生结果不同（seed 成分不同）
    let with_system = fallback_req(Some("You are Claude Code."), &[], &[("user", "Hello")]);
    assert_ne!(
        derive_fallback_conversation_id(&with_system),
        Some(id),
        "裸请求与带 system 的请求应派生不同 ID"
    );
}

#[test]
fn test_derive_fallback_bare_request_stable_across_turns() {
    // 裸请求同一会话跨轮：首条消息不变 → conversationId 稳定
    let turn1 = fallback_req(None, &[], &[("user", "Hello")]);
    let turn2 = fallback_req(
        None,
        &[],
        &[("user", "Hello"), ("assistant", "Hi"), ("user", "继续")],
    );
    assert_eq!(
        derive_fallback_conversation_id(&turn1),
        derive_fallback_conversation_id(&turn2),
        "裸请求同一会话跨轮次必须派生相同 conversationId"
    );
    // 不同首条消息 → 不同 ID（低熵折叠风险收窄到「首条消息完全一致」）
    let other = fallback_req(None, &[], &[("user", "hi")]);
    assert_ne!(
        derive_fallback_conversation_id(&turn1),
        derive_fallback_conversation_id(&other),
        "不同首条消息的裸请求必须派生不同 conversationId"
    );
}

#[test]
fn test_agent_continuation_id_stable_within_session() {
    use crate::anthropic::types::{Message as AnthropicMessage, Metadata};

    let session_uuid = "a0662283-7fd3-4399-a7eb-52b9a717ae88";
    let user_id = format!(
        "user_0dede55c6dcc4a11a30bbb5e7f22e6fdf86cdeba3820019cc27612af4e1243cd_account__session_{}",
        session_uuid
    );

    let make_req = || MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: Some(Metadata {
            user_id: Some(user_id.clone()),
        }),
    };

    let result1 = convert_request(&make_req()).unwrap();
    let result2 = convert_request(&make_req()).unwrap();

    assert_eq!(
        result1.conversation_state.agent_continuation_id,
        result2.conversation_state.agent_continuation_id,
        "同一 session 的 agentContinuationId 应该稳定"
    );

    assert_eq!(
        result1.conversation_state.conversation_id,
        result2.conversation_state.conversation_id,
    );
}

#[test]
fn test_agent_continuation_id_differs_across_sessions() {
    use crate::anthropic::types::{Message as AnthropicMessage, Metadata};

    let make_req = |session_uuid: &str| {
        let user_id = format!(
            "user_0dede55c6dcc4a11a30bbb5e7f22e6fdf86cdeba3820019cc27612af4e1243cd_account__session_{}",
            session_uuid
        );
        MessagesRequest {
            model: "claude-sonnet-4".to_string(),
            max_tokens: 1024,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Hello"),
            }],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: Some(Metadata {
                user_id: Some(user_id),
            }),
        }
    };

    let result1 = convert_request(&make_req("a0662283-7fd3-4399-a7eb-52b9a717ae88")).unwrap();
    let result2 = convert_request(&make_req("b1773394-8ge4-4400-b8fc-63c0b828bf99")).unwrap();

    assert_ne!(
        result1.conversation_state.agent_continuation_id,
        result2.conversation_state.agent_continuation_id,
        "不同 session 的 agentContinuationId 应该不同"
    );
}

#[test]
fn test_agent_continuation_id_stable_for_bare_request_without_metadata() {
    use crate::anthropic::types::Message as AnthropicMessage;

    let make_req = || MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };

    let result1 = convert_request(&make_req()).unwrap();
    let result2 = convert_request(&make_req()).unwrap();

    // 裸请求不再退化为随机 UUID：fallback 按首条消息派生稳定 conversationId，
    // agentContinuationId 随之稳定，同一会话跨轮可命中上游 prompt cache
    assert_eq!(
        result1.conversation_state.conversation_id,
        result2.conversation_state.conversation_id,
    );
    assert_eq!(
        result1.conversation_state.agent_continuation_id,
        result2.conversation_state.agent_continuation_id,
        "裸请求无 metadata 时 agentContinuationId 应随 fallback 派生保持稳定"
    );
}

#[test]
fn test_convert_request_with_session_metadata() {
    use crate::anthropic::types::{Message as AnthropicMessage, Metadata};

    // 测试带有 metadata 的请求，应该使用 session UUID 作为 conversationId
    let req = MessagesRequest {
            model: "claude-sonnet-4".to_string(),
            max_tokens: 1024,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Hello"),
            }],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: Some(Metadata {
                user_id: Some(
                    "user_0dede55c6dcc4a11a30bbb5e7f22e6fdf86cdeba3820019cc27612af4e1243cd_account__session_a0662283-7fd3-4399-a7eb-52b9a717ae88".to_string(),
                ),
            }),
        };

    let result = convert_request(&req).unwrap();
    assert_eq!(
        result.conversation_state.conversation_id,
        "a0662283-7fd3-4399-a7eb-52b9a717ae88"
    );
}

#[test]
fn test_convert_request_without_metadata() {
    use crate::anthropic::types::Message as AnthropicMessage;

    // 测试没有 metadata 的请求，应该生成新的 UUID
    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };

    let result = convert_request(&req).unwrap();
    // 验证生成的是有效的 UUID 格式
    assert_eq!(result.conversation_state.conversation_id.len(), 36);
    assert_eq!(
        result
            .conversation_state
            .conversation_id
            .chars()
            .filter(|c| *c == '-')
            .count(),
        4
    );
}

#[test]
fn test_is_compact_request_manual_slash_compact() {
    use crate::anthropic::types::Message as AnthropicMessage;

    let messages = vec![AnthropicMessage {
        role: "user".to_string(),
        content: serde_json::json!("/compact"),
    }];
    assert!(is_compact_request(&messages));

    // 带额外参数的 /compact 调用也应命中
    let messages = vec![AnthropicMessage {
        role: "user".to_string(),
        content: serde_json::json!("/compact focus on the last decision"),
    }];
    assert!(is_compact_request(&messages));
}

#[test]
fn test_is_compact_request_reactive_compact_prompt() {
    use crate::anthropic::types::Message as AnthropicMessage;

    // Claude Code v2.1+ auto-compact / 手动 /compact 均会发送这段合成摘要提示词
    let messages = vec![AnthropicMessage {
        role: "user".to_string(),
        content: serde_json::json!(
            "CRITICAL: Respond with TEXT ONLY. Do not call any tools. \
                 Create a detailed summary of this conversation, focusing on \
                 information that would be helpful for continuing the conversation."
        ),
    }];
    assert!(is_compact_request(&messages));
}

#[test]
fn test_is_compact_request_reactive_prompt_case_insensitive() {
    use crate::anthropic::types::Message as AnthropicMessage;

    // Claude Code 不同版本大小写不一致（"Do NOT" vs "do not"），检测应忽略大小写
    let messages = vec![AnthropicMessage {
        role: "user".to_string(),
        content: serde_json::json!(
            "Critical: Respond With Text Only. Create A Detailed Summary \
                 Of The Conversation so far."
        ),
    }];
    assert!(is_compact_request(&messages));
}

#[test]
fn test_is_compact_request_content_block_array() {
    use crate::anthropic::types::Message as AnthropicMessage;

    // content 为 content block 数组（而非纯字符串）时也应正确检测
    let messages = vec![AnthropicMessage {
        role: "user".to_string(),
        content: serde_json::json!([
            {"type": "text", "text": "/compact"}
        ]),
    }];
    assert!(is_compact_request(&messages));
}

#[test]
fn test_is_compact_request_normal_request_not_flagged() {
    use crate::anthropic::types::Message as AnthropicMessage;

    // 普通请求，包括恰好提到"总结"、"conversation"但不满足完整特征组合的场景
    let messages = vec![
        AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("帮我总结一下这段代码的逻辑"),
        },
        AnthropicMessage {
            role: "assistant".to_string(),
            content: serde_json::json!("这段代码做了……"),
        },
        AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!(
                "Can you create a detailed summary of the new feature design?"
            ),
        },
    ];
    assert!(!is_compact_request(&messages));
}

#[test]
fn test_is_compact_request_only_checks_last_user_turn() {
    use crate::anthropic::types::Message as AnthropicMessage;

    // 更早的历史消息包含压缩提示词特征，但已被一条 assistant 回复终结，
    // 当前这一轮是普通请求，不应被历史误判为压缩。
    let messages = vec![
        AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!(
                "CRITICAL: Respond with TEXT ONLY. Create a detailed summary \
                     of this conversation."
            ),
        },
        AnthropicMessage {
            role: "assistant".to_string(),
            content: serde_json::json!("Here is the summary..."),
        },
        AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("继续帮我写一下这个功能的测试用例"),
        },
    ];
    assert!(!is_compact_request(&messages));
}

#[test]
fn test_is_compact_request_empty_messages() {
    let messages: Vec<crate::anthropic::types::Message> = vec![];
    assert!(!is_compact_request(&messages));
}
