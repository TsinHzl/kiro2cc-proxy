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
use super::prompt::{is_dynamic_hook_injection, normalize_billing_header};
use super::result::ConversionError;
use super::thinking::{
    generate_thinking_prefix, gpt_anti_pseudo_tag_hint, has_thinking_tags, is_gpt_model,
};

/// 历史 assistant 文本回传上游前的净化：先剥思考标记行 / 时长行，再剥 dim 样式码
/// （含模型模仿产生的字面 `[2m…[0m` 整行包裹），避免上游模型模仿该格式。
/// 顺序不可颠倒：标记行识别依赖 dim 转义包裹的兼容判断。
fn sanitize_rendered_history_text(text: &str) -> String {
    let stripped = crate::anthropic::stream::strip_rendered_thinking(text);
    crate::anthropic::stream::strip_dim_markers(&stripped).into_owned()
}

/// 构建历史消息
///
/// # Arguments
/// * `req` - 原始请求，用于读取 `thinking` 等配置字段
/// * `messages` - 移除内联 system 和末尾 assistant prefill 后的消息引用，末尾必为 user。
/// * `system` - 顶层 system 与内联 system 按顺序归并后的文本，不改变原请求。
/// * `model_id` - 已映射的 Kiro 模型 ID
pub(super) fn build_history(
    req: &MessagesRequest,
    messages: &[&crate::anthropic::types::Message],
    system: Option<&[&str]>,
    model_id: &str,
    session_id: &str,
) -> Result<(Vec<Message>, String), ConversionError> {
    let mut history = Vec::new();

    // 生成 thinking 前缀（仅 luna 不使用 Claude/Kiro 文本控制标签）
    let thinking_prefix = generate_thinking_prefix(req, model_id);
    // GPT 系模型在客户端请求 thinking 时，额外注入反伪标签引导语（见函数文档）
    let anti_pseudo_tag_hint = gpt_anti_pseudo_tag_hint(req, model_id);

    // 1. 处理系统消息
    // 仅 GPT 系：CC 中途注入的动态块（hook 输出、工具/MCP 状态通知）逐轮累积，
    // 留在 history[0] 会使冻结缓存 key 每轮漂移、前缀缓存持续 miss。这里把它们
    // 从 history[0] 拼接中剔除，并返回给调用方放到当前消息开头——模型仍可见
    // （hook 注入的用户规则不丢失），但不参与缓存 hash。
    // 非 GPT 模型不分流：stable_system 即完整 system，dynamic 恒为空，
    // history[0] 与此前逐字节一致。
    let (stable_system, dynamic_system): (Vec<&str>, Vec<&str>) = match system {
        Some(blocks) if is_gpt_model(model_id) => blocks
            .iter()
            .copied()
            .partition(|s| !is_dynamic_hook_injection(s)),
        Some(blocks) => (blocks.to_vec(), Vec::new()),
        None => (Vec::new(), Vec::new()),
    };
    // 全部块都被分流（stable 为空）时按"无系统消息"处理，保留 thinking 前缀 / 引导语注入
    let system_present =
        system.is_some() && (dynamic_system.is_empty() || !stable_system.is_empty());
    let dynamic_content = dynamic_system.join("\n\n");

    if system_present {
        let system_content = stable_system.join("\n");

        if !system_content.is_empty() {
            // 注入thinking标签到系统消息最前面（如果需要且不存在）
            let static_content = if let Some(ref prefix) = thinking_prefix {
                if !has_thinking_tags(&system_content) {
                    format!("{}\n{}", prefix, system_content)
                } else {
                    system_content
                }
            } else {
                system_content
            };

            // 追加 GPT 反伪标签引导语（仅当请求携带 thinking 配置时）
            let final_content = if let Some(hint) = anti_pseudo_tag_hint {
                format!("{}\n{}", static_content, hint)
            } else {
                static_content
            };

            // 将 cch= 固定为 0，使 history[0] 跨请求稳定，命中 Kiro prompt cache。
            let cache_content = normalize_billing_header(final_content);

            // 只冻结稳定系统内容；reminder 保留在原消息中，不搬入系统缓存。
            // 完整内容参与 key，避免前缀相同的辅助请求串槽。
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

                if let Some(entry) = map.get_mut(&h0_key) {
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
                }
            };

            // 系统消息作为 user + assistant 配对
            let user_msg = HistoryUserMessage::new(final_content, model_id);
            history.push(Message::User(user_msg));

            let assistant_msg = HistoryAssistantMessage::new(HistoryAssistantMessage::SYSTEM_ACK);
            history.push(Message::Assistant(assistant_msg));
        } else if let Some(hint) = anti_pseudo_tag_hint {
            // 无系统消息内容，但仍需为 GPT 系模型注入反伪标签引导语
            let user_msg = HistoryUserMessage::new(hint.to_string(), model_id);
            history.push(Message::User(user_msg));

            let assistant_msg = HistoryAssistantMessage::new(HistoryAssistantMessage::SYSTEM_ACK);
            history.push(Message::Assistant(assistant_msg));
        }
    } else if let Some(ref prefix) = thinking_prefix {
        // 没有系统消息但有thinking配置，插入新的系统消息
        let user_msg = HistoryUserMessage::new(prefix.clone(), model_id);
        history.push(Message::User(user_msg));

        let assistant_msg = HistoryAssistantMessage::new(HistoryAssistantMessage::SYSTEM_ACK);
        history.push(Message::Assistant(assistant_msg));
    } else if let Some(hint) = anti_pseudo_tag_hint {
        // 没有系统消息、非 Claude thinking 协议模型（GPT 系），但客户端仍请求了
        // thinking：插入反伪标签引导语，避免模型自造 <analysis>/<summary> 等标签
        let user_msg = HistoryUserMessage::new(hint.to_string(), model_id);
        history.push(Message::User(user_msg));

        let assistant_msg = HistoryAssistantMessage::new(HistoryAssistantMessage::SYSTEM_ACK);
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

    Ok((history, dynamic_content))
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
            text_content = sanitize_rendered_history_text(s);
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                match serde_json::from_value::<ContentBlock>(item.clone()) {
                    Ok(block) => match block.block_type.as_str() {
                        // 原生 thinking 块在历史中整块丢弃：thinking 仅对当轮推理有意义，
                        // 保留在 history 中会导致 payload 膨胀（Opus 每轮可产生数万字符），
                        // 触发 Kiro 400 "Improperly formed request"。
                        "thinking" => {}
                        "text" => {
                            if let Some(text) = block.text {
                                // thinkingAsText 渲染出的 text 块：仅剥首行「💭 Thinking」标记，
                                // 思考正文按普通助手文本保留（上下文略增，已确认接受）。
                                // 注意与上方原生 thinking 的处置差异 —— 前者整块丢弃，后者保留正文。
                                text_content.push_str(&sanitize_rendered_history_text(&text));
                            }
                        }
                        "tool_use" => {
                            if let (Some(id), Some(name)) = (block.id, block.name) {
                                let input = block.input.unwrap_or(serde_json::json!({}));
                                tool_uses.push(ToolUseEntry::new(id, name).with_input(input));
                            }
                        }
                        _ => {}
                    },
                    Err(e) => {
                        tracing::warn!("历史内容块反序列化失败，块被丢弃: {}", e);
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
