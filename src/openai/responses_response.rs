// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic Messages 响应 → OpenAI Responses 响应
//!
//! 与 Chat Completions 的关键差异：产物是一棵 `response` 对象树（`output[]` 里每个
//! item 有独立 id 与 status），而不是 `choices[]`；截断不体现在 `finish_reason`，
//! 而是 `status: "incomplete"` + `incomplete_details.reason`。
//!
//! 响应的 `model` 字段一律回写**客户端请求的原始模型名**（Codex 会校验一致性）。
//!
//! # custom 工具的还原
//!
//! 请求侧把 `custom` 工具声明降级成 `{"input": string}` 的 JSON schema
//! （[`super::responses_request`]），上游因此以普通 `tool_use` 块回调。客户端只认
//! `custom_tool_call` item（入参是自由文本），所以这里要按请求侧给出的 custom 工具名
//! 集合把它还原回去——否则 Codex 会把调用当成未知工具。
//!
//! # reasoning 的处理
//!
//! 上游 thinking 文本会作为 `type: "reasoning"` item 放在 `output` 首位（与 OpenAI 的
//! 顺序一致），但不带 `encrypted_content`——本代理产不出可回传的加密推理内容，客户端
//! 把它原样回传时会被 [`super::responses_request`] 静默丢弃。

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use uuid::Uuid;

use super::chat_response::{
    INPUT_JSON_DELTA, SIGNATURE_DELTA, TEXT_DELTA, THINKING_DELTA, unix_now,
};

/// 生成 `<prefix>_<32位hex>` 形式的 id
fn new_id(prefix: &str) -> String {
    format!("{}_{}", prefix, Uuid::new_v4().simple())
}

/// 上游 `stop_reason` 是否表示输出被截断
///
/// 两种取值都来自 `src/anthropic/handlers.rs`：`max_tokens` 是命中 `max_tokens` 上限，
/// `model_context_window_exceeded` 是上下文窗口耗尽。对 Responses 客户端而言两者都是
/// "没写完就停了"，统一映射为 `max_output_tokens`。
fn is_truncated(stop_reason: Option<&str>) -> bool {
    matches!(
        stop_reason,
        Some("max_tokens") | Some("model_context_window_exceeded")
    )
}

/// 把 Anthropic 的 usage 换算为 Responses usage（字段名与 Chat Completions 不同）
///
/// 输入总量口径与 `cached_tokens` 取值的理由见 `chat_response::convert_usage` 的说明。
fn convert_usage(usage: Option<&Value>) -> Value {
    let read = |key: &str| {
        usage
            .and_then(|u| u.get(key))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    };
    let cached = read("cache_read_input_tokens");
    let input = read("input_tokens") + cached + read("cache_creation_input_tokens");
    let output = read("output_tokens");
    json!({
        "input_tokens": input,
        "output_tokens": output,
        "total_tokens": input + output,
        "input_tokens_details": {"cached_tokens": cached},
    })
}

/// 从 Anthropic content 块数组中分拣出的内容
#[derive(Debug, Default)]
struct ExtractedContent {
    text: String,
    reasoning: String,
    /// 已转换为 Responses `function_call` / `custom_tool_call` item 的工具调用
    tool_calls: Vec<Value>,
}

/// 遍历 Anthropic content 数组，按块类型分拣
///
/// `thinking` 块的 `signature` 字段是为通过下游检测伪造的无语义串
/// （`src/anthropic/stream.rs` 的 `generate_fake_signature`），只取 `thinking` 文本。
fn extract_content(content: Option<&Value>, custom_tools: &HashSet<String>) -> ExtractedContent {
    let mut out = ExtractedContent::default();
    let Some(blocks) = content.and_then(Value::as_array) else {
        return out;
    };

    for block in blocks {
        match block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "text" => {
                if let Some(t) = block.get("text").and_then(Value::as_str) {
                    out.text.push_str(t);
                }
            }
            "thinking" => {
                if let Some(t) = block.get("thinking").and_then(Value::as_str) {
                    out.reasoning.push_str(t);
                }
            }
            "tool_use" => {
                if let Some(call) = tool_use_to_item(block, custom_tools) {
                    out.tool_calls.push(call);
                }
            }
            other => {
                tracing::warn!(block_type = %other, "未识别的上游 content 块类型，已跳过");
            }
        }
    }

    out
}

/// Anthropic `tool_use` block → Responses `function_call` / `custom_tool_call` item
///
/// `call_id` 直接用上游的 `tool_use.id`：客户端下一轮会以该值回传
/// `function_call_output`，请求侧再原样还原为 `tool_result.tool_use_id`。
fn tool_use_to_item(block: &Value, custom_tools: &HashSet<String>) -> Option<Value> {
    let id = block.get("id").and_then(Value::as_str);
    let name = block.get("name").and_then(Value::as_str);
    // 缺字段的块只能跳过，但必须留痕：否则客户端收到的响应会莫名少一次工具调用且无从排查
    let (Some(id), Some(name)) = (id, name) else {
        tracing::warn!(
            has_id = id.is_some(),
            has_name = name.is_some(),
            "上游 tool_use 块缺少 id 或 name，已跳过该工具调用"
        );
        return None;
    };

    if custom_tools.contains(name) {
        return Some(json!({
            "type": "custom_tool_call",
            "id": new_id("ctc"),
            "call_id": id,
            "name": name,
            "input": custom_input_from_value(block.get("input")),
            "status": "completed",
        }));
    }

    // Responses 的 arguments 与 Chat Completions 一致，是 JSON 字符串而非对象
    let arguments = block
        .get("input")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "{}".to_string());

    Some(json!({
        "type": "function_call",
        "id": new_id("fc"),
        "call_id": id,
        "name": name,
        "arguments": arguments,
        "status": "completed",
    }))
}

/// 从降级 schema 的入参对象里取回 custom 工具的自由文本
///
/// 正常情况是 `{"input": "<原文>"}`（降级 schema 只有这一个字段）。模型没照 schema 作答时
/// 退化为整段 JSON 原文——宁可让客户端收到多余的包装，也不能给它空串。
fn custom_input_from_value(input: Option<&Value>) -> String {
    match input {
        Some(v) => match v.get("input").and_then(Value::as_str) {
            Some(s) => s.to_string(),
            None => v.to_string(),
        },
        None => String::new(),
    }
}

/// 同 [`custom_input_from_value`]，输入是流式累积出的 JSON 文本
fn custom_input_from_json_text(raw: &str) -> String {
    match serde_json::from_str::<Value>(raw) {
        Ok(v) => custom_input_from_value(Some(&v)),
        // 上游可能在参数发完前就断了，半截 JSON 原样交给客户端，由它决定怎么处理
        Err(_) => raw.to_string(),
    }
}

