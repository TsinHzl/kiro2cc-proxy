#[cfg(test)]
use super::*;

use super::convert::{convert_request, determine_agent_task_type, determine_chat_trigger_type};
use super::fields::model_max_output_tokens;
use super::history::{convert_assistant_message, merge_assistant_messages};
#[cfg(test)]
use super::model::map_model;
use super::pdf::extract_pdf_text_from_base64;
use super::prompt::append_recent_knowledge_hints;
use super::schema::normalize_json_schema;
use super::session::{
    derive_fallback_conversation_id, extract_session_id, is_compact_request, is_valid_uuid,
};
use super::thinking::generate_thinking_prefix;
use super::tools::{remove_orphaned_tool_uses, validate_tool_pairing};
use super::websearch::{
    collect_history_tool_names, create_placeholder_tool, is_web_search_server_tool,
    split_web_search_tool,
};

#[cfg(test)]
use crate::anthropic::types::{MessagesRequest, OutputConfig};

#[cfg(test)]
#[allow(unused_imports)]
use crate::kiro::model::requests::conversation::Message;

#[cfg(test)]
use crate::kiro::model::requests::conversation::{
    AssistantMessage, HistoryAssistantMessage, HistoryUserMessage, UserInputMessageContext,
    UserMessage,
};

#[allow(unused_imports)]
use crate::anthropic::types::Message as AnthropicMessage;
#[allow(unused_imports)]
use crate::anthropic::types::Tool as AnthropicTool2;
#[cfg(test)]
use crate::kiro::model::requests::tool::ToolResult;

#[cfg(test)]
#[allow(unused_imports)]
use crate::anthropic::types::ContentBlock as _;

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
fn test_map_model_opus() {
    assert!(
        map_model("claude-opus-4-20250514")
            .unwrap()
            .contains("opus")
    );
}

#[test]
fn test_map_model_haiku() {
    assert!(
        map_model("claude-haiku-4-20250514")
            .unwrap()
            .contains("haiku")
    );
}

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
fn test_map_model_passthrough_strips_thinking() {
    // 透传时剥离 -thinking 标记（thinking 由 req.thinking 单独控制）
    assert_eq!(map_model("gpt-4-thinking").unwrap(), "gpt-4");
    assert_eq!(
        map_model("brand-new-thinking-model").unwrap(),
        "brand-new-model"
    );
}

#[test]
fn test_map_model_empty_rejected() {
    // 空字符串仍拒绝
    assert!(map_model("").is_none());
    assert!(map_model("   ").is_none());
    assert!(map_model("-thinking").is_none());
}

#[test]
fn test_map_model_builtin_rules_unaffected() {
    // 内置规则优先级不变
    assert_eq!(map_model("claude-sonnet-4-5").unwrap(), "claude-sonnet-4.5");
    assert_eq!(map_model("gpt-5.6-sol").unwrap(), "gpt-5.6-sol");
    assert_eq!(map_model("glm-4.6").unwrap(), "glm-5");
}

#[test]
fn test_map_model_gpt_5_6_variants() {
    assert_eq!(map_model("gpt-5.6-sol").unwrap(), "gpt-5.6-sol");
    assert_eq!(map_model("gpt-5.6-terra").unwrap(), "gpt-5.6-terra");
    assert_eq!(map_model("gpt-5.6-luna").unwrap(), "gpt-5.6-luna");
    assert_eq!(map_model("gpt-5.6").unwrap(), "gpt-5.6-sol");
}

