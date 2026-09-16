// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! convert_request 主入口与触发类型判定

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::anthropic::types::MessagesRequest;
use crate::kiro::model::requests::conversation::{
    ConversationState, CurrentMessage, UserInputMessage, UserInputMessageContext,
};

use super::fields::build_additional_model_request_fields;
use super::history::build_history;
use super::message::process_message_content;
use super::model::map_model;
use super::prompt::{append_output_format_instruction, append_recent_knowledge_hints};
use super::result::{ConversionError, ConversionResult};
use super::session::{
    derive_agent_continuation_id, derive_fallback_conversation_id, extract_session_id,
    is_compact_request,
};
use super::tools::{convert_tools, remove_orphaned_tool_uses, validate_tool_pairing};
use super::websearch::{
    collect_history_tool_names, create_placeholder_tool, split_web_search_tool,
};

/// 将 Anthropic 请求转换为 Kiro 请求
pub fn convert_request(req: &MessagesRequest) -> Result<ConversionResult, ConversionError> {
    // 1. 映射模型
    let model_id = map_model(&req.model)
        .ok_or_else(|| ConversionError::UnsupportedModel(req.model.clone()))?;

    // 2. 检查消息列表
    if req.messages.is_empty() {
        return Err(ConversionError::EmptyMessages);
    }

    // 2.5. 预处理 prefill：Kiro 不支持末尾 assistant prefill
    let messages: &[_] = if req.messages.last().is_some_and(|m| m.role != "user") {
        tracing::info!("检测到末尾 assistant 消息（prefill），静默丢弃");
        let last_user_idx = req
            .messages
            .iter()
            .rposition(|m| m.role == "user")
            .ok_or(ConversionError::EmptyMessages)?;
        &req.messages[..=last_user_idx]
    } else {
        &req.messages
    };

    // 3. 生成会话 ID 和代理 ID
    // 优先级：
    //   1. metadata.user_id 中的 session UUID（Claude Code 标准格式 / 纯 UUID 直通）
    //   2. system + 工具名 + 首条消息的 SHA-256 派生（含裸请求，全部走首条消息 seed）
    //   3. 完全随机 UUID（仅防御路径：messages 为空时 fallback 不可用，实际不可达）
    let (conversation_id, id_source) = req
        .metadata
        .as_ref()
        .and_then(|m| m.user_id.as_ref())
        .and_then(|user_id| extract_session_id(user_id))
        .map(|id| (id, "metadata"))
        .or_else(|| derive_fallback_conversation_id(req).map(|id| (id, "fallback")))
        .unwrap_or_else(|| (Uuid::new_v4().to_string(), "random"));
    // agentContinuationId 基于 conversationId 派生，保持同一会话内稳定
    // 这样 Kiro 后端能识别连续请求，对历史消息做跨请求 prompt caching
    let agent_continuation_id = derive_agent_continuation_id(&conversation_id);
    tracing::info!(
        "[session] conversationId={} agentContinuationId={} source={} (同一会话的连续请求这两个值应保持不变)",
        conversation_id,
        agent_continuation_id,
        id_source
    );

    // 4. 确定触发类型
    let chat_trigger_type = determine_chat_trigger_type(req);

    // 5. 处理最后一条消息作为 current_message（经过 prefill 预处理，末尾必为 user）
    let last_message = messages.last().unwrap();
    let (text_content, images, tool_results) = process_message_content(&last_message.content)?;
    let text_content = append_recent_knowledge_hints(text_content);
    let text_content = append_output_format_instruction(text_content, &req.output_config);

    // 6. 转换工具定义
    // web_search server tool 不发给 Kiro（Kiro 不识别该格式），由 handlers 层桥接到
    // Kiro MCP 执行；未命中时 split_web_search_tool 返回 None，与直接转换逐字节一致。
    // 借用实现：未命中路径直接引用 req.tools，避免每个请求深拷贝全部工具定义
    let split_owned;
    let (web_search_max_uses, split_tools) = match split_web_search_tool(req) {
        Some((max_uses, ordinary)) => {
            split_owned = Some(ordinary);
            (Some(max_uses), &split_owned)
        }
        None => (None, &req.tools),
    };
    let mut tools = convert_tools(split_tools);

    // 7. 构建历史消息（需要先构建，以便收集历史中使用的工具）
    let mut history = build_history(req, messages, &model_id, &conversation_id)?;

    // 8. 验证并过滤 tool_use/tool_result 配对
    // 移除孤立的 tool_result（没有对应的 tool_use）
    // 同时返回孤立的 tool_use_id 集合，用于后续清理
    let (validated_tool_results, orphaned_tool_use_ids) =
        validate_tool_pairing(&history, &tool_results);

    // 9. 从历史中移除孤立的 tool_use（Kiro API 要求 tool_use 必须有对应的 tool_result）
    remove_orphaned_tool_uses(&mut history, &orphaned_tool_use_ids);

    // 10. 收集历史中使用的工具名称，为缺失的工具生成占位符定义
    // Kiro API 要求：历史消息中引用的工具必须在 tools 列表中有定义
    // 注意：Kiro 匹配工具名称时忽略大小写，所以这里也需要忽略大小写比较
    let history_tool_names = collect_history_tool_names(&history);
    let existing_tool_names: std::collections::HashSet<_> = tools
        .iter()
        .map(|t| t.tool_specification.name.to_lowercase())
        .collect();

    for tool_name in history_tool_names {
        if !existing_tool_names.contains(&tool_name.to_lowercase()) {
            tools.push(create_placeholder_tool(&tool_name));
        }
    }

    // 11. [cache-check] 打印 history 条目哈希，便于跨请求验证 prefix cache 稳定性
    for (i, msg) in history.iter().enumerate() {
        let json = serde_json::to_string(msg).unwrap_or_default();
        let hash = format!("{:x}", Sha256::digest(json.as_bytes()));
        tracing::info!(
            "[cache-check] session={} history[{}] hash={} len={}",
            conversation_id,
            i,
            &hash[..8],
            json.len(),
        );
    }

    // 11b. 构建 UserInputMessageContext —— tools 完整定义直接写入 context.tools（与 Kiro 官方 CLI 一致）
    let mut context = UserInputMessageContext::new();
    if !tools.is_empty() {
        context.tools = tools;
    }
    if !validated_tool_results.is_empty() {
        context = context.with_tool_results(validated_tool_results);
    }

    // 12. 构建当前消息
    // 保留文本内容，即使有工具结果也不丢弃用户文本
    // 空 content 兜底：Kiro 后端不接受空字符串。
    // 注意：此前用 "Continue" 会让模型把 tool_result-only 的 user 消息误判为
    // "用户让我继续" → 仅简短回 "已完成" 不复述工具结果（如 LS 输出）。
    // 改用中性提示词，明确告知"上方为工具结果"，让模型基于结果回复用户。
    let content = if text_content.is_empty() {
        "(tool result above)".to_string()
    } else {
        text_content
    };

    let mut user_input = UserInputMessage::new(content, &model_id)
        .with_context(context)
        .with_origin("AI_EDITOR");

    if !images.is_empty() {
        user_input = user_input.with_images(images);
    }

    let current_message = CurrentMessage::new(user_input);

    // 13. 构建 ConversationState
    let agent_task_type = determine_agent_task_type(req);
    tracing::debug!("[session] agentTaskType={}", agent_task_type);

    let conversation_state = ConversationState::new(conversation_id)
        .with_agent_continuation_id(agent_continuation_id)
        .with_agent_task_type(agent_task_type)
        .with_chat_trigger_type(chat_trigger_type)
        .with_current_message(current_message)
        .with_history(history);

    let additional_model_request_fields = build_additional_model_request_fields(req, &model_id);
    let is_compact = is_compact_request(messages);
    if is_compact {
        tracing::info!(
            "[COMPACT] 检测到 /compact 压缩请求，将使用压缩超时（见 provider::COMPACT_TIMEOUT_SECS）"
        );
    }

    Ok(ConversionResult {
        conversation_state,
        additional_model_request_fields,
        is_compact_request: is_compact,
        web_search_max_uses,
        thinking_adaptive_requested: req
            .thinking
            .as_ref()
            .map(|t| t.thinking_type == "adaptive")
            .unwrap_or(false),
    })
}

/// 确定聊天触发类型
/// "AUTO" 模式可能会导致 400 Bad Request 错误
pub(super) fn determine_chat_trigger_type(_req: &MessagesRequest) -> String {
    "MANUAL".to_string()
}

/// 确定代理任务类型
///
/// - 请求携带任意工具 → "spectask"：触发 Kiro 原生 toolUseEvent 响应
/// - 无工具 → "vibe"
pub(super) fn determine_agent_task_type(req: &MessagesRequest) -> &'static str {
    match &req.tools {
        Some(tools) if !tools.is_empty() => "spectask",
        _ => "vibe",
    }
}
