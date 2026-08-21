// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! OpenAI Chat Completions 请求 → Anthropic Messages 请求
//!
//! 只做协议翻译，不做任何上游调用。产物是一个可直接交给
//! [`crate::anthropic::handlers::post_messages`] 反序列化的 JSON 对象。
//!
//! # 无法表达的字段
//!
//! `tool_choice` 在下游 Kiro 管线中无读取点（`MessagesRequest::tool_choice` 标注
//! `#[allow(dead_code)]`），属本模块引入之前就存在的限制。为避免"客户端要求强制调用工具
//! 却静默失效"，这里在其值不为 `auto` 时记录 `WARN` 留痕，而不是默默丢掉。
//!
//! `temperature` / `top_p` / `frequency_penalty` / `presence_penalty` / `n` / `stop` /
//! `logprobs` / `seed` 同样无处安放（Kiro 上游不接受采样参数），一并忽略。

use serde_json::{Value, json};

use super::model_map::map_model;

/// 未指定 `max_tokens` 时的默认值，与 `/v1/models` 对外声明的 `max_tokens` 一致
pub(super) const DEFAULT_MAX_TOKENS: i64 = 32000;

/// 上游接受的最小推理预算，低于此值会被拒绝
const MIN_THINKING_BUDGET: i64 = 1024;

/// 客户端未提供 schema 时的空对象 schema（Kiro 拒绝 `null` 形态的 schema）
fn empty_object_schema() -> Value {
    json!({"type": "object", "properties": {}})
}

/// 请求转换结果
#[derive(Debug)]
pub(crate) struct ConvertedChatRequest {
    /// 客户端请求的原始模型名，响应必须回写该值
    pub(crate) client_model: String,
    /// 客户端是否要求流式
    pub(crate) stream: bool,
    /// 流式下是否追加 usage 帧（`stream_options.include_usage`）
    pub(crate) include_usage: bool,
    /// 转换后的 Anthropic 请求体
    pub(crate) anthropic_body: Value,
}

/// 把 OpenAI Chat Completions 请求体转换为 Anthropic Messages 请求体
///
/// `Err` 的内容是面向客户端的错误消息（调用方负责包装为 OpenAI 400 错误结构）。
pub(crate) fn convert(body: &Value) -> Result<ConvertedChatRequest, String> {
    let client_model = body
        .get("model")
        .and_then(Value::as_str)
        .filter(|m| !m.trim().is_empty())
        .ok_or_else(|| "字段 'model' 缺失或为空".to_string())?
        .to_string();

    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "字段 'messages' 缺失或不是数组".to_string())?;
    if messages.is_empty() {
        return Err("字段 'messages' 不能为空数组".to_string());
    }

    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let include_usage = body
        .get("stream_options")
        .and_then(|o| o.get("include_usage"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // max_completion_tokens 是新字段名，优先于已弃用的 max_tokens
    let max_tokens = body
        .get("max_completion_tokens")
        .and_then(Value::as_i64)
        .or_else(|| body.get("max_tokens").and_then(Value::as_i64))
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_MAX_TOKENS);

    let (system, anthropic_messages) = convert_messages(messages);
    // 入参非空不代表转换后非空：整段全是 system、或每条 content 都为空时会被过滤干净。
    // 空 messages 会被下游直接拒绝，在这里就给出可读的 400，与 responses 侧保持一致。
    if anthropic_messages.is_empty() {
        return Err("字段 'messages' 未包含任何可转换的内容".to_string());
    }

    let mut anthropic = json!({
        "model": map_model(&client_model),
        "max_tokens": max_tokens,
        "messages": anthropic_messages,
        "stream": stream,
    });

    if !system.is_empty() {
        anthropic["system"] = Value::Array(system);
    }

    if let Some(tools) = convert_tools(body.get("tools")) {
        anthropic["tools"] = Value::Array(tools);
    }

    warn_if_tool_choice_unsupported(body.get("tool_choice"));

    if let Some(thinking) = convert_reasoning_effort(body.get("reasoning_effort"), max_tokens) {
        anthropic["thinking"] = thinking;
    }

    Ok(ConvertedChatRequest {
        client_model,
        stream,
        include_usage,
        anthropic_body: anthropic,
    })
}

