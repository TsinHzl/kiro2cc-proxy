use super::super::entry::{CredentialEntry, DisabledReason, HealthStatus, StatsEntry};
use super::super::types::{MultiTokenManager, STATS_SAVE_DEBOUNCE};
use crate::common::fs::atomic_write;
use crate::kiro::model::credentials::KiroCredentials;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration as StdDuration, Instant};

impl MultiTokenManager {
    /// 将账号列表回写到源文件
    ///
    /// 仅在以下条件满足时回写：
    /// - 源文件是多账号格式（数组）
    /// - credentials_path 已设置
    ///
    /// # Returns
    /// - `Ok(true)` - 成功写入文件
    /// - `Ok(false)` - 跳过写入（非多账号格式或无路径配置）
    /// - `Err(_)` - 写入失败
    pub(crate) fn persist_credentials(&self) -> anyhow::Result<bool> {
        use anyhow::Context;

        let path = match &self.credentials_path {
            Some(p) => p,
            None => return Ok(false),
        };

        // 收集所有账号
        let credentials: Vec<KiroCredentials> = {
            let entries = self.entries.lock();
            entries
                .iter()
                .map(|e| {
                    let mut cred = e.credentials.clone();
                    cred.canonicalize_auth_method();
                    // 仅把「手动禁用」同步到配置文件——它是用户的显式意图，必须长期生效。
                    // 额度耗尽/连续失败属于运行时自动判定，写入这里会在重启后（若 stats
                    // 缓存缺失）被当成手动禁用，从而绕过所有自愈逻辑把账号永久钉死。
                    cred.disabled = e.disabled && e.disabled_reason == Some(DisabledReason::Manual);
                    cred
                })
                .collect()
        };

        // 单账号格式（credentials.json 原为单对象）时保持单对象形状回写，避免把用户
        // 原本的单对象文件强行转成数组；账号被删光清空后没有形状可保持，回退为数组
        // （加载时仍可正确解析为空多账号格式）。
        let json = if !self.is_multiple_format.load(Ordering::Relaxed) && credentials.len() == 1 {
            serde_json::to_string_pretty(&credentials[0]).context("序列化账号失败")?
        } else {
            serde_json::to_string_pretty(&credentials).context("序列化账号失败")?
        };

        // 原子写 + 串行化，确保数据落盘且不被并发写交错（容器持久化卷必须 fsync）
        let write_result = {
            let path = path.clone();
            let json = json.clone();
            let do_write = move || -> std::io::Result<()> {
                let _guard = self.persist_lock.lock();
                atomic_write(&path, json.as_bytes())
            };
            if tokio::runtime::Handle::try_current().is_ok() {
                tokio::task::block_in_place(do_write)
            } else {
                do_write()
            }
        };

        if let Err(e) = write_result {
            let detail = format!(
                "回写账号文件失败: path={:?}, credentials_count={}, json_bytes={}, os_error={:?}",
                path,
                credentials.len(),
                json.len(),
                e
            );
            tracing::error!("{}", detail);
            anyhow::bail!(detail);
        }

        tracing::debug!("已回写账号到文件（已 fsync）: {:?}", path);
        Ok(true)
    }

    /// 获取缓存目录（账号文件所在目录）
    pub fn cache_dir(&self) -> Option<PathBuf> {
        self.credentials_path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
    }

    /// 统计数据文件路径
    pub(crate) fn stats_path(&self) -> Option<PathBuf> {
        self.cache_dir().map(|d| d.join("kiro_stats.json"))
    }

    /// 历史最大账号 ID 计数器文件路径
    ///
    /// 与 credentials.json 同目录持久化，独立于账号列表本身，确保账号被删除后
    /// 该文件仍保留曾经分配过的最大 ID，防止下次新增账号时复用已删除账号的 ID
    /// （复用会导致新账号在 usage/failure/throttle 日志中"继承"旧账号的历史记录）。
    fn id_counter_path(&self) -> Option<PathBuf> {
        self.cache_dir().map(|d| d.join("kiro_id_counter.json"))
    }

