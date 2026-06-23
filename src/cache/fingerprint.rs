// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 账号级前缀指纹追踪
//!
//! 替代 `PromptCacheUsage::from_ratio_config` 末层兜底：用累积 SHA-256 + 消息边界
//! 在跨请求之间识别共享前缀，输出贴近真实命中的 cache_read/cache_creation。
//!
//! # 算法
//! 1. 把请求按"系统段 + 各 message 段"切成有序 segments
//! 2. 对每段做 canonicalize（文本 trim、tool_use 含 input 排序 JSON、image source.data 短 hash）
//! 3. 累积 SHA-256：hash[k] = SHA-256(seg[0] || seg[1] || ... || seg[k])
//!    — 保证前缀单调性：若 k 命中则 0..k 必命中
//! 4. 与账号历史表顺序比对，命中段刷新 last_hit_at
//! 5. cache_read = min(matched_cumulative_tokens, 0.85 × total_input)
//!
//! # 不变性
//! - cache_creation_5m + cache_creation_1h == cache_creation_input_tokens
//! - cache_read + cache_creation <= total_input

use crate::anthropic::types::{Message, SystemMessage, Tool};
use crate::cache::{PromptCacheUsage, split_creation_by_ephemeral_ratio};
use crate::model::config::CacheSimulationConfig;
use crate::token::count_tokens;
use parking_lot::{Mutex, RwLock};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EphemeralTier {
    FiveM,
    OneH,
}

#[derive(Debug, Clone)]
pub struct Breakpoint {
    pub hash: [u8; 32],
    #[allow(dead_code)] // 供未来诊断/admin UI 使用
    pub cumulative_tokens: i32,
    pub tier: EphemeralTier,
    pub last_hit_at: Instant,
}

#[derive(Debug, Default, Clone)]
pub struct FingerprintTable {
    pub breakpoints: Vec<Breakpoint>,
}

#[derive(Debug, Clone)]
pub struct ContentSegment {
    pub hash: [u8; 32],
    pub cumulative_tokens: i32,
}

#[derive(Debug)]
pub struct FingerprintTracker {
    tables: Arc<RwLock<HashMap<String, Mutex<FingerprintTable>>>>,
    config: CacheSimulationConfig,
    shutdown: Arc<AtomicBool>,
}

const CACHE_READ_CAP_RATIO: f64 = 0.85;

impl FingerprintTracker {
    pub fn new(config: CacheSimulationConfig) -> Arc<Self> {
        let tracker = Arc::new(Self {
            tables: Arc::new(RwLock::new(HashMap::new())),
            config,
            shutdown: Arc::new(AtomicBool::new(false)),
        });
        tracker.start_background_evict(Duration::from_secs(30));
        tracker
    }

    #[allow(dead_code)]
    pub fn new_for_test(config: CacheSimulationConfig) -> Arc<Self> {
        Arc::new(Self {
            tables: Arc::new(RwLock::new(HashMap::new())),
            config,
            shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    #[allow(dead_code)]
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    fn start_background_evict(self: &Arc<Self>, interval: Duration) {
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let Some(this) = weak.upgrade() else { break };
                if this.shutdown.load(Ordering::SeqCst) {
                    break;
                }
                this.evict_expired();
            }
        });
    }

    pub fn build_profile(
        system: Option<&[SystemMessage]>,
        messages: &[Message],
    ) -> Vec<ContentSegment> {
        Self::build_profile_with_tools(system, messages, None)
    }

    /// 与 `build_profile` 同，但额外把 tools 纳入指纹链
    /// （不同 tools 集应产生不同指纹，避免误报命中）
    pub fn build_profile_with_tools(
        system: Option<&[SystemMessage]>,
        messages: &[Message],
        tools: Option<&[Tool]>,
    ) -> Vec<ContentSegment> {
        let mut segments: Vec<String> = Vec::new();

        if let Some(sys) = system {
            let text: String = sys
                .iter()
                .map(|s| s.text.trim())
                .collect::<Vec<_>>()
                .join("\n");
            if !text.is_empty() {
                segments.push(format!("S:{}", text));
            }
        }

        // tools 段：以稳定序列化纳入指纹（tools 集变化 → 全部命中失效）
        if let Some(ts) = tools
            && !ts.is_empty()
        {
            let tools_repr: Vec<String> = ts
                .iter()
                .map(|t| {
                    let schema_val =
                        serde_json::to_value(&t.input_schema).unwrap_or(serde_json::Value::Null);
                    format!(
                        "{}:{}:{}",
                        t.name,
                        t.description,
                        canonical_json(&schema_val)
                    )
                })
                .collect();
            segments.push(format!("T:{}", tools_repr.join("\u{1F}")));
        }

        for msg in messages {
            let content_repr = canonicalize_message_content(&msg.content);
            segments.push(format!("M:{}:{}", msg.role, content_repr));
        }

        let mut hasher = Sha256::new();
        let mut cumulative: u64 = 0;
        let mut profile: Vec<ContentSegment> = Vec::with_capacity(segments.len());

        for seg in segments {
            hasher.update(seg.as_bytes());
            let hash_bytes: [u8; 32] = hasher.clone().finalize().into();
            cumulative = cumulative.saturating_add(count_tokens(&seg));
            profile.push(ContentSegment {
                hash: hash_bytes,
                cumulative_tokens: cumulative.min(i32::MAX as u64) as i32,
            });
        }

        profile
    }