/// 把 OpenAI messages 拆成 (system 块数组, Anthropic messages 数组)
fn convert_messages(messages: &[Value]) -> (Vec<Value>, Vec<Value>) {
    let mut system = Vec::new();
    let mut acc = MessageAccumulator::default();

    for msg in messages {
        let role = msg.get("role").and_then(Value::as_str).unwrap_or_default();
        match role {
            // developer 是 system 的新名字，两者等价
            "system" | "developer" => {
                let text = flatten_text(msg.get("content"));
                if !text.is_empty() {
                    system.push(json!({"type": "text", "text": text}));
                }
            }
            "user" => {
                let blocks = convert_user_content(msg.get("content"));
                if !blocks.is_empty() {
                    acc.push("user", blocks);
                }
            }
            "assistant" => {
                let blocks = convert_assistant_message(msg);
                if !blocks.is_empty() {
                    acc.push("assistant", blocks);
                }
            }
            // 工具结果在 Anthropic 协议里属于 user 消息
            "tool" | "function" => match msg.get("tool_call_id").and_then(Value::as_str) {
                Some(id) if !id.is_empty() => {
                    acc.push(
                        "user",
                        vec![json!({
                            "type": "tool_result",
                            "tool_use_id": id,
                            "content": flatten_text(msg.get("content")),
                        })],
                    );
                }
                _ => {
                    // 缺 tool_call_id 无法构造 tool_result，降级为普通文本以免丢内容
                    tracing::warn!(
                        role = %role,
                        "工具消息缺少 tool_call_id，已降级为普通 user 文本"
                    );
                    let text = flatten_text(msg.get("content"));
                    if !text.is_empty() {
                        acc.push("user", vec![json!({"type": "text", "text": text})]);
                    }
                }
            },
            other => {
                tracing::warn!(role = %other, "未识别的消息 role，已跳过");
            }
        }
    }

    (system, acc.into_messages())
}

/// 相邻同 role 消息合并器
///
/// Anthropic 要求 user / assistant 交替出现，而 OpenAI 允许连续多条同 role 消息
/// （典型场景：并行工具调用产生多条 `role:"tool"`）。这里把相邻同 role 的内容块
/// 合并进同一条消息。
#[derive(Default)]
pub(super) struct MessageAccumulator {
    messages: Vec<(String, Vec<Value>)>,
}

impl MessageAccumulator {
    pub(super) fn push(&mut self, role: &str, blocks: Vec<Value>) {
        match self.messages.last_mut() {
            Some(last) if last.0 == role => last.1.extend(blocks),
            _ => self.messages.push((role.to_string(), blocks)),
        }
    }

    pub(super) fn into_messages(self) -> Vec<Value> {
        self.messages
            .into_iter()
            .map(|(role, content)| json!({"role": role, "content": content}))
            .collect()
    }
}

/// 把 content 压平成纯文本（system 消息与 tool 结果只需要文本）
///
/// 支持三种形态：字符串、`[{type:"text",text}]` 数组、其他（按 JSON 原文兜底，
/// 避免内容凭空消失）。
pub(super) fn flatten_text(content: Option<&Value>) -> String {
    match content {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| match p {
                Value::String(s) => Some(s.clone()),
                Value::Object(_) => p
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| p.get("output").and_then(Value::as_str).map(str::to_string)),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
    }
}

/// 转换 user 消息的 content（字符串或多模态数组）
fn convert_user_content(content: Option<&Value>) -> Vec<Value> {
    match content {
        Some(Value::String(s)) if !s.is_empty() => vec![json!({"type": "text", "text": s})],
        Some(Value::Array(parts)) => parts.iter().filter_map(convert_content_part).collect(),
        _ => Vec::new(),
    }
}

