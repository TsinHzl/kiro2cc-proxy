// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! input item 遍历与转换：message / function_call / tool_result / compaction 等 item
//! 分派到 system 块、消息累加器或工具收集器

use std::collections::HashSet;

use serde_json::{Value, json};

use super::tools::{
    DEFAULT_TOOL_NAMESPACE, ToolCollector, ToolInputForm, base64_decode_compaction,
    flatten_tool_name,
};
use crate::openai::chat_request::{
    MessageAccumulator, convert_image_url, flatten_text, parse_tool_arguments, part_text,
};

/// 遍历 `input` 数组，把各类 item 分派到 system 块、消息累加器或工具收集器
///
/// 返回值表示是否检测到 `compaction_trigger` item（即 Codex remote compaction v2 请求）。
pub(crate) fn convert_input_items(
    items: &[Value],
    system: &mut Vec<Value>,
    acc: &mut MessageAccumulator,
    tools: &mut ToolCollector,
) -> bool {
    let mut is_compaction = false;
    // 已转出 tool_use 的 id：工具输出只有在这里能找到对应调用时才能作为 tool_result 发出
    let mut emitted_tool_use_ids: HashSet<String> = HashSet::new();
    for item in items {
        // 少数客户端省略 type，只给 role + content
        let item_type = match item.get("type").and_then(Value::as_str) {
            Some(t) => t,
            None if item.get("role").is_some() => "message",
            None => {
                tracing::warn!("input item 既无 type 也无 role，已跳过");
                continue;
            }
        };

        match item_type {
            "message" => convert_message_item(item, system, acc),
            "function_call" => {
                if let Some(block) = tool_use_block(item, ToolInputForm::Json) {
                    record_tool_use_id(&block, &mut emitted_tool_use_ids);
                    acc.push("assistant", vec![block]);
                }
            }
            "custom_tool_call" => {
                if let Some(block) = tool_use_block(item, ToolInputForm::FreeText) {
                    record_tool_use_id(&block, &mut emitted_tool_use_ids);
                    acc.push("assistant", vec![block]);
                }
            }
            // 工具结果在 Anthropic 协议里属于 user 消息
            "function_call_output" | "custom_tool_call_output" => {
                let blocks = tool_output_blocks(item, &emitted_tool_use_ids);
                if !blocks.is_empty() {
                    acc.push("user", blocks);
                }
            }
            // Codex CLI 0.148 起把工具声明从顶层 `tools` 搬到了这个 developer item 里，
            // 顶层只剩 null。不认它就等于模型一个工具都看不到。
            "additional_tools" => {
                if let Some(list) = item.get("tools").and_then(Value::as_array) {
                    tools.push_list(list, 0);
                }
            }
            // Codex 会把上一轮的 reasoning item 原样回传。Anthropic 的 thinking 块需要配套
            // 签名，伪造签名回传只会增加被上游拒的风险；丢弃它不影响后续对话，故静默跳过
            // （每轮都出现，WARN 会成噪声）。
            "reasoning" => {}
            // Codex remote compaction v2：客户端在 input 末尾追加该 item 触发服务端压缩。
            // 上游（Kiro/Anthropic 协议）不支持此机制；由代理层拦截并在响应侧模拟。
            // 把标志位置为 true；来自上游的真实请求内容（其余 input items）已正常转换。
            "compaction_trigger" => {
                tracing::info!("检测到 compaction_trigger，启用 remote compaction v2 模拟模式");
                is_compaction = true;
            }
            // Codex 在后续轮次会把之前收到的 compaction item 原样回传（放入 input 数组）。
            // encrypted_content 是代理自己 base64 编码的摘要文本，解码后注入为 assistant 消息，
            // 让上游模型在后续对话中能感知到历史摘要内容。
            "compaction" | "compaction_summary" => {
                if let Some(encoded) = item.get("encrypted_content").and_then(Value::as_str) {
                    match base64_decode_compaction(encoded) {
                        Some(summary) if !summary.is_empty() => {
                            tracing::info!(
                                "解码 compaction item，注入历史摘要（{} 字节）",
                                summary.len()
                            );
                            acc.push("assistant", vec![json!({"type": "text", "text": summary})]);
                        }
                        _ => {
                            tracing::warn!(
                                "compaction item 的 encrypted_content 解码失败或为空，已跳过"
                            );
                        }
                    }
                } else {
                    tracing::warn!("compaction item 缺少 encrypted_content 字段，已跳过");
                }
            }
            other => {
                tracing::warn!(item_type = %other, "未识别的 input item 类型，已跳过");
            }
        }
    }
    is_compaction
}

