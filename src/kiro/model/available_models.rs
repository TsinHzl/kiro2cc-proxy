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
    /// 官方费率倍率
    #[serde(default)]
    pub rate_multiplier: Option<f64>,
}
