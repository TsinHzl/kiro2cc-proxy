// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! RPM（Requests Per Minute）实时监控
//!
//! 使用滑动窗口统计最近 60 秒内的请求数量，
//! 支持全局、按账号、按 API Key 三个维度。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// 滑动窗口大小（秒）
const WINDOW_SECS: usize = 60;

#[derive(Clone, Copy, Default)]
struct Bucket {
    timestamp: u64,
    count: u64,
}

/// 单个维度的请求时间戳队列（基于 Ring Buffer 的 O(1) 实现）
struct TimestampQueue {
    buckets: [Bucket; WINDOW_SECS],
}

impl TimestampQueue {
    fn new() -> Self {
        Self {
            buckets: [Bucket::default(); WINDOW_SECS],
        }
    }

    /// 记录一次请求
    fn record(&mut self, now_secs: u64) {
        let index = (now_secs as usize) % WINDOW_SECS;
        let bucket = &mut self.buckets[index];
        if bucket.timestamp == now_secs {
            bucket.count += 1;
        } else {
            bucket.timestamp = now_secs;
            bucket.count = 1;
        }
    }

    /// 清理过期条目并返回当前窗口内的请求数
    fn count(&self, now_secs: u64) -> u64 {
        self.buckets
            .iter()
            .filter(|b| now_secs.saturating_sub(b.timestamp) < WINDOW_SECS as u64)
            .map(|b| b.count)
            .sum()
    }

    /// 计算当 RPM 达到上限时，最早的一个 bucket 将在多少秒后滑出窗口
    ///
    /// 返回 None 表示当前未满或无有效 bucket
    fn time_until_earliest_expires(&self, now_secs: u64) -> Option<u64> {
        self.buckets
            .iter()
            .filter(|b| b.count > 0 && now_secs.saturating_sub(b.timestamp) < WINDOW_SECS as u64)
            .map(|b| {
                // bucket 在 (b.timestamp + WINDOW_SECS) 时滑出窗口
                (b.timestamp + WINDOW_SECS as u64).saturating_sub(now_secs)
            })
            .min()
            .map(|s| s.max(1))
    }
}

/// RPM 追踪器
///
/// 线程安全，使用单个 Mutex 保护所有状态。
/// 内存开销恒定：每个队列固定为 `[Bucket; 60]` 大小。
pub struct RpmTracker {
    inner: Mutex<RpmTrackerInner>,
}

struct RpmTrackerInner {
    /// 全局请求队列
    global: TimestampQueue,
    /// 按账号 ID 分组
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
    /// 按账号 ID 分组的 RPM
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
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut inner = self.inner.lock().unwrap();
        inner.global.record(now_secs);
        if let Some(key_id) = api_key_id {
            inner
                .by_api_key
                .entry(key_id)
                .or_insert_with(TimestampQueue::new)
                .record(now_secs);
        }
    }

    /// 记录账号维度的请求（在 provider 成功调用后调用）
    pub fn record_credential(&self, credential_id: u64) {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut inner = self.inner.lock().unwrap();
        inner
            .by_credential
            .entry(credential_id)
            .or_insert_with(TimestampQueue::new)
            .record(now_secs);
    }

    /// 计算指定账号在 RPM 满时，最快多久后会有一个 slot 释放
    ///
    /// 返回 None 表示当前 RPM 未满或无数据
    pub fn time_until_slot(&self, credential_id: u64, max_rpm: u32) -> Option<std::time::Duration> {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let inner = self.inner.lock().unwrap();
        let queue = inner.by_credential.get(&credential_id)?;
        if queue.count(now_secs) < max_rpm as u64 {
            return None;
        }
        queue
            .time_until_earliest_expires(now_secs)
            .map(std::time::Duration::from_secs)
    }

    /// 查询指定账号的当前 RPM
    pub fn credential_rpm(&self, credential_id: u64) -> u64 {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let inner = self.inner.lock().unwrap();
        inner
            .by_credential
            .get(&credential_id)
            .map(|q| q.count(now_secs))
            .unwrap_or(0)
    }

    /// 获取当前 RPM 快照
    pub fn snapshot(&self) -> RpmSnapshot {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut inner = self.inner.lock().unwrap();

        let global = inner.global.count(now_secs);

        // 获取并顺便清理废弃为空的键，防止长时间未用的 map 残留
        inner.by_credential.retain(|_, queue| queue.count(now_secs) > 0);
        let by_credential: HashMap<u64, u64> = inner
            .by_credential
            .iter()
            .map(|(&id, queue)| (id, queue.count(now_secs)))
            .collect();

        inner.by_api_key.retain(|_, queue| queue.count(now_secs) > 0);
        let by_api_key: HashMap<u32, u64> = inner
            .by_api_key
            .iter()
            .map(|(&id, queue)| (id, queue.count(now_secs)))
            .collect();

        RpmSnapshot {
            global,
            by_credential,
            by_api_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpm_ring_buffer() {
        let mut queue = TimestampQueue::new();
        // 模拟当前时间
        let now_secs = 1000;
        
        queue.record(now_secs);
        queue.record(now_secs);
        
        assert_eq!(queue.count(now_secs), 2);
        assert_eq!(queue.count(now_secs + 10), 2); // 10秒后依然在 60 秒窗口内
        
        // 测试过期的边界
        assert_eq!(queue.count(now_secs + 60), 0); // 正好 60 秒（已过期）
        assert_eq!(queue.count(now_secs + 61), 0);
        
        // 模拟一分钟后同一槽位被复用
        queue.record(now_secs + 60);
        assert_eq!(queue.count(now_secs + 60), 1);
        
        // 模拟多个不同秒数的请求
        queue.record(now_secs + 60);
        queue.record(now_secs + 61);
        queue.record(now_secs + 62);
        
        assert_eq!(queue.count(now_secs + 62), 4);
        
        // 当 now_secs 推进到 now_secs + 123 时，所有数据均应过期（最后一次记录在 1062）
        assert_eq!(queue.count(now_secs + 123), 0);
    }
}
