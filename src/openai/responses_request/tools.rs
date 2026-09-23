// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 工具声明收集与转换：namespace 展开、custom 自由文本工具降级、同名去重

use std::collections::HashSet;

use serde_json::{Value, json};

use crate::openai::chat_request::convert_tool;

/// `namespace` 容器的嵌套深度上限；超出即跳过，避免畸形请求把栈递归穿了
pub(crate) const MAX_NAMESPACE_DEPTH: usize = 4;

/// Codex remote compaction v2 系统提示
///
/// 当检测到 `compaction_trigger` input item 时，注入该提示替代原始压缩指令。
/// 要求模型以"对话摘要"形式生成一段文字，作为后续轮次的上下文压缩内容。
pub(crate) const COMPACTION_SYSTEM_PROMPT: &str = "\
You are performing a CONTEXT COMPACTION operation. \
Your task is to produce a concise but comprehensive summary of the current conversation that \
will allow seamlessly continuing the work in a future context window.\n\
\n\
Include:\n\
- The current task/goal and its status\n\
- Key decisions and conclusions reached\n\
- Important context, file paths, or technical details\n\
- Work completed so far\n\
- Pending actions or next steps\n\
\n\
Write this as a first-person internal note to yourself (as if you are writing notes to a \
successor instance of yourself who will continue this exact task). Be concise yet complete. \
Do not include any preamble or explanation — just the summary content.";

/// 将代理自制的 compaction `encrypted_content` 解码为原始摘要文本
///
/// 本代理不产出真正的 Fernet 加密内容，而是用标准 base64 编码摘要文字。
/// 解码成功时返回摘要文本；格式不匹配或解码失败时返回 `None`（由调用方决定降级行为）。
pub(crate) fn base64_decode_compaction(encoded: &str) -> Option<String> {
    use base64::Engine as _;
    // 尝试标准 base64 解码（含 padding 变种）
    let decoded_bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(encoded))
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(encoded))
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(encoded))
        .ok()?;
    String::from_utf8(decoded_bytes).ok()
}

/// `custom` 工具的入参在 Responses 协议里是自由文本，Anthropic 只接受 JSON schema。
/// 降级为单字段对象，配合 [`ToolInputForm::FreeText`] 把原始文本塞进 `input` 字段。
pub(crate) fn custom_tool_schema() -> Value {
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
pub(crate) const FREEFORM_ADAPTATION_NOTE: &str = concat!(
    "\n\n---\n[Gateway adaptation] This freeform tool is exposed as a JSON tool. ",
    "Put the raw tool text (unquoted, no code fences) into the `input` string field."
);

/// OpenAI 工具协议的默认命名空间名——只有这个容器会被展开，理由见 [`ToolCollector::push_one`]
pub(crate) const DEFAULT_TOOL_NAMESPACE: &str = "functions";

/// 工具调用入参的两种形态
#[derive(Clone, Copy)]
pub(crate) enum ToolInputForm {
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
    /// 是否为 Codex remote compaction v2 压缩请求
    ///
    /// 当 `input` 末尾包含 `{"type":"compaction_trigger"}` 时置为 `true`。
    /// 响应侧需要将模型的文字摘要包装为 `type: "compaction"` output item，
    /// 否则 Codex 会报 "expected exactly one compaction output item, got 0"。
    pub(crate) is_compaction: bool,
}

/// 汇总各来源的工具声明
///
/// 两个来源：顶层 `tools`，以及 `input` 里的 `additional_tools` item。先收的优先——
/// 顶层声明在遍历 `input` 之前入栈，同名时后来者被丢弃。
#[derive(Default)]
pub(crate) struct ToolCollector {
    /// 已转换为 Anthropic 形态的工具
    pub(crate) tools: Vec<Value>,
    /// 已收录的工具名，用于同名去重
    seen: HashSet<String>,
    /// 其中入参为自由文本（`custom`）的工具名
    pub(crate) custom: HashSet<String>,
}

impl ToolCollector {
    pub(crate) fn push_list(&mut self, list: &[Value], depth: usize) {
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

/// 单个工具声明 → Anthropic tool
pub(crate) fn convert_one_tool(tool: &Value) -> Option<Value> {
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
