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
    /// Kiro 侧模型描述
    #[serde(default)]
    pub description: Option<String>,
    /// 官方费率倍率
    #[serde(default)]
    pub rate_multiplier: Option<f64>,
    /// 上游声明的输入模态（如 `["TEXT", "IMAGE"]`）；未返回或为 null 时为空数组，
    /// Admin 模型页据此展示「支持文本 / 图片」徽章
    #[serde(default, deserialize_with = "deserialize_nullable_string_vec")]
    pub supported_input_types: Vec<String>,
    /// token 上限
    #[serde(default)]
    pub token_limits: TokenLimits,
    /// Kiro 侧模型的额外请求字段 schema（如 thinking/output_config/max_tokens），当前仅解析用于反序列化校验，暂无消费方
    #[serde(default)]
    #[allow(dead_code)]
    pub additional_model_request_fields_schema: Option<serde_json::Value>,
}

/// 把上游显式 `null` 归一为空数组。
///
/// `#[serde(default)]` 只覆盖「字段缺失」，不覆盖「字段为 null」；
/// 上游若返回 `"supportedInputTypes": null`，`Vec<String>` 会以
/// `invalid type: null, expected a sequence` 失败，进而让整份 `models[]` 解析失败。
fn deserialize_nullable_string_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<String>>::deserialize(deserializer)?.unwrap_or_default())
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
