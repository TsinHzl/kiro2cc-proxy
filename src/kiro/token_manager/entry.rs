use crate::kiro::model::credentials::KiroCredentials;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Instant;

// 多账号 Token 管理器
// ============================================================================

/// 单个账号条目的状态
pub(crate) struct CredentialEntry {
    /// 账号唯一 ID
    pub(crate) id: u64,
    /// 账号信息
    pub(crate) credentials: KiroCredentials,
    /// API 调用连续失败次数
    pub(crate) failure_count: u32,
    /// Token 刷新连续失败次数（独立于 failure_count，语义为"刷新失败"而非"API 调用失败"）
    pub(crate) refresh_failure_count: u32,
    /// 是否已禁用
    pub(crate) disabled: bool,
    /// 禁用原因（用于区分手动禁用 vs 自动禁用，便于自愈）
    pub(crate) disabled_reason: Option<DisabledReason>,
    /// API 调用成功次数
    pub(crate) success_count: u64,
    /// 最后一次 API 调用时间（RFC3339 格式）
    pub(crate) last_used_at: Option<String>,
    /// 被限流次数（429 响应，累计）
    pub(crate) throttle_count: u64,
    /// 最后一次被限流时间（内存中，不持久化）
    pub(crate) last_throttled_at: Option<Instant>,
    /// 最后一次被限流时间（UTC，持久化，用于健康状态窗口计算）
    pub(crate) last_throttled_wall: Option<DateTime<Utc>>,
    /// 最后一次 token 刷新时间（用于冷却期控制）
    pub(crate) last_refreshed_at: Option<Instant>,
    /// 轮转偏移量：429 时 +1，成功时清零；选择账号时优先选 bias 最小的
    pub(crate) rotation_bias: u32,
    /// 被判定额度用尽的时间（UTC，持久化），用于跨自然月后自动恢复
    pub(crate) quota_exhausted_at: Option<DateTime<Utc>>,
}

/// 禁用原因
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisabledReason {
    /// Admin API 手动禁用
    Manual,
    /// 连续失败达到阈值后自动禁用
    TooManyFailures,
    /// 额度已用尽（如 MONTHLY_REQUEST_COUNT）
    QuotaExceeded,
    /// refreshToken 已被服务端永久撤销（invalid_grant），需人工更换凭证
    InvalidRefreshToken,
    /// 企业 IdC 账号缺少 profileArn，上游数据面接口强制要求该字段，需人工补充凭证
    ProfileArnMissing,
    /// Token 刷新连续失败达到阈值后自动禁用
    TooManyRefreshFailures,
}

impl DisabledReason {
    /// 面向客户端的原因描述（用于错误消息，便于从日志直接判断真因）
    pub(crate) fn describe(&self) -> &'static str {
        match self {
            DisabledReason::Manual => "已被手动禁用",
            DisabledReason::TooManyFailures => "因连续认证失败被自动禁用",
            DisabledReason::QuotaExceeded => "本月请求额度已用尽",
            DisabledReason::InvalidRefreshToken => "refreshToken 已被服务端撤销，需人工更换凭证",
            DisabledReason::ProfileArnMissing => {
                "缺少 profileArn，无法发起对话请求，需在账号配置中补充"
            }
            DisabledReason::TooManyRefreshFailures => "因连续 Token 刷新失败被自动禁用",
        }
    }
}

/// 账号健康状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// 正常，无失败无限流
    Healthy,
    /// 轻微问题，有少量历史限流或 1 次失败
    Warning,
    /// 降级，近期频繁限流或 2 次连续失败
    Degraded,
    /// 不健康，极近期高频限流或即将被禁用
    Unhealthy,
    /// 已禁用（手动或自动）
    Disabled,
}

#[allow(dead_code)]
impl HealthStatus {
    /// 返回前端展示用的颜色标识
    pub fn color(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "green",
            HealthStatus::Warning => "yellow",
            HealthStatus::Degraded => "orange",
            HealthStatus::Unhealthy => "red",
            HealthStatus::Disabled => "gray",
        }
    }

    /// 返回中文标签
    pub fn label(&self) -> &'static str {
        match self {
            HealthStatus::Healthy => "健康",
            HealthStatus::Warning => "警告",
            HealthStatus::Degraded => "降级",
            HealthStatus::Unhealthy => "不健康",
            HealthStatus::Disabled => "已禁用",
        }
    }
}

/// 统计数据持久化条目
#[derive(Serialize, Deserialize)]
pub(crate) struct StatsEntry {
    pub(crate) success_count: u64,
    pub(crate) last_used_at: Option<String>,
    #[serde(default)]
    pub(crate) throttle_count: u64,
    #[serde(default)]
    pub(crate) last_throttled_wall: Option<String>,
    /// 自动禁用原因（仅持久化自动判定的原因；Manual 由 credentials.json 承载）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) disabled_reason: Option<DisabledReason>,
    /// 额度用尽时间（UTC RFC3339），用于跨自然月自动恢复
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) quota_exhausted_at: Option<String>,
}
