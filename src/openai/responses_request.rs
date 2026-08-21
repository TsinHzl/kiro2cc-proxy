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

use std::collections::HashSet;

use serde_json::{Value, json};

use super::chat_request::{
    DEFAULT_MAX_TOKENS, MessageAccumulator, convert_image_url, convert_reasoning_effort,
    convert_tool, flatten_text, parse_tool_arguments, warn_if_tool_choice_unsupported,
};
use super::model_map::map_model;

/// `namespace` 容器的嵌套深度上限；超出即跳过，避免畸形请求把栈递归穿了
const MAX_NAMESPACE_DEPTH: usize = 4;

/// `custom` 工具的入参在 Responses 协议里是自由文本，Anthropic 只接受 JSON schema。
/// 降级为单字段对象，配合 [`ToolInputForm::FreeText`] 把原始文本塞进 `input` 字段。
fn custom_tool_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"input": {"type": "string"}},
        "required": ["input"],
        "additionalProperties": false,
    })
}

/// 追加到 `custom`（自由文本）工具 description 末尾的适配说明
///
/// **必需**：这类工具的原始描述往往明确要求"raw text, not JSON, no code fences"
/// （Codex 的 `exec` 逐字如此），与 [`custom_tool_schema`] 的单字段包装直接冲突。
/// 不加说明时模型会照原描述吐裸文本，导致入参解析不出来。只在末尾追加，不改动原描述。
const FREEFORM_ADAPTATION_NOTE: &str = concat!(
    "\n\n---\n[Gateway adaptation] This freeform tool is exposed as a JSON tool. ",
    "Put the raw tool text (unquoted, no code fences) into the `input` string field."
);

/// OpenAI 工具协议的默认命名空间名——只有这个容器会被展开，理由见 [`ToolCollector::push_one`]
const DEFAULT_TOOL_NAMESPACE: &str = "functions";

/// 工具调用入参的两种形态
#[derive(Clone, Copy)]
enum ToolInputForm {
    /// `function_call.arguments`：JSON 字符串
    Json,
    /// `custom_tool_call.input`：自由文本
    FreeText,
}

/// 请求转换结果
#[derive(Debug)]
pub(crate) struct ConvertedResponsesRequest {
    /// 客户端请求的原始模型名，响应必须回写该值
    pub(crate) client_model: String,
    /// 客户端是否要求流式
    pub(crate) stream: bool,
    /// 转换后的 Anthropic 请求体
    pub(crate) anthropic_body: Value,
    /// 声明为 `custom` 的工具名。响应侧必须把这些工具的调用还原成
    /// `custom_tool_call` item（自由文本入参），否则客户端认不出来
    pub(crate) custom_tools: HashSet<String>,
}

/// 汇总各来源的工具声明
///
/// 两个来源：顶层 `tools`，以及 `input` 里的 `additional_tools` item。先收的优先——
/// 顶层声明在遍历 `input` 之前入栈，同名时后来者被丢弃。
#[derive(Default)]
struct ToolCollector {
    /// 已转换为 Anthropic 形态的工具
    tools: Vec<Value>,
    /// 已收录的工具名，用于同名去重
    seen: HashSet<String>,
    /// 其中入参为自由文本（`custom`）的工具名
    custom: HashSet<String>,
}

impl ToolCollector {
    fn push_list(&mut self, list: &[Value], depth: usize) {
        for tool in list {
            self.push_one(tool, depth);
        }
    }

