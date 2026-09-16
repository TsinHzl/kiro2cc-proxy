// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic → Kiro 协议转换器
//!
//! 负责将 Anthropic API 请求格式转换为 Kiro API 请求格式。
//! 原单体 converter.rs 按功能域拆分为以下子模块（外部符号路径不变）：
//! - `schema`: JSON Schema 规范化（$ref 展开、Kiro 严格模式清洗）
//! - `cache`: 会话级 history[0] 冻结缓存基建
//! - `prompt`: 系统提醒剥除/提取、计费头规范化、输出格式与近期知识提示注入
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
//! - `tests`: 原内联测试整体迁移（零改动）

mod cache;
mod convert;
mod fields;
mod history;
mod message;
mod model;
mod pdf;
mod prompt;
mod result;
mod schema;
mod session;
#[cfg(test)]
mod tests;
mod thinking;
mod tools;
mod websearch;

pub use convert::convert_request;
pub use result::{ConversionError, ConversionResult};

pub(crate) use thinking::additional_fields_skipped;
pub(crate) use thinking::is_luna_model;