#[test]
fn test_normalize_json_schema_repairs_nested_invalid_values() {
    let schema = serde_json::json!({
        "type": ["object", "null"],
        "properties": {
            "path": {
                "type": ["string", "null"],
                "required": null,
                "properties": null,
                "format": "uri"
            },
            "opts": {
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "additionalProperties": null,
                        "default": 10
                    }
                },
                "required": [123, "limit"],
                "anyOf": [{"type": "object"}]
            },
            "mode": {
                "type": "string",
                "enum": ["fast", null, "safe", {"bad": true}]
            }
        },
        "required": null,
        "items": null,
        "additionalProperties": "sometimes",
        "$schema": "https://json-schema.org/draft/2020-12/schema"
    });

    let normalized = normalize_json_schema(schema);

    assert_eq!(normalized["type"], "object");
    assert_eq!(normalized["required"], serde_json::json!([]));
    assert_eq!(normalized["additionalProperties"], true);
    assert_eq!(normalized["properties"]["path"]["type"], "string");
    assert!(normalized["properties"]["path"].get("properties").is_none());
    assert!(normalized["properties"]["path"].get("required").is_none());
    assert!(normalized["properties"]["path"].get("format").is_none());
    assert_eq!(
        normalized["properties"]["opts"]["required"],
        serde_json::json!(["limit"])
    );
    assert!(normalized["properties"]["opts"].get("anyOf").is_none());
    assert!(
        normalized["properties"]["opts"]["properties"]["limit"]
            .get("additionalProperties")
            .is_none()
    );
    assert!(
        normalized["properties"]["opts"]["properties"]["limit"]
            .get("default")
            .is_none()
    );
    assert_eq!(
        normalized["properties"]["mode"]["enum"],
        serde_json::json!(["fast", "safe"])
    );
    assert!(normalized.get("$schema").is_none());
}

#[test]
fn test_normalize_resolves_ref_from_defs() {
    // MCP/pydantic 风格：属性用 $ref 指向 $defs 中的子 schema。
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "filter": { "$ref": "#/$defs/Filter" }
        },
        "required": ["filter"],
        "$defs": {
            "Filter": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "limit": { "type": "integer" }
                },
                "required": ["name"]
            }
        }
    });

    let normalized = normalize_json_schema(schema);

    // $ref 应被展开为实际子 schema，而非退化为空对象
    let filter = &normalized["properties"]["filter"];
    assert_eq!(filter["type"], "object");
    assert_eq!(filter["properties"]["name"]["type"], "string");
    assert_eq!(filter["properties"]["limit"]["type"], "integer");
    assert_eq!(filter["required"], serde_json::json!(["name"]));
    // $defs 与 $ref 不应残留（Kiro 不认）
    assert!(normalized.get("$defs").is_none());
    assert!(filter.get("$ref").is_none());
}

#[test]
fn test_normalize_ref_cycle_does_not_panic() {
    // 自引用循环：展开应在深度上限处兜底，不栈溢出。
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "node": { "$ref": "#/$defs/Node" }
        },
        "$defs": {
            "Node": {
                "type": "object",
                "properties": {
                    "child": { "$ref": "#/$defs/Node" }
                }
            }
        }
    });

    let normalized = normalize_json_schema(schema);
    assert_eq!(normalized["properties"]["node"]["type"], "object");
    assert!(normalized.get("$defs").is_none());
}

#[test]
fn test_normalize_unresolvable_ref_degrades_to_object() {
    // OpenAPI 风格 / 外部 / 不存在的 $ref：无法展开，应降级为宽松 object，
    // 不留下悬空 $ref。
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "a": { "$ref": "#/components/schemas/Foo" },
            "b": { "$ref": "#/$defs/Missing" }
        }
    });

    let normalized = normalize_json_schema(schema);
    assert_eq!(normalized["properties"]["a"]["type"], "object");
    assert_eq!(normalized["properties"]["b"]["type"], "object");
    assert!(normalized["properties"]["a"].get("$ref").is_none());
    assert!(normalized["properties"]["b"].get("$ref").is_none());
}

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

#[test]
fn test_map_model_thinking_suffix_sonnet() {
    // thinking 后缀不应影响 sonnet 模型映射
    let result = map_model("claude-sonnet-4-5-20250929-thinking");
    assert_eq!(result, Some("claude-sonnet-4.5".to_string()));
}

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
fn test_map_model_thinking_suffix_opus_4_5() {
    // thinking 后缀不应影响 opus 4.5 模型映射
    let result = map_model("claude-opus-4-5-20251101-thinking");
    assert_eq!(result, Some("claude-opus-4.5".to_string()));
}

#[test]
fn test_map_model_thinking_suffix_opus_4_6() {
    // thinking 后缀不应影响 opus 4.6 模型映射
    let result = map_model("claude-opus-4-6-thinking");
    assert_eq!(result, Some("claude-opus-4.6".to_string()));
}