    fn push_one(&mut self, tool: &Value, depth: usize) {
        let tool_type = tool
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("function");

        // namespace 本身不可调用，只是分组容器，真正的工具在它的 tools 数组里。
        // 分组名不拼进工具名：Anthropic 的工具名不接受 `.`，而模型回调时给的就是叶子名。
        //
        // 只展开名为 `functions` 的容器：它是 OpenAI 工具协议的默认命名空间，其子工具按裸名
        // 回调，剥掉容器后名字与响应编码都不用动。其余 namespace（collaboration 等）的子工具
        // 按裸名和 `ns.名` 调用均被客户端拒绝，展开只会造出一批调不动的死工具、白占工具槽位。
        // "是不是默认命名空间"在协议里没有字段可表达，只能按名字判别。
        if tool_type == "namespace" {
            let ns_name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
            if ns_name != DEFAULT_TOOL_NAMESPACE {
                tracing::warn!(
                    namespace = %ns_name,
                    "跳过非默认命名空间的工具容器：其子工具无法被客户端调用"
                );
                return;
            }
            match tool.get("tools").and_then(Value::as_array) {
                Some(inner) if depth < MAX_NAMESPACE_DEPTH => self.push_list(inner, depth + 1),
                Some(_) => {
                    tracing::warn!(depth, "namespace 嵌套超过深度上限，已跳过");
                }
                None => {}
            }
            return;
        }

        let Some(converted) = convert_one_tool(tool) else {
            return;
        };
        let Some(name) = converted
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return;
        };
        if !self.seen.insert(name.clone()) {
            tracing::warn!(tool_name = %name, "工具重名，保留先出现的声明");
            return;
        }
        if tool_type == "custom" {
            self.custom.insert(name);
        }
        self.tools.push(converted);
    }
}

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
    match body.get("input") {
        // 最简形态：整段 prompt 就是一条 user 文本
        Some(Value::String(s)) => {
            if !s.is_empty() {
                acc.push("user", vec![json!({"type": "text", "text": s})]);
            }
        }
        Some(Value::Array(items)) => convert_input_items(items, &mut system, &mut acc, &mut tools),
        _ => return Err("字段 'input' 缺失或类型不支持（应为字符串或数组）".to_string()),
    }

    let messages = acc.into_messages();
    if messages.is_empty() {
        return Err("字段 'input' 未包含任何可转换的内容".to_string());
    }

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
    })
}

/// 遍历 `input` 数组，把各类 item 分派到 system 块、消息累加器或工具收集器
fn convert_input_items(
    items: &[Value],
    system: &mut Vec<Value>,
    acc: &mut MessageAccumulator,
    tools: &mut ToolCollector,
) {
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
                    acc.push("assistant", vec![block]);
                }
            }
            "custom_tool_call" => {
                if let Some(block) = tool_use_block(item, ToolInputForm::FreeText) {
                    acc.push("assistant", vec![block]);
                }
            }
            // 工具结果在 Anthropic 协议里属于 user 消息
            "function_call_output" | "custom_tool_call_output" => {
                if let Some(block) = tool_result_block(item) {
                    acc.push("user", vec![block]);
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
            other => {
                tracing::warn!(item_type = %other, "未识别的 input item 类型，已跳过");
            }
        }
    }
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

    let input = match form {
        ToolInputForm::Json => parse_tool_arguments(item.get("arguments"), name),
        // 自由文本入参装进降级 schema 约定的 input 字段
        ToolInputForm::FreeText => {
            let text = item.get("input").and_then(Value::as_str).unwrap_or_else(|| {
                tracing::warn!("custom_tool_call 缺少 input 字段，已降级为空文本");
                ""
            });
            json!({"input": text})
        }
    };

    Some(json!({"type": "tool_use", "id": id, "name": name, "input": input}))
}