/// `type: "message"` item → system 文本块或一条 Anthropic 消息
fn convert_message_item(item: &Value, system: &mut Vec<Value>, acc: &mut MessageAccumulator) {
    let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
    match role {
        // developer 是 system 的新名字，两者等价
        "system" | "developer" => {
            let text = flatten_text(item.get("content"));
            if !text.is_empty() {
                system.push(json!({"type": "text", "text": text}));
            }
        }
        "user" | "assistant" => {
            let blocks = convert_message_content(item.get("content"));
            if !blocks.is_empty() {
                acc.push(role, blocks);
            }
        }
        other => {
            tracing::warn!(role = %other, "未识别的 message role，已跳过");
        }
    }
}

/// message item 的 content（字符串或 part 数组）
fn convert_message_content(content: Option<&Value>) -> Vec<Value> {
    match content {
        Some(Value::String(s)) if !s.is_empty() => vec![json!({"type": "text", "text": s})],
        Some(Value::Array(parts)) => parts.iter().filter_map(convert_content_part).collect(),
        _ => Vec::new(),
    }
}

/// 单个 Responses content part → Anthropic content block
fn convert_content_part(part: &Value) -> Option<Value> {
    let part_type = part.get("type").and_then(Value::as_str).unwrap_or_default();
    match part_type {
        // input_text 出现在 user item，output_text 出现在 assistant 历史，text 是宽松写法
        "input_text" | "output_text" | "text" => {
            let text = part.get("text").and_then(Value::as_str).unwrap_or_default();
            (!text.is_empty()).then(|| json!({"type": "text", "text": text}))
        }
        // Responses 的 image_url 是裸字符串；同时兼容 Chat Completions 的嵌套对象写法
        "input_image" | "image_url" => {
            let field = part.get("image_url")?;
            let url = field
                .as_str()
                .or_else(|| field.get("url").and_then(Value::as_str))
                .unwrap_or_default();
            convert_image_url(url)
        }
        other => {
            tracing::warn!(part_type = %other, "未识别的 content part 类型，已跳过");
            None
        }
    }
}

/// `function_call` / `custom_tool_call` → Anthropic `tool_use` block
fn tool_use_block(item: &Value, form: ToolInputForm) -> Option<Value> {
    let id = call_id(item)?;
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .or_else(|| {
            tracing::warn!("工具调用 item 缺少 name，已跳过");
            None
        })?;
    // 带 namespace 的调用（Codex 的 MCP / 子代理工具）：name 只是叶子名。声明侧把这类工具
    // 展开成了扁平名，历史里回放的调用必须用同一规则算出同样的名字，否则与声明对不上
    let flat_name;
    let name = match item
        .get("namespace")
        .and_then(Value::as_str)
        .filter(|ns| !ns.is_empty() && *ns != DEFAULT_TOOL_NAMESPACE)
    {
        Some(ns) => {
            flat_name = flatten_tool_name(ns, name);
            flat_name.as_str()
        }
        None => name,
    };

    let input = match form {
        ToolInputForm::Json => parse_tool_arguments(item.get("arguments"), name),
        // 自由文本入参装进降级 schema 约定的 input 字段
        ToolInputForm::FreeText => {
            let text = item
                .get("input")
                .and_then(Value::as_str)
                .unwrap_or_else(|| {
                    tracing::warn!("custom_tool_call 缺少 input 字段，已降级为空文本");
                    ""
                });
            json!({"input": text})
        }
    };

    Some(json!({"type": "tool_use", "id": id, "name": name, "input": input}))
}

/// 记录已转出的 `tool_use` block 的 id
fn record_tool_use_id(block: &Value, ids: &mut HashSet<String>) {
    if let Some(id) = block.get("id").and_then(Value::as_str) {
        ids.insert(id.to_string());
    }
}

