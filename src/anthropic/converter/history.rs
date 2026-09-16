// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 历史消息构建：系统消息冻结、user/assistant 合并

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use crate::anthropic::types::{ContentBlock, MessagesRequest};
use crate::kiro::model::requests::conversation::{
    AssistantMessage, HistoryAssistantMessage, HistoryUserMessage, Message,
    UserInputMessageContext, UserMessage,
};
use crate::kiro::model::requests::tool::ToolUseEntry;

use super::cache::{CacheEntry, PREV_H0, evict_oldest_if_full};
use super::message::process_message_content;
use super::prompt::{extract_system_reminders, normalize_billing_header};
use super::result::ConversionError;
use super::thinking::{generate_thinking_prefix, gpt_anti_pseudo_tag_hint, has_thinking_tags};
use super::tools::SYSTEM_CHUNKED_POLICY;

/// 构建历史消息
///
/// # Arguments
/// * `req` - 原始请求，用于读取 `system`、`thinking` 等配置字段
/// * `messages` - 经过 prefill 预处理的消息切片，末尾必定是 user 消息。
///   注意：该切片与 `req.messages` 可能不同（prefill 时会截断末尾的 assistant 消息），
///   调用方应始终使用此参数而非 `req.messages`。
/// * `model_id` - 已映射的 Kiro 模型 ID
pub(super) fn build_history(
    req: &MessagesRequest,
    messages: &[crate::anthropic::types::Message],
    model_id: &str,
    session_id: &str,
) -> Result<Vec<Message>, ConversionError> {
    let mut history = Vec::new();

    // 生成 thinking 前缀（仅 luna 不使用 Claude/Kiro 文本控制标签）
    let thinking_prefix = generate_thinking_prefix(req, model_id);
    // GPT 系模型在客户端请求 thinking 时，额外注入反伪标签引导语（见函数文档）
    let anti_pseudo_tag_hint = gpt_anti_pseudo_tag_hint(req, model_id);

    // 1. 处理系统消息
    if let Some(ref system) = req.system {
        let system_content: String = system
            .iter()
            .map(|s| s.text.clone())
            .collect::<Vec<_>>()
            .join("\n");

        if !system_content.is_empty() {
            let static_content = format!("{}\n{}", system_content, SYSTEM_CHUNKED_POLICY);

            // 注入thinking标签到系统消息最前面（如果需要且不存在）
            let static_content = if let Some(ref prefix) = thinking_prefix {
                if !has_thinking_tags(&static_content) {
                    format!("{}\n{}", prefix, static_content)
                } else {
                    static_content
                }
            } else {
                static_content
            };

            // 追加 GPT 反伪标签引导语（仅当请求携带 thinking 配置时）
            let final_content = if let Some(hint) = anti_pseudo_tag_hint {
                format!("{}\n{}", static_content, hint)
            } else {
                static_content
            };

            // 将 cch= 固定为 0，使 history[0] 跨请求稳定，命中 Kiro prompt cache。
            let cache_content = normalize_billing_header(final_content);

            let reminders = extract_system_reminders(messages);

            // 只冻结稳定系统内容；动态 reminder 每轮重新追加，避免 compact 后继续
            // 发送上一轮冻结的过期提醒。完整内容参与 key，避免前缀相同的辅助请求串槽。
            let final_content = {
                let cache = PREV_H0.get_or_init(|| Mutex::new(HashMap::new()));
                let mut map = cache.lock().unwrap_or_else(|e| e.into_inner());
                let mut hasher = Sha256::new();
                hasher.update(cache_content.as_bytes());
                let h0_key = format!(
                    "{}#{}",
                    session_id,
                    &format!("{:x}", hasher.finalize())[..16]
                );

                let stable_content = if let Some(entry) = map.get_mut(&h0_key) {
                    entry.last_used = Instant::now();
                    tracing::info!(
                        "[exp2] history[0] frozen hash={} len={} session={}",
                        &h0_key[h0_key.len().saturating_sub(16)..],
                        entry.value.len(),
                        session_id
                    );
                    entry.value.clone()
                } else {
                    tracing::info!(
                        "[exp2] history[0] first hash={} len={} session={}",
                        &h0_key[h0_key.len().saturating_sub(16)..],
                        cache_content.len(),
                        session_id
                    );
                    map.insert(h0_key, CacheEntry::new(cache_content.clone()));
                    evict_oldest_if_full(&mut map);
                    cache_content
                };

                if reminders.is_empty() {
                    stable_content
                } else {
                    format!("{}\n{}", stable_content, reminders)
                }
            };

            // 系统消息作为 user + assistant 配对
            let user_msg = HistoryUserMessage::new(final_content, model_id);
            history.push(Message::User(user_msg));

            let assistant_msg = HistoryAssistantMessage::new("I will follow these instructions.");
            history.push(Message::Assistant(assistant_msg));
        } else if let Some(hint) = anti_pseudo_tag_hint {
            // 无系统消息内容，但仍需为 GPT 系模型注入反伪标签引导语
            let user_msg = HistoryUserMessage::new(hint.to_string(), model_id);
            history.push(Message::User(user_msg));

            let assistant_msg = HistoryAssistantMessage::new("I will follow these instructions.");
            history.push(Message::Assistant(assistant_msg));
        }
    } else if let Some(ref prefix) = thinking_prefix {
        // 没有系统消息但有thinking配置，插入新的系统消息
        let user_msg = HistoryUserMessage::new(prefix.clone(), model_id);
        history.push(Message::User(user_msg));

        let assistant_msg = HistoryAssistantMessage::new("I will follow these instructions.");
        history.push(Message::Assistant(assistant_msg));
    } else if let Some(hint) = anti_pseudo_tag_hint {
        // 没有系统消息、非 Claude thinking 协议模型（GPT 系），但客户端仍请求了
        // thinking：插入反伪标签引导语，避免模型自造 <analysis>/<summary> 等标签
        let user_msg = HistoryUserMessage::new(hint.to_string(), model_id);
        history.push(Message::User(user_msg));

        let assistant_msg = HistoryAssistantMessage::new("I will follow these instructions.");
        history.push(Message::Assistant(assistant_msg));
    }

    // 2. 处理常规消息历史
    // 最后一条消息作为 currentMessage，不加入历史
    // 经过 prefill 预处理后，messages 末尾必定是 user，故直接截掉最后一条即可
    let history_end_index = messages.len().saturating_sub(1);

    // 收集并配对消息
    let mut user_buffer: Vec<&crate::anthropic::types::Message> = Vec::new();
    let mut assistant_buffer: Vec<&crate::anthropic::types::Message> = Vec::new();

    for msg in &messages[..history_end_index] {
        if msg.role == "user" {
            // 先处理累积的 assistant 消息
            if !assistant_buffer.is_empty() {
                let merged = merge_assistant_messages(&assistant_buffer)?;
                history.push(Message::Assistant(merged));
                assistant_buffer.clear();
            }
            user_buffer.push(msg);
        } else if msg.role == "assistant" {
            // 先处理累积的 user 消息
            if !user_buffer.is_empty() {
                let merged_user = merge_user_messages(&user_buffer, model_id)?;
                history.push(Message::User(merged_user));
                user_buffer.clear();
            }
            // 累积 assistant 消息（支持连续多条）
            assistant_buffer.push(msg);
        }
    }

    // 处理末尾累积的 assistant 消息
    if !assistant_buffer.is_empty() {
        let merged = merge_assistant_messages(&assistant_buffer)?;
        history.push(Message::Assistant(merged));
    }

    // 处理结尾的孤立 user 消息
    if !user_buffer.is_empty() {
        let merged_user = merge_user_messages(&user_buffer, model_id)?;
        history.push(Message::User(merged_user));

        // 自动配对一个 "OK" 的 assistant 响应
        let auto_assistant = HistoryAssistantMessage::new("OK");
        history.push(Message::Assistant(auto_assistant));
    }

    Ok(history)
}

