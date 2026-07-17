// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 支持模型列表查询数据模型
//!
//! 包含 ListAvailableModels API 的响应类型定义

use serde::Deserialize;

/// 支持模型列表查询响应
#[derive(Debug, Clone, Deserialize)]
pub struct AvailableModelsResponse {
    /// 模型列表
    #[serde(default)]
    pub models: Vec<AvailableModelInfo>,
}

/// 单个模型的费率信息
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableModelInfo {
    /// Kiro 侧模型 ID
    pub model_id: String,
    /// Kiro 侧模型显示名称
    #[serde(default)]
    pub model_name: String,
    /// 官方费率倍率
    #[serde(default)]
    pub rate_multiplier: Option<f64>,
    /// token 上限
    #[serde(default)]
    pub token_limits: TokenLimits,
}

/// 模型 token 上限
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenLimits {
    /// 最大输入 token 数（当前仅解析，暂无消费方；保留以匹配上游真实字段结构并纳入反序列化回归测试）
    #[serde(default)]
    #[allow(dead_code)]
    pub max_input_tokens: u64,
    /// 最大输出 token 数
    #[serde(default)]
    pub max_output_tokens: u64,
}