/// `function_call_output` / `custom_tool_call_output` → user content blocks
///
/// 能在前文找到对应 `tool_use` 的输出转成 Anthropic `tool_result` block。
///
/// 找不到的是孤立输出（issue #66：Codex Desktop 0.167 每轮把界面状态以没有 `call_id`、
/// 前面也没有 `function_call` 的 `function_call_output` 塞进历史）。作为 `tool_result`
/// 发出会被上游以 `unexpected tool_use_id` 拒绝，而且它进入历史后每次请求都会重复这个
/// 错误，会话就此卡死。这里改成带工具名标签的普通文本块，保留其中的上下文。
fn tool_output_blocks(item: &Value, emitted_tool_use_ids: &HashSet<String>) -> Vec<Value> {
    // 与 `tool_use_block` 的配对规则一致：优先 `call_id`，兼容只写 `id` 的客户端
    let paired_id = ["call_id", "id"]
        .iter()
        .filter_map(|k| item.get(*k).and_then(Value::as_str))
        .find(|s| !s.is_empty())
        .filter(|id| emitted_tool_use_ids.contains(*id));

    if let Some(id) = paired_id {
        return vec![json!({
            "type": "tool_result",
            "tool_use_id": id,
            "content": tool_output_content(item.get("output")),
        })];
    }

    let label = orphan_output_label(item.get("name").and_then(Value::as_str));
    // 孤立输出会进入历史并随每次请求重放，用 WARN 会按请求逐条刷日志（同 `reasoning` 的处理）
    tracing::debug!(
        tool = %label,
        "工具输出找不到对应的 tool_use，已降级为普通 user 文本"
    );
    match tool_output_content(item.get("output")) {
        Value::String(text) if text.is_empty() => Vec::new(),
        Value::String(text) => {
            vec![json!({"type": "text", "text": format!("<{label}>{text}</{label}>")})]
        }
        Value::Array(blocks) if blocks.is_empty() => Vec::new(),
        Value::Array(blocks) => {
            let mut out = Vec::with_capacity(blocks.len() + 2);
            out.push(json!({"type": "text", "text": format!("<{label}>")}));
            out.extend(blocks);
            out.push(json!({"type": "text", "text": format!("</{label}>")}));
            out
        }
        _ => Vec::new(),
    }
}

/// 孤立输出的标签名：取工具名，只保留适合做 XML 标签的字符，缺失或为空时用通用名
fn orphan_output_label(name: Option<&str>) -> String {
    let cleaned: String = name
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .collect();
    if cleaned.is_empty() {
        "tool_output".to_string()
    } else {
        cleaned
    }
}

/// 工具输出 → `tool_result.content`
///
/// 输出数组里带图片 part（Codex `view_image`、截图类工具）时，保留为 Anthropic
/// content 块数组，让图片随后进入 Kiro 的 `images`；否则仍压平成纯文本，与既有行为一致。
fn tool_output_content(output: Option<&Value>) -> Value {
    let Some(Value::Array(parts)) = output else {
        return Value::String(flatten_text(output));
    };
    if !parts.iter().any(is_image_part) {
        return Value::String(flatten_text(output));
    }
    let blocks = parts
        .iter()
        .filter_map(|part| {
            if is_image_part(part) {
                // 转换失败（缺 image_url / 空串）时留占位，不静默丢弃：否则只含此类 part 的
                // 输出会变成空 content 数组，与 Anthropic 层对未转发图片的占位策略也不一致
                convert_content_part(part).or_else(|| {
                    tracing::warn!("工具输出图片转换失败，降级为文本占位");
                    Some(json!({
                        "type": "text",
                        "text": "[image omitted: missing or empty image_url]"
                    }))
                })
            } else {
                part_text(part)
                    .filter(|text| !text.is_empty())
                    .map(|text| json!({"type": "text", "text": text}))
            }
        })
        .collect::<Vec<_>>();
    Value::Array(blocks)
}

fn is_image_part(part: &Value) -> bool {
    matches!(
        part.get("type").and_then(Value::as_str),
        Some("input_image" | "image_url")
    )
}

/// 取工具调用的配对 id：`call_id` 是协议字段，`id` 是部分客户端的写法
fn call_id(item: &Value) -> Option<String> {
    ["call_id", "id"]
        .iter()
        .filter_map(|k| item.get(*k).and_then(Value::as_str))
        .find(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            tracing::warn!("工具调用 item 缺少 call_id，已跳过（无法与 tool_use 配对）");
            None
        })
}
