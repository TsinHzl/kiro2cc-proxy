use super::entry::{CredentialEntry, DisabledReason, HealthStatus};
use crate::http_client::ProxyConfig;
use crate::kiro::model::credentials::KiroCredentials;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use tokio::sync::Mutex as TokioMutex;

use crate::model::config::Config;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::time::{Duration as StdDuration, Instant};

// ============================================================================

/// 账号条目快照（用于 Admin API 读取）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialEntrySnapshot {
    /// 账号唯一 ID
    pub id: u64,
    /// 优先级
    pub priority: u32,
    /// 是否被禁用
    pub disabled: bool,
    /// 连续失败次数
    pub failure_count: u32,
    /// 认证方式
    pub auth_method: Option<String>,
    /// 是否有 Profile ARN
    pub has_profile_arn: bool,
    /// Token 过期时间
    pub expires_at: Option<String>,
    /// refreshToken 的 SHA-256 哈希（用于前端重复检测）
    pub refresh_token_hash: Option<String>,
    /// 用户邮箱（用于前端显示）
    pub email: Option<String>,
    /// 用户昵称/备注名（用于前端显示）
    pub nickname: Option<String>,
    /// API 调用成功次数
    pub success_count: u64,
    /// 最后一次 API 调用时间（RFC3339 格式）
    pub last_used_at: Option<String>,
    /// Token 刷新连续失败次数
    pub refresh_failure_count: u32,
    /// 是否配置了账号级代理
    pub has_proxy: bool,
    /// 代理 URL（用于前端展示）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    /// 健康状态
    pub health_status: HealthStatus,
    /// 被限流次数（429 响应，累计）
    pub throttle_count: u64,
    /// 禁用原因（manual / too_many_failures / quota_exceeded / invalid_refresh_token /
    /// too_many_refresh_failures）；None = 未禁用。Admin 面板据此区分「已禁用」与「额度已用尽」
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<DisabledReason>,
    /// 账号级 thinking adaptive 注入开关
    pub thinking_adaptive: bool,
}

/// 账号管理器状态快照
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerSnapshot {
    /// 账号条目列表
    pub entries: Vec<CredentialEntrySnapshot>,
    /// 当前活跃账号 ID
    pub current_id: u64,
    /// 总账号数量
    pub total: usize,
    /// 可用账号数量
    pub available: usize,
}

/// 多账号 Token 管理器
///
/// 支持多个账号的管理，实现固定优先级 + 故障转移策略
/// 故障统计基于 API 调用结果，而非 Token 刷新结果
pub struct MultiTokenManager {
    pub(crate) config: Config,
    pub(crate) proxy: Option<ProxyConfig>,
    /// 账号条目列表
    pub(crate) entries: Mutex<Vec<CredentialEntry>>,
    /// 当前活动账号 ID
    pub(crate) current_id: Mutex<u64>,
    /// Token 刷新锁，确保同一时间只有一个刷新操作
    pub(crate) refresh_lock: TokioMutex<()>,
    /// 账号文件路径（用于回写）
    pub(crate) credentials_path: Option<PathBuf>,
    /// 是否为多账号格式（数组格式才回写）
    pub(crate) is_multiple_format: AtomicBool,
    /// 负载均衡模式（运行时可修改）
    pub(crate) load_balancing_mode: Mutex<String>,
    /// 最近一次统计持久化时间（用于 debounce）
    pub(crate) last_stats_save_at: Mutex<Option<Instant>>,
    /// 统计数据是否有未落盘更新
    pub(crate) stats_dirty: AtomicBool,
    /// Round-Robin 计数器（balanced 模式下用于均匀轮转账号）
    pub(crate) rr_counter: AtomicU64,
    /// Sticky cache：agentContinuationId → 账号绑定关系
    pub(crate) sticky_cache: Mutex<HashMap<String, StickyCacheEntry>>,
    /// Sticky cache 命中次数（lock-free 统计）
    pub(crate) sticky_hits: AtomicU64,
    /// Sticky cache 未命中次数（包括无 continuation_id、TTL 过期、账号不健康）
    pub(crate) sticky_misses: AtomicU64,
    /// 持久化串行锁：串行化 credentials/stats 的序列化+写盘，避免多路径并发交错写
    pub(crate) persist_lock: Mutex<()>,
    /// 历史最大账号 ID（单调递增，跨账号删除/重启持久化，防止 ID 被复用导致
    /// 新账号继承已删除旧账号的用量/失败/限流历史记录）
    pub(crate) next_id_counter: AtomicU64,
}

/// 每个账号最大 API 调用失败次数
pub(crate) const MAX_FAILURES_PER_CREDENTIAL: u32 = 3;
/// 「范围内全部账号均因额度用尽而不可用」的机器可识别标记
///
/// 由 `describe_unavailable` 写入错误消息，供 HTTP 层映射为 402 而非 502 —— 额度耗尽
/// 当月不可恢复，若按 5xx 返回会让客户端把它当作瞬态故障反复重试。
pub const QUOTA_EXHAUSTED_ALL_MARKER: &str = "QUOTA_EXHAUSTED_ALL";
/// 统计数据持久化防抖间隔
pub(crate) const STATS_SAVE_DEBOUNCE: StdDuration = StdDuration::from_secs(30);
/// Sticky cache 条目存活时间（60 分钟不活跃后自动淘汰）
pub(crate) const STICKY_CACHE_TTL: StdDuration = StdDuration::from_secs(60 * 60);

/// 同一会话在同一账号上连续 429 多少次后才解除 sticky 绑定
///
/// Kiro 的 429 常是端点级短时限流，配合 rotation_bias 递增与端点桶封禁已足以让
/// 新会话避让该账号；过早解绑会让长会话反复丢失 prompt cache，反而放大限流。
pub(crate) const STICKY_THROTTLE_EVICT_THRESHOLD: u32 = 3;

pub(crate) const TOKEN_REFRESH_COOLDOWN: StdDuration = StdDuration::from_secs(30);

/// Sticky cache 条目：记录会话到账号的绑定关系
pub(crate) struct StickyCacheEntry {
    pub(crate) credential_id: u64,
    /// 最后一次命中/写入时间，用于 TTL 计算
    pub(crate) inserted_at: Instant,
    /// 该会话在当前绑定账号上连续遭遇 429 的次数
    ///
    /// 单次 429 多为端点级瞬时限流，立即解绑会丢弃已建立的 prompt cache。
    /// 仅当连续限流达到 STICKY_THROTTLE_EVICT_THRESHOLD 才判定该账号确实不适合
    /// 承载此会话，执行解绑重选。任一次成功即清零。
    pub(crate) consecutive_throttles: u32,
}

/// API 调用上下文
///
/// 绑定特定账号的调用上下文，确保 token、credentials 和 id 的一致性
/// 用于解决并发调用时 current_id 竞态问题
#[derive(Clone)]
pub struct CallContext {
    /// 账号 ID（用于 report_success/report_failure）
    pub id: u64,
    /// 账号信息（用于构建请求头）
    pub credentials: KiroCredentials,
    /// 访问 Token
    pub token: String,
}
