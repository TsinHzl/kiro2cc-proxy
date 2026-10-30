// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic → Kiro 协议转换器
//!
//! 负责将 Anthropic API 请求格式转换为 Kiro API 请求格式。
//! 原单体 converter.rs 按功能域拆分为以下子模块（外部符号路径不变）：
//! - `schema`: JSON Schema 规范化（$ref 展开、Kiro 严格模式清洗）
//! - `cache`: 会话级 history[0] 冻结缓存基建
//! - `baseline`: 会话系统块基线（会话中途新增的系统块分流出 history[0]）
//! - `prompt`: 计费头规范化、输出格式与近期知识提示注入
//! - `drift_diag`: history[0] 漂移诊断（DEBUG 日志，列出新增/消失的系统块）
//! - `prefix_diag`: history[1..] 与 tools 的前缀漂移诊断（INFO 日志，指出第一处变化的消息序号）
//! - `model`: Anthropic 模型名 → Kiro 模型 ID 映射
//! - `result`: ConversionResult / ConversionError
//! - `session`: 会话 ID 提取/派生、compact 压缩请求检测
//! - `websearch`: web_search server tool 拆分与历史占位工具
//! - `convert`: convert_request 主入口与触发类型判定
//! - `message`: 消息内容解析（文本/图片/tool_result）
//! - `pdf`: PDF 文本提取
//! - `tools`: 工具定义转换与 tool_use/tool_result 配对校验
//! - `fields`: additionalModelRequestFields 构建
//! - `thinking`: thinking 前缀生成与模型谓词
//! - `history`: 历史消息构建与合并
//! - `tests/`: 原内联测试按被测子模块拆分为目录（model/schema/pdf/prompt/fields/session/websearch/tools/thinking_gpt/history_cache）

mod baseline;
mod cache;
mod convert;
mod drift_diag;
mod fields;
mod history;
mod message;
mod model;
mod pdf;
mod prefix_diag;
mod prompt;
mod result;
mod schema;
mod session;
#[cfg(test)]
mod tests {

    /// 构造用于 fallback 派生测试的最小请求（system + 工具名 + 消息序列）
    fn fallback_req(
        system: Option<&str>,
        tool_names: &[&str],
        messages: &[(&str, &str)],
    ) -> crate::anthropic::types::MessagesRequest {
        use crate::anthropic::types::{Message as AnthropicMessage, SystemMessage, Tool};
        crate::anthropic::types::MessagesRequest {
            model: "claude-sonnet-4".to_string(),
            max_tokens: 1024,
            messages: messages
                .iter()
                .map(|(role, text)| AnthropicMessage {
                    role: (*role).to_string(),
                    content: serde_json::json!(*text),
                })
                .collect(),
            stream: false,
            system: system.map(|s| {
                vec![SystemMessage {
                    text: s.to_string(),
                }]
            }),
            tools: if tool_names.is_empty() {
                None
            } else {
                Some(
                    tool_names
                        .iter()
                        .map(|name| Tool {
                            tool_type: None,
                            name: (*name).to_string(),
                            description: String::new(),
                            input_schema: Default::default(),
                            max_uses: None,
                            defer_loading: None,
                        })
                        .collect(),
                )
            },
            tool_choice: None,
            thinking: None,
            output_config: None,
            metadata: None,
        }
    }

    mod fields;
    mod history_cache;
    mod model;
    mod pdf;
    mod prompt;
    mod schema;
    mod session;
    mod thinking_gpt;
    mod tool_images;
    mod tools;
    mod websearch;
}
mod thinking;
mod tools;
mod websearch;

pub use convert::convert_request;
pub(crate) use model::map_model;
pub use result::{ConversionError, ConversionResult};
pub use tools::{
    DEFAULT_WRITE_CHUNKING_MAX_LINES, set_write_chunking_max_lines, write_chunking_max_lines,
};

pub(crate) use thinking::{
    additional_fields_skipped, is_gpt_model, is_luna_model, native_thinking_supported,
};
