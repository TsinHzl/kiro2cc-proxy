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