#[test]
fn test_map_model_thinking_suffix_haiku() {
    // thinking 后缀不应影响 haiku 模型映射
    let result = map_model("claude-haiku-4-5-20251001-thinking");
    assert_eq!(result, Some("claude-haiku-4.5".to_string()));
}

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

fn ws_tool(
    tool_type: Option<&str>,
    name: &str,
    max_uses: Option<i32>,
) -> crate::anthropic::types::Tool {
    crate::anthropic::types::Tool {
        tool_type: tool_type.map(|s| s.to_string()),
        name: name.to_string(),
        description: String::new(),
        input_schema: Default::default(),
        max_uses,
        defer_loading: None,
    }
}

#[test]
fn test_is_web_search_server_tool() {
    // tool_type 含 web_search 即命中（官方 server tool 格式）
    assert!(is_web_search_server_tool(&ws_tool(
        Some("web_search_20250305"),
        "web_search",
        None
    )));
    // 部分客户端只发 name == "web_search" 也命中
    assert!(is_web_search_server_tool(&ws_tool(
        None,
        "web_search",
        None
    )));
    // 普通工具不命中
    assert!(!is_web_search_server_tool(&ws_tool(None, "Bash", None)));
    assert!(!is_web_search_server_tool(&ws_tool(
        Some("custom_bash_20240101"),
        "Bash",
        None
    )));
}

#[test]
fn test_split_web_search_tool_mixed_list() {
    // 混合列表：剔除 server tool、提取 max_uses、保留普通工具
    let mut req = fallback_req(None, &["Read", "Write"], &[("user", "hi")]);
    let tools = req.tools.as_mut().unwrap();
    tools.push(ws_tool(Some("web_search_20250305"), "web_search", Some(3)));

    let (max_uses, ordinary) = split_web_search_tool(&req).expect("应命中 server tool");
    assert_eq!(max_uses, Some(3));
    assert_eq!(ordinary.len(), 2);
    assert!(ordinary.iter().all(|t| t.name != "web_search"));
    assert!(ordinary.iter().any(|t| t.name == "Read"));
    assert!(ordinary.iter().any(|t| t.name == "Write"));
}

#[test]
fn test_split_web_search_tool_no_hit() {
    // 无 server tool：返回 None，普通工具列表原样
    let req = fallback_req(None, &["Read", "Bash"], &[("user", "hi")]);
    assert!(split_web_search_tool(&req).is_none());

    // 无 tools 字段同样返回 None
    let req = fallback_req(None, &[], &[("user", "hi")]);
    assert!(split_web_search_tool(&req).is_none());
}

#[test]
fn test_split_web_search_tool_no_max_uses() {
    // 携带 server tool 但未声明 max_uses：内层 None
    let mut req = fallback_req(None, &["Read"], &[("user", "hi")]);
    req.tools
        .as_mut()
        .unwrap()
        .push(ws_tool(Some("web_search_20250305"), "web_search", None));

    let (max_uses, ordinary) = split_web_search_tool(&req).expect("应命中 server tool");
    assert_eq!(max_uses, None);
    assert_eq!(ordinary.len(), 1);
    assert_eq!(ordinary[0].name, "Read");
}

#[test]
fn test_split_web_search_tool_multiple_declarations_first_wins() {
    // 异常场景：同一请求声明多个 web_search server tool——首个声明的
    // max_uses 生效，不被后续声明覆盖
    let mut req = fallback_req(None, &["Read"], &[("user", "hi")]);
    let tools = req.tools.as_mut().unwrap();
    tools.push(ws_tool(Some("web_search_20250305"), "web_search", Some(3)));
    tools.push(ws_tool(Some("web_search_20260101"), "web_search", Some(7)));

    let (max_uses, ordinary) = split_web_search_tool(&req).expect("应命中 server tool");
    assert_eq!(max_uses, Some(3));
    // 所有命中项均从普通工具列表剔除
    assert_eq!(ordinary.len(), 1);
    assert_eq!(ordinary[0].name, "Read");
}