/// `function_call_output` / `custom_tool_call_output` → Anthropic `tool_result` block
fn tool_result_block(item: &Value) -> Option<Value> {
    let id = call_id(item)?;
    Some(json!({
        "type": "tool_result",
        "tool_use_id": id,
        "content": flatten_text(item.get("output")),
    }))
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

/// 单个工具声明 → Anthropic tool
fn convert_one_tool(tool: &Value) -> Option<Value> {
    match tool
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("function")
    {
        "function" => convert_tool(tool),
        "custom" => {
            let name = tool
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty())?;
            let description = tool
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            tracing::warn!(
                tool_name = %name,
                "custom 工具的自由文本入参已降级为单字段 JSON schema（上游只接受 JSON schema）"
            );
            Some(json!({
                "name": name,
                "description": format!("{description}{FREEFORM_ADAPTATION_NOTE}"),
                "input_schema": custom_tool_schema(),
            }))
        }
        // web_search / local_shell / image_generation 等内建工具由 OpenAI 侧执行，上游没有对等实现
        other => {
            tracing::warn!(tool_type = %other, "不支持的工具类型，已跳过（上游仅支持函数工具）");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert_ok(body: Value) -> ConvertedResponsesRequest {
        convert(&body).expect("转换应成功")
    }

    #[test]
    fn instructions_become_system_and_string_input_becomes_user_message() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "instructions": "You are Codex.",
            "input": "list files",
        }));
        assert_eq!(r.client_model, "gpt-5-codex");
        assert!(!r.stream);
        assert_eq!(r.anthropic_body["model"], "gpt-5.6-terra");
        assert_eq!(r.anthropic_body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(
            r.anthropic_body["system"],
            json!([{"type": "text", "text": "You are Codex."}])
        );
        assert_eq!(
            r.anthropic_body["messages"],
            json!([{"role": "user", "content": [{"type": "text", "text": "list files"}]}])
        );
        assert!(r.anthropic_body.get("tools").is_none());
        assert!(r.anthropic_body.get("thinking").is_none());
    }

    #[test]
    fn freeform_tool_description_gets_adaptation_note() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "tools": [{
                "type": "custom", "name": "shell",
                "description": "Send raw text, not JSON.",
            }],
        }));
        let tool = &r.anthropic_body["tools"][0];
        let desc = tool["description"].as_str().unwrap();
        assert!(
            desc.starts_with("Send raw text, not JSON."),
            "原描述须保留在前：{desc}"
        );
        assert!(
            desc.ends_with(FREEFORM_ADAPTATION_NOTE),
            "须追加适配说明：{desc}"
        );
        assert_eq!(tool["input_schema"]["additionalProperties"], false);
    }

    #[test]
    fn max_output_tokens_maps_to_max_tokens() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "max_output_tokens": 8192,
        }));
        assert_eq!(r.anthropic_body["max_tokens"], 8192);
    }

    #[test]
    fn rejects_missing_model_and_input() {
        assert!(convert(&json!({"input": "hi"})).is_err());
        assert!(convert(&json!({"model": "  ", "input": "hi"})).is_err());
        assert!(convert(&json!({"model": "gpt-5-codex"})).is_err());
        assert!(convert(&json!({"model": "gpt-5-codex", "input": []})).is_err());
        // 只有 reasoning item：没有任何可转换内容
        assert!(
            convert(&json!({
                "model": "gpt-5-codex",
                "input": [{"type": "reasoning", "summary": []}],
            }))
            .is_err()
        );
    }

    #[test]
    fn rejects_stateful_request_with_previous_response_id() {
        let err = convert(&json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "previous_response_id": "resp_abc123",
        }))
        .expect_err("有状态请求应被拒绝");
        assert!(err.contains("previous_response_id"));
    }

    #[test]
    fn empty_or_null_previous_response_id_is_accepted() {
        assert!(
            convert(&json!({"model": "gpt-5-codex", "input": "hi", "previous_response_id": null}))
                .is_ok()
        );
        assert!(
            convert(&json!({"model": "gpt-5-codex", "input": "hi", "previous_response_id": ""}))
                .is_ok()
        );
    }

    #[test]
    fn message_items_carry_input_and_output_text() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "第一问"}]},
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "第一答"}]},
                {"role": "user", "content": "第二问"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([
                {"role": "user", "content": [{"type": "text", "text": "第一问"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "第一答"}]},
                {"role": "user", "content": [{"type": "text", "text": "第二问"}]},
            ])
        );
    }

    #[test]
    fn developer_message_item_goes_to_system() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "instructions": "base",
            "input": [
                {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "extra"}]},
                {"type": "message", "role": "user", "content": "go"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["system"],
            json!([
                {"type": "text", "text": "base"},
                {"type": "text", "text": "extra"},
            ])
        );
    }

    #[test]
    fn function_call_and_output_form_tool_roundtrip() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "读一下 README"},
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"README.md\"}",
                },
                {"type": "function_call_output", "call_id": "call_1", "output": "# Title"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([
                {"role": "user", "content": [{"type": "text", "text": "读一下 README"}]},
                {"role": "assistant", "content": [{
                    "type": "tool_use",
                    "id": "call_1",
                    "name": "read_file",
                    "input": {"path": "README.md"},
                }]},
                {"role": "user", "content": [{
                    "type": "tool_result",
                    "tool_use_id": "call_1",
                    "content": "# Title",
                }]},
            ])
        );
    }

    #[test]
    fn parallel_function_calls_merge_into_one_assistant_message() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "go"},
                {"type": "function_call", "call_id": "c1", "name": "a", "arguments": "{}"},
                {"type": "function_call", "call_id": "c2", "name": "b", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "c1", "output": "ra"},
                {"type": "function_call_output", "call_id": "c2", "output": "rb"},
            ],
        }));
        let messages = r.anthropic_body["messages"].as_array().unwrap();
        // user / assistant(2 个 tool_use) / user(2 个 tool_result)
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["content"].as_array().unwrap().len(), 2);
        assert_eq!(messages[2]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn custom_tool_call_wraps_free_text_input() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "跑个命令"},
                {"type": "custom_tool_call", "call_id": "call_x", "name": "shell", "input": "ls -la"},
                {"type": "custom_tool_call_output", "call_id": "call_x", "output": "total 0"},
            ],
        }));
        let messages = r.anthropic_body["messages"].as_array().unwrap();
        assert_eq!(
            messages[1]["content"][0],
            json!({
                "type": "tool_use",
                "id": "call_x",
                "name": "shell",
                "input": {"input": "ls -la"},
            })
        );
        assert_eq!(
            messages[2]["content"][0],
            json!({
                "type": "tool_result",
                "tool_use_id": "call_x",
                "content": "total 0",
            })
        );
    }

    #[test]
    fn tool_items_missing_call_id_or_name_are_skipped() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "go"},
                {"type": "function_call", "name": "no_id", "arguments": "{}"},
                {"type": "function_call", "call_id": "c1", "arguments": "{}"},
                {"type": "function_call_output", "output": "orphan"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([{"role": "user", "content": [{"type": "text", "text": "go"}]}])
        );
    }

    #[test]
    fn invalid_arguments_json_degrades_to_empty_object() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "message", "role": "user", "content": "go"},
                {"type": "function_call", "call_id": "c1", "name": "t", "arguments": "{not json"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"][1]["content"][0]["input"],
            json!({})
        );
    }

    #[test]
    fn flat_function_tool_and_custom_tool_are_converted() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "tools": [
                {
                    "type": "function",
                    "name": "read_file",
                    "description": "读文件",
                    "parameters": {"type": "object", "properties": {"path": {"type": "string"}}},
                    "strict": true,
                },
                {"type": "custom", "name": "shell", "description": "跑命令"},
                {"type": "web_search"},
            ],
        }));
        assert_eq!(
            r.anthropic_body["tools"],
            json!([
                {
                    "name": "read_file",
                    "description": "读文件",
                    "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}},
                },
                {
                    "name": "shell",
                    // custom 工具的描述末尾追加适配说明，原文保持在前
                    "description": format!("跑命令{FREEFORM_ADAPTATION_NOTE}"),
                    "input_schema": custom_tool_schema(),
                },
            ])
        );
    }

    #[test]
    fn additional_tools_item_expands_namespaces_into_flat_tools() {
        // Codex CLI 0.148 的真实形态：顶层 tools 为 null，工具挂在 developer item 上，
        // 且按 namespace 分组
        let r = convert_ok(json!({
            "model": "gpt-5.6-terra",
            "input": [
                {"type": "message", "role": "user", "content": "读一下 demo.txt"},
                {
                    "type": "additional_tools",
                    "role": "developer",
                    "tools": [
                        {
                            "type": "namespace",
                            "name": "functions",
                            "description": "",
                            "tools": [
                                {"type": "custom", "name": "exec", "description": "跑 JS",
                                 "format": {"type": "grammar", "syntax": "lark", "definition": "x"}},
                                {"type": "function", "name": "wait", "description": "等",
                                 "parameters": {"type": "object", "properties": {}}, "strict": true},
                            ],
                        },
                        {
                            "type": "namespace",
                            "name": "collaboration",
                            "tools": [
                                {"type": "function", "name": "list_agents", "description": "列出",
                                 "parameters": {"type": "object", "properties": {}}},
                            ],
                        },
                    ],
                },
            ],
            "tools": null,
        }));

        let names: Vec<&str> = r.anthropic_body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        // namespace 名不拼进工具名：模型回调时给的就是叶子名。
        // collaboration 的 list_agents 被整体跳过——非默认命名空间的子工具客户端调不动
        assert_eq!(names, vec!["exec", "wait"]);
        assert_eq!(
            r.anthropic_body["tools"][0]["input_schema"],
            custom_tool_schema()
        );
        // exec 是 custom，响应侧要还原成 custom_tool_call
        assert_eq!(r.custom_tools, ["exec".to_string()].into_iter().collect());
        // additional_tools item 本身不产生任何消息
        assert_eq!(r.anthropic_body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn top_level_tool_wins_over_same_name_in_additional_tools() {
        let r = convert_ok(json!({
            "model": "gpt-5.6-terra",
            "input": [
                {"type": "message", "role": "user", "content": "go"},
                {
                    "type": "additional_tools",
                    "tools": [{"type": "custom", "name": "exec", "description": "来自 item"}],
                },
            ],
            "tools": [{
                "type": "function",
                "name": "exec",
                "description": "来自顶层",
                "parameters": {"type": "object", "properties": {}},
            }],
        }));
        let tools = r.anthropic_body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["description"], "来自顶层");
        // 胜出的是 function 形态，不该被登记为 custom
        assert!(r.custom_tools.is_empty());
    }

    #[test]
    fn namespace_beyond_depth_limit_is_skipped() {
        // 逐层包裹出超过 MAX_NAMESPACE_DEPTH 的嵌套。每层都必须是默认命名空间，
        // 否则会被白名单在最外层直接跳过，深度守卫根本走不到
        let mut tool = json!({
            "type": "function",
            "name": "deep",
            "parameters": {"type": "object", "properties": {}},
        });
        for _ in 0..=MAX_NAMESPACE_DEPTH {
            tool = json!({"type": "namespace", "name": DEFAULT_TOOL_NAMESPACE, "tools": [tool]});
        }
        let r = convert_ok(json!({
            "model": "gpt-5.6-terra",
            "input": "hi",
            "tools": [tool],
        }));
        assert!(r.anthropic_body.get("tools").is_none());
    }

    #[test]
    fn unsupported_tools_only_yields_no_tools_field() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "tools": [{"type": "web_search"}],
        }));
        assert!(r.anthropic_body.get("tools").is_none());
    }

    #[test]
    fn reasoning_effort_maps_to_thinking() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "reasoning": {"effort": "high", "summary": "auto"},
        }));
        assert_eq!(
            r.anthropic_body["thinking"],
            json!({"type": "enabled", "budget_tokens": 24576})
        );

        let minimal = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "reasoning": {"effort": "minimal"},
        }));
        assert!(minimal.anthropic_body.get("thinking").is_none());
    }

    #[test]
    fn encrypted_content_include_is_ignored() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "include": ["reasoning.encrypted_content"],
            "store": false,
            "parallel_tool_calls": false,
        }));
        // include / store / parallel_tool_calls 都不该出现在下游请求体里
        let obj = r.anthropic_body.as_object().unwrap();
        assert!(!obj.contains_key("include"));
        assert!(!obj.contains_key("store"));
        assert!(!obj.contains_key("parallel_tool_calls"));
        assert_eq!(obj.len(), 4); // model / max_tokens / messages / stream
    }

    #[test]
    fn tool_choice_other_than_auto_still_converts() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": "hi",
            "tool_choice": {"type": "function", "name": "read_file"},
        }));
        // tool_choice 下游无读取点，只 WARN 留痕，不写入请求体
        assert!(r.anthropic_body.get("tool_choice").is_none());
    }

    #[test]
    fn input_image_data_url_becomes_image_block() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [{
                "type": "message",
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "看图"},
                    {"type": "input_image", "image_url": "data:image/png;base64,AAAA"},
                ],
            }],
        }));
        assert_eq!(
            r.anthropic_body["messages"][0]["content"][1],
            json!({
                "type": "image",
                "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"},
            })
        );
    }

    #[test]
    fn stream_flag_is_propagated() {
        let r = convert_ok(json!({"model": "gpt-5-codex", "input": "hi", "stream": true}));
        assert!(r.stream);
        assert_eq!(r.anthropic_body["stream"], true);
    }

    #[test]
    fn unknown_item_types_and_parts_are_skipped() {
        let r = convert_ok(json!({
            "model": "gpt-5-codex",
            "input": [
                {"type": "brand_new_item", "foo": 1},
                {"type": "message", "role": "user", "content": [
                    {"type": "input_audio", "data": "x"},
                    {"type": "input_text", "text": "still here"},
                ]},
            ],
        }));
        assert_eq!(
            r.anthropic_body["messages"],
            json!([{"role": "user", "content": [{"type": "text", "text": "still here"}]}])
        );
    }
}