    pub fn compute(
        &self,
        account_id: &str,
        profile: &[ContentSegment],
        total_input: i32,
    ) -> Option<PromptCacheUsage> {
        if !self.config.fingerprint_enabled || profile.is_empty() || total_input <= 0 {
            return None;
        }

        let ttl_5m = Duration::from_secs(self.config.fingerprint_ttl_5m);
        let ttl_1h = Duration::from_secs(self.config.fingerprint_ttl_1h);
        let now = Instant::now();

        let tables = self.tables.read();
        let table_mutex = tables.get(account_id);

        let matched_cumulative: i32 = if let Some(mtx) = table_mutex {
            let mut tbl = mtx.lock();
            let mut matched = 0i32;
            let limit = profile.len().min(tbl.breakpoints.len());
            for k in 0..limit {
                let bp = &mut tbl.breakpoints[k];
                let expired = match bp.tier {
                    EphemeralTier::FiveM => now.duration_since(bp.last_hit_at) > ttl_5m,
                    EphemeralTier::OneH => now.duration_since(bp.last_hit_at) > ttl_1h,
                };
                if expired || bp.hash != profile[k].hash {
                    break;
                }
                bp.last_hit_at = now;
                matched = profile[k].cumulative_tokens;
            }
            matched
        } else {
            0
        };
        drop(tables);

        let cap = ((total_input as f64) * CACHE_READ_CAP_RATIO).floor() as i32;
        let cache_read = matched_cumulative.clamp(0, cap.max(0));
        let cache_creation = total_input.saturating_sub(cache_read);

        let (creation_5m, creation_1h) =
            split_creation_by_ephemeral_ratio(cache_creation, self.config.ephemeral_1h_ratio);

        Some(
            PromptCacheUsage {
                input_tokens: 0,
                cache_creation_input_tokens: cache_creation,
                cache_read_input_tokens: cache_read,
                cache_creation_5m_input_tokens: creation_5m,
                cache_creation_1h_input_tokens: creation_1h,
            }
            .clamp_to_total(total_input),
        )
    }

    pub fn update(&self, account_id: &str, profile: Vec<ContentSegment>) {
        if !self.config.fingerprint_enabled || profile.is_empty() {
            return;
        }
        let ratio_1h = self.config.ephemeral_1h_ratio.clamp(0.0, 1.0);
        let max_bp = self.config.fingerprint_max_breakpoints_per_account.max(1);
        let now = Instant::now();

        {
            let need_create = !self.tables.read().contains_key(account_id);
            if need_create {
                let mut w = self.tables.write();
                w.entry(account_id.to_string())
                    .or_insert_with(|| Mutex::new(FingerprintTable::default()));
            }
        }

        let tables = self.tables.read();
        let Some(mtx) = tables.get(account_id) else {
            return;
        };
        let mut tbl = mtx.lock();

        let mut matched = 0usize;
        let limit = profile.len().min(tbl.breakpoints.len());
        while matched < limit && tbl.breakpoints[matched].hash == profile[matched].hash {
            tbl.breakpoints[matched].last_hit_at = now;
            matched += 1;
        }

        tbl.breakpoints.truncate(matched);
        for (i, seg) in profile.iter().enumerate().skip(matched) {
            let assign_1h = ((i as f64 + 1.0) * ratio_1h).floor() as usize
                > (i as f64 * ratio_1h).floor() as usize;
            let tier = if assign_1h {
                EphemeralTier::OneH
            } else {
                EphemeralTier::FiveM
            };
            tbl.breakpoints.push(Breakpoint {
                hash: seg.hash,
                cumulative_tokens: seg.cumulative_tokens,
                tier,
                last_hit_at: now,
            });
        }

        // LRU 淘汰：累积 SHA-256 依赖前缀单调，**只能保留前缀段**（即丢弃末尾长尾），
        // 不能按 last_hit_at 重排（会打乱累积链导致整表命中率归零）。
        if tbl.breakpoints.len() > max_bp {
            tbl.breakpoints.truncate(max_bp);
        }
    }

