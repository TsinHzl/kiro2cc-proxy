//! converter 测试（自 tests.rs 拆分，纯代码搬移）
#![cfg(test)]

use super::super::convert::convert_request;
use super::super::thinking::generate_thinking_prefix;
#[allow(unused_imports)]
use crate::anthropic::types::ContentBlock as _;
use crate::anthropic::types::MessagesRequest;
#[allow(unused_imports)]
use crate::kiro::model::requests::conversation::Message;

#[test]
fn test_gpt_thinking_is_not_injected_into_history() {
    use crate::anthropic::types::{Message as AnthropicMessage, Metadata, SystemMessage, Thinking};

    let req = MessagesRequest {
        model: "gpt-5.6-luna".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: Some(vec![SystemMessage {
            text: "Follow the user request.".to_string(),
        }]),
        tools: None,
        tool_choice: None,
        thinking: Some(Thinking {
            thinking_type: "adaptive".to_string(),
            budget_tokens: 20000,
        }),
        output_config: None,
        metadata: Some(Metadata {
            user_id: Some("user_account__session_3a1f8d1e-6b0c-4a3e-8c1f-2b4d5e6f7a80".to_string()),
        }),
    };

    let result = convert_request(&req).unwrap();
    let Message::User(history_user) = &result.conversation_state.history[0] else {
        panic!("系统提示应转换为 history user 消息");
    };
    let content = &history_user.user_input_message.content;

    assert!(!content.contains("<thinking_mode>"));
    assert!(!content.contains("<thinking_effort>"));
    assert_eq!(
        result
            .additional_model_request_fields
            .as_ref()
            .and_then(|f| f["reasoning"]["effort"].as_str()),
        Some("high"),
        "gpt-5.6-luna + thinking 请求应生成 reasoning.effort=\"high\""
    );
    // 收窄修复：luna 在客户端请求 thinking 时，应注入反伪标签引导语，
    // 防止模型在缺乏结构化 thinking 协议约束且自身不产出推理内容时，
    // 自造 <analysis>/<summary> 等标签（该提示仅对 luna 生效，不含 terra/sol）。
    assert!(
        content.contains("pseudo-XML"),
        "GPT 模型 + thinking 请求应注入反伪标签引导语，实际内容: {content}"
    );
}

#[test]
fn test_gpt_anti_pseudo_tag_hint_not_injected_without_thinking_request() {
    // 客户端未请求 thinking 时，不应注入反伪标签引导语（避免污染日常请求的
    // 系统提示与 prompt cache key）。
    use crate::anthropic::types::{Message as AnthropicMessage, SystemMessage};

    let req = MessagesRequest {
        model: "gpt-5.6-luna".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: Some(vec![SystemMessage {
            text: "Follow the user request.".to_string(),
        }]),
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };

    let result = convert_request(&req).unwrap();
    let Message::User(history_user) = &result.conversation_state.history[0] else {
        panic!("系统提示应转换为 history user 消息");
    };
    let content = &history_user.user_input_message.content;

    assert!(!content.contains("pseudo-XML"));
}

#[test]
fn test_gpt_anti_pseudo_tag_hint_not_injected_when_thinking_explicitly_disabled() {
    // CR 修复回归测试：客户端可能显式传 `{"type": "disabled"}` 来关闭 thinking
    // （而非完全省略 thinking 字段）。此时 req.thinking.is_some() 为真但
    // is_enabled() 为假，必须与 resolve_thinking_enabled 判断口径一致，
    // 不应注入反伪标签引导语，否则会污染 prompt cache key 且语义矛盾
    // （一个明确要求不思考的请求却被当作"请求了思考"处理）。
    use crate::anthropic::types::{Message as AnthropicMessage, SystemMessage, Thinking};

    let req = MessagesRequest {
        model: "gpt-5.6-luna".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: Some(vec![SystemMessage {
            text: "Follow the user request.".to_string(),
        }]),
        tools: None,
        tool_choice: None,
        thinking: Some(Thinking {
            thinking_type: "disabled".to_string(),
            budget_tokens: 20000,
        }),
        output_config: None,
        metadata: None,
    };

    let result = convert_request(&req).unwrap();
    let Message::User(history_user) = &result.conversation_state.history[0] else {
        panic!("系统提示应转换为 history user 消息");
    };
    let content = &history_user.user_input_message.content;

    assert!(
        !content.contains("pseudo-XML"),
        "thinking.type=disabled 时不应注入反伪标签引导语，实际内容: {content}"
    );
}

#[test]
fn test_gpt_anti_pseudo_tag_hint_injected_for_luna_without_system_message() {
    // luna 没有传 system，但请求了 thinking：仍需插入反伪标签引导语，
    // 否则该场景下模型完全没有任何行为约束。
    use crate::anthropic::types::{Message as AnthropicMessage, Thinking};

    let req = MessagesRequest {
        model: "gpt-5.6-luna".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 20000,
        }),
        output_config: None,
        metadata: None,
    };

    let result = convert_request(&req).unwrap();
    let Message::User(history_user) = &result.conversation_state.history[0] else {
        panic!("无 system 时仍应插入反伪标签引导语作为 history user 消息");
    };
    let content = &history_user.user_input_message.content;

    assert!(content.contains("pseudo-XML"));
}

