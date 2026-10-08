// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 工具定义转换与 tool_use/tool_result 配对校验

use crate::kiro::model::requests::conversation::Message;
use crate::kiro::model::requests::tool::{InputSchema, Tool, ToolResult, ToolSpecification};

use super::schema::normalize_json_schema;

/// 验证并过滤 tool_use/tool_result 配对
///
/// 收集所有 tool_use_id，验证 tool_result 是否匹配
/// 静默跳过孤立的 tool_use 和 tool_result，输出警告日志
///
/// # Arguments
/// * `history` - 历史消息引用
/// * `tool_results` - 当前消息中的 tool_result 列表
///
/// # Returns
/// 元组：(经过验证和过滤后的 tool_result 列表, 孤立的 tool_use_id 集合)
pub(super) fn validate_tool_pairing(
    history: &[Message],
    tool_results: &[ToolResult],
) -> (Vec<ToolResult>, std::collections::HashSet<String>) {
    use std::collections::HashSet;

    // 1. 收集所有历史中的 tool_use_id
    let mut all_tool_use_ids: HashSet<String> = HashSet::new();
    // 2. 收集历史中已经有 tool_result 的 tool_use_id
    let mut history_tool_result_ids: HashSet<String> = HashSet::new();

    for msg in history {
        match msg {
            Message::Assistant(assistant_msg) => {
                if let Some(ref tool_uses) = assistant_msg.assistant_response_message.tool_uses {
                    for tool_use in tool_uses {
                        all_tool_use_ids.insert(tool_use.tool_use_id.clone());
                    }
                }
            }
            Message::User(user_msg) => {
                // 收集历史 user 消息中的 tool_results
                for result in &user_msg
                    .user_input_message
                    .user_input_message_context
                    .tool_results
                {
                    history_tool_result_ids.insert(result.tool_use_id.clone());
                }
            }
        }
    }

    // 3. 计算真正未配对的 tool_use_ids（排除历史中已配对的）
    let mut unpaired_tool_use_ids: HashSet<String> = all_tool_use_ids
        .difference(&history_tool_result_ids)
        .cloned()
        .collect();

    // 4. 过滤并验证当前消息的 tool_results
    let mut filtered_results = Vec::new();

    for result in tool_results {
        if unpaired_tool_use_ids.contains(&result.tool_use_id) {
            // 配对成功
            filtered_results.push(result.clone());
            unpaired_tool_use_ids.remove(&result.tool_use_id);
        } else if all_tool_use_ids.contains(&result.tool_use_id) {
            // tool_use 存在但已经在历史中配对过了，这是重复的 tool_result
            tracing::warn!(
                "跳过重复的 tool_result：该 tool_use 已在历史中配对，tool_use_id={}",
                result.tool_use_id
            );
        } else {
            // 孤立 tool_result - 找不到对应的 tool_use
            tracing::warn!(
                "跳过孤立的 tool_result：找不到对应的 tool_use，tool_use_id={}",
                result.tool_use_id
            );
        }
    }

    // 5. 检测真正孤立的 tool_use（有 tool_use 但在历史和当前消息中都没有 tool_result）
    for orphaned_id in &unpaired_tool_use_ids {
        tracing::warn!(
            "检测到孤立的 tool_use：找不到对应的 tool_result，将从历史中移除，tool_use_id={}",
            orphaned_id
        );
    }

    (filtered_results, unpaired_tool_use_ids)
}

/// 从历史消息中移除孤立的 tool_use
///
/// Kiro API 要求每个 tool_use 必须有对应的 tool_result，否则返回 400 Bad Request。
/// 此函数遍历历史中的 assistant 消息，移除没有对应 tool_result 的 tool_use。
///
/// # Arguments
/// * `history` - 可变的历史消息列表
/// * `orphaned_ids` - 需要移除的孤立 tool_use_id 集合
pub(super) fn remove_orphaned_tool_uses(
    history: &mut [Message],
    orphaned_ids: &std::collections::HashSet<String>,
) {
    if orphaned_ids.is_empty() {
        return;
    }

    for msg in history.iter_mut() {
        if let Message::Assistant(assistant_msg) = msg
            && let Some(ref mut tool_uses) = assistant_msg.assistant_response_message.tool_uses
        {
            let original_len = tool_uses.len();
            tool_uses.retain(|tu| !orphaned_ids.contains(&tu.tool_use_id));

            // 如果移除后为空，设置为 None
            if tool_uses.is_empty() {
                assistant_msg.assistant_response_message.tool_uses = None;
            } else if tool_uses.len() != original_len {
                tracing::debug!(
                    "从 assistant 消息中移除了 {} 个孤立的 tool_use",
                    original_len - tool_uses.len()
                );
            }
        }
    }
}