/// 构造一个 `type: "message"` 的 output item
fn message_item(text: &str) -> Value {
    json!({
        "type": "message",
        "id": new_id("msg"),
        "status": "completed",
        "role": "assistant",
        "content": [{"type": "output_text", "text": text, "annotations": []}],
    })
}

/// 构造一个 `type: "reasoning"` 的 output item
fn reasoning_item(text: &str) -> Value {
    json!({
        "type": "reasoning",
        "id": new_id("rs"),
        "summary": [{"type": "summary_text", "text": text}],
    })
}

/// 把 Anthropic 非流式响应转换为 Responses `response` 对象
pub(crate) fn convert_non_stream(
    anthropic: &Value,
    client_model: &str,
    custom_tools: &HashSet<String>,
) -> Value {
    let extracted = extract_content(anthropic.get("content"), custom_tools);
    let stop_reason = anthropic.get("stop_reason").and_then(Value::as_str);

    let mut output = Vec::new();
    if !extracted.reasoning.is_empty() {
        output.push(reasoning_item(&extracted.reasoning));
    }
    // 只有工具调用、没有可见文本时不产出空的 message item
    if !extracted.text.is_empty() {
        output.push(message_item(&extracted.text));
    }
    output.extend(extracted.tool_calls);

    let mut response = json!({
        "id": new_id("resp"),
        "object": "response",
        "created_at": unix_now(),
        "status": "completed",
        "model": client_model,
        "output": output,
        "usage": convert_usage(anthropic.get("usage")),
    });

    if is_truncated(stop_reason) {
        response["status"] = json!("incomplete");
        response["incomplete_details"] = json!({"reason": "max_output_tokens"});
    }

    response
}

// === 流式 ===
//
// 事件名与字段布局参考 CLIProxyAPI（MIT License）对 Codex Responses 流的实现，
// 以及 OpenAI Responses API 的公开事件文档。Codex CLI 依赖以下不变量：
//
// 1. 每个事件的 `sequence_number` 从 0 起严格递增（跨所有事件类型共用一个计数器）
// 2. 每个 output item 必须成对出现 `response.output_item.added` / `.done`
// 3. 文本与推理的 `output_index` 不同（它们是两个独立的 output item）
// 4. 流必须以 `response.completed` 或 `response.incomplete` 收尾，否则客户端会一直等

/// 上游一个 content block 只映射一个 part，故 part 序号恒为 0
const SINGLE_PART_INDEX: i64 = 0;

/// 一个已打开、尚未收尾的 output item
struct OpenItem {
    output_index: i64,
    /// 该 item 的 id（`msg_` / `rs_` / `fc_` 前缀）
    item_id: String,
    kind: OpenKind,
    /// 已累积的增量文本（收尾事件要给出完整值）
    buffer: String,
}

/// output item 的三种形态，对应上游的 text / thinking / tool_use 块
enum OpenKind {
    Text,
    Reasoning,
    ToolCall {
        call_id: String,
        name: String,
        /// 声明为 `custom` 的工具，收尾时要产出 `custom_tool_call` 而非 `function_call`
        custom: bool,
    },
}

/// Anthropic SSE → OpenAI Responses SSE 转换状态机
///
/// 逐事件喂入 [`ResponsesStreamConverter::on_event`]，返回若干条待下发的 SSE 帧文本
/// （已含 `event:` / `data:` 行与结尾空行）。上游流结束后调用
/// [`ResponsesStreamConverter::finish`] 收尾，保证即使上游中途断开，客户端也能拿到
/// 闭合的 item 与终止事件。
pub(crate) struct ResponsesStreamConverter {
    id: String,
    created_at: i64,
    /// 客户端请求的原始模型名
    model: String,
    sequence: i64,
    created_sent: bool,
    finished: bool,
    truncated: bool,
    usage: Option<Value>,
    next_output_index: i64,
    /// Anthropic block index → 打开中的 output item
    open: HashMap<i64, OpenItem>,
    /// 已收尾的 output item，用于 `response.completed` 的快照
    completed_items: Vec<Value>,
    /// 请求侧声明为 `custom` 的工具名
    custom_tools: HashSet<String>,
}

impl ResponsesStreamConverter {
    pub(crate) fn new(client_model: &str, custom_tools: HashSet<String>) -> Self {
        Self {
            id: new_id("resp"),
            created_at: unix_now(),
            model: client_model.to_string(),
            sequence: 0,
            created_sent: false,
            finished: false,
            truncated: false,
            usage: None,
            next_output_index: 0,
            open: HashMap::new(),
            completed_items: Vec::new(),
            custom_tools,
        }
    }

    /// 处理一个上游事件，返回待下发的 SSE 帧
    pub(crate) fn on_event(&mut self, name: &str, data: &Value) -> Vec<String> {
        match name {
            "message_start" => self.ensure_created(),
            "content_block_start" => self.on_block_start(data),
            "content_block_delta" => self.on_block_delta(data),
            "content_block_stop" => {
                let index = block_index(data);
                self.close_item(index)
            }
            "message_delta" => {
                if let Some(reason) = data
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(Value::as_str)
                {
                    self.truncated = is_truncated(Some(reason));
                }
                if let Some(usage) = data.get("usage") {
                    self.usage = Some(convert_usage(Some(usage)));
                }
                Vec::new()
            }
            "message_stop" => self.finish(),
            "error" => self.on_error(data),
            other => {
                tracing::warn!(event = %other, "未识别的上游 SSE 事件，已跳过");
                Vec::new()
            }
        }
    }

    /// 上游流结束时收尾：闭合所有未完成的 item，再下发终止事件
    pub(crate) fn finish(&mut self) -> Vec<String> {
        if self.finished {
            return Vec::new();
        }
        // 即使上游一个内容块都没给，也要让客户端看到合法的事件序列
        let mut frames = self.ensure_created();
        frames.extend(self.close_all_open());

        self.finished = true;
        let status = if self.truncated {
            "incomplete"
        } else {
            "completed"
        };
        let event_name = if self.truncated {
            "response.incomplete"
        } else {
            "response.completed"
        };
        let snapshot = self.snapshot(status);
        frames.push(self.event(event_name, json!({"response": snapshot})));
        frames
    }

    /// 仍打开的 item 按 output_index 升序补齐收尾事件
    ///
    /// 保证不变量「每个 output item 的 `added` / `done` 成对」在任何终止路径下都成立。
    fn close_all_open(&mut self) -> Vec<String> {
        let mut pending: Vec<i64> = self.open.keys().copied().collect();
        pending.sort_by_key(|k| self.open[k].output_index);
        let mut frames = Vec::new();
        for index in pending {
            frames.extend(self.close_item(index));
        }
        frames
    }