    pub fn evict_expired(&self) {
        if !self.config.fingerprint_enabled {
            return;
        }
        let ttl_5m = Duration::from_secs(self.config.fingerprint_ttl_5m);
        let ttl_1h = Duration::from_secs(self.config.fingerprint_ttl_1h);
        let now = Instant::now();

        let tables = self.tables.read();
        for mtx in tables.values() {
            let mut tbl = mtx.lock();
            tbl.breakpoints.retain(|b| {
                let ttl = match b.tier {
                    EphemeralTier::FiveM => ttl_5m,
                    EphemeralTier::OneH => ttl_1h,
                };
                now.duration_since(b.last_hit_at) <= ttl
            });
        }
    }

    #[allow(dead_code)]
    pub fn config(&self) -> CacheSimulationConfig {
        self.config
    }
}

fn canonicalize_message_content(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(s) => s.trim().to_string(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(canonicalize_content_block)
            .collect::<Vec<_>>()
            .join("\u{1F}"),
        _ => content.to_string(),
    }
}

fn canonicalize_content_block(block: &serde_json::Value) -> String {
    let ty = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match ty {
        "text" => block
            .get("text")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        "tool_use" => {
            let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let input = block
                .get("input")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            format!("tool_use:{}:{}", name, canonical_json(&input))
        }
        "tool_result" => {
            let id = block
                .get("tool_use_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let inner = block
                .get("content")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            format!(
                "tool_result:{}:{}",
                id,
                canonicalize_message_content(&inner)
            )
        }
        "image" | "document" => {
            let source = block.get("source");
            let media_type = source
                .and_then(|s| s.get("media_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let data = source
                .and_then(|s| s.get("data"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let mut hasher = Sha256::new();
            hasher.update(data.as_bytes());
            let h = hasher.finalize();
            format!("{}:{}:{}", ty, media_type, hex_short(&h))
        }
        _ => ty.to_string(),
    }
}

fn canonical_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(m) => {
            let sorted: BTreeMap<&String, &serde_json::Value> = m.iter().collect();
            let mut parts: Vec<String> = Vec::with_capacity(sorted.len());
            for (k, val) in sorted {
                parts.push(format!("{}:{}", k, canonical_json(val)));
            }
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        other => other.to_string(),
    }
}

fn hex_short(bytes: &[u8]) -> String {
    bytes.iter().take(8).map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::types::{Message, SystemMessage};
    use serde_json::json;

    fn cfg(enabled: bool, ttl_5m: u64) -> CacheSimulationConfig {
        CacheSimulationConfig {
            fingerprint_enabled: enabled,
            fingerprint_ttl_5m: ttl_5m,
            fingerprint_ttl_1h: 3600,
            ephemeral_1h_ratio: 0.0,
            fingerprint_max_breakpoints_per_account: 256,
        }
    }

    fn umsg(role: &str, text: &str) -> Message {
        Message {
            role: role.to_string(),
            content: json!(text),
        }
    }

    fn sysm(text: &str) -> SystemMessage {
        SystemMessage {
            text: text.to_string(),
        }
    }

    #[test]
    fn test_same_prefix_hits_on_second_request() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("You are helpful")];
        let m1 = vec![umsg("user", "Hello"), umsg("assistant", "Hi")];
        let m2 = vec![
            umsg("user", "Hello"),
            umsg("assistant", "Hi"),
            umsg("user", "How are you?"),
        ];

        let p1 = FingerprintTracker::build_profile(Some(&sys_msgs), &m1);
        tracker.update("acct-1", p1);

        let p2 = FingerprintTracker::build_profile(Some(&sys_msgs), &m2);
        let u = tracker.compute("acct-1", &p2, 1000).unwrap();
        assert!(u.cache_read_input_tokens > 0, "expected cache hit");
        assert!(u.cache_read_input_tokens <= (1000.0 * 0.85) as i32);
        assert_eq!(
            u.input_tokens + u.cache_read_input_tokens + u.cache_creation_input_tokens,
            1000
        );
    }

    #[test]
    fn test_no_prefix_match_first_request() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("Sys A")];
        let m = vec![umsg("user", "totally different content")];
        let p = FingerprintTracker::build_profile(Some(&sys_msgs), &m);
        let u = tracker.compute("acct-x", &p, 500).unwrap();
        assert_eq!(u.cache_read_input_tokens, 0);
        assert_eq!(u.cache_creation_input_tokens, 500);
    }

    #[test]
    fn test_partial_prefix_match() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("shared system")];
        let m1 = vec![umsg("user", "shared user A"), umsg("assistant", "diff A")];
        let m2 = vec![umsg("user", "shared user A"), umsg("assistant", "diff B")];
        let p1 = FingerprintTracker::build_profile(Some(&sys_msgs), &m1);
        tracker.update("acct", p1);
        let p2 = FingerprintTracker::build_profile(Some(&sys_msgs), &m2);
        let u = tracker.compute("acct", &p2, 1000).unwrap();
        assert!(u.cache_read_input_tokens > 0);
        assert!(u.cache_read_input_tokens < 1000);
    }

    #[test]
    fn test_completely_equal_requests() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("sys")];
        let m = vec![umsg("user", "hi"), umsg("assistant", "hello")];
        let p = FingerprintTracker::build_profile(Some(&sys_msgs), &m);
        let cumulative_max = p.last().map(|s| s.cumulative_tokens).unwrap_or(0);
        tracker.update("acct", p.clone());
        // 选 total_input 远大于 cumulative_max，触发"完全匹配 < 85% 封顶"分支
        let total = (cumulative_max * 10).max(100);
        let u = tracker.compute("acct", &p, total).unwrap();
        // 完全相等：cache_read 应等于 min(matched_cumulative, 0.85 × total)
        let cap = (total as f64 * 0.85).floor() as i32;
        let expected_read = cumulative_max.min(cap);
        assert_eq!(u.cache_read_input_tokens, expected_read);
        assert_eq!(
            u.cache_read_input_tokens + u.cache_creation_input_tokens,
            total
        );
    }

    #[test]
    fn test_ttl_expiry() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 0));
        let sys_msgs = vec![sysm("a")];
        let m = vec![umsg("user", "b")];
        let p = FingerprintTracker::build_profile(Some(&sys_msgs), &m);
        tracker.update("acct", p.clone());
        std::thread::sleep(Duration::from_millis(10));
        tracker.evict_expired();
        let u = tracker.compute("acct", &p, 200).unwrap();
        assert_eq!(u.cache_read_input_tokens, 0);
    }

    #[test]
    fn test_account_isolation() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("s")];
        let m = vec![umsg("user", "x")];
        let p = FingerprintTracker::build_profile(Some(&sys_msgs), &m);
        tracker.update("acct-A", p.clone());
        let u = tracker.compute("acct-B", &p, 100).unwrap();
        assert_eq!(u.cache_read_input_tokens, 0);
    }

    #[test]
    fn test_disabled_returns_none() {
        let tracker = FingerprintTracker::new_for_test(cfg(false, 300));
        let p = FingerprintTracker::build_profile(None, &[umsg("user", "x")]);
        assert!(tracker.compute("a", &p, 100).is_none());
    }

    #[test]
    fn test_tool_use_input_diff_breaks_match() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("s")];
        let m1 = vec![Message {
            role: "assistant".into(),
            content: json!([{"type":"tool_use","name":"f","input":{"a":1}}]),
        }];
        let m2 = vec![Message {
            role: "assistant".into(),
            content: json!([{"type":"tool_use","name":"f","input":{"a":2}}]),
        }];
        tracker.update(
            "acct",
            FingerprintTracker::build_profile(Some(&sys_msgs), &m1),
        );
        let p2 = FingerprintTracker::build_profile(Some(&sys_msgs), &m2);
        let u = tracker.compute("acct", &p2, 1000).unwrap();
        assert!(u.cache_read_input_tokens < 100);
    }

    #[test]
    fn test_image_source_diff_breaks_match() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let m1 = vec![Message {
            role: "user".into(),
            content: json!([{"type":"image","source":{"media_type":"image/png","data":"AAA"}}]),
        }];
        let m2 = vec![Message {
            role: "user".into(),
            content: json!([{"type":"image","source":{"media_type":"image/png","data":"BBB"}}]),
        }];
        tracker.update("a", FingerprintTracker::build_profile(None, &m1));
        let p2 = FingerprintTracker::build_profile(None, &m2);
        let u = tracker.compute("a", &p2, 500).unwrap();
        assert_eq!(u.cache_read_input_tokens, 0);
    }

    #[test]
    fn test_clamp_invariants() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("s")];
        let m = vec![umsg("user", "x")];
        let p = FingerprintTracker::build_profile(Some(&sys_msgs), &m);
        tracker.update("a", p.clone());
        let u = tracker.compute("a", &p, 50).unwrap();
        assert!(u.cache_read_input_tokens + u.cache_creation_input_tokens <= 50);
        assert!(u.input_tokens >= 0);
    }
}
