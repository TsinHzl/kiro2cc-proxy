// 流式部分：Anthropic SSE → OpenAI Responses SSE 转换状态机

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use super::nonstream::{
    compaction_item, convert_usage, custom_input_from_json_text, is_truncated, new_id,
};

use super::super::chat_response::{
    INPUT_JSON_DELTA, SIGNATURE_DELTA, TEXT_DELTA, THINKING_DELTA, unix_now,
};

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
    /// 是否为 Codex remote compaction v2 请求
    ///
    /// 为 true 时，`finish()` 将把所有文本内容拼合后包装为 `type: "compaction"` output item，
    /// 而不是普通的 message/reasoning/tool_call items。
    is_compaction: bool,
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
            is_compaction: false,
        }
    }

    /// 创建压缩模式的流式转换器
    ///
    /// 在 `finish()` 时将文本响应包装为 `type: "compaction"` output item 而非普通 message item。
    pub(crate) fn new_compaction(client_model: &str, custom_tools: HashSet<String>) -> Self {
        let mut conv = Self::new(client_model, custom_tools);
        conv.is_compaction = true;
        conv
    }

    /// 处理一个上游事件，返回待下发的 SSE 帧
    pub(crate) fn on_event(&mut self, name: &str, data: &Value) -> Vec<String> {
        // 压缩模式：content_block_* 事件内部仍需执行（以累积文本），但不向客户端转发
        let is_compaction = self.is_compaction;
        let pass = |f: Vec<String>| if is_compaction { Vec::new() } else { f };
        match name {
            "message_start" => self.ensure_created(),
            "content_block_start" => pass(self.on_block_start(data)),
            "content_block_delta" => pass(self.on_block_delta(data)),
            "content_block_stop" => pass(self.close_item(block_index(data))),
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

        if self.is_compaction {
            return self.finish_compaction(frames);
        }

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

    /// 压缩模式收尾：从已累积的 completed_items 中提取文本摘要，
    /// 构造单个 `type: "compaction"` output item 的 added/done 事件对并下发。
    fn finish_compaction(&mut self, mut frames: Vec<String>) -> Vec<String> {
        let summary = self
            .completed_items
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(Value::as_str) == Some("message") {
                    item.get("content")
                        .and_then(Value::as_array)
                        .and_then(|arr| {
                            let texts: Vec<&str> = arr
                                .iter()
                                .filter_map(|c| c.get("text").and_then(Value::as_str))
                                .collect();
                            if texts.is_empty() {
                                None
                            } else {
                                Some(texts.join(""))
                            }
                        })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let summary = summary.trim();
        if summary.is_empty() {
            tracing::warn!(
                "compaction 模式下模型未返回文本内容（可能为纯 reasoning 或 tool call），\
                 将产出空摘要 item，Codex 可能无法正确恢复上下文"
            );
        } else {
            tracing::info!(
                "流式模式生成 compaction output item（摘要 {} 字节）",
                summary.len()
            );
        }

        let cmp_item = compaction_item(summary);
        // 之前的普通 item 事件已被抑制，output_index 从 0 重新计数
        let cmp_output_index = 0i64;

        self.completed_items.clear();
        self.completed_items.push(cmp_item.clone());

        frames.push(self.event(
            "response.output_item.added",
            json!({"output_index": cmp_output_index, "item": cmp_item.clone()}),
        ));
        frames.push(self.event(
            "response.output_item.done",
            json!({"output_index": cmp_output_index, "item": cmp_item}),
        ));

        let snapshot = self.snapshot("completed");
        frames.push(self.event("response.completed", json!({"response": snapshot})));
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
        let (message, error_type, code) = super::super::error::extract_stream_error(data);
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