    /// 流式过程中上游报错：下发 error 事件后终止，不伪装成正常结束
    ///
    /// 不再补 `response.completed`——那会让客户端把半截输出当成完整回答。但已打开的 item
    /// 仍要闭合：`error` 之后不会再有任何事件，漏掉 `output_item.done` 会让客户端一直
    /// 等一个永不到来的收尾帧。
    fn on_error(&mut self, data: &Value) -> Vec<String> {
        if self.finished {
            return Vec::new();
        }
        let (message, error_type, code) = super::error::extract_stream_error(data);
        tracing::warn!(error_type = %error_type, "上游流式响应报错，已下发 error 事件并终止");

        let mut frames = self.ensure_created();
        frames.extend(self.close_all_open());
        self.finished = true;
        frames.push(self.event(
            "error",
            json!({
                "code": code.map(Value::from).unwrap_or(Value::Null),
                "message": message,
                "param": Value::Null,
            }),
        ));
        frames
    }

    /// 首两个事件（`response.created` + `response.in_progress`）只发一次
    fn ensure_created(&mut self) -> Vec<String> {
        if self.created_sent {
            return Vec::new();
        }
        self.created_sent = true;
        let snapshot = self.snapshot("in_progress");
        vec![
            self.event("response.created", json!({"response": snapshot.clone()})),
            self.event("response.in_progress", json!({"response": snapshot})),
        ]
    }

