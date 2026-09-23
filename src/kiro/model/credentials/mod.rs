// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Kiro OAuth 凭证数据模型（模块目录）
//!
//! 支持从 Kiro IDE 的凭证文件加载，使用 Social 认证方式
//! 支持单账号和多账号配置格式
//!
//! 按功能拆分：model（结构体与 ARN 常量）、config（配置加载）、methods（行为方法）、tests（测试）。

mod config;
mod methods;
mod model;

#[cfg(test)]
mod tests;

pub use config::CredentialsConfig;
pub use model::KiroCredentials;
#[allow(unused_imports)] // 跨模块测试（provider/token_manager）引用
pub(crate) use model::{
    BUILDER_ID_PLACEHOLDER_PROFILE_ARN, SOCIAL_PROFILE_ARN, canonicalize_auth_method_value,
    fallback_profile_arn_value,
};
