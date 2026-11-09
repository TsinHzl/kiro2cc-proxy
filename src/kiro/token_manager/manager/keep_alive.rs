//! 账号保活：后台定期刷新长期闲置的账号，让 refreshToken 保持活跃。
//!
//! 背景：Token 只在账号被请求用到时才刷新，被禁用、优先级靠后的账号从不刷新，
//! 它们的 refreshToken 长期不被使用，可能在服务端过期。保活任务对这类账号定期刷新一次，
//! 并把服务端轮换出的新 refreshToken 写回账号文件。
//!
//! 为降低被风控的风险，做了以下约束：
//! - 始终开启，没有开关；
//! - 间隔以天计（默认 72 小时，下限 24 小时），只处理距上次刷新已满间隔的账号；
//! - 账号之间随机间隔 30~120 秒，每轮最多处理 [`MAX_PER_ROUND`] 个，不会批量同时刷新；
//! - 失败不重试：同一账号失败后至少等满一个间隔才会再尝试；
//! - 已被服务端撤销（`InvalidRefreshToken`）或刷新连续失败被禁用的账号直接跳过。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Duration, Utc};

use super::super::entry::DisabledReason;
use super::super::refresh::{RefreshTokenInvalidError, refresh_token};
use super::super::types::MultiTokenManager;
use crate::kiro::model::credentials::KiroCredentials;

/// 保活间隔下限（小时），防止误配成高频刷新
const MIN_KEEP_ALIVE_HOURS: u32 = 24;
/// 默认保活间隔（小时）
const DEFAULT_KEEP_ALIVE_HOURS: u32 = 72;
/// 每轮检查的间隔
const CHECK_INTERVAL: StdDuration = StdDuration::from_secs(3600);
/// 每轮最多保活的账号数，剩余的留给下一轮
const MAX_PER_ROUND: usize = 5;
/// 同一轮内相邻两个账号之间的随机等待范围（秒）
const GAP_SECS: std::ops::RangeInclusive<u64> = 30..=120;
/// 启动后首轮检查前的随机等待范围（秒），避免重启后立刻发请求
const STARTUP_DELAY_SECS: std::ops::RangeInclusive<u64> = 120..=600;

static KEEP_ALIVE_HOURS: AtomicU32 = AtomicU32::new(DEFAULT_KEEP_ALIVE_HOURS);

/// 设置保活间隔（小时），低于下限按下限处理（启动时由 main 根据配置设置）
pub fn set_keep_alive_hours(hours: u32) {
    KEEP_ALIVE_HOURS.store(hours.max(MIN_KEEP_ALIVE_HOURS), Ordering::Relaxed);
}

/// 当前生效的保活间隔（小时）
pub fn keep_alive_hours() -> u32 {
    KEEP_ALIVE_HOURS.load(Ordering::Relaxed)
}

/// 判断账号是否需要保活（纯逻辑，便于单测）
///
/// 最近一次刷新时间用 `expires_at` 近似：刷新成功后 expires_at 会被设为「刷新时刻 + 有效期」，
/// 有效期只有几小时，相对以天计的间隔可以忽略；这样不需要额外持久化「上次刷新时间」，重启后依然准确。
/// `expires_at` 缺失或无法解析时视为需要保活。
pub(crate) fn needs_keep_alive(
    credentials: &KiroCredentials,
    disabled_reason: Option<DisabledReason>,
    now: DateTime<Utc>,
    interval: Duration,
) -> bool {
    if matches!(
        disabled_reason,
        Some(DisabledReason::InvalidRefreshToken | DisabledReason::TooManyRefreshFailures)
    ) {
        return false;
    }
    if credentials
        .refresh_token
        .as_deref()
        .is_none_or(|t| t.is_empty())
    {
        return false;
    }
    match credentials
        .expires_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
    {
        Some(expires) => now - expires.with_timezone(&Utc) >= interval,
        None => true,
    }
}