/// 合并多个 user 消息
pub(super) fn merge_user_messages(
    messages: &[&crate::anthropic::types::Message],
    model_id: &str,
) -> Result<HistoryUserMessage, ConversionError> {
    let mut content_parts = Vec::new();
    let mut all_images = Vec::new();
    let mut all_tool_results = Vec::new();

    for msg in messages {
        let (text, images, tool_results) = process_message_content(&msg.content)?;
        if !text.is_empty() {
            content_parts.push(text);
        }
        all_images.extend(images);
        all_tool_results.extend(tool_results);
    }

    let content = content_parts.join("\n");
    // 空 content 兜底：历史 user 消息中仅含 tool_result 时，Kiro 不接受空字符串。
    // 与 convert_request 保持同样占位词，避免 "Continue" 误导模型。
    let content = if content.is_empty() {
        "(tool result above)".to_string()
    } else {
        content
    };
    // 保留文本内容，即使有工具结果也不丢弃用户文本
    let mut user_msg = UserMessage::new(&content, model_id);

    if !all_images.is_empty() {
        user_msg = user_msg.with_images(all_images);
    }

    if !all_tool_results.is_empty() {
        let mut ctx = UserInputMessageContext::new();
        ctx = ctx.with_tool_results(all_tool_results);
        user_msg = user_msg.with_context(ctx);
    }

    Ok(HistoryUserMessage {
        user_input_message: user_msg,
    })
}