#[test]
fn test_split_web_search_tool_first_declared_none_then_some() {
    // 首个声明未带 max_uses、后续声明带：首个 None 不锁定上限，向后取首个
    // 有效值（守卫语义为"首个有效声明生效"，避免有效上限被静默丢失）
    let mut req = fallback_req(None, &["Read"], &[("user", "hi")]);
    let tools = req.tools.as_mut().unwrap();
    tools.push(ws_tool(Some("web_search_20250305"), "web_search", None));
    tools.push(ws_tool(Some("web_search_20260101"), "web_search", Some(7)));

    let (max_uses, ordinary) = split_web_search_tool(&req).expect("应命中 server tool");
    assert_eq!(max_uses, Some(7));
    assert_eq!(ordinary.len(), 1);
}

#[test]
fn test_convert_request_removes_web_search_from_context_tools() {
    use crate::anthropic::types::Message as AnthropicMessage;

    let req = MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: vec![AnthropicMessage {
            role: "user".to_string(),
            content: serde_json::json!("搜索一下今天的新闻"),
        }],
        stream: false,
        system: None,
        tools: Some(vec![
            ws_tool(None, "Read", None),
            ws_tool(Some("web_search_20250305"), "web_search", Some(3)),
        ]),
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    };

    let result = convert_request(&req).unwrap();

    // web_search max_uses 透传给桥接层
    assert_eq!(result.web_search_max_uses, Some(Some(3)));

    // Kiro context.tools 仅含普通工具，web_search 被剔除
    let tools = &result
        .conversation_state
        .current_message
        .user_input_message
        .user_input_message_context
        .tools;
    assert!(
        tools
            .iter()
            .all(|t| t.tool_specification.name != "web_search"),
        "context.tools 不应包含 web_search"
    );
    assert!(
        tools.iter().any(|t| t.tool_specification.name == "Read"),
        "context.tools 应保留普通工具"
    );
}

#[test]
fn test_convert_request_no_web_search_passes_through() {
    // 无 server tool：web_search_max_uses 为外层 None，工具列表与直接转换一致
    let req = fallback_req(None, &["Read"], &[("user", "hi")]);
    let result = convert_request(&req).unwrap();
    assert_eq!(result.web_search_max_uses, None);
    let tools = &result
        .conversation_state
        .current_message
        .user_input_message
        .user_input_message_context
        .tools;
    assert!(tools.iter().any(|t| t.tool_specification.name == "Read"));
}