impl MultiTokenManager {
    /// 需要保活的账号 ID（含被禁用的账号，按 ID 升序）
    pub(crate) fn keep_alive_candidates(&self, now: DateTime<Utc>, interval: Duration) -> Vec<u64> {
        let entries = self.entries.lock();
        let mut ids: Vec<u64> = entries
            .iter()
            .filter(|e| {
                let reason = if e.disabled { e.disabled_reason } else { None };
                needs_keep_alive(&e.credentials, reason, now, interval)
            })
            .map(|e| e.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// 强制刷新指定账号一次（不论 accessToken 是否过期），并写回轮换后的 refreshToken
    ///
    /// 与请求路径的刷新共用 refresh_lock / credential_identity_lock，避免并发刷新同一账号
    /// 时旧 refreshToken 被重复使用。
    pub(crate) async fn keep_alive_refresh(&self, id: u64) -> anyhow::Result<()> {
        let _guard = self.refresh_lock.lock().await;
        let _identity_guard = self.credential_identity_lock.lock().await;

        let current = {
            let entries = self.entries.lock();
            entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.credentials.clone())
                .ok_or_else(|| anyhow::anyhow!("账号 #{} 不存在", id))?
        };

        let proxy = current.effective_proxy(self.proxy.as_ref());
        let refreshed = refresh_token(&current, &self.config, proxy.as_ref()).await?;
        self.commit_refreshed_credentials_if_identity_matches(id, &current, &refreshed)?;
        self.persist_credentials()?;
        Ok(())
    }

    /// 启动保活后台任务。任务常驻，每小时检查一轮
    pub fn spawn_keep_alive(self: &Arc<Self>) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            // 失败的账号在满一个间隔之前不再尝试（仅内存记录，重启后清空）
            let mut failed_at: HashMap<u64, Instant> = HashMap::new();
            tokio::time::sleep(StdDuration::from_secs(fastrand::u64(STARTUP_DELAY_SECS))).await;
            loop {
                manager.keep_alive_round(&mut failed_at).await;
                tokio::time::sleep(CHECK_INTERVAL).await;
            }
        });
    }

    async fn keep_alive_round(&self, failed_at: &mut HashMap<u64, Instant>) {
        let hours = keep_alive_hours();
        let interval = Duration::hours(i64::from(hours));
        let retry_after = StdDuration::from_secs(u64::from(hours) * 3600);
        failed_at.retain(|_, t| t.elapsed() < retry_after);

        let candidates: Vec<u64> = self
            .keep_alive_candidates(Utc::now(), interval)
            .into_iter()
            .filter(|id| !failed_at.contains_key(id))
            .take(MAX_PER_ROUND)
            .collect();
        if candidates.is_empty() {
            return;
        }
        tracing::info!("账号保活：本轮处理 {} 个账号", candidates.len());

        for (i, id) in candidates.into_iter().enumerate() {
            if i > 0 {
                tokio::time::sleep(StdDuration::from_secs(fastrand::u64(GAP_SECS))).await;
            }
            match self.keep_alive_refresh(id).await {
                Ok(()) => tracing::info!("账号 #{} 保活刷新成功", id),
                Err(e) => {
                    failed_at.insert(id, Instant::now());
                    if e.downcast_ref::<RefreshTokenInvalidError>().is_some() {
                        self.report_refresh_token_invalid(id);
                    } else {
                        tracing::warn!(
                            "账号 #{} 保活刷新失败（不重试，{} 小时后再试）: {}",
                            id,
                            hours,
                            e
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(refresh: Option<&str>, expires_at: Option<DateTime<Utc>>) -> KiroCredentials {
        KiroCredentials {
            refresh_token: refresh.map(str::to_string),
            expires_at: expires_at.map(|t| t.to_rfc3339()),
            ..Default::default()
        }
    }

    #[test]
    fn idle_account_needs_keep_alive() {
        let now = Utc::now();
        let c = creds(Some("rt"), Some(now - Duration::hours(80)));
        assert!(needs_keep_alive(&c, None, now, Duration::hours(72)));
    }

    #[test]
    fn recently_refreshed_account_is_skipped() {
        let now = Utc::now();
        // 刚刷新过：expires_at 在未来
        let fresh = creds(Some("rt"), Some(now + Duration::hours(1)));
        assert!(!needs_keep_alive(&fresh, None, now, Duration::hours(72)));
        // 已过期但还没满一个间隔
        let recent = creds(Some("rt"), Some(now - Duration::hours(10)));
        assert!(!needs_keep_alive(&recent, None, now, Duration::hours(72)));
    }

    #[test]
    fn missing_or_bad_expiry_needs_keep_alive() {
        let now = Utc::now();
        assert!(needs_keep_alive(
            &creds(Some("rt"), None),
            None,
            now,
            Duration::hours(72)
        ));
        let mut bad = creds(Some("rt"), None);
        bad.expires_at = Some("not-a-date".to_string());
        assert!(needs_keep_alive(&bad, None, now, Duration::hours(72)));
    }

    #[test]
    fn missing_refresh_token_is_skipped() {
        let now = Utc::now();
        let old = Some(now - Duration::days(30));
        assert!(!needs_keep_alive(
            &creds(None, old),
            None,
            now,
            Duration::hours(72)
        ));
        assert!(!needs_keep_alive(
            &creds(Some(""), old),
            None,
            now,
            Duration::hours(72)
        ));
    }

    #[test]
    fn revoked_or_failing_accounts_are_skipped() {
        let now = Utc::now();
        let c = creds(Some("rt"), Some(now - Duration::days(30)));
        for reason in [
            DisabledReason::InvalidRefreshToken,
            DisabledReason::TooManyRefreshFailures,
        ] {
            assert!(!needs_keep_alive(
                &c,
                Some(reason),
                now,
                Duration::hours(72)
            ));
        }
        // 手动禁用、额度用尽的账号仍需要保活：它们之后会被重新启用或跨月自动恢复
        for reason in [
            DisabledReason::Manual,
            DisabledReason::QuotaExceeded,
            DisabledReason::TooManyFailures,
        ] {
            assert!(needs_keep_alive(&c, Some(reason), now, Duration::hours(72)));
        }
    }

    #[test]
    fn manager_candidates_include_idle_and_exclude_revoked() {
        use crate::model::config::Config;

        let now = Utc::now();
        let fresh = creds(Some("fresh-token"), Some(now + Duration::hours(1)));
        let idle = creds(Some("idle-token"), Some(now - Duration::days(10)));
        let revoked = creds(Some("revoked-token"), Some(now - Duration::days(10)));
        let manager = MultiTokenManager::new(
            Config::default(),
            vec![fresh, idle, revoked],
            None,
            None,
            false,
        )
        .unwrap();

        // 账号 ID 依次为 1、2、3；3 号被服务端撤销后不再保活
        manager.report_refresh_token_invalid(3);
        let ids = manager.keep_alive_candidates(now, Duration::hours(72));
        assert_eq!(ids, vec![2]);
    }

    #[test]
    fn interval_has_a_floor() {
        set_keep_alive_hours(1);
        assert_eq!(keep_alive_hours(), MIN_KEEP_ALIVE_HOURS);
        set_keep_alive_hours(DEFAULT_KEEP_ALIVE_HOURS);
    }
}