    /// 从指定 credentials 文件路径推算出的缓存目录中加载历史最大 ID 计数器（静态版本，
    /// 供 `new()` 在实例构造前调用）
    pub(crate) fn load_id_counter_from_path(credentials_path: Option<&Path>) -> u64 {
        let Some(dir) = credentials_path.and_then(|p| p.parent()) else {
            return 0;
        };
        let path = dir.join("kiro_id_counter.json");
        let Ok(content) = std::fs::read_to_string(&path) else {
            return 0;
        };
        #[derive(Deserialize)]
        struct IdCounterFile {
            #[serde(rename = "maxId")]
            max_id: u64,
        }
        serde_json::from_str::<IdCounterFile>(&content)
            .map(|c| c.max_id)
            .unwrap_or(0)
    }

    /// 将当前历史最大 ID 计数器持久化到磁盘（写入不小于 `min_value` 的值）
    ///
    /// 锁内重新读取磁盘现有值并与 `min_value`、内存计数器三者取最大值再写入，避免并发
    /// 调用时较大值先落盘、较小值后落盘将其覆盖——否则进程在该窗口内崩溃重启，会从磁盘
    /// 读到被覆盖的较小值，削弱计数器的单调性保证（进而可能复用已分配过的 ID）。
    ///
    /// 磁盘 IO（`atomic_write` 含 `fsync`）是同步阻塞调用；若当前处于 tokio 运行时上下文中
    /// （如从 `add_credential` 等 async 方法调用），需通过 `block_in_place` 转交给阻塞线程池
    /// 执行，避免阻塞 tokio worker 线程影响其他请求的调度。
    pub(crate) fn save_id_counter_at_least(&self, min_value: u64) {
        let Some(path) = self.id_counter_path() else {
            return;
        };
        let credentials_path = self.credentials_path.clone();
        let in_memory = self.next_id_counter.load(Ordering::Relaxed);
        let do_write = move || {
            let on_disk = Self::load_id_counter_from_path(credentials_path.as_deref());
            let target = on_disk.max(min_value).max(in_memory);
            let json = serde_json::json!({ "maxId": target }).to_string();
            atomic_write(&path, json.as_bytes())
        };

        let _guard = self.persist_lock.lock();
        let result = if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(do_write)
        } else {
            do_write()
        };
        if let Err(e) = result {
            tracing::warn!("保存账号 ID 计数器失败: {}", e);
        }
    }

    /// 分配一个新的、从未被使用过的账号 ID（线程安全，单调递增，持久化）
    pub(crate) fn allocate_new_id(&self) -> u64 {
        let new_id = self.next_id_counter.fetch_add(1, Ordering::SeqCst) + 1;
        self.save_id_counter_at_least(new_id);
        new_id
    }

    /// 从磁盘加载统计数据并应用到当前条目
    pub(crate) fn load_stats(&self) {
        let path = match self.stats_path() {
            Some(p) => p,
            None => return,
        };

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return, // 首次运行时文件不存在
        };

        let stats: HashMap<String, StatsEntry> = match serde_json::from_str(&content) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("解析统计缓存失败，将忽略: {}", e);
                return;
            }
        };

        let mut entries = self.entries.lock();
        for entry in entries.iter_mut() {
            if let Some(s) = stats.get(&entry.id.to_string()) {
                entry.success_count = s.success_count;
                entry.last_used_at = s.last_used_at.clone();
                entry.throttle_count = s.throttle_count;
                if let Some(ref ts) = s.last_throttled_wall {
                    entry.last_throttled_wall = ts.parse::<DateTime<Utc>>().ok();
                }
                // 恢复自动判定的禁用态。credentials.json 只承载手动禁用，因此
                // 额度耗尽/连续失败的状态由此处接管；手动禁用优先，不被覆盖。
                // quota_exhausted_at 随 disabled_reason 一起恢复，避免 Manual 账号
                // 残留一个语义不符的历史耗尽时间戳。
                if entry.disabled_reason != Some(DisabledReason::Manual)
                    && let Some(reason) = s.disabled_reason
                {
                    entry.disabled = true;
                    entry.disabled_reason = Some(reason);
                    entry.quota_exhausted_at = s
                        .quota_exhausted_at
                        .as_ref()
                        .and_then(|ts| ts.parse::<DateTime<Utc>>().ok());
                }
            }
        }
        *self.last_stats_save_at.lock() = Some(Instant::now());
        self.stats_dirty.store(false, Ordering::Relaxed);
        tracing::info!("已从缓存加载 {} 条统计数据", stats.len());
    }

    /// 将当前统计数据持久化到磁盘
    pub(crate) fn save_stats(&self) {
        let path = match self.stats_path() {
            Some(p) => p,
            None => return,
        };

        let stats: HashMap<String, StatsEntry> = {
            let entries = self.entries.lock();
            entries
                .iter()
                .map(|e| {
                    (
                        e.id.to_string(),
                        StatsEntry {
                            success_count: e.success_count,
                            last_used_at: e.last_used_at.clone(),
                            throttle_count: e.throttle_count,
                            last_throttled_wall: e.last_throttled_wall.map(|t| t.to_rfc3339()),
                            // 仅持久化自动判定的原因；Manual 由 credentials.json 承载，
                            // 写入 stats 会让手动/自动禁用无法区分
                            disabled_reason: e.disabled_reason.filter(|r| {
                                matches!(
                                    r,
                                    DisabledReason::QuotaExceeded
                                        | DisabledReason::TooManyFailures
                                        | DisabledReason::InvalidRefreshToken
                                        | DisabledReason::TooManyRefreshFailures
                                        | DisabledReason::ProfileArnMissing
                                )
                            }),
                            quota_exhausted_at: e.quota_exhausted_at.map(|t| t.to_rfc3339()),
                        },
                    )
                })
                .collect()
        };

        match serde_json::to_string_pretty(&stats) {
            Ok(json) => {
                let _guard = self.persist_lock.lock();
                if let Err(e) = atomic_write(&path, json.as_bytes()) {
                    tracing::warn!("保存统计缓存失败: {}", e);
                } else {
                    *self.last_stats_save_at.lock() = Some(Instant::now());
                    self.stats_dirty.store(false, Ordering::Relaxed);
                }
            }
            Err(e) => tracing::warn!("序列化统计数据失败: {}", e),
        }
    }

    /// 标记统计数据已更新，并按 debounce 策略决定是否立即落盘
    pub(crate) fn save_stats_debounced(&self) {
        self.stats_dirty.store(true, Ordering::Relaxed);

        let should_flush = {
            let last = *self.last_stats_save_at.lock();
            match last {
                Some(last_saved_at) => last_saved_at.elapsed() >= STATS_SAVE_DEBOUNCE,
                None => true,
            }
        };

        if should_flush {
            self.save_stats();
        }
    }

    /// 根据账号条目计算健康状态
    pub(crate) fn compute_health(entry: &CredentialEntry) -> HealthStatus {
        if entry.disabled {
            return HealthStatus::Disabled;
        }

        // 认证失败（401/403）是严重问题，直接根据次数判断
        if entry.failure_count >= 3 {
            return HealthStatus::Unhealthy;
        }
        if entry.failure_count >= 2 {
            return HealthStatus::Degraded;
        }
        if entry.failure_count >= 1 {
            return HealthStatus::Warning;
        }

        // 限流判断：样本不足时默认健康，避免少量请求时误判
        let total_calls = entry.success_count + entry.throttle_count;
        if total_calls < 5 {
            return HealthStatus::Healthy;
        }

        let throttle_rate = entry.throttle_count as f64 / total_calls as f64;
        let very_recently_throttled = entry
            .last_throttled_at
            .map(|t| t.elapsed() < StdDuration::from_secs(120))
            .unwrap_or(false);
        let recently_throttled = entry
            .last_throttled_at
            .map(|t| t.elapsed() < StdDuration::from_secs(600))
            .unwrap_or(false);

        if very_recently_throttled && throttle_rate > 0.5 {
            HealthStatus::Unhealthy
        } else if recently_throttled && throttle_rate > 0.3 {
            HealthStatus::Degraded
        } else if recently_throttled && throttle_rate > 0.15 {
            HealthStatus::Warning
        } else {
            HealthStatus::Healthy
        }
    }
}