#[test]
fn test_gpt_anti_pseudo_tag_hint_not_injected_for_terra_or_sol() {
    // 范围收窄：terra/sol 没有 luna 那样的实测问题依据，即使客户端请求了
    // thinking，也不应注入反伪标签引导语（该提示仅为 luna 场景设计）。
    use crate::anthropic::types::{Message as AnthropicMessage, SystemMessage, Thinking};

    for model in ["gpt-5.6-terra", "gpt-5.6-sol"] {
        let req = MessagesRequest {
            model: model.to_string(),
            max_tokens: 1024,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Hello"),
            }],
            stream: false,
            system: Some(vec![SystemMessage {
                text: "Follow the user request.".to_string(),
            }]),
            tools: None,
            tool_choice: None,
            thinking: Some(Thinking {
                thinking_type: "enabled".to_string(),
                budget_tokens: 20000,
            }),
            output_config: None,
            metadata: None,
        };

        let result = convert_request(&req).unwrap();
        let Message::User(history_user) = &result.conversation_state.history[0] else {
            panic!("系统提示应转换为 history user 消息（model={model}）");
        };
        let content = &history_user.user_input_message.content;

        assert!(
            !content.contains("pseudo-XML"),
            "model={model} 不应注入反伪标签引导语，实际内容: {content}"
        );
    }
}

#[test]
fn test_non_gpt_model_thinking_request_no_anti_pseudo_tag_hint() {
    // 非 GPT 模型走 Claude/Kiro 结构化 thinking 协议，不应注入 GPT 专用的
    // 反伪标签引导语（该模型已有 <thinking_mode> 标签约束）。
    use crate::anthropic::types::{Message as AnthropicMessage, SystemMessage, Thinking};

    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: Some(vec![SystemMessage {
            text: "Follow the user request.".to_string(),
        }]),
        tools: None,
        tool_choice: None,
        thinking: Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 20000,
        }),
        output_config: None,
        metadata: None,
    };

    let result = convert_request(&req).unwrap();
    let Message::User(history_user) = &result.conversation_state.history[0] else {
        panic!("系统提示应转换为 history user 消息");
    };
    let content = &history_user.user_input_message.content;

    assert!(content.contains("<thinking_mode>"));
    assert!(!content.contains("pseudo-XML"));
}

#[test]
fn test_thinking_prefix_adaptive_effort_alignment() {
    // 回归测试（issue #40 CR #1）：generate_thinking_prefix 的 adaptive 分支
    // adaptive 模式不注入任何 thinking 标签（对齐 Kiro CLI 行为）。
    use crate::anthropic::types::{Message as AnthropicMessage, OutputConfig, Thinking};

    // 场景 1：adaptive + 无 output_config → None（不注入任何 thinking 标签）
    let req = MessagesRequest {
        model: "claude-sonnet-5".to_string(),
        max_tokens: 32000,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: Some(Thinking {
            thinking_type: "adaptive".to_string(),
            budget_tokens: 20000,
        }),
        output_config: None,
        metadata: None,
    };
    assert!(
        generate_thinking_prefix(&req, "claude-sonnet-5").is_none(),
        "adaptive 模式下不应注入任何 thinking 标签（对齐 Kiro CLI 直连行为）"
    );

    // 场景 2：adaptive + output_config.effort="medium" → 同样返回 None
    let req = MessagesRequest {
        model: "claude-sonnet-5".to_string(),
        max_tokens: 32000,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: Some(Thinking {
            thinking_type: "adaptive".to_string(),
            budget_tokens: 20000,
        }),
        output_config: Some(OutputConfig {
            effort: "medium".to_string(),
            format: None,
        }),
        metadata: None,
    };
    assert!(
        generate_thinking_prefix(&req, "claude-sonnet-5").is_none(),
        "adaptive 模式下不管有没有 output_config，都不应注入 thinking 标签"
    );

    // 场景 3：enabled thinking 不受影响，仍带 max_thinking_length
    let req = MessagesRequest {
        model: "claude-sonnet-5".to_string(),
        max_tokens: 32000,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 24576,
        }),
        output_config: None,
        metadata: None,
    };
    let prefix = generate_thinking_prefix(&req, "claude-sonnet-5").unwrap();
    assert!(prefix.contains("<max_thinking_length>24576</max_thinking_length>"));
}

#[test]
fn test_thinking_prefix_gpt_generation() {
    // 回归测试：GPT 系 thinking 前缀注入范围收窄至 luna。
    // 此前对全部 gpt-* 一刀切跳过，导致 sol/terra 在客户端请求 extended
    // thinking 时也永远不注入 <thinking_mode> 标签，thinking 恒不生效。
    use crate::anthropic::types::{Message as AnthropicMessage, Thinking};

    let mk_req = || MessagesRequest {
        model: "gpt-5.6-terra".to_string(),
        max_tokens: 32000,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("Hello"),
        }],
        stream: false,
        system: None,
        tools: None,
        tool_choice: None,
        thinking: Some(Thinking {
            thinking_type: "enabled".to_string(),
            budget_tokens: 24576,
        }),
        output_config: None,
        metadata: None,
    };

    // sol / terra：enabled thinking 应照常注入 <thinking_mode> 文本协议标签
    for model in ["gpt-5.6-terra", "gpt-5.6-sol"] {
        let prefix = generate_thinking_prefix(&mk_req(), model).unwrap_or_else(|| {
            panic!("{model} 在 enabled thinking 下应注入 thinking 前缀");
        });
        assert!(
            prefix.contains("<thinking_mode>enabled</thinking_mode>"),
            "{model} 注入的前缀应包含 thinking_mode 标签，实际: {prefix}"
        );
    }

    // luna：已知上游恒返回 thinking=0 且不支持该协议，仍保持跳过
    assert!(
        generate_thinking_prefix(&mk_req(), "gpt-5.6-luna").is_none(),
        "luna 不支持 thinking 文本协议，应维持跳过"
    );
}