/// 转换单个多模态 content part
fn convert_content_part(part: &Value) -> Option<Value> {
    let part_type = part.get("type").and_then(Value::as_str).unwrap_or_default();
    match part_type {
        // input_text 是 Responses 风格的别名，一并兼容
        "text" | "input_text" => {
            let text = part.get("text").and_then(Value::as_str).unwrap_or_default();
            (!text.is_empty()).then(|| json!({"type": "text", "text": text}))
        }
        "image_url" => {
            let url = part
                .get("image_url")
                .and_then(|o| o.get("url"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            convert_image_url(url)
        }
        other => {
            tracing::warn!(part_type = %other, "未识别的 content part 类型，已跳过");
            None
        }
    }
}

/// 把 `image_url` 转成 Anthropic image block
///
/// 只有 data URL 能转（Kiro 上游不会代为拉取远程图片）。远程 URL 降级为一行文本，
/// 使模型至少知道这里本该有张图，而不是内容凭空消失。
pub(super) fn convert_image_url(url: &str) -> Option<Value> {
    if url.is_empty() {
        return None;
    }
    match parse_data_url(url) {
        Some((media_type, data)) => Some(json!({
            "type": "image",
            "source": {"type": "base64", "media_type": media_type, "data": data},
        })),
        None => {
            tracing::warn!("image_url 不是 data URL，已降级为文本占位（上游不支持远程图片拉取）");
            Some(json!({"type": "text", "text": format!("[image: {url}]")}))
        }
    }
}

/// 解析 `data:<media_type>;base64,<data>`
fn parse_data_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let media_type = meta.strip_suffix(";base64")?;
    if media_type.is_empty() || data.is_empty() {
        return None;
    }
    Some((media_type.to_string(), data.to_string()))
}

/// 转换 assistant 消息（文本 + tool_calls）
///
/// 历史 `reasoning_content` 不回传：thinking block 需要配套签名，伪造签名回传给上游
/// 只会增加被拒风险，而丢弃它不影响后续对话。
fn convert_assistant_message(msg: &Value) -> Vec<Value> {
    let mut blocks = Vec::new();

    match msg.get("content") {
        Some(Value::String(s)) if !s.is_empty() => {
            blocks.push(json!({"type": "text", "text": s}));
        }
        Some(Value::Array(parts)) => {
            blocks.extend(parts.iter().filter_map(convert_content_part));
        }
        _ => {}
    }

    if let Some(calls) = msg.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            if let Some(block) = convert_tool_call(call) {
                blocks.push(block);
            }
        }
    }

    blocks
}

/// 单个 `tool_calls[]` 项 → Anthropic `tool_use` block
fn convert_tool_call(call: &Value) -> Option<Value> {
    let id = call.get("id").and_then(Value::as_str).unwrap_or_default();
    let function = call.get("function")?;
    let name = function.get("name").and_then(Value::as_str)?;
    if id.is_empty() || name.is_empty() {
        tracing::warn!("tool_calls 项缺少 id 或 function.name，已跳过");
        return None;
    }

    let input = parse_tool_arguments(function.get("arguments"), name);

    Some(json!({"type": "tool_use", "id": id, "name": name, "input": input}))
}

/// 解析工具调用入参：JSON 字符串（协议规定形态）或对象（部分客户端的宽松形态）
///
/// 解析失败时退化为空参数并 WARN，而不是丢掉整个 tool_use——丢掉会让后续的
/// `tool_result` 找不到配对的 `tool_use_id`，直接被上游拒。
pub(super) fn parse_tool_arguments(raw: Option<&Value>, tool_name: &str) -> Value {
    match raw {
        // 空串代表无参数
        Some(Value::String(s)) if !s.trim().is_empty() => serde_json::from_str::<Value>(s)
            .unwrap_or_else(|e| {
                tracing::warn!(
                    tool_name = %tool_name,
                    error = %e,
                    "工具调用的 arguments 不是合法 JSON，已作为空参数处理"
                );
                json!({})
            }),
        Some(Value::Object(map)) => Value::Object(map.clone()),
        _ => json!({}),
    }
}

/// 转换工具声明数组；无有效工具时返回 `None`（不写出空 `tools` 字段）
fn convert_tools(tools: Option<&Value>) -> Option<Vec<Value>> {
    let list = tools?.as_array()?;
    let converted: Vec<Value> = list.iter().filter_map(convert_tool).collect();
    (!converted.is_empty()).then_some(converted)
}

