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

// ---- 动态块分流出 history[0]，保持前缀缓存稳定 ----

fn dynamic_split_req(model: &str, system: Vec<&str>, user: &str) -> MessagesRequest {
    use crate::anthropic::types::{Message as AnthropicMessage, SystemMessage};
    MessagesRequest {
        model: model.to_string(),
        max_tokens: 2048,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!(user),
        }],
        stream: false,
        system: Some(
            system
                .into_iter()
                .map(|t| SystemMessage {
                    text: t.to_string(),
                })
                .collect(),
        ),
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    }
}

fn history0_and_current(req: &MessagesRequest) -> (String, String) {
    let r = convert_request(req).unwrap();
    let h0 = serde_json::to_value(&r.conversation_state.history[0]).unwrap()
        ["userInputMessage"]["content"]
        .as_str()
        .unwrap()
        .to_string();
    let cur = r
        .conversation_state
        .current_message
        .user_input_message
        .content
        .clone();
    (h0, cur)
}

const STABLE: &str = "STABLE RULES";
const HOOK: &str = "UserPromptSubmit hook success: Session status updated.";
const DEFERRED: &str = "The following deferred tools are now available via ToolSearch.\nFoo";

#[test]
fn gpt_dynamic_blocks_leave_history0_and_go_to_current_message() {
    let (h0, cur) = history0_and_current(&dynamic_split_req(
        "gpt-5.6-terra",
        vec![STABLE, HOOK, DEFERRED],
        "hi",
    ));
    assert_eq!(h0, STABLE, "history[0] 只应含稳定系统内容");
    assert!(cur.ends_with("hi"), "用户原文必须在最后（指令居末）: {cur}");
    assert!(cur.starts_with("<system-reminder>"), "{cur}");
    assert!(cur.contains(HOOK) && cur.contains(DEFERRED));
}

#[test]
fn gpt_history0_stable_across_turns_with_accumulating_dynamic_blocks() {
    let (h0_a, _) = history0_and_current(&dynamic_split_req(
        "gpt-5.6-terra",
        vec![STABLE, HOOK],
        "turn1",
    ));
    let (h0_b, _) = history0_and_current(&dynamic_split_req(
        "gpt-5.6-terra",
        vec![STABLE, HOOK, DEFERRED, HOOK, HOOK],
        "turn2",
    ));
    assert_eq!(h0_a, h0_b, "动态块累积不得改变 history[0]");
}

#[test]
fn gpt_all_dynamic_system_keeps_thinking_prefix() {
    use crate::anthropic::types::Thinking;
    let mut req = dynamic_split_req("gpt-5.6-sol", vec![HOOK], "hi");
    req.thinking = Some(Thinking {
        thinking_type: "enabled".to_string(),
        budget_tokens: 1000,
    });
    let (h0, cur) = history0_and_current(&req);
    assert!(
        h0.contains("<thinking_mode>enabled</thinking_mode>"),
        "{h0}"
    );
    assert!(!h0.contains(HOOK));
    assert!(cur.contains(HOOK));
}

#[test]
fn claude_dynamic_blocks_leave_history0_and_go_to_current_message() {
    // issue #47：非 GPT 模型同样需要分流，否则累积的 hook / 通知块使 history[0] 逐轮漂移
    for model in ["claude-sonnet-4-6", "claude-opus-5-5", "deepseek-3.2"] {
        let (h0, cur) = history0_and_current(&dynamic_split_req(
            model,
            vec![STABLE, HOOK, DEFERRED],
            "hi",
        ));
        assert_eq!(h0, STABLE, "{model}: history[0] 只应含稳定系统内容");
        assert!(cur.starts_with("<system-reminder>"), "{model}: {cur}");
        assert!(cur.ends_with("hi"), "{model}: {cur}");
        assert!(cur.contains(HOOK) && cur.contains(DEFERRED), "{model}");
    }
}

