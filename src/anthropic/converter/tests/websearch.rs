//! converter 测试（自 tests.rs 拆分，纯代码搬移）
#![cfg(test)]

use super::super::convert::convert_request;
use super::super::websearch::{
    collect_history_tool_names, is_web_search_server_tool, split_web_search_tool,
};
use super::fallback_req;
#[allow(unused_imports)]
use crate::anthropic::types::ContentBlock as _;
use crate::anthropic::types::MessagesRequest;
#[allow(unused_imports)]
use crate::kiro::model::requests::conversation::Message;
use crate::kiro::model::requests::conversation::{AssistantMessage, HistoryAssistantMessage};

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

    // server tool 被剔除，但注入普通格式的 web_search 桥接工具定义
    // （否则 Kiro 侧模型不知道搜索能力可用，不会发起 toolUse，桥接永不触发）
    let tools = &result
        .conversation_state
        .current_message
        .user_input_message
        .user_input_message_context
        .tools;
    let ws = tools
        .iter()
        .find(|t| t.tool_specification.name == "web_search")
        .expect("context.tools 应包含注入的 web_search 桥接工具");
    assert!(
        ws.tool_specification
            .input_schema
            .json
            .get("properties")
            .is_some(),
        "桥接工具应为普通 tool spec 格式（含 input_schema）"
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