#[test]
fn test_collect_history_tool_names_excludes_web_search() {
    use crate::kiro::model::requests::tool::ToolUseEntry;

    // 桥接产生的 web_search toolUse 不应生成占位符定义
    let mut assistant_msg = AssistantMessage::new("Let me search.");
    assistant_msg = assistant_msg.with_tool_uses(vec![
        ToolUseEntry::new("ws-1", "web_search")
            .with_input(serde_json::json!({"query": "rust news"})),
        ToolUseEntry::new("tool-1", "read").with_input(serde_json::json!({"path": "/test.txt"})),
    ]);

    let history = vec![Message::Assistant(HistoryAssistantMessage {
        assistant_response_message: assistant_msg,
    })];

    let tool_names = collect_history_tool_names(&history);
    assert!(!tool_names.contains(&"web_search".to_string()));
    assert!(tool_names.contains(&"read".to_string()));
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
fn fallback_req(
    system: Option<&str>,
    tool_names: &[&str],
    messages: &[(&str, &str)],
) -> MessagesRequest {
    use crate::anthropic::types::{Message as AnthropicMessage, SystemMessage, Tool};
    MessagesRequest {
        model: "claude-sonnet-4".to_string(),
        max_tokens: 1024,
        messages: messages
            .iter()
            .map(|(role, text)| AnthropicMessage {
                role: (*role).to_string(),
                content: serde_json::json!(*text),
            })
            .collect(),
        stream: false,
        system: system.map(|s| {
            vec![SystemMessage {
                text: s.to_string(),
            }]
        }),
        tools: if tool_names.is_empty() {
            None
        } else {
            Some(
                tool_names
                    .iter()
                    .map(|name| Tool {
                        tool_type: None,
                        name: (*name).to_string(),
                        description: String::new(),
                        input_schema: Default::default(),
                        max_uses: None,
                        defer_loading: None,
                    })
                    .collect(),
            )
        },
        tool_choice: None,
        thinking: None,
        output_config: None,
        metadata: None,
    }
}

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
fn test_extract_pdf_text_non_ascii_no_panic() {
    use base64::Engine as _;

    // 回归：'(' 字面量串内含多字节 UTF-8，lookahead 切片旧实现按字符串字节
    // 切片可能落在字符中间 panic，新实现按字节数组匹配 ASCII，应安全
    let pdf = "stream\nBT (你好世界测试内容) Tj ET\nendstream";
    let data = base64::engine::general_purpose::STANDARD.encode(pdf);
    // 不校验具体返回值，只要求不 panic
    let _ = extract_pdf_text_from_base64(&data);
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
fn test_map_model_opus_4_6_unchanged() {
    // 回归：opus-4-6 默认走 claude-opus-4.6
    assert_eq!(
        map_model("claude-opus-4-6"),
        Some("claude-opus-4.6".to_string())
    );
}

#[test]
fn test_gpt_5_6_additional_model_request_fields_is_none() {
    // 实测（抓包）：gpt-5.6-* 使用 additionalModelRequestFields.reasoning.effort 路径，
    // 不接受 output_config / max_tokens（均返回 400 REQUEST_BODY_INVALID）。
    // 本测试验证 GPT 系生成正确的 reasoning.effort 结构，且默认注入 effort="high"。
    use crate::anthropic::types::Message as AnthropicMessage;

    let req = MessagesRequest {
        model: "gpt-5.6-sol".to_string(),
        max_tokens: 128000,
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
    let fields = result
        .additional_model_request_fields
        .as_ref()
        .expect("gpt-5.6 系列必须包含 additionalModelRequestFields");
    assert_eq!(
        fields["reasoning"]["effort"].as_str(),
        Some("high"),
        "gpt-5.6 系列未携带 output_config 时应默认注入 reasoning.effort=\"high\""
    );
    assert!(
        fields.get("output_config").is_none(),
        "gpt-5.6 系列不得包含 output_config 字段"
    );
    assert!(
        fields.get("max_tokens").is_none(),
        "gpt-5.6 系列不得包含 max_tokens 字段"
    );
}

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

#[test]
fn test_model_max_output_tokens_opus_5() {
    assert_eq!(model_max_output_tokens("claude-opus-5"), 128000);
    assert_eq!(model_max_output_tokens("claude-opus-5-thinking"), 128000);
    assert_eq!(model_max_output_tokens("Claude-Opus-5"), 128000);
    assert_eq!(model_max_output_tokens("Claude Opus 5"), 128000);

    // 回归：其他档位不变
    assert_eq!(model_max_output_tokens("claude-opus-4.6"), 64000);
    assert_eq!(model_max_output_tokens("claude-opus-4.5"), 64000);
    assert_eq!(model_max_output_tokens("claude-sonnet-5"), 64000);
    assert_eq!(model_max_output_tokens("claude-haiku-4.5"), 64000);
}

#[test]
fn test_additional_model_request_fields_max_tokens_minimum_applies_to_all_claude_models() {
    // 回归测试：Kiro 侧 schema 对 max_tokens 强制 minimum = 1024，对支持
    // additionalModelRequestFields 的 Claude 代际生效（实测 claude-sonnet-4-6
    // 在 max_tokens=200 时同样报 400 "must have a minimum value of 1024.0"），
    // 不能只对 cap==128000（opus-4.7/4.8/5）的模型生效。
    // 注意："4.5" 代际（sonnet/opus/haiku）整体跳过该字段（400 实测，
    // 见 build_additional_model_request_fields 文档），故不在本测试范围。
    use crate::anthropic::types::Message as AnthropicMessage;

    for model in [
        "claude-sonnet-4-6",
        "claude-sonnet-5",
        "claude-opus-4-6",
        "claude-opus-5",
    ] {
        let req = MessagesRequest {
            model: model.to_string(),
            max_tokens: 200, // 低于 1024 的边界输入
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
        let fields = result
            .additional_model_request_fields
            .expect("非 GPT 模型应构建 additionalModelRequestFields");
        let max_tokens = fields["max_tokens"].as_i64().unwrap_or(0);

        assert!(
            max_tokens >= 1024,
            "model={model} max_tokens 应被下限收敛到至少 1024，实际={max_tokens}"
        );
    }
}

#[test]
fn test_output_config_effort_passthrough_with_default() {
    // effort 透传 + 默认注入（回退 issue #40 的仅透传行为）：客户端显式携带
    // output_config 时按原值转发；未携带时默认注入 effort="high"。
    use crate::anthropic::types::{Message as AnthropicMessage, OutputConfig};

    // 场景 1：客户端未携带 output_config → 默认注入 effort="high"
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
        thinking: None,
        output_config: None,
        metadata: None,
    };
    let result = convert_request(&req).unwrap();
    let fields = result
        .additional_model_request_fields
        .expect("非 4.5 代模型应构建 additionalModelRequestFields");
    assert_eq!(
        fields["output_config"]["effort"].as_str(),
        Some("high"),
        "客户端未携带 output_config 时应默认注入 effort=\"high\""
    );

    // 场景 2：客户端显式携带 output_config → effort 按客户端值透传
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
        thinking: None,
        output_config: Some(OutputConfig {
            effort: "low".to_string(),
            format: None,
        }),
        metadata: None,
    };
    let result = convert_request(&req).unwrap();
    let fields = result
        .additional_model_request_fields
        .expect("非 4.5 代模型应构建 additionalModelRequestFields");
    assert_eq!(
        fields["output_config"]["effort"].as_str(),
        Some("low"),
        "effort 必须按客户端传入值透传"
    );

    // 场景 3：客户端携带 output_config 但 effort 为空串 → 兜底为 "high"
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
        thinking: None,
        output_config: Some(OutputConfig {
            effort: "".to_string(),
            format: None,
        }),
        metadata: None,
    };
    let result = convert_request(&req).unwrap();
    let fields = result
        .additional_model_request_fields
        .expect("非 4.5 代模型应构建 additionalModelRequestFields");
    assert_eq!(
        fields["output_config"]["effort"].as_str(),
        Some("high"),
        "空串 effort 应兜底为 \"high\"，避免转发非法值"
    );
}

#[test]
fn test_4_5_generation_skips_fields_even_with_output_config() {
    // 回归测试（CR #3 补充）：4.5 代际模型即使客户端携带 output_config，
    // 也必须整体跳过 additionalModelRequestFields（该代际 schema 不接受
    // 任何结构化字段，见 build_additional_model_request_fields 文档）。
    use crate::anthropic::types::{Message as AnthropicMessage, OutputConfig};

    for model in ["claude-sonnet-4-5", "claude-opus-4-5", "claude-haiku-4-5"] {
        let req = MessagesRequest {
            model: model.to_string(),
            max_tokens: 32000,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: serde_json::json!("Hello"),
            }],
            stream: false,
            system: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            output_config: Some(OutputConfig {
                effort: "low".to_string(),
                format: None,
            }),
            metadata: None,
        };
        let result = convert_request(&req).unwrap();
        assert!(
            result.additional_model_request_fields.is_none(),
            "{model} 携带 output_config 时仍应整体跳过 additionalModelRequestFields"
        );
    }
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

