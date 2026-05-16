//! RPM（Requests Per Minute）实时监控
//!
//! 使用滑动窗口统计最近 60 秒内的请求数量，
//! 支持全局、按凭据、按 API Key 三个维度。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use serde::Serialize;

/// 滑动窗口大小（秒）
const WINDOW_SECS: u64 = 60;

/// 单个维度的请求时间戳队列
struct TimestampQueue {
    /// 请求时间戳（单调递增）
    timestamps: Vec<Instant>,
}

impl TimestampQueue {
    fn new() -> Self {
        Self {
            timestamps: Vec::new(),
        }
    }

    /// 记录一次请求
    fn record(&mut self, now: Instant) {
        self.timestamps.push(now);
    }

    /// 清理过期条目并返回当前窗口内的请求数
    fn count(&mut self, now: Instant) -> u64 {
        let cutoff = now - std::time::Duration::from_secs(WINDOW_SECS);
        // 二分查找第一个 >= cutoff 的位置
        let pos = self.timestamps.partition_point(|t| *t < cutoff);
        if pos > 0 {
            self.timestamps.drain(..pos);
        }
        self.timestamps.len() as u64
    }
}

/// RPM 追踪器
///
/// 线程安全，使用单个 Mutex 保护所有状态。
/// 内存开销极小：每个请求仅存储一个 Instant（8 字节），60 秒后自动清理。
pub struct RpmTracker {
    inner: Mutex<RpmTrackerInner>,
}

struct RpmTrackerInner {
    /// 全局请求队列
    global: TimestampQueue,
    /// 按凭据 ID 分组
    by_credential: HashMap<u64, TimestampQueue>,
    /// 按 API Key ID 分组
    by_api_key: HashMap<u32, TimestampQueue>,
}

/// RPM 快照（用于 API 响应）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpmSnapshot {
    /// 全局 RPM
    pub global: u64,
    /// 按凭据 ID 分组的 RPM
    pub by_credential: HashMap<u64, u64>,
    /// 按 API Key ID 分组的 RPM
    pub by_api_key: HashMap<u32, u64>,
}

impl RpmTracker {
    /// 创建新的 RPM 追踪器
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RpmTrackerInner {
                global: TimestampQueue::new(),
                by_credential: HashMap::new(),
                by_api_key: HashMap::new(),
            }),
        }
    }

    /// 记录一次请求（在 handler 入口调用）
    ///
    /// 记录全局 RPM 和 per-API-Key RPM
    pub fn record_request(&self, api_key_id: Option<u32>) {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        inner.global.record(now);
        if let Some(key_id) = api_key_id {
            inner
                .by_api_key
                .entry(key_id)
                .or_insert_with(TimestampQueue::new)
                .record(now);
        }
    }

    /// 记录凭据维度的请求（在 provider 成功调用后调用）
    pub fn record_credential(&self, credential_id: u64) {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        inner
            .by_credential
            .entry(credential_id)
            .or_insert_with(TimestampQueue::new)
            .record(now);
    }

    /// 获取当前 RPM 快照
    pub fn snapshot(&self) -> RpmSnapshot {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();

        let global = inner.global.count(now);

        let by_credential: HashMap<u64, u64> = inner
            .by_credential
            .iter_mut()
            .map(|(&id, queue)| (id, queue.count(now)))
            .filter(|(_, count)| *count > 0)
            .collect();

        let by_api_key: HashMap<u32, u64> = inner
            .by_api_key
            .iter_mut()
            .map(|(&id, queue)| (id, queue.count(now)))
            .filter(|(_, count)| *count > 0)
            .collect();

        RpmSnapshot {
            global,
            by_credential,
            by_api_key,
        }
    }
}
