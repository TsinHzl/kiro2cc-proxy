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
use super::message::process_message_content_with_tool_filter;
use super::prompt::{
    is_dynamic_hook_injection, is_token_budget_reminder, normalize_billing_header,
};
use super::result::ConversionError;
use super::thinking::{generate_thinking_prefix, gpt_anti_pseudo_tag_hint, has_thinking_tags};

/// 历史 assistant 文本回传上游前的净化：先整块剥离文本化思考（标记行 + 正文 + 时长行），
/// 再剥末尾的 credits 汇总行，最后剥 dim 样式码（含模型模仿产生的字面 `[2m…[0m` 整行包裹），
/// 避免上游模型模仿该格式。
/// 顺序不可颠倒：标记行与汇总行的识别都依赖 dim 转义包裹的兼容判断。
fn sanitize_rendered_history_text(text: &str) -> String {
    let stripped = crate::anthropic::stream::strip_rendered_thinking(text);
    let stripped = crate::anthropic::stream::strip_credits_footer(&stripped);
    crate::anthropic::stream::strip_dim_markers(&stripped).into_owned()
}

/// user 消息正文为空（如只含 tool_result）时的占位词。
///
/// 用中性提示明示"上方为工具结果"，避免用 "Continue" 让模型误判为"用户让我继续"。
/// assistant 侧的空 content 占位符策略不同，不使用该常量。
pub(crate) const EMPTY_USER_CONTENT_PLACEHOLDER: &str = "(tool result above)";

/// 把正文包进 `<system-reminder>` 块。
pub(super) fn wrap_system_reminder(body: &str) -> String {
    format!("<system-reminder>\n{body}\n</system-reminder>")
}

/// 系统区的一个文本块及其来源。
///
/// 顶层 `system`、会话开头的内联 system、代理自己追加的约束，`anchor` 都是 `None`；
/// 会话中途（至少有一条 user/assistant 消息之后）出现的内联 `role:"system"` 消息带锚点，
/// 指向它在对话里相邻的 user 消息。
#[derive(Clone, Copy)]
pub(super) struct SysBlock<'a> {
    pub text: &'a str,
    pub anchor: Option<InlineAnchor>,
}

impl<'a> SysBlock<'a> {
    pub(super) fn plain(text: &'a str) -> Self {
        Self { text, anchor: None }
    }
}

/// 内联 system 消息在对话中的位置：挂到第 `msg` 条（`messages` 下标）user 消息的
/// 开头（`after=false`）或末尾（`after=true`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InlineAnchor {
    pub msg: usize,
    pub after: bool,
}

/// 挂到某条 user 消息前后的动态通知文本（按出现顺序）。
#[derive(Default)]
pub(super) struct Attach<'a> {
    before: Vec<&'a str>,
    after: Vec<&'a str>,
}

impl<'a> Attach<'a> {
    fn push(&mut self, anchor: InlineAnchor, text: &'a str) {
        if anchor.after {
            self.after.push(text);
        } else {
            self.before.push(text);
        }
    }

    /// 把通知包进 `<system-reminder>` 后与消息正文拼接：开头的在前，末尾的在后。
    /// 正文为空（如只含 tool_result 的消息）时补占位词，保持与既有空 content 兜底一致。
    pub(super) fn render(&self, text: String) -> String {
        if self.before.is_empty() && self.after.is_empty() {
            return text;
        }
        let wrap = |blocks: &[&str]| wrap_system_reminder(&blocks.join("\n\n"));
        let mut parts = Vec::with_capacity(3);
        if !self.before.is_empty() {
            parts.push(wrap(&self.before));
        }
        parts.push(if text.is_empty() {
            EMPTY_USER_CONTENT_PLACEHOLDER.to_string()
        } else {
            text
        });
        if !self.after.is_empty() {
            parts.push(wrap(&self.after));
        }
        parts.join("\n\n")
    }
}

/// `build_history` 的产出
pub(super) struct BuiltHistory<'a> {
    pub history: Vec<Message>,
    /// 无法就地放置的动态块（顶层 system 里的动态块、token 预算提示等），
    /// 由调用方放到当前消息开头
    pub dynamic_content: String,
    /// 挂到当前（最后一条）user 消息前后的内联通知
    pub current_attach: Option<Attach<'a>>,
    pub prefix_cached: bool,
}

