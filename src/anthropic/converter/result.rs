// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 转换结果与转换错误类型

use crate::kiro::model::requests::conversation::ConversationState;

/// 转换结果
#[derive(Debug)]
pub struct ConversionResult {
    /// 转换后的 Kiro 请求
    pub conversation_state: ConversationState,
    /// 模型专属请求参数（thinking、output_config、max_tokens）
    pub additional_model_request_fields: Option<serde_json::Value>,
    /// 是否为 Claude Code 的 `/compact`（手动或 auto-compact）压缩请求
    ///
    /// 用于 provider 层选择更长的上游超时（压缩请求通常处理超长历史，
    /// 耗时明显高于普通请求）。检测逻辑见 `is_compact_request`。
    pub is_compact_request: bool,
    /// web_search server tool 声明的次数上限（请求未携带该 server tool 时为 None）
    ///
    /// 供 handlers 桥接层计算多轮搜索上限 `min(max_uses, 5)`。
    pub web_search_max_uses: Option<Option<i32>>,
    /// 客户端是否请求了 `thinking: {"type": "adaptive"}`
    ///
    /// 账号在 provider 阶段才确定，converter 阶段先记录该意图，
    /// 由 provider 按目标账号的 `thinking_adaptive` 开关决定是否注入。
    pub thinking_adaptive_requested: bool,
}

/// 转换错误
#[derive(Debug)]
pub enum ConversionError {
    UnsupportedModel(String),
    EmptyMessages,
}

impl std::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConversionError::UnsupportedModel(model) => write!(f, "模型不支持: {}", model),
            ConversionError::EmptyMessages => write!(f, "消息列表为空"),
        }
    }
}

impl std::error::Error for ConversionError {}