/// 单个工具声明 → Anthropic tool
///
/// 同时兼容嵌套形态（`{type:"function", function:{...}}`，Chat Completions）与
/// 扁平形态（`{type:"function", name, parameters}`，Responses / 部分 SDK）。
pub(super) fn convert_tool(tool: &Value) -> Option<Value> {
    let spec = tool.get("function").unwrap_or(tool);
    let name = spec
        .get("name")
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())?;
    let description = spec
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // parameters 缺失或为 null 时补空对象 schema：Kiro 拒绝 null schema
    let input_schema = match spec.get("parameters") {
        Some(Value::Object(map)) => Value::Object(map.clone()),
        _ => empty_object_schema(),
    };

    Some(json!({
        "name": name,
        "description": description,
        "input_schema": input_schema,
    }))
}

/// `tool_choice` 非 `auto` 时留痕（下游无读取点，字段实际不生效）
pub(super) fn warn_if_tool_choice_unsupported(tool_choice: Option<&Value>) {
    let choice = match tool_choice {
        None | Some(Value::Null) => return,
        Some(v) => v,
    };
    if choice.as_str() == Some("auto") {
        return;
    }
    tracing::warn!(
        tool_choice = %choice,
        "下游 Kiro 管线不支持强制工具选择，tool_choice 不会生效（本变更前既有限制）"
    );
}