/// 构建历史消息
///
/// # Arguments
/// * `req` - 原始请求，用于读取 `thinking` 等配置字段
/// * `messages` - 移除内联 system 和末尾 assistant prefill 后的消息引用，末尾必为 user。
/// * `system` - 顶层 system 与内联 system 按顺序归并后的文本块，不改变原请求。
/// * `model_id` - 已映射的 Kiro 模型 ID
pub(super) fn build_history<'a>(
    req: &MessagesRequest,
    messages: &[&crate::anthropic::types::Message],
    system: Option<&[SysBlock<'a>]>,
    model_id: &str,
    session_id: &str,
) -> Result<BuiltHistory<'a>, ConversionError> {
    let mut history = Vec::new();
    let mut h0_frozen = true;

    // 生成 thinking 前缀（仅 luna 不使用 Claude/Kiro 文本控制标签）
    let thinking_prefix = generate_thinking_prefix(req, model_id);
    // GPT 系模型在客户端请求 thinking 时，额外注入反伪标签引导语（见函数文档）
    let anti_pseudo_tag_hint = gpt_anti_pseudo_tag_hint(req, model_id);

    // 1. 处理系统消息
    // 所有模型：CC 中途注入的动态块（hook 输出、工具/MCP 状态通知）逐轮累积，
    // 留在 history[0] 会使冻结缓存 key 每轮漂移、history[0] 之后的整段前缀缓存
    // 持续 miss（issue #47：claude-opus-5.5 会话 history[0] 每轮 +50~+970 字节）。
    // 这里把它们从 history[0] 拼接中剔除，模型仍可见（hook 注入的用户规则不丢失），
    // 但不参与缓存 hash。放置位置：
    // - 来自会话中途内联 system 消息的块（带锚点）：就地挂回它在对话里相邻的 user 消息
    //   （issue #59：此前一律放到当前消息开头，客户端每轮回传完整历史，
    //   几十轮前的一次性通知每轮都被当成新通知重复提醒）；
    // - 其余（顶层 system 里的动态块、token 预算提示）：返回给调用方放到当前消息开头。
    let blocks: &[SysBlock<'a>] = system.unwrap_or(&[]);
    let (stable_blocks, mut dynamic_blocks): (Vec<SysBlock<'a>>, Vec<SysBlock<'a>>) = blocks
        .iter()
        .copied()
        .partition(|b| !is_dynamic_hook_injection(b.text) && !is_token_budget_reminder(b.text));
    // 文本前缀只能识别已知的动态块；CC 还会中途追加按路径加载的 rules 等新块（实测一次
    // 4KB 的 rules 块使 history[0] 重置、单轮多花约 0.5 credits）。再按会话基线把
    // 「会话开始后才出现」的块也分流出去，与块的文本无关（见 `baseline` 模块）。
    let stable_texts: Vec<&str> = stable_blocks.iter().map(|b| b.text).collect();
    let new_flags = super::baseline::new_block_flags(session_id, &stable_texts);
    let mut stable_system: Vec<&str> = Vec::with_capacity(stable_blocks.len());
    for (block, is_new) in stable_blocks.into_iter().zip(new_flags) {
        if is_new {
            dynamic_blocks.push(block);
        } else {
            stable_system.push(block.text);
        }
    }
    // 全部块都被分流（stable 拼接结果为空，含仅剩空文本块）时按"无系统消息"处理，
    // 保留 thinking 前缀 / 引导语注入
    let stable_content = stable_system.join("\n");
    let diverted_texts: Vec<&str> = dynamic_blocks.iter().map(|b| b.text).collect();
    super::drift_diag::log_diverted(session_id, &diverted_texts);
    let system_present =
        system.is_some() && (dynamic_blocks.is_empty() || !stable_content.is_empty());

    // 把被分流的块按锚点拆成「就地挂回」与「放当前消息开头」两类。
    // token 预算提示是逐轮变化的状态而不是一次性事件，仍放当前消息开头，
    // 避免历史消息里的预算数值参与前缀缓存哈希。空白块没有可挂的内容，也走开头路径。
    let mut head_blocks: Vec<&str> = Vec::new();
    let mut attached: HashMap<usize, Attach<'a>> = HashMap::new();
    for block in &dynamic_blocks {
        match block.anchor {
            Some(anchor)
                if !is_token_budget_reminder(block.text) && !block.text.trim().is_empty() =>
            {
                attached
                    .entry(anchor.msg)
                    .or_default()
                    .push(anchor, block.text);
            }
            _ => head_blocks.push(block.text),
        }
    }
    let dynamic_content = head_blocks.join("\n\n");

    if system_present {
        let system_content = stable_content;

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
                    super::drift_diag::observe(session_id, &stable_system, false);
                    tracing::info!(
                        "[exp2] history[0] frozen hash={} len={} session={}",
                        &h0_key[h0_key.len().saturating_sub(16)..],
                        entry.value.len(),
                        session_id
                    );
                    entry.value.clone()
                } else {
                    h0_frozen = false;
                    super::drift_diag::observe(session_id, &stable_system, true);
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
    let mut user_buffer: Vec<(&crate::anthropic::types::Message, Option<&Attach<'a>>)> = Vec::new();
    let mut assistant_buffer: Vec<&crate::anthropic::types::Message> = Vec::new();

    for (index, msg) in messages[..history_end_index].iter().enumerate() {
        if msg.role == "user" {
            // 先处理累积的 assistant 消息
            if !assistant_buffer.is_empty() {
                let merged = merge_assistant_messages(&assistant_buffer)?;
                history.push(Message::Assistant(merged));
                assistant_buffer.clear();
            }
            user_buffer.push((msg, attached.get(&index)));
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

    Ok(BuiltHistory {
        history,
        dynamic_content,
        current_attach: attached.remove(&history_end_index),
        prefix_cached: determine_prefix_cached(history_end_index > 0, h0_frozen),
    })
}

/// `prefix_cached` 判定：本请求的缓存前缀（history[0] + 历史轮次）是否已命中会话级冻结缓存。
///
/// 完整判定矩阵（`h0_frozen` 仅在 `system_present && !stable_content.is_empty()`
/// 分支走 `PREV_H0` 查表时才可能被置 `false`，其余分支保持初始值 `true`）：
///
/// | system_present | stable_content | h0_frozen 取值 | prefix_cached 决定因素 |
/// |---|---|---|---|
/// | true  | 非空 | PREV_H0 查表实际结果 | 历史轮次 × 查表命中 |
/// | true  | 空（全被分流） | 恒 true | 仅历史轮次 |
/// | false | —    | 恒 true | 仅历史轮次 |
///
/// 「无 system 分支仅由历史轮次门控」的前提：Anthropic Messages 协议无状态，客户端
/// 每次请求全量重发历史，无 system 会话的前缀（tools + 历史）跨轮次字节稳定，
/// 第 2 轮起计入 cache_read 是正确近似；若未来引入增量发送模式需重新评估。
/// 详见 `result.rs::ConversionResult::prefix_cached` 文档。
fn determine_prefix_cached(has_history_turns: bool, h0_frozen: bool) -> bool {
    has_history_turns && h0_frozen
}

/// 合并多个 user 消息；每条消息可带挂在其前后的内联 system 通知
pub(super) fn merge_user_messages(
    messages: &[(&crate::anthropic::types::Message, Option<&Attach<'_>>)],
    model_id: &str,
) -> Result<HistoryUserMessage, ConversionError> {
    let mut content_parts = Vec::new();
    let mut all_images = Vec::new();
    let mut all_tool_results = Vec::new();

    for (msg, attach) in messages {
        let (text, images, tool_results) =
            process_message_content_with_tool_filter(&msg.content, all_images.len(), |_| true)?;
        let text = match attach {
            Some(attach) => attach.render(text),
            None => text,
        };
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
        EMPTY_USER_CONTENT_PLACEHOLDER.to_string()
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
                                // thinkingAsText 渲染出的 text 块：与上方原生 thinking 一致整块
                                // 丢弃（标记行 + 思考正文 + 时长行），上游上下文不含思考内容；
                                // 仅在找不到时长行（流被中断）时降级为只剥标记行。
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

    // Kiro API 要求 content 字段不能为空：只有 tool_use，或文本全是被丢弃的思考块
    // （thinking / 文本化思考）而清空时，都需要占位符。
    // 注意：此处与 user 侧（convert_request / merge_user_messages）的 "(tool result above)"
    // 策略不同 —— assistant 侧是"模型自己历史的 tool_use 调用"，仅需占位无需语义引导；
    // 而 user 侧需明示"上方为工具结果"以避免模型误读为"用户让我继续"。
    let final_content = if text_content.is_empty() {
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

    // 各条的占位符 " " 已被上面的 trim 过滤；全部为空（只有 tool_use，或全是被丢弃的
    // 思考块）时必须重新补占位符，与 convert_assistant_message 的非空约束保持一致
    let content = if content_parts.is_empty() {
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