    fn on_block_start(&mut self, data: &Value) -> Vec<String> {
        let index = block_index(data);
        let Some(block) = data.get("content_block") else {
            return Vec::new();
        };
        match block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            // 文本与推理块不在 start 时开 item，等首个非空增量再惰性打开：上游对每轮工具
            // 调用都会先发一个空 text 块，提前打开会给客户端多塞一个空 message item。
            // 块自带初始文本时（协议允许非空）按首段增量处理，不丢内容。
            "text" => {
                let initial = block
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let mut frames = self.ensure_created();
                frames.extend(self.push_text_delta(index, &initial, false));
                frames
            }
            "thinking" => {
                let initial = block
                    .get("thinking")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let mut frames = self.ensure_created();
                frames.extend(self.push_text_delta(index, &initial, true));
                frames
            }
            "tool_use" => {
                let call_id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.open_tool_call(index, call_id, name)
            }
            other => {
                tracing::warn!(block_type = %other, "未识别的上游 content 块类型，已跳过");
                Vec::new()
            }
        }
    }

    fn on_block_delta(&mut self, data: &Value) -> Vec<String> {
        let Some(delta) = data.get("delta") else {
            return Vec::new();
        };
        let index = block_index(data);
        let delta_type = delta
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match delta_type {
            TEXT_DELTA => {
                let text = delta
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                self.push_text_delta(index, text, false)
            }
            THINKING_DELTA => {
                let text = delta
                    .get("thinking")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                self.push_text_delta(index, text, true)
            }
            INPUT_JSON_DELTA => {
                let partial = delta
                    .get("partial_json")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                self.push_arguments_delta(index, partial)
            }
            // 伪造签名：丢弃且不留痕（每个 thinking 块都会注入一次）
            SIGNATURE_DELTA => Vec::new(),
            other => {
                tracing::warn!(delta_type = %other, "未识别的 content_block_delta 类型，已跳过");
                Vec::new()
            }
        }
    }

    /// 文本或推理增量。`reasoning` 决定走 output_text 还是 reasoning_summary_text 事件族
    fn push_text_delta(&mut self, index: i64, text: &str, reasoning: bool) -> Vec<String> {
        if text.is_empty() {
            return Vec::new();
        }
        // 上游漏发 content_block_start 时惰性补齐，保证 added/done 成对
        let mut frames = if self.open.contains_key(&index) {
            Vec::new()
        } else if reasoning {
            self.open_reasoning(index)
        } else {
            self.open_text(index)
        };

        let Some(item) = self.open.get_mut(&index) else {
            return frames;
        };
        let matches_kind = match item.kind {
            OpenKind::Text => !reasoning,
            OpenKind::Reasoning => reasoning,
            OpenKind::ToolCall { .. } => false,
        };
        if !matches_kind {
            tracing::warn!(
                block_index = index,
                reasoning = reasoning,
                "增量类型与已打开的 output item 不匹配，已跳过"
            );
            return frames;
        }
        item.buffer.push_str(text);
        let item_id = item.item_id.clone();
        let output_index = item.output_index;

        let (event_name, key) = if reasoning {
            ("response.reasoning_summary_text.delta", "summary_index")
        } else {
            ("response.output_text.delta", "content_index")
        };
        frames.push(self.event(
            event_name,
            json!({
                "item_id": item_id,
                "output_index": output_index,
                key: SINGLE_PART_INDEX,
                "delta": text,
            }),
        ));
        frames
    }

    fn push_arguments_delta(&mut self, index: i64, partial: &str) -> Vec<String> {
        if partial.is_empty() {
            return Vec::new();
        }
        let Some(item) = self.open.get_mut(&index) else {
            // 没有 content_block_start 就拿不到 call_id 与 name，无法构造 function_call item
            tracing::warn!(
                block_index = index,
                "收到 input_json_delta 但对应的 tool_use 块未开始，已跳过"
            );
            return Vec::new();
        };
        let custom = match item.kind {
            OpenKind::ToolCall { custom, .. } => custom,
            _ => {
                tracing::warn!(
                    block_index = index,
                    "input_json_delta 落在非工具 output item 上，已跳过"
                );
                return Vec::new();
            }
        };
        item.buffer.push_str(partial);
        let item_id = item.item_id.clone();
        let output_index = item.output_index;

        // custom 工具的入参是自由文本，而增量是包装 JSON 的碎片（`{"input": "…` ），
        // 逐段下发会让客户端拿到带引号的半截 JSON。改为攒到 close_item 一次性下发解包后的原文。
        if custom {
            return Vec::new();
        }

        vec![self.event(
            "response.function_call_arguments.delta",
            json!({
                "item_id": item_id,
                "output_index": output_index,
                "delta": partial,
            }),
        )]
    }

    fn open_text(&mut self, index: i64) -> Vec<String> {
        let mut frames = self.ensure_created();
        let item_id = new_id("msg");
        let output_index = self.take_output_index();
        self.open.insert(
            index,
            OpenItem {
                output_index,
                item_id: item_id.clone(),
                kind: OpenKind::Text,
                buffer: String::new(),
            },
        );

        frames.push(self.event(
            "response.output_item.added",
            json!({
                "output_index": output_index,
                "item": {
                    "type": "message",
                    "id": item_id,
                    "status": "in_progress",
                    "role": "assistant",
                    "content": [],
                },
            }),
        ));
        frames.push(self.event(
            "response.content_part.added",
            json!({
                "item_id": item_id,
                "output_index": output_index,
                "content_index": SINGLE_PART_INDEX,
                "part": {"type": "output_text", "text": "", "annotations": []},
            }),
        ));
        frames
    }

    fn open_reasoning(&mut self, index: i64) -> Vec<String> {
        let mut frames = self.ensure_created();
        let item_id = new_id("rs");
        let output_index = self.take_output_index();
        self.open.insert(
            index,
            OpenItem {
                output_index,
                item_id: item_id.clone(),
                kind: OpenKind::Reasoning,
                buffer: String::new(),
            },
        );

        frames.push(self.event(
            "response.output_item.added",
            json!({
                "output_index": output_index,
                "item": {"type": "reasoning", "id": item_id, "summary": []},
            }),
        ));
        frames.push(self.event(
            "response.reasoning_summary_part.added",
            json!({
                "item_id": item_id,
                "output_index": output_index,
                "summary_index": SINGLE_PART_INDEX,
                "part": {"type": "summary_text", "text": ""},
            }),
        ));
        frames
    }

    fn open_tool_call(&mut self, index: i64, call_id: String, name: String) -> Vec<String> {
        let mut frames = self.ensure_created();
        let custom = self.custom_tools.contains(&name);
        let item_id = new_id(if custom { "ctc" } else { "fc" });
        let output_index = self.take_output_index();
        self.open.insert(
            index,
            OpenItem {
                output_index,
                item_id: item_id.clone(),
                kind: OpenKind::ToolCall {
                    call_id: call_id.clone(),
                    name: name.clone(),
                    custom,
                },
                buffer: String::new(),
            },
        );

        let item = if custom {
            json!({
                "type": "custom_tool_call",
                "id": item_id,
                "call_id": call_id,
                "name": name,
                "input": "",
                "status": "in_progress",
            })
        } else {
            json!({
                "type": "function_call",
                "id": item_id,
                "call_id": call_id,
                "name": name,
                "arguments": "",
                "status": "in_progress",
            })
        };
        frames.push(self.event(
            "response.output_item.added",
            json!({"output_index": output_index, "item": item}),
        ));
        frames
    }

    /// 闭合某个上游 block 对应的 output item，下发其收尾事件
    fn close_item(&mut self, index: i64) -> Vec<String> {
        let Some(item) = self.open.remove(&index) else {
            return Vec::new();
        };
        let OpenItem {
            output_index,
            item_id,
            kind,
            buffer,
        } = item;

        match kind {
            OpenKind::Text => {
                let done_item = json!({
                    "type": "message",
                    "id": item_id,
                    "status": "completed",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": buffer, "annotations": []}],
                });
                self.completed_items.push(done_item.clone());
                vec![
                    self.event(
                        "response.output_text.done",
                        json!({
                            "item_id": item_id,
                            "output_index": output_index,
                            "content_index": SINGLE_PART_INDEX,
                            "text": buffer,
                        }),
                    ),
                    self.event(
                        "response.content_part.done",
                        json!({
                            "item_id": item_id,
                            "output_index": output_index,
                            "content_index": SINGLE_PART_INDEX,
                            "part": {"type": "output_text", "text": buffer, "annotations": []},
                        }),
                    ),
                    self.event(
                        "response.output_item.done",
                        json!({"output_index": output_index, "item": done_item}),
                    ),
                ]
            }
            OpenKind::Reasoning => {
                let done_item = json!({
                    "type": "reasoning",
                    "id": item_id,
                    "summary": [{"type": "summary_text", "text": buffer}],
                });
                self.completed_items.push(done_item.clone());
                vec![
                    self.event(
                        "response.reasoning_summary_text.done",
                        json!({
                            "item_id": item_id,
                            "output_index": output_index,
                            "summary_index": SINGLE_PART_INDEX,
                            "text": buffer,
                        }),
                    ),
                    self.event(
                        "response.reasoning_summary_part.done",
                        json!({
                            "item_id": item_id,
                            "output_index": output_index,
                            "summary_index": SINGLE_PART_INDEX,
                            "part": {"type": "summary_text", "text": buffer},
                        }),
                    ),
                    self.event(
                        "response.output_item.done",
                        json!({"output_index": output_index, "item": done_item}),
                    ),
                ]
            }
            OpenKind::ToolCall {
                call_id,
                name,
                custom: true,
            } => {
                let input = custom_input_from_json_text(&buffer);
                let done_item = json!({
                    "type": "custom_tool_call",
                    "id": item_id,
                    "call_id": call_id,
                    "name": name,
                    "input": input,
                    "status": "completed",
                });
                self.completed_items.push(done_item.clone());
                let mut frames = Vec::new();
                // 客户端只在 done 时用完整 input，delta 仅供界面回显；空 input 不必发
                if !input.is_empty() {
                    frames.push(self.event(
                        "response.custom_tool_call_input.delta",
                        json!({
                            "item_id": item_id,
                            "output_index": output_index,
                            "delta": input,
                        }),
                    ));
                }
                frames.push(self.event(
                    "response.custom_tool_call_input.done",
                    json!({
                        "item_id": item_id,
                        "output_index": output_index,
                        "input": input,
                    }),
                ));
                frames.push(self.event(
                    "response.output_item.done",
                    json!({"output_index": output_index, "item": done_item}),
                ));
                frames
            }
            OpenKind::ToolCall {
                call_id,
                name,
                custom: false,
            } => {
                // 无参工具的 arguments 必须是合法 JSON，空串会让客户端解析失败
                let arguments = if buffer.trim().is_empty() {
                    "{}".to_string()
                } else {
                    buffer
                };
                let done_item = json!({
                    "type": "function_call",
                    "id": item_id,
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments,
                    "status": "completed",
                });
                self.completed_items.push(done_item.clone());
                vec![
                    self.event(
                        "response.function_call_arguments.done",
                        json!({
                            "item_id": item_id,
                            "output_index": output_index,
                            "arguments": arguments,
                        }),
                    ),
                    self.event(
                        "response.output_item.done",
                        json!({"output_index": output_index, "item": done_item}),
                    ),
                ]
            }
        }
    }

    fn take_output_index(&mut self) -> i64 {
        let i = self.next_output_index;
        self.next_output_index += 1;
        i
    }

    /// `response` 对象快照；`in_progress` 阶段 usage 尚未知，按协议给 `null`
    fn snapshot(&self, status: &str) -> Value {
        let mut response = json!({
            "id": self.id,
            "object": "response",
            "created_at": self.created_at,
            "status": status,
            "model": self.model,
            "output": self.completed_items,
            "usage": self.usage.clone().unwrap_or(Value::Null),
        });
        if status == "incomplete" {
            response["incomplete_details"] = json!({"reason": "max_output_tokens"});
        }
        response
    }

    /// 组装一帧 SSE：事件名同时写入 `event:` 行与 data 的 `type` 字段
    ///
    /// 两处都写是有意为之：SDK 按 `event:` 行分发，Codex CLI 按 data 里的 `type` 分发。
    fn event(&mut self, name: &str, mut payload: Value) -> String {
        let seq = self.sequence;
        self.sequence += 1;
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("type".to_string(), json!(name));
            obj.insert("sequence_number".to_string(), json!(seq));
        }
        format!("event: {}\ndata: {}\n\n", name, payload)
    }
}

