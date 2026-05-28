// Copyright (c) 2026 Harllan He. Licensed under MIT.
use serde::Deserialize;

use crate::kiro::parser::error::ParseResult;
use crate::kiro::parser::frame::Frame;

use super::base::EventPayload;

/// 单条代码引用
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeReference {
    /// 开源许可证名称（如 "MIT"）
    #[serde(default)]
    pub license_name: String,
    /// 仓库名称
    #[serde(default)]
    pub repository: String,
    /// 源文件 URL
    #[serde(default)]
    pub url: String,
}

/// 代码引用事件
///
/// Kiro 后端在生成代码时，若检测到与开源代码相似的片段，
/// 会通过此事件返回来源信息，用于开源许可证合规追踪。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeReferenceEvent {
    /// 代码引用列表
    #[serde(default)]
    pub references: Vec<CodeReference>,
}

impl EventPayload for CodeReferenceEvent {
    fn from_frame(frame: &Frame) -> ParseResult<Self> {
        frame.payload_as_json()
    }
}
