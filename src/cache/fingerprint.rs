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

    #[cfg(test)]
    pub fn build_profile(
        system: Option<&[SystemMessage]>,
        messages: &[Message],
    ) -> Vec<ContentSegment> {
        Self::build_profile_with_tools(system, messages, None)
    }

    /// 生成包含 system + tools + messages 的完整指纹链
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
        // Tool Search 例外：带 defer_loading=true 的工具不进入 T: 段，
        // 其完整定义改由对应 user 消息中的 `tool_reference` 块展开后挂载到 message 段，
        // 让 deferred 工具随对话历史一起进入 cache 前缀。
        if let Some(ts) = tools
            && !ts.is_empty()
        {
            let tools_repr: Vec<String> = ts
                .iter()
                .filter(|t| !t.defer_loading.unwrap_or(false))
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
            if !tools_repr.is_empty() {
                segments.push(format!("T:{}", tools_repr.join("\u{1F}")));
            }
        }

        for msg in messages {
            let content_repr = canonicalize_message_content(&msg.content, tools);
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

fn canonicalize_message_content(content: &serde_json::Value, tools: Option<&[Tool]>) -> String {
    match content {
        serde_json::Value::String(s) => s.trim().to_string(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(|b| canonicalize_content_block(b, tools))
            .collect::<Vec<_>>()
            .join("\u{1F}"),
        _ => content.to_string(),
    }
}

fn canonicalize_content_block(block: &serde_json::Value, tools: Option<&[Tool]>) -> String {
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
                canonicalize_message_content(&inner, tools)
            )
        }
        // Tool Search 协议：tool_reference 块代表"此处加载了某个 deferred 工具"。
        // 我们在哈希里把它展开为该工具的完整定义（name+description+schema），
        // 让 deferred 工具的 schema 随当前 message 段一起进入 cache 前缀。
        // 后续轮次只要相同 tool_reference 块仍在同位置，命中即可继承该工具的 cache。
        "tool_reference" => {
            let name = block
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Some(ts) = tools
                && let Some(t) = ts.iter().find(|t| t.name == name)
            {
                let schema_val =
                    serde_json::to_value(&t.input_schema).unwrap_or(serde_json::Value::Null);
                format!(
                    "tool_reference_expanded:{}:{}:{}",
                    t.name,
                    t.description,
                    canonical_json(&schema_val)
                )
            } else {
                format!("tool_reference:{}", name)
            }
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

    /// 测试辅助：构造带可选 defer_loading 的 Anthropic Tool
    fn tool(name: &str, description: &str, defer_loading: Option<bool>) -> Tool {
        use std::collections::HashMap;
        let mut schema = HashMap::new();
        schema.insert(
            "type".to_string(),
            serde_json::Value::String("object".to_string()),
        );
        Tool {
            tool_type: None,
            name: name.to_string(),
            description: description.to_string(),
            input_schema: schema,
            max_uses: None,
            defer_loading,
        }
    }

    /// Tool Search 契约：tools 数组追加 defer_loading=true 工具,
    /// 且对应 user 消息含 tool_reference 锚点 → 前缀指纹保持稳定,
    /// 且 deferred 工具的 schema 通过 message 段被锁入 cache 前缀。
    #[test]
    fn test_deferred_tool_via_reference_enters_cache() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("sys")];

        // Round 1: 只有常驻工具
        let tools_r1 = vec![tool("Bash", "Run shell", None)];
        let msgs_r1 = vec![umsg("user", "想找点事做")];

        // Round 2: Tool Search 加载了 AskUserQuestion,
        //   tools[] 多了 defer_loading=true 工具,
        //   末尾 user 消息追加 tool_reference 锚点。
        let tools_r2 = vec![
            tool("Bash", "Run shell", None),
            tool("AskUserQuestion", "Ask user a question", Some(true)),
        ];
        let msgs_r2 = vec![
            umsg("user", "想找点事做"),
            Message {
                role: "user".into(),
                content: json!([
                    { "type": "text", "text": "上一轮加载工具" },
                    { "type": "tool_reference", "tool_name": "AskUserQuestion" }
                ]),
            },
        ];

        let p1 = FingerprintTracker::build_profile_with_tools(
            Some(&sys_msgs),
            &msgs_r1,
            Some(&tools_r1),
        );
        tracker.update("acct", p1.clone());

        let p2 = FingerprintTracker::build_profile_with_tools(
            Some(&sys_msgs),
            &msgs_r2,
            Some(&tools_r2),
        );

        // sys + tools(过滤后只剩 Bash) + msg[0] 共 3 段应整段命中 r1
        assert!(p1.len() >= 3);
        assert!(p2.len() >= 4);
        for i in 0..p1.len() {
            assert_eq!(
                p1[i].hash, p2[i].hash,
                "segment {} 应保持稳定 (deferred 工具不应进 T: 段)",
                i
            );
        }
        let u = tracker.compute("acct", &p2, 1000).unwrap();
        assert!(
            u.cache_read_input_tokens > 0,
            "deferred 工具不应破坏 cache prefix"
        );

        // Round 3: 同样 tools[] + 追加新 msg, msg[1] 的 tool_reference 锚点稳定
        //   → 通过 msg[1] 段把 AskUserQuestion 的 schema 锁入 cache
        tracker.update("acct", p2.clone());
        let msgs_r3 = {
            let mut m = msgs_r2.clone();
            m.push(umsg("assistant", "好的我帮你"));
            m
        };
        let p3 = FingerprintTracker::build_profile_with_tools(
            Some(&sys_msgs),
            &msgs_r3,
            Some(&tools_r2),
        );
        // p2 的所有段应在 p3 头部完全命中（含 tool_reference 展开后的 msg[1] 段）
        for i in 0..p2.len() {
            assert_eq!(
                p2[i].hash, p3[i].hash,
                "segment {} tool_reference 展开后应跨轮稳定",
                i
            );
        }
        let u3 = tracker.compute("acct", &p3, 2000).unwrap();
        assert!(u3.cache_read_input_tokens > u.cache_read_input_tokens);
    }

    /// 边界：defer_loading=true 但 message 里无 tool_reference 锚点
    /// → 工具既不入 T: 段也不入 msg 段，等同未声明此工具。
    #[test]
    fn test_deferred_tool_without_reference_stays_out_of_cache() {
        let sys_msgs = vec![sysm("sys")];
        let msgs = vec![umsg("user", "hello")];

        let tools_a = vec![tool("Bash", "Run shell", None)];
        let tools_b = vec![
            tool("Bash", "Run shell", None),
            // 客户端把 schema 塞进 tools 但 user message 没有 tool_reference
            tool("AskUserQuestion", "Ask user a question", Some(true)),
        ];

        let p_a = FingerprintTracker::build_profile_with_tools(
            Some(&sys_msgs),
            &msgs,
            Some(&tools_a),
        );
        let p_b = FingerprintTracker::build_profile_with_tools(
            Some(&sys_msgs),
            &msgs,
            Some(&tools_b),
        );

        assert_eq!(p_a.len(), p_b.len());
        for i in 0..p_a.len() {
            assert_eq!(
                p_a[i].hash, p_b[i].hash,
                "无 tool_reference 锚点时 deferred 工具不应影响任何段哈希",
            );
        }
    }

    /// 防回归：不带 defer_loading 标记的新工具应破坏前缀。
    #[test]
    fn test_non_deferred_tool_addition_breaks_prefix() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("sys")];
        let msgs = vec![umsg("user", "hi")];

        let tools_v1 = vec![tool("Bash", "Run shell", None)];
        let tools_v2 = vec![
            tool("Bash", "Run shell", None),
            tool("AskUserQuestion", "Ask user", None), // 注意：未带 defer_loading
        ];

        let p1 = FingerprintTracker::build_profile_with_tools(
            Some(&sys_msgs),
            &msgs,
            Some(&tools_v1),
        );
        tracker.update("acct", p1);
        let p2 = FingerprintTracker::build_profile_with_tools(
            Some(&sys_msgs),
            &msgs,
            Some(&tools_v2),
        );

        let u = tracker.compute("acct", &p2, 1000).unwrap();
        // tools 段被破坏 → 仅 system 段（极少 token）命中
        assert!(
            u.cache_read_input_tokens < 50,
            "新增非 deferred 工具应破坏前缀, got read={}",
            u.cache_read_input_tokens
        );
    }

    /// 边界：同一 user 消息里出现多个 tool_reference 块（顺序加载多个 deferred 工具）。
    /// → 每个块独立展开，整体 hash 跨轮稳定。
    #[test]
    fn test_multiple_tool_reference_blocks_stable() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("sys")];
        let tools = vec![
            tool("Bash", "Run shell", None),
            tool("AskUserQuestion", "Ask user", Some(true)),
            tool("WebSearch", "Search web", Some(true)),
        ];
        let msgs = vec![Message {
            role: "user".into(),
            content: json!([
                { "type": "text", "text": "go" },
                { "type": "tool_reference", "tool_name": "AskUserQuestion" },
                { "type": "tool_reference", "tool_name": "WebSearch" }
            ]),
        }];

        let p1 = FingerprintTracker::build_profile_with_tools(
            Some(&sys_msgs),
            &msgs,
            Some(&tools),
        );
        tracker.update("acct", p1.clone());

        let p2 = FingerprintTracker::build_profile_with_tools(
            Some(&sys_msgs),
            &msgs,
            Some(&tools),
        );
        for i in 0..p1.len() {
            assert_eq!(p1[i].hash, p2[i].hash, "multi tool_reference 应跨轮稳定");
        }
        let u = tracker.compute("acct", &p2, 800).unwrap();
        assert!(u.cache_read_input_tokens > 0);
    }

    /// 边界：tools 为 None 但 message 含 tool_reference 块
    /// → 走 fallback 分支输出 "tool_reference:<name>"，跨轮稳定。
    #[test]
    fn test_tool_reference_with_no_tools_param() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("sys")];
        let msgs = vec![Message {
            role: "user".into(),
            content: json!([
                { "type": "text", "text": "hi" },
                { "type": "tool_reference", "tool_name": "AskUserQuestion" }
            ]),
        }];

        let p1 = FingerprintTracker::build_profile_with_tools(Some(&sys_msgs), &msgs, None);
        tracker.update("acct", p1.clone());

        let p2 = FingerprintTracker::build_profile_with_tools(Some(&sys_msgs), &msgs, None);
        for i in 0..p1.len() {
            assert_eq!(p1[i].hash, p2[i].hash);
        }
        let u = tracker.compute("acct", &p2, 300).unwrap();
        assert!(u.cache_read_input_tokens > 0);
    }

    /// 边界：tool_reference 引用 tools[] 里没有的工具
    /// → 降级为 "tool_reference:<name>" 文本，跨轮位置稳定仍可命中。
    #[test]
    fn test_tool_reference_to_unknown_tool_falls_back() {
        let tracker = FingerprintTracker::new_for_test(cfg(true, 300));
        let sys_msgs = vec![sysm("sys")];
        let msgs = vec![Message {
            role: "user".into(),
            content: json!([
                { "type": "text", "text": "hi" },
                { "type": "tool_reference", "tool_name": "MissingTool" }
            ]),
        }];

        let tools = vec![tool("Bash", "Run shell", None)];

        let p1 = FingerprintTracker::build_profile_with_tools(
            Some(&sys_msgs),
            &msgs,
            Some(&tools),
        );
        tracker.update("acct", p1.clone());

        let p2 = FingerprintTracker::build_profile_with_tools(
            Some(&sys_msgs),
            &msgs,
            Some(&tools),
        );
        for i in 0..p1.len() {
            assert_eq!(p1[i].hash, p2[i].hash, "未知工具的 tool_reference 仍应稳定");
        }
        let u = tracker.compute("acct", &p2, 500).unwrap();
        assert!(u.cache_read_input_tokens > 0);
    }
}
