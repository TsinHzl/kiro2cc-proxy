// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! OpenAI Responses 请求 → Anthropic Messages 请求
//!
//! 与 [`super::chat_request`] 的差异集中在三处：
//!
//! 1. 对话历史在 `input` 数组里，且工具调用/工具结果是**与消息平级的 item**，
//!    而不是挂在 assistant 消息内部的 `tool_calls`
//! 2. system 提示走独立的 `instructions` 字段
//! 3. 工具声明有两个来源：顶层 `tools`，以及 `input` 里的 `additional_tools` item
//!    （Codex CLI 0.148 起只用后者），且多出 `custom`（自由文本入参）与
//!    `namespace`（工具分组容器）两类
//!
//! 文本压平、图片转换、工具 schema 规范化、`reasoning` 映射等公共逻辑直接复用
//! `chat_request` 中已验证的实现。
//!
//! # 无法表达的字段
//!
//! - `previous_response_id`：本代理无状态（不存储任何一轮响应），无法按 id 续接上下文。
//!   非空时直接 400，而不是静默丢弃后返回一个"忘记了前文"的回答。
//! - `include` 的 `reasoning.encrypted_content`：上游不产出加密推理内容，静默忽略。
//!   Codex CLI 每轮都会带上它，WARN 会变成刷屏噪声。
//! - `store` / `parallel_tool_calls` / `temperature` / `top_p` / `text.verbosity` /
//!   `truncation`：Kiro 上游无对应入参，一并忽略（与 `chat_request` 的处理一致）。
//! - `tool_choice`：下游管线无读取点，非 `auto` 时 WARN 留痕（既有限制）。

use serde_json::{Value, json};

use super::chat_request::{
    DEFAULT_MAX_TOKENS, MessageAccumulator, convert_reasoning_effort, flatten_text,
    warn_if_tool_choice_unsupported,
};
use super::model_map::map_model;
mod items;
mod tools;

#[cfg(test)]
mod tests;

use items::convert_input_items;
use tools::{COMPACTION_SYSTEM_PROMPT, ConvertedResponsesRequest, ToolCollector};

/// 把 OpenAI Responses 请求体转换为 Anthropic Messages 请求体
///
/// `Err` 的内容是面向客户端的错误消息（调用方负责包装为 OpenAI 400 错误结构）。
pub(crate) fn convert(body: &Value) -> Result<ConvertedResponsesRequest, String> {
    // 有状态请求必须在触达上游前拒绝：继续下去只会拿到缺失前文的错误回答
    if let Some(prev) = body.get("previous_response_id").and_then(Value::as_str)
        && !prev.trim().is_empty()
    {
        return Err(
            "不支持 previous_response_id：本代理无状态，请在 input 中回传完整对话历史".to_string(),
        );
    }

    let client_model = body
        .get("model")
        .and_then(Value::as_str)
        .filter(|m| !m.trim().is_empty())
        .ok_or_else(|| "字段 'model' 缺失或为空".to_string())?
        .to_string();

    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);

    let max_tokens = body
        .get("max_output_tokens")
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_MAX_TOKENS);

    let mut system = Vec::new();
    let instructions = flatten_text(body.get("instructions"));
    if !instructions.is_empty() {
        system.push(json!({"type": "text", "text": instructions}));
    }

    // 顶层 tools 先收，同名时压过 input 里 additional_tools 的声明
    let mut tools = ToolCollector::default();
    if let Some(list) = body.get("tools").and_then(Value::as_array) {
        tools.push_list(list, 0);
    }

    let mut acc = MessageAccumulator::default();
    // is_compaction 仅作标志，不做 early-return，两条路径共用一次 body 构建
    let is_compaction = match body.get("input") {
        // 最简形态：整段 prompt 就是一条 user 文本
        Some(Value::String(s)) => {
            if !s.is_empty() {
                acc.push("user", vec![json!({"type": "text", "text": s})]);
            }
            false
        }
        Some(Value::Array(items)) => {
            let is_comp = convert_input_items(items, &mut system, &mut acc, &mut tools);
            if is_comp {
                // 仅注入压缩摘要指令，不提前返回
                system.insert(0, json!({"type": "text", "text": COMPACTION_SYSTEM_PROMPT}));
            }
            is_comp
        }
        _ => return Err("字段 'input' 缺失或类型不支持（应为字符串或数组）".to_string()),
    };

    let messages = acc.into_messages();
    if messages.is_empty() {
        return Err("字段 'input' 未包含任何可转换的内容".to_string());
    }

    // 统一的 Anthropic body 构建逻辑，压缩与正常路径共用
    let mut anthropic = json!({
        "model": map_model(&client_model),
        "max_tokens": max_tokens,
        "messages": messages,
        "stream": stream,
    });

    if !system.is_empty() {
        anthropic["system"] = Value::Array(system);
    }

    if !tools.tools.is_empty() {
        anthropic["tools"] = Value::Array(std::mem::take(&mut tools.tools));
    }

    super::pass_through_user(body, &mut anthropic);

    warn_if_tool_choice_unsupported(body.get("tool_choice"));

    if let Some(thinking) = convert_reasoning_effort(
        body.get("reasoning").and_then(|r| r.get("effort")),
        max_tokens,
    ) {
        anthropic["thinking"] = thinking;
    }

    Ok(ConvertedResponsesRequest {
        client_model,
        stream,
        anthropic_body: anthropic,
        custom_tools: tools.custom,
        is_compaction,
    })
}