/// 请求带 Write / Edit / MultiEdit 任一工具时追加到系统提示末尾的分块写入约束。
///
/// 背景（issue #46）：上游不会增量流式输出工具参数，模型生成大块 `tool_use` 参数期间上游
/// 对该流不发送任何数据，静默约 240s 后以 `RST_STREAM(INTERNAL_ERROR)` 重置；单次写入约
/// 8000 汉字（~215s）可成功，约 20000 汉字（~240s）必定失败，原样重试会再次失败，
/// HTTP/2 PING 保活无效。代理无法让上游提前吐数据，只能引导模型把大文件拆成多次较小的调用。
///
/// 150 行上限覆盖所有会携带大块文本参数的编辑类工具：Write 的 `content`、Edit 的
/// `new_string`、MultiEdit 单次调用内所有编辑的合计。MultiEdit 把多处修改合并进一次调用，
/// 参数总量按合计计算，否则逐处都小于上限也会整体超时。
///
/// 3.4.0 起移除了原先的 Write/Edit 分块策略（工具描述后缀 + 系统提示），本常量只放系统提示、
/// 不改动客户端的工具描述（见 `test_client_workflow_tool_descriptions_are_not_augmented`），
/// 内容为常量，不影响 prompt cache 的前缀稳定性。
pub(super) const WRITE_CHUNKING_SYSTEM_NOTE: &str = "IMPORTANT: The service aborts any single tool call whose arguments take longer than about 4 minutes to generate. Never put more than about 150 lines (roughly 4000 tokens) of content in the arguments of one tool call. This limit applies to the Write content, to the Edit new_string, and to the combined size of all edits inside one MultiEdit call. For anything larger, write the first part with Write (or one Edit) and then append each further part with separate Edit or Bash calls, splitting a large MultiEdit into several smaller calls. Follow this silently without mentioning this limit.";

/// 工具列表中是否包含会携带大块文本参数的编辑类工具（Write / Edit / MultiEdit，名称不区分大小写）
pub(super) fn has_content_writing_tool(tools: &[Tool]) -> bool {
    tools.iter().any(|t| {
        let name = t.tool_specification.name.as_str();
        ["write", "edit", "multiedit"]
            .iter()
            .any(|n| name.eq_ignore_ascii_case(n))
    })
}

/// 转换工具定义
pub(super) fn convert_tools(tools: &Option<Vec<crate::anthropic::types::Tool>>) -> Vec<Tool> {
    let Some(tools) = tools else {
        return Vec::new();
    };

    let mut converted = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for t in tools {
        let name = t.name.trim();
        if name.is_empty() {
            tracing::warn!("跳过空名称工具定义");
            continue;
        }

        let name_key = name.to_lowercase();
        if !seen.insert(name_key) {
            tracing::warn!(tool_name = name, "跳过重复工具定义");
            continue;
        }

        let mut description = t.description.trim().to_string();
        if description.is_empty() {
            description = format!("Tool available to the assistant: {}", name);
        }

        // 限制描述长度为 10000 字符（安全截断 UTF-8，单次遍历）
        let description = match description.char_indices().nth(10000) {
            Some((idx, _)) => description[..idx].to_string(),
            None => description,
        };

        converted.push(Tool {
            tool_specification: ToolSpecification {
                name: name.to_string(),
                description,
                input_schema: InputSchema::from_json(normalize_json_schema(serde_json::json!(
                    t.input_schema
                ))),
            },
        });
    }

    converted
}