#[test]
fn test_claude_4_5_generation_additional_model_request_fields_is_none() {
    // 回归测试（haiku-4.5 全部请求 400 修复）：实测 claude-sonnet-4.5 /
    // claude-opus-4.5 / claude-haiku-4.5 的 Kiro schema 均不接受
    // additionalModelRequestFields（thinking/output_config/max_tokens 均报
    // 400 REQUEST_BODY_INVALID），需整体省略该字段。历史上该跳过逻辑曾被
    // 重构为仅判断 GPT 系而丢失，导致 haiku-4.5 全量 502。
    use crate::anthropic::types::{Message as AnthropicMessage, Thinking};

    // 覆盖 /v1/models 暴露的带日期变体（含 -thinking）及简写形式
    for model in [
        "claude-haiku-4-5",
        "claude-sonnet-4-5",
        "claude-sonnet-4-5-20250929",
        "claude-sonnet-4-5-20250929-thinking",
        "claude-opus-4-5",
        "claude-opus-4-5-20251101",
        "claude-opus-4-5-20251101-thinking",
    ] {
        let req = MessagesRequest {
            model: model.to_string(),
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

        let result = convert_request(&req).unwrap();
        assert!(
            result.additional_model_request_fields.is_none(),
            "model={model} 4.5 代际必须整体省略 additionalModelRequestFields 字段"
        );
    }
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