#[test]
fn claude_history0_stable_across_turns_with_accumulating_dynamic_blocks() {
    let (h0_a, _) = history0_and_current(&dynamic_split_req(
        "claude-opus-5-5",
        vec![STABLE, HOOK],
        "turn1",
    ));
    let (h0_b, _) = history0_and_current(&dynamic_split_req(
        "claude-opus-5-5",
        vec![STABLE, HOOK, DEFERRED, HOOK, HOOK],
        "turn2",
    ));
    assert_eq!(h0_a, h0_b, "动态块累积不得改变 history[0]");
}

#[test]
fn claude_without_dynamic_blocks_keeps_history0_and_current_unchanged() {
    // 请求不含动态块时与分流逻辑无关：history[0] 为完整 system 按 "\n" 拼接，
    // 当前消息不被追加任何内容
    let (h0, cur) = history0_and_current(&dynamic_split_req(
        "claude-opus-4-6",
        vec![STABLE, "Second stable block"],
        "hi",
    ));
    assert_eq!(h0, format!("{STABLE}\nSecond stable block"));
    assert_eq!(cur, "hi");
}

#[test]
fn claude_empty_stable_block_with_dynamic_keeps_thinking_prefix() {
    // CR 回归：稳定块仅剩空文本时，分流后须按"无系统消息"处理，thinking 前缀不能丢
    use crate::anthropic::types::Thinking;
    let mut req = dynamic_split_req("claude-opus-5-5", vec!["", HOOK], "hi");
    req.thinking = Some(Thinking {
        thinking_type: "enabled".to_string(),
        budget_tokens: 1000,
    });
    let (h0, cur) = history0_and_current(&req);
    assert!(
        h0.contains("<thinking_mode>enabled</thinking_mode>"),
        "{h0}"
    );
    assert!(!h0.contains(HOOK));
    assert!(cur.contains(HOOK));
}

#[test]
fn dynamic_hook_predicate_matches_expected_shapes() {
    use super::super::prompt::is_dynamic_hook_injection as p;
    for yes in [
        "UserPromptSubmit hook success: x",
        "  PreToolUse hook additional context: x",
        "PreToolUse:Bash hook success: x",
        "PostToolUse:Edit hook success: x",
        "Stop hook feedback: x",
        "The following deferred tools are now available via ToolSearch.",
        "The following MCP servers are still connecting — x",
    ] {
        assert!(p(yes), "应识别为动态块: {yes}");
    }
    for no in [
        "STABLE RULES",
        "SessionStart hook additional context: <EXTREMELY_IMPORTANT>",
        "SessionStart:startup hook success: x",
        "Stopping hook is not a hook block",
        "UserPromptSubmitter hook x",
    ] {
        assert!(!p(no), "不应识别为动态块: {no}");
    }
}

#[test]
fn token_budget_reminder_predicate_matches_expected_shapes() {
    use super::super::prompt::is_token_budget_reminder as p;
    for yes in [
        "<total_tokens>15000000 tokens left</total_tokens>",
        "  <total_tokens>14957542 tokens left</total_tokens>\n",
        "<total_tokens></total_tokens>",
    ] {
        assert!(p(yes), "应识别为预算提示: {yes}");
    }
    for no in [
        "",
        "STABLE RULES",
        // 仅以该标签开头但还带有其他内容的真实系统提示不得被误分流
        "<total_tokens>1 tokens left</total_tokens>\nAlways answer in Chinese.",
        "Note: <total_tokens>1 tokens left</total_tokens>",
        "<total_tokens>1 <b>tokens</b> left</total_tokens>",
        "<total_tokens>1 tokens left",
    ] {
        assert!(!p(no), "不应识别为预算提示: {no}");
    }
}