/// 取事件里的上游 block index（缺失时按 0 处理，与 chat 侧一致）
fn block_index(data: &Value) -> i64 {
    data.get("index").and_then(Value::as_i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 绝大多数用例没有 custom 工具声明；本地定义遮蔽 glob 导入的同名函数，
    /// 需要 custom 语义的用例显式调 [`super::convert_non_stream`]
    fn convert_non_stream(anthropic: &Value, client_model: &str) -> Value {
        super::convert_non_stream(anthropic, client_model, &HashSet::new())
    }

    fn custom_names(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    /// 把一帧 SSE 文本拆成 (事件名, data JSON)
    fn parse_frame(frame: &str) -> (String, Value) {
        let mut lines = frame.trim_end().lines();
        let name = lines
            .next()
            .and_then(|l| l.strip_prefix("event: "))
            .expect("缺少 event 行")
            .to_string();
        let data = lines
            .next()
            .and_then(|l| l.strip_prefix("data: "))
            .expect("缺少 data 行");
        (
            name,
            serde_json::from_str(data).expect("data 不是合法 JSON"),
        )
    }

    fn parse_all(frames: &[String]) -> Vec<(String, Value)> {
        frames.iter().map(|f| parse_frame(f)).collect()
    }

    fn event_names(frames: &[String]) -> Vec<String> {
        parse_all(frames).into_iter().map(|(n, _)| n).collect()
    }

    /// 依次喂入事件并收集所有下发帧，末尾自动 finish
    fn run_stream(events: &[(&str, Value)]) -> Vec<String> {
        run_stream_with_custom(events, HashSet::new())
    }

    fn run_stream_with_custom(events: &[(&str, Value)], custom: HashSet<String>) -> Vec<String> {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", custom);
        let mut frames = Vec::new();
        for (name, data) in events {
            frames.extend(conv.on_event(name, data));
        }
        frames.extend(conv.finish());
        frames
    }

    /// 一轮完整的工具调用块事件（参数分两段发，覆盖增量拼装）
    fn tool_block_events(name: &'static str) -> Vec<(&'static str, Value)> {
        vec![
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "tool_use", "id": "toolu_1", "name": name}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"input\":\"ls "}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "-la\"}"}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            ("message_stop", json!({})),
        ]
    }

    fn text_block_events(text: &str) -> Vec<(&'static str, Value)> {
        vec![
            ("message_start", json!({"message": {"id": "msg_up"}})),
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "text", "text": ""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "text_delta", "text": text}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            (
                "message_delta",
                json!({"delta": {"stop_reason": "end_turn"}, "usage": {"input_tokens": 5, "output_tokens": 2}}),
            ),
            ("message_stop", json!({})),
        ]
    }

    #[test]
    fn plain_text_stream_emits_full_event_sequence() {
        let frames = run_stream(&text_block_events("你好"));
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );

        let parsed = parse_all(&frames);
        // sequence_number 从 0 起严格递增
        for (i, (_, data)) in parsed.iter().enumerate() {
            assert_eq!(data["sequence_number"], json!(i as i64));
        }

        let (_, done) = &parsed[5];
        assert_eq!(done["text"], json!("你好"));
        assert_eq!(done["content_index"], json!(0));

        let (_, completed) = parsed.last().unwrap();
        let response = &completed["response"];
        assert_eq!(response["status"], json!("completed"));
        assert_eq!(response["model"], json!("gpt-5.6-terra"));
        assert_eq!(response["usage"]["input_tokens"], json!(5));
        assert_eq!(response["usage"]["total_tokens"], json!(7));
        assert_eq!(response["output"][0]["content"][0]["text"], json!("你好"));
    }

    #[test]
    fn created_snapshot_has_no_output_or_usage_yet() {
        let frames = run_stream(&text_block_events("hi"));
        let (name, created) = parse_frame(&frames[0]);
        assert_eq!(name, "response.created");
        assert_eq!(created["response"]["status"], json!("in_progress"));
        assert_eq!(created["response"]["output"], json!([]));
        assert_eq!(created["response"]["usage"], Value::Null);
    }

    #[test]
    fn empty_text_block_produces_no_output_item() {
        // 上游在工具调用前会先发一个空 text 块，不能因此给客户端塞空 message item
        let frames = run_stream(&[
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "text", "text": ""}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            (
                "content_block_start",
                json!({"index": 1, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "ls"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 1, "delta": {"type": "input_json_delta", "partial_json": "{}"}}),
            ),
            ("content_block_stop", json!({"index": 1})),
            ("message_stop", json!({})),
        ]);

        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
        // 工具 item 占 output_index 0（空文本块没有占用编号）
        let parsed = parse_all(&frames);
        assert_eq!(parsed[2].1["output_index"], json!(0));
        let completed = &parsed.last().unwrap().1["response"];
        assert_eq!(completed["output"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn block_start_with_initial_text_keeps_it() {
        let frames = run_stream(&[
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "text", "text": "开头"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "text_delta", "text": "结尾"}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            ("message_stop", json!({})),
        ]);

        let parsed = parse_all(&frames);
        let done = parsed
            .iter()
            .find(|(n, _)| n == "response.output_text.done")
            .expect("缺少 output_text.done");
        assert_eq!(done.1["text"], json!("开头结尾"));
    }

    #[test]
    fn tool_call_stream_emits_arguments_events() {
        let frames = run_stream(&[
            ("message_start", json!({"message": {"id": "msg_up"}})),
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "read_file"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"path\""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": ":\"a.rs\"}"}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            (
                "message_delta",
                json!({"delta": {"stop_reason": "tool_use"}}),
            ),
            ("message_stop", json!({})),
        ]);

        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.done",
                "response.output_item.done",
                "response.completed",
            ]
        );

        let parsed = parse_all(&frames);
        let added = &parsed[2].1["item"];
        assert_eq!(added["type"], json!("function_call"));
        assert_eq!(added["call_id"], json!("toolu_1"));
        assert_eq!(added["name"], json!("read_file"));
        assert_eq!(added["status"], json!("in_progress"));

        assert_eq!(parsed[5].1["arguments"], json!("{\"path\":\"a.rs\"}"));
        let done_item = &parsed[6].1["item"];
        assert_eq!(done_item["arguments"], json!("{\"path\":\"a.rs\"}"));
        assert_eq!(done_item["status"], json!("completed"));
        // 工具调用不产生 message item
        let completed = &parsed.last().unwrap().1["response"];
        assert_eq!(completed["output"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn tool_call_without_arguments_yields_empty_json_object() {
        let frames = run_stream(&[
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "now"}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            ("message_stop", json!({})),
        ]);
        let parsed = parse_all(&frames);
        let done = parsed
            .iter()
            .find(|(n, _)| n == "response.function_call_arguments.done")
            .expect("缺少 arguments.done");
        assert_eq!(done.1["arguments"], json!("{}"));
    }

    #[test]
    fn reasoning_stream_uses_distinct_output_index_and_hides_signature() {
        let frames = run_stream(&[
            ("message_start", json!({"message": {"id": "msg_up"}})),
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "thinking", "thinking": ""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "thinking_delta", "thinking": "先看文件"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "signature_delta", "signature": "A".repeat(120)}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            (
                "content_block_start",
                json!({"index": 1, "content_block": {"type": "text", "text": ""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 1, "delta": {"type": "text_delta", "text": "结论"}}),
            ),
            ("content_block_stop", json!({"index": 1})),
            ("message_stop", json!({})),
        ]);

        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.reasoning_summary_part.added",
                "response.reasoning_summary_text.delta",
                "response.reasoning_summary_text.done",
                "response.reasoning_summary_part.done",
                "response.output_item.done",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );

        let parsed = parse_all(&frames);
        // reasoning 与文本必须占不同的 output_index
        assert_eq!(parsed[2].1["output_index"], json!(0));
        assert_eq!(parsed[8].1["output_index"], json!(1));
        assert_eq!(parsed[4].1["summary_index"], json!(0));

        // 伪造签名不得出现在任何下发帧中
        let joined = frames.concat();
        assert!(!joined.contains(&"A".repeat(100)));
        assert!(!joined.contains("signature"));
    }

    #[test]
    fn sequence_numbers_strictly_increase_across_mixed_blocks() {
        let frames = run_stream(&[
            ("message_start", json!({"message": {"id": "msg_up"}})),
            (
                "content_block_start",
                json!({"index": 0, "content_block": {"type": "thinking", "thinking": ""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "thinking_delta", "thinking": "t"}}),
            ),
            ("content_block_stop", json!({"index": 0})),
            (
                "content_block_start",
                json!({"index": 1, "content_block": {"type": "text", "text": ""}}),
            ),
            (
                "content_block_delta",
                json!({"index": 1, "delta": {"type": "text_delta", "text": "a"}}),
            ),
            ("content_block_stop", json!({"index": 1})),
            (
                "content_block_start",
                json!({"index": 2, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "ls"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 2, "delta": {"type": "input_json_delta", "partial_json": "{}"}}),
            ),
            ("content_block_stop", json!({"index": 2})),
            ("message_stop", json!({})),
        ]);

        let parsed = parse_all(&frames);
        let seqs: Vec<i64> = parsed
            .iter()
            .map(|(_, d)| d["sequence_number"].as_i64().expect("缺少 sequence_number"))
            .collect();
        assert_eq!(seqs, (0..seqs.len() as i64).collect::<Vec<_>>());
        // 三个块占三个不同的 output_index
        let completed = &parsed.last().unwrap().1["response"];
        assert_eq!(completed["output"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn truncated_stream_ends_with_response_incomplete() {
        for reason in ["max_tokens", "model_context_window_exceeded"] {
            let frames = run_stream(&[
                (
                    "content_block_start",
                    json!({"index": 0, "content_block": {"type": "text", "text": ""}}),
                ),
                (
                    "content_block_delta",
                    json!({"index": 0, "delta": {"type": "text_delta", "text": "半截"}}),
                ),
                ("content_block_stop", json!({"index": 0})),
                (
                    "message_delta",
                    json!({"delta": {"stop_reason": reason}, "usage": {"input_tokens": 1, "output_tokens": 1}}),
                ),
                ("message_stop", json!({})),
            ]);

            let names = event_names(&frames);
            assert_eq!(names.last().unwrap(), "response.incomplete", "{}", reason);
            assert!(
                !names.iter().any(|n| n == "response.completed"),
                "{}",
                reason
            );

            let (_, last) = parse_frame(frames.last().unwrap());
            assert_eq!(last["response"]["status"], json!("incomplete"));
            assert_eq!(
                last["response"]["incomplete_details"]["reason"],
                json!("max_output_tokens")
            );
        }
    }

    #[test]
    fn finish_is_idempotent() {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", HashSet::new());
        conv.on_event("message_start", &json!({"message": {"id": "msg_up"}}));
        let first = conv.finish();
        assert_eq!(event_names(&first), vec!["response.completed"]);
        assert!(conv.finish().is_empty());
    }

    #[test]
    fn interrupted_stream_closes_open_items() {
        // 上游断开：只有 start + delta，没有 content_block_stop / message_stop
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", HashSet::new());
        let mut frames = conv.on_event(
            "content_block_start",
            &json!({"index": 0, "content_block": {"type": "text", "text": ""}}),
        );
        frames.extend(conv.on_event(
            "content_block_delta",
            &json!({"index": 0, "delta": {"type": "text_delta", "text": "半"}}),
        ));
        frames.extend(conv.finish());

        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
    }

    #[test]
    fn empty_stream_still_yields_created_and_completed() {
        let frames = run_stream(&[("message_stop", json!({}))]);
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.completed",
            ]
        );
        let (_, last) = parse_frame(frames.last().unwrap());
        assert_eq!(last["response"]["output"], json!([]));
    }

    #[test]
    fn delta_without_block_start_is_lazily_opened() {
        let frames = run_stream(&[
            (
                "content_block_delta",
                json!({"index": 3, "delta": {"type": "text_delta", "text": "裸增量"}}),
            ),
            ("message_stop", json!({})),
        ]);
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
    }

    #[test]
    fn orphan_arguments_delta_and_unknown_events_are_skipped() {
        let frames = run_stream(&[
            // 没有 tool_use 块就来的参数增量：拿不到 call_id / name，只能丢弃
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "{}"}}),
            ),
            (
                "content_block_delta",
                json!({"index": 0, "delta": {"type": "citations_delta", "citation": {}}}),
            ),
            (
                "content_block_start",
                json!({"index": 1, "content_block": {"type": "redacted_thinking", "data": "x"}}),
            ),
            ("content_block_stop", json!({"index": 9})),
            ("unknown_event", json!({})),
            ("message_stop", json!({})),
        ]);
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.completed",
            ]
        );
    }

    #[test]
    fn upstream_error_event_terminates_without_completed() {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", HashSet::new());
        let mut frames = conv.on_event("message_start", &json!({"message": {"id": "msg_up"}}));
        frames.extend(conv.on_event(
            "error",
            &json!({"error": {"type": "overloaded_error", "message": "上游繁忙"}}),
        ));
        // 错误已终止流，后续 finish 不再补 completed
        frames.extend(conv.finish());

        let names = event_names(&frames);
        assert_eq!(
            names,
            vec!["response.created", "response.in_progress", "error"]
        );
        let (_, err) = parse_frame(frames.last().unwrap());
        assert_eq!(err["message"], json!("上游繁忙"));
    }

    #[test]
    fn upstream_error_closes_open_items_before_terminating() {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", HashSet::new());
        let mut frames = conv.on_event("message_start", &json!({"message": {"id": "msg_up"}}));
        // 文本块已打开但未收到 content_block_stop 就报错
        frames.extend(conv.on_event(
            "content_block_start",
            &json!({"index": 0, "content_block": {"type": "text", "text": "半截"}}),
        ));
        frames.extend(conv.on_event(
            "error",
            &json!({"error": {"type": "overloaded_error", "message": "上游繁忙"}}),
        ));
        frames.extend(conv.finish());

        let names = event_names(&frames);
        // added 必须有配对的 done，且 error 是最后一个事件
        assert_eq!(
            names
                .iter()
                .filter(|n| *n == "response.output_item.added")
                .count(),
            names
                .iter()
                .filter(|n| *n == "response.output_item.done")
                .count(),
        );
        assert_eq!(names.last().unwrap(), "error");
        assert!(!names.iter().any(|n| n == "response.completed"));
        assert!(!names.iter().any(|n| n == "response.incomplete"));

        // sequence_number 仍严格递增无跳号
        let seqs: Vec<i64> = parse_all(&frames)
            .iter()
            .map(|(_, d)| d["sequence_number"].as_i64().expect("缺少 sequence_number"))
            .collect();
        assert_eq!(seqs, (0..seqs.len() as i64).collect::<Vec<_>>());
    }

    #[test]
    fn error_after_finish_emits_nothing() {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", HashSet::new());
        let _ = conv.on_event("message_start", &json!({"message": {"id": "msg_up"}}));
        let _ = conv.finish();
        let extra = conv.on_event(
            "error",
            &json!({"error": {"type": "overloaded_error", "message": "迟到的错误"}}),
        );
        assert!(extra.is_empty(), "finish 之后不应再下发任何帧");
    }

    #[test]
    fn plain_text_response_has_single_message_item() {
        let out = convert_non_stream(
            &json!({
                "id": "msg_up",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "你好"}],
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 12, "output_tokens": 3},
            }),
            "gpt-5-codex",
        );

        assert!(out["id"].as_str().unwrap().starts_with("resp_"));
        assert_eq!(out["object"], "response");
        assert!(out["created_at"].as_i64().unwrap() > 0);
        assert_eq!(out["status"], "completed");
        assert_eq!(out["model"], "gpt-5-codex");
        assert!(out.get("incomplete_details").is_none());

        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["type"], "message");
        assert!(output[0]["id"].as_str().unwrap().starts_with("msg_"));
        assert_eq!(output[0]["status"], "completed");
        assert_eq!(output[0]["role"], "assistant");
        assert_eq!(
            output[0]["content"],
            json!([{"type": "output_text", "text": "你好", "annotations": []}])
        );

        assert_eq!(
            out["usage"],
            json!({
                "input_tokens": 12, "output_tokens": 3, "total_tokens": 15,
                "input_tokens_details": {"cached_tokens": 0},
            })
        );
    }

    #[test]
    fn multiple_text_blocks_are_concatenated() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "text", "text": "前半"},
                    {"type": "text", "text": "后半"},
                ],
                "stop_reason": "end_turn",
            }),
            "gpt-5-codex",
        );
        assert_eq!(out["output"][0]["content"][0]["text"], "前半后半");
    }

    #[test]
    fn tool_use_becomes_function_call_item_without_message() {
        let out = convert_non_stream(
            &json!({
                "content": [{
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "shell",
                    "input": {"cmd": "ls"},
                }],
                "stop_reason": "tool_use",
                "usage": {"input_tokens": 5, "output_tokens": 7},
            }),
            "gpt-5-codex",
        );

        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["type"], "function_call");
        assert!(output[0]["id"].as_str().unwrap().starts_with("fc_"));
        assert_eq!(output[0]["call_id"], "toolu_1");
        assert_eq!(output[0]["name"], "shell");
        assert_eq!(output[0]["arguments"], r#"{"cmd":"ls"}"#);
        assert_eq!(output[0]["status"], "completed");
        // 工具调用轮次仍是 completed —— 截断才用 incomplete
        assert_eq!(out["status"], "completed");
    }

    #[test]
    fn text_and_tool_use_keep_message_before_function_call() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "text", "text": "我来跑一下"},
                    {"type": "tool_use", "id": "toolu_1", "name": "shell", "input": {}},
                ],
                "stop_reason": "tool_use",
            }),
            "gpt-5-codex",
        );
        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output[0]["type"], "message");
        assert_eq!(output[1]["type"], "function_call");
    }

    #[test]
    fn parallel_tool_uses_keep_distinct_call_ids() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "tool_use", "id": "toolu_1", "name": "a", "input": {}},
                    {"type": "tool_use", "id": "toolu_2", "name": "b", "input": {}},
                ],
                "stop_reason": "tool_use",
            }),
            "gpt-5-codex",
        );
        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output[0]["call_id"], "toolu_1");
        assert_eq!(output[1]["call_id"], "toolu_2");
        assert_ne!(output[0]["id"], output[1]["id"]);
    }

    #[test]
    fn thinking_becomes_reasoning_item_ahead_of_message() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "thinking", "thinking": "先看目录", "signature": "A".repeat(120)},
                    {"type": "text", "text": "结论"},
                ],
                "stop_reason": "end_turn",
            }),
            "gpt-5-codex",
        );

        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output[0]["type"], "reasoning");
        assert!(output[0]["id"].as_str().unwrap().starts_with("rs_"));
        assert_eq!(
            output[0]["summary"],
            json!([{"type": "summary_text", "text": "先看目录"}])
        );
        assert_eq!(output[1]["type"], "message");

        // 伪造签名不得出现在任何位置
        let serialized = out.to_string();
        assert!(!serialized.contains(&"A".repeat(100)));
        assert!(!serialized.contains("signature"));
    }

    #[test]
    fn max_tokens_stop_reason_yields_incomplete_status() {
        for reason in ["max_tokens", "model_context_window_exceeded"] {
            let out = convert_non_stream(
                &json!({
                    "content": [{"type": "text", "text": "半句"}],
                    "stop_reason": reason,
                }),
                "gpt-5-codex",
            );
            assert_eq!(out["status"], "incomplete", "stop_reason={reason}");
            assert_eq!(
                out["incomplete_details"],
                json!({"reason": "max_output_tokens"}),
                "stop_reason={reason}"
            );
            // 已产出的内容仍要保留
            assert_eq!(out["output"][0]["content"][0]["text"], "半句");
        }
    }

    #[test]
    fn input_tokens_include_cached_input() {
        // 与 chat 侧同一口径：input_tokens 须为输入总量，命中部分另列 cached_tokens
        let out = convert_non_stream(
            &json!({
                "stop_reason": "end_turn",
                "usage": {
                    "input_tokens": 100, "output_tokens": 7,
                    "cache_creation_input_tokens": 300,
                    "cache_read_input_tokens": 600,
                },
            }),
            "gpt-5-codex",
        );
        assert_eq!(
            out["usage"],
            json!({
                "input_tokens": 1000, "output_tokens": 7, "total_tokens": 1007,
                "input_tokens_details": {"cached_tokens": 600},
            })
        );
    }

    #[test]
    fn missing_usage_and_content_still_yields_valid_response() {
        let out = convert_non_stream(&json!({"stop_reason": "end_turn"}), "gpt-5-codex");
        assert_eq!(out["status"], "completed");
        assert_eq!(out["output"], json!([]));
        assert_eq!(
            out["usage"],
            json!({
                "input_tokens": 0, "output_tokens": 0, "total_tokens": 0,
                "input_tokens_details": {"cached_tokens": 0},
            })
        );
    }

    #[test]
    fn unknown_block_type_is_skipped() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "brand_new_block", "foo": 1},
                    {"type": "text", "text": "ok"},
                ],
                "stop_reason": "end_turn",
            }),
            "gpt-5-codex",
        );
        let output = out["output"].as_array().unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["content"][0]["text"], "ok");
    }

    #[test]
    fn tool_use_missing_id_or_name_is_skipped() {
        let out = convert_non_stream(
            &json!({
                "content": [
                    {"type": "tool_use", "name": "no_id", "input": {}},
                    {"type": "tool_use", "id": "toolu_1", "input": {}},
                ],
                "stop_reason": "tool_use",
            }),
            "gpt-5-codex",
        );
        assert_eq!(out["output"], json!([]));
    }

    #[test]
    fn custom_tool_call_is_restored_with_free_text_input() {
        let out = super::convert_non_stream(
            &json!({
                "content": [
                    {"type": "tool_use", "id": "toolu_1", "name": "exec",
                     "input": {"input": "tools.read_file({path: 'demo.txt'})"}},
                    {"type": "tool_use", "id": "toolu_2", "name": "wait", "input": {"ms": 100}},
                ],
                "stop_reason": "tool_use",
            }),
            "gpt-5.6-terra",
            &custom_names(&["exec"]),
        );
        let output = out["output"].as_array().unwrap();
        assert_eq!(output[0]["type"], "custom_tool_call");
        assert_eq!(output[0]["call_id"], "toolu_1");
        assert_eq!(output[0]["name"], "exec");
        // 降级 schema 的包装被剥掉，客户端拿到原始自由文本
        assert_eq!(output[0]["input"], "tools.read_file({path: 'demo.txt'})");
        assert!(output[0].get("arguments").is_none());
        assert!(output[0]["id"].as_str().unwrap().starts_with("ctc_"));
        // 未声明为 custom 的工具仍走 function_call
        assert_eq!(output[1]["type"], "function_call");
        assert_eq!(output[1]["arguments"], "{\"ms\":100}");
    }

    #[test]
    fn custom_tool_input_falls_back_to_raw_json_when_schema_ignored() {
        let out = super::convert_non_stream(
            &json!({
                "content": [{"type": "tool_use", "id": "toolu_1", "name": "exec",
                             "input": {"cmd": "ls", "cwd": "/tmp"}}],
                "stop_reason": "tool_use",
            }),
            "gpt-5.6-terra",
            &custom_names(&["exec"]),
        );
        // 模型没照降级 schema 作答时不能给空串，整段 JSON 原样交给客户端
        assert_eq!(
            out["output"][0]["input"],
            "{\"cmd\":\"ls\",\"cwd\":\"/tmp\"}"
        );
    }

    #[test]
    fn custom_tool_stream_emits_custom_tool_call_input_events() {
        let frames = run_stream_with_custom(&tool_block_events("exec"), custom_names(&["exec"]));
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                // 参数增量是包装 JSON 的碎片，攒到收尾才下发解包后的原文
                "response.custom_tool_call_input.delta",
                "response.custom_tool_call_input.done",
                "response.output_item.done",
                "response.completed",
            ]
        );

        let parsed = parse_all(&frames);
        assert_eq!(parsed[2].1["item"]["type"], json!("custom_tool_call"));
        assert_eq!(parsed[2].1["item"]["input"], json!(""));
        assert_eq!(parsed[3].1["delta"], json!("ls -la"));
        assert_eq!(parsed[4].1["input"], json!("ls -la"));
        assert_eq!(parsed[5].1["item"]["type"], json!("custom_tool_call"));
        assert_eq!(parsed[5].1["item"]["call_id"], json!("toolu_1"));
        assert_eq!(parsed[5].1["item"]["input"], json!("ls -la"));
        assert_eq!(parsed[5].1["item"]["status"], json!("completed"));
        // 快照里也是 custom_tool_call
        assert_eq!(
            parsed[6].1["response"]["output"][0]["type"],
            json!("custom_tool_call")
        );
        for (i, (_, data)) in parsed.iter().enumerate() {
            assert_eq!(data["sequence_number"], json!(i as i64));
        }
    }

    #[test]
    fn non_custom_tool_stream_keeps_function_call_events() {
        // 同一组事件在未声明 custom 时必须仍走 function_call 事件族（零回归）
        let frames = run_stream(&tool_block_events("exec"));
        assert_eq!(
            event_names(&frames),
            vec![
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.delta",
                "response.function_call_arguments.done",
                "response.output_item.done",
                "response.completed",
            ]
        );
        let parsed = parse_all(&frames);
        assert_eq!(parsed[5].1["arguments"], json!("{\"input\":\"ls -la\"}"));
    }

    #[test]
    fn interrupted_custom_tool_stream_yields_partial_json_as_input() {
        let mut conv = ResponsesStreamConverter::new("gpt-5.6-terra", custom_names(&["exec"]));
        conv.on_event(
            "content_block_start",
            &json!({"index": 0, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "exec"}}),
        );
        conv.on_event(
            "content_block_delta",
            &json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"input\":\"ls"}}),
        );
        let frames = conv.finish();
        let parsed = parse_all(&frames);
        // 半截 JSON 解不出 input 字段，原样下发而不是丢空串
        let done = parsed
            .iter()
            .find(|(n, _)| n == "response.custom_tool_call_input.done")
            .expect("应有 input.done 事件");
        assert_eq!(done.1["input"], json!("{\"input\":\"ls"));
    }
}
