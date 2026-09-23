// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 账号配置加载（单对象/数组格式识别与排序）

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::model::KiroCredentials;

/// 账号配置（支持单对象或数组格式）
///
/// 自动识别配置文件格式：
/// - 单对象格式（旧格式，向后兼容）
/// - 数组格式（新格式，支持多账号）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CredentialsConfig {
    /// 单个账号（旧格式）
    Single(Box<KiroCredentials>),
    /// 多账号数组（新格式）
    Multiple(Vec<KiroCredentials>),
}

impl CredentialsConfig {
    /// 从文件加载账号配置
    ///
    /// - 如果文件不存在，返回空数组
    /// - 如果文件内容为空，返回空数组
    /// - 支持单对象或数组格式
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();

        // 文件不存在时返回空数组
        if !path.exists() {
            return Ok(CredentialsConfig::Multiple(vec![]));
        }

        let content = fs::read_to_string(path)?;

        // 文件为空时返回空数组
        if content.trim().is_empty() {
            return Ok(CredentialsConfig::Multiple(vec![]));
        }

        let config = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// 转换为按优先级排序的账号列表
    pub fn into_sorted_credentials(self) -> Vec<KiroCredentials> {
        match self {
            CredentialsConfig::Single(mut cred) => {
                cred.canonicalize_auth_method();
                vec![*cred]
            }
            CredentialsConfig::Multiple(mut creds) => {
                // 按优先级排序（数字越小优先级越高）
                creds.sort_by_key(|c| c.priority);
                for cred in &mut creds {
                    cred.canonicalize_auth_method();
                }
                creds
            }
        }
    }

    /// 获取账号数量
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        match self {
            CredentialsConfig::Single(_) => 1,
            CredentialsConfig::Multiple(creds) => creds.len(),
        }
    }

    /// 判断是否为空
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        match self {
            CredentialsConfig::Single(_) => false,
            CredentialsConfig::Multiple(creds) => creds.is_empty(),
        }
    }

    /// 判断是否为多账号格式（数组格式）
    pub fn is_multiple(&self) -> bool {
        matches!(self, CredentialsConfig::Multiple(_))
    }
}