/// 带逐轮累积的内联 `<total_tokens>` system 消息的多轮请求（模拟 Claude Code 的真实形态）
fn budget_req(model: &str, top_system: Option<&str>, budget_msgs: usize) -> MessagesRequest {
    use crate::anthropic::types::{Message as AnthropicMessage, Metadata, SystemMessage};
    let mut messages = vec![
        AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("turn1"),
        },
        AnthropicMessage {
            role: "assistant".to_string(),
            content: serde_json::json!("ok"),
        },
    ];
    for i in 0..budget_msgs {
        messages.push(AnthropicMessage {
            role: "system".to_string(),
            content: serde_json::json!(format!(
                "<total_tokens>{} tokens left</total_tokens>",
                15_000_000 - i * 2691
            )),
        });
    }
    messages.push(AnthropicMessage {
        role: "user".to_string(),
        content: serde_json::json!("turn2"),
    });
    MessagesRequest {
        model: model.to_string(),
        max_tokens: 2048,
        messages,
        stream: false,
        system: top_system.map(|t| {
            vec![SystemMessage {
                text: t.to_string(),
            }]
        }),
        tools: None,
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: Some(Metadata {
            user_id: Some(
                "user_x_account__session_0b1c2d3e-4f50-4a6b-8c7d-9e0f1a2b3c4d".to_string(),
            ),
        }),
    }
}

#[test]
fn token_budget_system_messages_leave_history0_and_go_to_current_message() {
    // 预算提示逐轮累积：0 条 / 1 条 / 5 条下 history[0] 必须完全一致（缓存稳定），
    // 预算提示出现在当前消息开头的 <system-reminder> 中（模型仍可见），不留在历史里
    for model in ["claude-opus-5-5", "claude-opus-4-6", "gpt-5.6-sol"] {
        let (h0_base, cur_base) = history0_and_current(&budget_req(model, Some(STABLE), 0));
        assert_eq!(cur_base, "turn2", "{model}: 无预算提示时当前消息不变");
        for n in [1, 5] {
            let r = convert_request(&budget_req(model, Some(STABLE), n)).unwrap();
            let hist = serde_json::to_string(&r.conversation_state.history).unwrap();
            assert!(!hist.contains("total_tokens"), "{model} n={n}: {hist}");
            let (h0, cur) = history0_and_current(&budget_req(model, Some(STABLE), n));
            assert_eq!(h0, h0_base, "{model} n={n}: 预算提示不得影响 history[0]");
            assert!(
                cur.starts_with("<system-reminder>\n"),
                "{model} n={n}: {cur}"
            );
            assert!(cur.ends_with("\n\nturn2"), "{model} n={n}: {cur}");
            assert_eq!(cur.matches("<total_tokens>").count(), n, "{model}: {cur}");
        }
    }
}

#[test]
fn token_budget_only_inline_system_is_treated_as_no_system() {
    // 没有顶层 system、内联 system 全是预算提示：稳定区为空，按"无系统消息"处理
    // （history[0] 与完全没有 system 时一致，预算提示只出现在当前消息）
    let (h0_base, _) = history0_and_current(&budget_req("claude-opus-5-5", None, 0));
    let (h0, cur) = history0_and_current(&budget_req("claude-opus-5-5", None, 3));
    assert_eq!(h0, h0_base);
    assert!(
        cur.contains("<total_tokens>") && cur.ends_with("turn2"),
        "{cur}"
    );
}

#[test]
fn token_budget_lookalike_system_message_is_kept() {
    // 以预算标签开头但还带有其他内容的内联 system 消息是真实指令，必须留在 history[0]
    use crate::anthropic::types::Message as AnthropicMessage;
    let mut req = budget_req("claude-opus-5-5", Some(STABLE), 0);
    let last = req.messages.len() - 1;
    req.messages.insert(
        last,
        AnthropicMessage {
            role: "system".to_string(),
            content: serde_json::json!(
                "<total_tokens>1 tokens left</total_tokens>\nAlways answer in Chinese."
            ),
        },
    );
    let (h0, _) = history0_and_current(&req);
    assert!(h0.contains("Always answer in Chinese."), "{h0}");
}