/// `reasoning_effort` → Anthropic `thinking`
///
/// `max_tokens` 参与收敛：推理预算必须小于 `max_tokens`，否则上游拒绝整个请求。客户端给了很小的
/// `max_tokens` 时按 effort 直接取值会撞上这条约束，因此先按 `max_tokens` 下调；下调后不足
/// [`MIN_THINKING_BUDGET`] 就整体不开推理——留一个失效的极小预算同样会被上游拒绝。
pub(super) fn convert_reasoning_effort(effort: Option<&Value>, max_tokens: i64) -> Option<Value> {
    let effort = effort?.as_str()?;
    let requested: i64 = match effort {
        "low" => 4096,
        "medium" => 12000,
        "high" => 24576,
        // minimal / none 表示不要推理
        "minimal" | "none" => return None,
        other => {
            tracing::warn!(reasoning_effort = %other, "未识别的 reasoning_effort，已忽略");
            return None;
        }
    };

    let budget = requested.min(max_tokens - 1);
    if budget < MIN_THINKING_BUDGET {
        tracing::warn!(
            reasoning_effort = %effort,
            requested_budget = requested,
            max_tokens,
            min_budget = MIN_THINKING_BUDGET,
            "max_tokens 太小，容不下推理预算，本次请求不启用 thinking"
        );
        return None;
    }
    if budget < requested {
        tracing::debug!(
            requested_budget = requested,
            budget,
            max_tokens,
            "推理预算受 max_tokens 约束已下调"
        );
    }
    Some(json!({"type": "enabled", "budget_tokens": budget}))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert_ok(body: Value) -> ConvertedChatRequest {
        convert(&body).expect("转换应成功")
    }

    #[test]
    fn maps_model_and_defaults() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [{"role": "user", "content": "hi"}],
        }));
        assert_eq!(r.client_model, "gpt-5-codex");
        assert!(!r.stream);
        assert!(!r.include_usage);
        assert_eq!(r.anthropic_body["model"], "gpt-5.6-terra");
        assert_eq!(r.anthropic_body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(r.anthropic_body["stream"], false);
        assert_eq!(
            r.anthropic_body["messages"],
            json!([{"role": "user", "content": [{"type": "text", "text": "hi"}]}])
        );
        assert!(r.anthropic_body.get("system").is_none());
        assert!(r.anthropic_body.get("tools").is_none());
    }

    #[test]
    fn rejects_missing_model_and_messages() {
        assert!(convert(&json!({"messages": [{"role": "user", "content": "x"}]})).is_err());
        assert!(convert(&json!({"model": "", "messages": []})).is_err());
        assert!(convert(&json!({"model": "gpt-5-codex"})).is_err());
        assert!(convert(&json!({"model": "gpt-5-codex", "messages": []})).is_err());
    }

    #[test]
    fn rejects_messages_that_convert_to_nothing() {
        // 非空入参但转换后无任何 user/assistant 消息，须在本层拒绝而非把空 messages 发给下游
        for messages in [
            json!([{"role": "system", "content": "only system"}]),
            json!([{"role": "user", "content": ""}]),
            json!([{"role": "unknown_role", "content": "x"}]),
        ] {
            let err = convert(&json!({"model": "gpt-5-codex", "messages": messages}))
                .expect_err("应拒绝转换后为空的 messages");
            assert!(err.contains("messages"), "错误信息应指向 messages：{err}");
        }
    }

    #[test]
    fn max_completion_tokens_wins_over_max_tokens() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "max_tokens": 100,
            "max_completion_tokens": 4096,
            "messages": [{"role": "user", "content": "hi"}],
        }));
        assert_eq!(r.anthropic_body["max_tokens"], 4096);
    }

    #[test]
    fn legacy_max_tokens_still_honored() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "max_tokens": 256,
            "messages": [{"role": "user", "content": "hi"}],
        }));
        assert_eq!(r.anthropic_body["max_tokens"], 256);
    }

    #[test]
    fn system_and_developer_go_to_system_field() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [
                {"role": "system", "content": "be brief"},
                {"role": "developer", "content": "use rust"},
                {"role": "user", "content": "hi"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["system"],
            json!([
                {"type": "text", "text": "be brief"},
                {"type": "text", "text": "use rust"},
            ])
        );
        // system / developer 不得出现在 messages 中
        let messages = r.anthropic_body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
    }

    #[test]
    fn stream_options_include_usage_is_read() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "stream": true,
            "stream_options": {"include_usage": true},
            "messages": [{"role": "user", "content": "hi"}],
        }));
        assert!(r.stream);
        assert!(r.include_usage);
        assert_eq!(r.anthropic_body["stream"], true);
    }

    #[test]
    fn converts_nested_function_tool_declaration() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "查天气",
                    "parameters": {"type": "object", "properties": {"city": {"type": "string"}}},
                },
            }],
        }));
        assert_eq!(
            r.anthropic_body["tools"],
            json!([{
                "name": "get_weather",
                "description": "查天气",
                "input_schema": {"type": "object", "properties": {"city": {"type": "string"}}},
            }])
        );
    }

    #[test]
    fn converts_flat_function_tool_declaration() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{"type": "function", "name": "ls", "parameters": {"type": "object"}}],
        }));
        assert_eq!(r.anthropic_body["tools"][0]["name"], "ls");
        assert_eq!(r.anthropic_body["tools"][0]["description"], "");
        assert_eq!(
            r.anthropic_body["tools"][0]["input_schema"],
            json!({"type": "object"})
        );
    }

    #[test]
    fn tool_without_parameters_gets_empty_object_schema() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{"type": "function", "function": {"name": "noop"}}],
        }));
        assert_eq!(
            r.anthropic_body["tools"][0]["input_schema"],
            json!({"type": "object", "properties": {}})
        );
    }

    #[test]
    fn tool_message_becomes_user_tool_result() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [
                {"role": "user", "content": "天气?"},
                {"role": "assistant", "tool_calls": [{
                    "id": "toolu_x",
                    "type": "function",
                    "function": {"name": "get_weather", "arguments": "{\"city\":\"SH\"}"},
                }]},
                {"role": "tool", "tool_call_id": "toolu_x", "content": "18°C"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([
                {"role": "user", "content": [{"type": "text", "text": "天气?"}]},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "toolu_x", "name": "get_weather", "input": {"city": "SH"}},
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "toolu_x", "content": "18°C"},
                ]},
            ])
        );
    }

    #[test]
    fn parallel_tool_results_merge_into_one_user_message() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [
                {"role": "user", "content": "go"},
                {"role": "assistant", "tool_calls": [
                    {"id": "a", "type": "function", "function": {"name": "f", "arguments": "{}"}},
                    {"id": "b", "type": "function", "function": {"name": "g", "arguments": "{}"}},
                ]},
                {"role": "tool", "tool_call_id": "a", "content": "1"},
                {"role": "tool", "tool_call_id": "b", "content": "2"},
            ],
        }));
        let messages = r.anthropic_body["messages"].as_array().unwrap();
        // 两条 tool 消息必须合并为一条 user 消息，否则上游会拒绝非交替结构
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"].as_array().unwrap().len(), 2);
        assert_eq!(messages[1]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn assistant_text_and_tool_call_coexist() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": "让我查一下", "tool_calls": [{
                    "id": "t1", "type": "function",
                    "function": {"name": "f", "arguments": "{\"k\":1}"},
                }]},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"][1]["content"],
            json!([
                {"type": "text", "text": "让我查一下"},
                {"type": "tool_use", "id": "t1", "name": "f", "input": {"k": 1}},
            ])
        );
    }

    #[test]
    fn malformed_tool_arguments_degrade_to_empty_input() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "tool_calls": [{
                    "id": "t1", "type": "function",
                    "function": {"name": "f", "arguments": "{broken"},
                }]},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"][1]["content"][0]["input"],
            json!({})
        );
    }

    #[test]
    fn multimodal_data_url_image_converts_to_base64_source() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "这是什么"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,QUJD"}},
            ]}],
        }));
        assert_eq!(
            r.anthropic_body["messages"][0]["content"],
            json!([
                {"type": "text", "text": "这是什么"},
                {"type": "image", "source": {
                    "type": "base64", "media_type": "image/png", "data": "QUJD",
                }},
            ])
        );
    }

    #[test]
    fn remote_image_url_degrades_to_text_placeholder() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [{"role": "user", "content": [
                {"type": "image_url", "image_url": {"url": "https://x/y.png"}},
            ]}],
        }));
        assert_eq!(
            r.anthropic_body["messages"][0]["content"],
            json!([{"type": "text", "text": "[image: https://x/y.png]"}])
        );
    }

    #[test]
    fn reasoning_effort_maps_to_thinking() {
        for (effort, budget) in [("low", 4096), ("medium", 12000), ("high", 24576)] {
            let r = convert_ok(json!({
                "model": "gpt-5-codex",
                "reasoning_effort": effort,
                "messages": [{"role": "user", "content": "hi"}],
            }));
            assert_eq!(
                r.anthropic_body["thinking"],
                json!({"type": "enabled", "budget_tokens": budget}),
                "effort={effort}"
            );
        }
    }

    #[test]
    fn thinking_budget_is_clamped_below_max_tokens() {
        // max_tokens=8192 容不下 high 的 24576，须下调到 8191 而不是原样发给上游
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "max_tokens": 8192,
            "reasoning_effort": "high",
            "messages": [{"role": "user", "content": "hi"}],
        }));
        assert_eq!(
            r.anthropic_body["thinking"],
            json!({"type": "enabled", "budget_tokens": 8191})
        );
    }

    #[test]
    fn thinking_omitted_when_max_tokens_too_small_for_budget() {
        // 收敛后不足 MIN_THINKING_BUDGET，整体不开推理
        for max_tokens in [1, 512, MIN_THINKING_BUDGET] {
            let r = convert_ok(json!({
                "model": "gpt-5-codex",
                "max_tokens": max_tokens,
                "reasoning_effort": "high",
                "messages": [{"role": "user", "content": "hi"}],
            }));
            assert!(
                r.anthropic_body.get("thinking").is_none(),
                "max_tokens={max_tokens} 时不应带 thinking"
            );
        }
    }

    #[test]
    fn minimal_and_unknown_reasoning_effort_omit_thinking() {
        for effort in ["minimal", "none", "ultra"] {
            let r = convert_ok(json!({
                "model": "gpt-5-codex",
                "reasoning_effort": effort,
                "messages": [{"role": "user", "content": "hi"}],
            }));
            assert!(
                r.anthropic_body.get("thinking").is_none(),
                "effort={effort} 不应产生 thinking"
            );
        }
    }

    #[test]
    fn tool_choice_never_reaches_anthropic_body() {
        // 非 auto 时只记 WARN，不写入下游请求（下游无读取点）
        for choice in [
            json!("required"),
            json!("none"),
            json!({"type": "function"}),
        ] {
            let r = convert_ok(json!({
                "model": "gpt-5-codex",
                "tool_choice": choice,
                "messages": [{"role": "user", "content": "hi"}],
            }));
            assert!(r.anthropic_body.get("tool_choice").is_none());
        }
    }

    #[test]
    fn unknown_role_is_skipped() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "mystery", "content": "???"},
            ],
        }));
        assert_eq!(r.anthropic_body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn tool_message_without_call_id_degrades_to_user_text() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "messages": [{"role": "tool", "content": "orphan"}],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([{"role": "user", "content": [{"type": "text", "text": "orphan"}]}])
        );
    }
}