/// 转换 assistant 消息
pub(super) fn convert_assistant_message(
    msg: &crate::anthropic::types::Message,
) -> Result<HistoryAssistantMessage, ConversionError> {
    let mut text_content = String::new();
    let mut tool_uses = Vec::new();

    match &msg.content {
        serde_json::Value::String(s) => {
            text_content = s.clone();
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                if let Ok(block) = serde_json::from_value::<ContentBlock>(item.clone()) {
                    match block.block_type.as_str() {
                        // 历史消息中剥离 thinking 内容：thinking 仅对当轮推理有意义，
                        // 保留在 history 中会导致 payload 膨胀（Opus 每轮可产生数万字符），
                        // 触发 Kiro 400 "Improperly formed request"。
                        "thinking" => {}
                        "text" => {
                            if let Some(text) = block.text {
                                text_content.push_str(&text);
                            }
                        }
                        "tool_use" => {
                            if let (Some(id), Some(name)) = (block.id, block.name) {
                                let input = block.input.unwrap_or(serde_json::json!({}));
                                tool_uses.push(ToolUseEntry::new(id, name).with_input(input));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }

    // Kiro API 要求 content 字段不能为空，当只有 tool_use 时需要占位符。
    // 注意：此处与 user 侧（convert_request / merge_user_messages）的 "(tool result above)"
    // 策略不同 —— assistant 侧是"模型自己历史的 tool_use 调用"，仅需占位无需语义引导；
    // 而 user 侧需明示"上方为工具结果"以避免模型误读为"用户让我继续"。
    let final_content = if text_content.is_empty() && !tool_uses.is_empty() {
        " ".to_string()
    } else {
        text_content
    };

    // 确定性 messageId：基于 content + tool_use IDs 做 SHA-256，保证同一历史条目跨请求稳定
    let message_id = {
        let mut seed = String::from("assistant-msg:");
        seed.push_str(&final_content);
        for tu in &tool_uses {
            seed.push(':');
            seed.push_str(&tu.tool_use_id);
        }
        let hash = Sha256::digest(seed.as_bytes());
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            hash[0],
            hash[1],
            hash[2],
            hash[3],
            hash[4],
            hash[5],
            hash[6],
            hash[7],
            hash[8],
            hash[9],
            hash[10],
            hash[11],
            hash[12],
            hash[13],
            hash[14],
            hash[15]
        )
    };

    let mut assistant = AssistantMessage {
        message_id: Some(message_id),
        content: final_content,
        tool_uses: None,
    };
    if !tool_uses.is_empty() {
        assistant = assistant.with_tool_uses(tool_uses);
    }

    Ok(HistoryAssistantMessage {
        assistant_response_message: assistant,
    })
}

/// 合并多个连续的 assistant 消息为一条
/// 用于处理网络不稳定时产生的连续 assistant 消息（Issue #79）
pub(super) fn merge_assistant_messages(
    messages: &[&crate::anthropic::types::Message],
) -> Result<HistoryAssistantMessage, ConversionError> {
    assert!(!messages.is_empty());
    if messages.len() == 1 {
        return convert_assistant_message(messages[0]);
    }

    let mut all_tool_uses: Vec<ToolUseEntry> = Vec::new();
    let mut content_parts: Vec<String> = Vec::new();

    for msg in messages {
        let converted = convert_assistant_message(msg)?;
        let am = converted.assistant_response_message;
        if !am.content.trim().is_empty() {
            content_parts.push(am.content);
        }
        if let Some(tus) = am.tool_uses {
            all_tool_uses.extend(tus);
        }
    }

    let content = if content_parts.is_empty() && !all_tool_uses.is_empty() {
        " ".to_string()
    } else {
        content_parts.join("\n\n")
    };

    let message_id = {
        let mut seed = String::from("assistant-msg:");
        seed.push_str(&content);
        for tu in &all_tool_uses {
            seed.push(':');
            seed.push_str(&tu.tool_use_id);
        }
        let hash = Sha256::digest(seed.as_bytes());
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            hash[0],
            hash[1],
            hash[2],
            hash[3],
            hash[4],
            hash[5],
            hash[6],
            hash[7],
            hash[8],
            hash[9],
            hash[10],
            hash[11],
            hash[12],
            hash[13],
            hash[14],
            hash[15]
        )
    };

    let mut assistant = AssistantMessage {
        message_id: Some(message_id),
        content,
        tool_uses: None,
    };
    if !all_tool_uses.is_empty() {
        assistant = assistant.with_tool_uses(all_tool_uses);
    }
    Ok(HistoryAssistantMessage {
        assistant_response_message: assistant,
    })
}
