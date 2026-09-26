// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! web_search server tool 拆分与历史占位工具

use crate::anthropic::types::MessagesRequest;
use crate::kiro::model::requests::conversation::Message;
use crate::kiro::model::requests::tool::{InputSchema, Tool, ToolSpecification};

/// 判断工具是否为 Anthropic server tool 格式的 web_search
///
/// 识别依据（双条件，兼容 CC 版本差异）：
/// - `tool_type` 含 `web_search`（如 `web_search_20250305`）
/// - 或 `name == "web_search"`（部分 CC 版本只发 name + max_uses）
pub(crate) fn is_web_search_server_tool(t: &crate::anthropic::types::Tool) -> bool {
    t.tool_type
        .as_deref()
        .is_some_and(|ty| ty.contains("web_search"))
        || t.name == "web_search"
}

/// 从请求工具列表中拆分出 web_search server tool 与普通工具
///
/// 命中时返回 `Some((max_uses, 剔除后的普通工具列表))`；未命中返回 `None`（调用方
/// 按现状透传，零行为变化）。max_uses 取 server tool 声明的次数上限，供桥接层
/// 计算多轮搜索上限 `min(max_uses, 5)`。
pub(crate) fn split_web_search_tool(
    req: &MessagesRequest,
) -> Option<(Option<i32>, Vec<crate::anthropic::types::Tool>)> {
    let tools = req.tools.as_ref()?;
    let mut max_uses = None;
    let mut hit = false;
    let mut ordinary: Vec<crate::anthropic::types::Tool> = Vec::with_capacity(tools.len());
    for t in tools {
        if is_web_search_server_tool(t) {
            hit = true;
            // 首个非 None 声明的 max_uses 生效（协议约定一份请求仅一个 web_search
            // server tool；多声明属于客户端异常，取首个避免"最后声明覆盖"的
            // 未定义行为。注意首个命中项可能未声明 max_uses——此时继续向后
            // 找首个显式声明，均未声明则保持 None 走默认值 5）
            if max_uses.is_none() {
                max_uses = t.max_uses;
            }
        } else {
            ordinary.push(t.clone());
        }
    }
    if hit {
        Some((max_uses, ordinary))
    } else {
        None
    }
}

/// 构建 web_search 桥接工具定义（普通 tool spec 格式，Kiro 可识别）
///
/// 请求携带 web_search server tool 时注入 context.tools，让 Kiro 侧模型
/// 知道搜索能力可用并主动发起 `web_search` toolUse；handlers 桥接层截获该
/// toolUse 后走 Kiro MCP 真实搜索（见 handlers/bridge.rs）。
pub(super) fn create_web_search_bridge_tool() -> Tool {
    Tool {
        tool_specification: ToolSpecification {
            name: "web_search".to_string(),
            description: "Search the web for real-time or up-to-date information \
                          (news, weather, prices, documentation, etc.). Use this tool \
                          whenever the user asks about current events or facts that may \
                          have changed after your knowledge cutoff."
                .to_string(),
            input_schema: InputSchema::from_json(serde_json::json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to execute"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            })),
        },
    }
}

/// 收集历史消息中使用的所有工具名称
///
/// 桥接产生的 web_search toolUse 不参与占位符生成——Kiro 不识别该 server tool，
/// 且避免污染 context.tools。
pub(super) fn collect_history_tool_names(history: &[Message]) -> Vec<String> {
    let mut tool_names = Vec::new();

    for msg in history {
        if let Message::Assistant(assistant_msg) = msg
            && let Some(ref tool_uses) = assistant_msg.assistant_response_message.tool_uses
        {
            for tool_use in tool_uses {
                if tool_use.name == "web_search" {
                    continue;
                }
                if !tool_names.contains(&tool_use.name) {
                    tool_names.push(tool_use.name.clone());
                }
            }
        }
    }

    tool_names
}

/// 为历史中使用但不在 tools 列表中的工具创建占位符定义
/// Kiro API 要求：历史消息中引用的工具必须在 currentMessage.tools 中有定义
pub(super) fn create_placeholder_tool(name: &str) -> Tool {
    Tool {
        tool_specification: ToolSpecification {
            name: name.to_string(),
            description: "Tool used in conversation history".to_string(),
            input_schema: InputSchema::from_json(serde_json::json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": true
            })),
        },
    }
}
