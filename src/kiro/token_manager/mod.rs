// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Token 管理模块
//!
//! 负责 Token 过期检测和刷新，支持 Social 和 IdC 认证方式
//! 支持多账号 (MultiTokenManager) 管理

use chrono::{DateTime, Duration, Utc};

use crate::kiro::model::credentials::KiroCredentials;

/// 检查 Token 是否在指定时间内过期
pub(crate) fn is_token_expiring_within(
    credentials: &KiroCredentials,
    minutes: i64,
) -> Option<bool> {
    credentials
        .expires_at
        .as_ref()
        .and_then(|expires_at| DateTime::parse_from_rfc3339(expires_at).ok())
        .map(|expires| expires <= Utc::now() + Duration::minutes(minutes))
}

mod entry;
mod manager;
mod refresh;
mod types;

pub use entry::{DisabledReason, HealthStatus};
pub(crate) use refresh::UsageLimitsUnsupportedError;
#[cfg(test)]
pub(crate) use refresh::{is_token_expired, is_token_expiring_soon, refresh_token};
pub(crate) use types::QUOTA_EXHAUSTED_ALL_MARKER;
pub use types::{CallContext, MultiTokenManager};

#[cfg(test)]
mod tests;
