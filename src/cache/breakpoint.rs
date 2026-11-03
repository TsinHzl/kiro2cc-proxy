// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 按 Anthropic prompt caching 规则模拟缓存记账（cache_control 断点）
//!
//! Kiro meteringEvent 只返回 credits，不返回 cache_read / cache_creation。本模块在本地
//! 复刻 Anthropic 的缓存语义，为每个请求算出贴近官方口径的
//! `input / cache_creation(5m/1h) / cache_read` 拆分：
//!
//! 1. 请求按 tools → system → messages 的顺序展开为有序 block 序列；
//!    每个 block 的键 = SHA-256(tenant ‖ model ‖ block[0] ‖ … ‖ block[i])（剔除 cache_control 后规范化）；
//!    tenant 为 API Key 标识，model 为映射后的 Kiro 模型 ID
//! 2. 只有带 `cache_control` 的 block（断点）才会写入缓存条目；
//!    顶层 `cache_control`（自动缓存）等价于在最后一个 block 上打断点
//! 3. 读取：从每个断点往前回看最多 20 个 block，取最长的未过期命中，命中即刷新 TTL
//! 4. 断点处累计 token 低于最小可缓存长度时不建条目（官方按模型取值，见 `min_cacheable_tokens`）
//! 5. 拆分：read = 命中位置累计 token；creation = 命中位置 → 最后一个有效断点；
//!    input = 最后一个有效断点之后
//!
//! 请求不带任何 cache_control 时与官方一致：全部计为普通输入。
//!
//! 缓存按 API Key 隔离（对应官方的 workspace 级隔离），避免一个 Key 通过 cache_read
//! 探测其他 Key 是否发送过同一前缀。条目只存哈希、档位与过期时间（墙钟毫秒），
//! 周期性原子写入 `prompt_cache.json`，重启后读回，缓存状态可跨进程延续。
//!
//! 写入采用两阶段：`plan` 只读（命中刷新 TTL 除外），返回待写条目；上游成功产出
//! 响应后才 `commit`，失败请求不产生缓存写入。
//!
//! 注意：这是本地模拟，让下游账单的拆分口径贴近官方，**不反映 Kiro 侧真实缓存命中与
//! 真实成本**（真实成本以 meteringEvent 的 credits 为准）。

use crate::cache::PromptCacheUsage;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 每个断点往前回看的 block 数（Anthropic 官方值）
const LOOKBACK_BLOCKS: usize = 20;
/// 后台过期清理 + 快照落盘周期
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(30);
/// 缓存表条目上限（防御异常流量撑爆内存；超出时淘汰最早过期的条目）
const MAX_ENTRIES: usize = 200_000;

/// 缓存 TTL 档位
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    #[serde(rename = "5m")]
    FiveM,
    #[serde(rename = "1h")]
    OneH,
}

impl Tier {
    fn ttl_ms(self) -> u64 {
        match self {
            Tier::FiveM => 5 * 60 * 1000,
            Tier::OneH => 60 * 60 * 1000,
        }
    }

    /// 解析 `cache_control` 对象：`{"type":"ephemeral","ttl":"1h"}` → OneH，其余 ephemeral → FiveM
    fn from_cache_control(cc: &serde_json::Value) -> Option<Self> {
        if cc.get("type").and_then(|v| v.as_str()) != Some("ephemeral") {
            return None;
        }
        match cc.get("ttl").and_then(|v| v.as_str()) {
            Some("1h") => Some(Tier::OneH),
            _ => Some(Tier::FiveM),
        }
    }
}

/// 展开后的单个 block
#[derive(Debug, Clone)]
pub struct Block {
    /// 累积哈希：tenant ‖ model ‖ block[0..=i]
    pub hash: [u8; 32],
    /// 截至本 block（含）的累计 token
    pub cum_tokens: i32,
    /// 带 cache_control 时为 Some(档位)
    pub breakpoint: Option<Tier>,
}

/// 一个请求的缓存画像（在消费请求体前构建）
#[derive(Debug, Clone, Default)]
pub struct CacheProfile {
    pub blocks: Vec<Block>,
    /// 该模型的官方最小可缓存长度（未配置全局覆盖时生效）
    pub min_tokens: i32,
}

impl CacheProfile {
    pub fn total_tokens(&self) -> i32 {
        self.blocks.last().map_or(0, |b| b.cum_tokens)
    }
}
/// 跳过 block 顶层 `cache_control` 做规范化 JSON：断点每轮会移动，标记本身不能进入哈希。
///
/// 只剔除顶层键（与 `breakpoint_of` 的判定层级一致）：嵌套位置的同名键（如工具
/// `input_schema` 里恰好叫 `cache_control` 的属性）属于内容本身，必须参与哈希。
/// 直接跳过而非 clone 后删除，避免每个 block 一次深拷贝。
fn canonical_block(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().filter(|k| *k != "cache_control").collect();
            keys.sort_unstable();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        super::fingerprint::json_key(k),
                        super::fingerprint::canonical_json(&m[k])
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        other => super::fingerprint::canonical_json(other),
    }
}

/// 工具是否为延迟加载（`defer_loading: true`）
fn is_deferred_tool(tool: &serde_json::Value) -> bool {
    tool.get("defer_loading").and_then(|v| v.as_bool()) == Some(true)
}

/// 递归收集内容里 `tool_reference` 块引用的工具名（含嵌套在 `tool_result.content` 中的）
fn collect_tool_references<'a>(v: &'a serde_json::Value, out: &mut Vec<&'a str>) {
    match v {
        serde_json::Value::Object(m) => {
            if m.get("type").and_then(|t| t.as_str()) == Some("tool_reference")
                && let Some(name) = m.get("tool_name").and_then(|n| n.as_str())
            {
                out.push(name);
            }
            m.values().for_each(|c| collect_tool_references(c, out));
        }
        serde_json::Value::Array(a) => a.iter().for_each(|c| collect_tool_references(c, out)),
        _ => {}
    }
}

/// 请求是否带有 `cache_control`：只看顶层与 tools / system / messages 的 block 层，
/// 与 `build_profile` 识别断点的位置一致（不扫描 `input_schema` 等内容内部）
pub fn has_cache_control(body: &serde_json::Value) -> bool {
    let marked = |v: &serde_json::Value| v.get("cache_control").is_some();
    let array = |k: &str| body.get(k).and_then(|v| v.as_array());
    marked(body)
        || array("tools").is_some_and(|tools| tools.iter().any(marked))
        || array("system").is_some_and(|blocks| blocks.iter().any(marked))
        || array("messages").is_some_and(|msgs| {
            msgs.iter().any(|m| {
                m.get("content")
                    .and_then(|c| c.as_array())
                    .is_some_and(|blocks| blocks.iter().any(marked))
            })
        })
}

/// Anthropic 官方各模型的最小可缓存长度（入参为 `map_model` 映射后的 Kiro 模型 ID）。
///
/// 数据来源：platform.claude.com prompt caching 文档 "Cache limitations"。
/// 未列出的模型（含非 Claude 模型）按 1024 处理。
pub fn min_cacheable_tokens(kiro_model: &str) -> i32 {
    match kiro_model {
        "claude-opus-5.5" | "claude-opus-5" | "claude-sonnet-5.5" | "claude-fable-5"
        | "claude-fable-5.1" | "claude-haiku-5.5" => 512,
        "claude-opus-4.7" => 2048,
        "claude-opus-4.6" | "claude-opus-4.5" | "claude-haiku-4.5" => 4096,
        _ => 1024,
    }
}

/// OpenAI 协议（/v1/chat/completions、/v1/responses）没有 cache_control，官方语义是
/// 自动前缀缓存。翻译成 Anthropic 请求后补一个顶层自动缓存标记，使断点记账对这类
/// 客户端（Codex 等）按"自动缓存最长前缀"计算；客户端已自带标记时不覆盖。
/// OpenAI 侧 usage 只把 cache_read 报为 `cached_tokens`，写入计入 prompt_tokens，
/// 与 OpenAI 官方"无单独写入费用"的口径一致。
pub fn ensure_auto_cache_control(body: &mut serde_json::Value) {
    if has_cache_control(body) {
        return;
    }
    if let Some(obj) = body.as_object_mut() {
        obj.insert(
            "cache_control".to_string(),
            serde_json::json!({"type": "ephemeral"}),
        );
    }
}

fn breakpoint_of(v: &serde_json::Value) -> Option<Tier> {
    v.get("cache_control").and_then(Tier::from_cache_control)
}

fn count_tool_tokens(tool: &serde_json::Value) -> u64 {
    let s = |k: &str| tool.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let schema = tool
        .get("input_schema")
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .unwrap_or_default();
    crate::token::count_tokens(s("name"))
        + crate::token::count_tokens(s("description"))
        + crate::token::count_tokens(&schema)
}

/// 从原始请求 JSON 构建缓存画像。
///
/// 直接解析原始 body 而不是 `MessagesRequest`：类型化结构的 `SystemMessage` / `Tool`
/// 在反序列化时会丢掉 `cache_control`。token 计数与 `token::count_all_tokens` 同口径
/// （同一 `count_tokens` / `count_content_block`），保证比例换算不失真。
///
/// - `tenant`：缓存隔离域（API Key 标识），并入哈希起点
/// - `kiro_model`：映射后的 Kiro 模型 ID，同一模型的不同写法共享缓存
pub fn build_profile(body: &serde_json::Value, tenant: &str, kiro_model: &str) -> CacheProfile {
    // (规范化内容, token, 断点)
    let mut segs: Vec<(String, u64, Option<Tier>)> = Vec::new();

    if let Some(tools) = body.get("tools").and_then(|v| v.as_array()) {
        // Tool Search 契约（与 fingerprint 路径一致，见 types.rs Tool.defer_loading）：
        // defer_loading=true 的工具不进入顶层前缀，否则中途增减延迟工具会使 system/历史全部失效
        for tool in tools.iter().filter(|t| !is_deferred_tool(t)) {
            segs.push((
                format!("T:{}", canonical_block(tool)),
                count_tool_tokens(tool),
                breakpoint_of(tool),
            ));
        }
    }

    match body.get("system") {
        Some(serde_json::Value::String(s)) => {
            segs.push((format!("S:{s}"), crate::token::count_tokens(s), None));
        }
        Some(serde_json::Value::Array(blocks)) => {
            for b in blocks {
                let text = b.get("text").and_then(|v| v.as_str()).unwrap_or("");
                segs.push((
                    format!("S:{}", canonical_block(b)),
                    crate::token::count_tokens(text),
                    breakpoint_of(b),
                ));
            }
        }
        _ => {}
    }

    // 延迟工具定义：不进顶层前缀，改在引用它的 tool_reference 块处展开（见下），
    // 与 fingerprint 路径一致——否则只改被引用延迟工具的 schema 时缓存键不变，会误命中
    let deferred_tools: std::collections::HashMap<&str, &serde_json::Value> = body
        .get("tools")
        .and_then(|v| v.as_array())
        .map(|tools| {
            tools
                .iter()
                .filter(|t| is_deferred_tool(t))
                .filter_map(|t| Some((t.get("name")?.as_str()?, t)))
                .collect()
        })
        .unwrap_or_default();

    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        for msg in messages {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
            match msg.get("content") {
                Some(serde_json::Value::String(s)) => {
                    segs.push((format!("M:{role}:{s}"), crate::token::count_tokens(s), None));
                }
                Some(serde_json::Value::Array(blocks)) => {
                    for (i, b) in blocks.iter().enumerate() {
                        // 块序号进键：同一消息内拆分方式不同时不应误命中
                        let mut key = format!("M:{role}:{i}:{}", canonical_block(b));
                        let mut tokens = crate::token::count_content_block(b);
                        if !deferred_tools.is_empty() {
                            let mut refs = Vec::new();
                            collect_tool_references(b, &mut refs);
                            for name in refs {
                                if let Some(tool) = deferred_tools.get(name) {
                                    key.push_str(&format!("|D:{}", canonical_block(tool)));
                                    tokens = tokens.saturating_add(count_tool_tokens(tool));
                                }
                            }
                        }
                        segs.push((key, tokens, breakpoint_of(b)));
                    }
                }
                _ => {}
            }
        }
    }

    // 顶层 cache_control（自动缓存）：断点落在最后一个 block 上（已有显式断点则不覆盖）
    if let Some(tier) = body.get("cache_control").and_then(Tier::from_cache_control)
        && let Some(last) = segs.last_mut()
        && last.2.is_none()
    {
        last.2 = Some(tier);
    }

    let mut hasher = Sha256::new();
    for part in [tenant, kiro_model] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    let mut cum: u64 = 0;
    let blocks = segs
        .into_iter()
        .map(|(content, tokens, breakpoint)| {
            // 段间分隔符防止 "ab"+"c" 与 "a"+"bc" 拼接碰撞
            hasher.update((content.len() as u64).to_le_bytes());
            hasher.update(content.as_bytes());
            cum = cum.saturating_add(tokens);
            Block {
                hash: hasher.clone().finalize().into(),
                cum_tokens: cum.min(i32::MAX as u64) as i32,
                breakpoint,
            }
        })
        .collect();

    CacheProfile {
        blocks,
        min_tokens: min_cacheable_tokens(kiro_model),
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Entry {
    tier: Tier,
    /// 过期时间（UNIX 毫秒，墙钟——可跨进程持久化）
    expires_at_ms: u64,
}

/// 待提交的缓存写入（上游成功后 commit）
#[derive(Debug, Clone, Default)]
pub struct PendingWrites {
    entries: Vec<([u8; 32], Tier)>,
}

/// 单个请求的缓存记账结果
#[derive(Debug, Clone, Default)]
pub struct CachePlan {
    /// 画像口径下的 token 拆分（本地 block 累计值）
    pub read: i32,
    pub creation_5m: i32,
    pub creation_1h: i32,
    pub total: i32,
    pub pending: PendingWrites,
}

impl CachePlan {
    /// 把画像口径的拆分按比例映射到请求最终的 input 总量（与 count_all_tokens 同口径，
    /// 两者通常一致；映射只为消除舍入差并保证 read + creation + input == total）
    pub fn usage_for(&self, final_input_tokens: i32) -> PromptCacheUsage {
        let creation = self.creation_5m + self.creation_1h;
        let raw = PromptCacheUsage {
            input_tokens: self
                .total
                .saturating_sub(self.read)
                .saturating_sub(creation)
                .max(0),
            cache_creation_input_tokens: creation,
            cache_read_input_tokens: self.read,
            cache_creation_5m_input_tokens: self.creation_5m,
            cache_creation_1h_input_tokens: self.creation_1h,
        };
        raw.scale_to(final_input_tokens)
    }
}

/// 随请求传递的记账句柄：持有计划结果与缓存表，响应成功落库时 `commit`
#[derive(Debug, Clone)]
pub struct CacheAccounting {
    cache: Arc<BreakpointCache>,
    plan: CachePlan,
}

impl CacheAccounting {
    /// 从原始请求体构建画像并立即 plan（须在转发上游前调用）。
    ///
    /// 解析 JSON、规范化与 BPE 计数对大上下文是可观的 CPU 开销，放到 blocking 线程池，
    /// 不占用异步 worker。`tenant` 为缓存隔离域（API Key 标识），`kiro_model` 为映射后的模型 ID。
    pub async fn from_request(
        cache: &Arc<BreakpointCache>,
        body: bytes::Bytes,
        tenant: String,
        kiro_model: String,
    ) -> Option<Self> {
        let cache = Arc::clone(cache);
        match tokio::task::spawn_blocking(move || {
            Self::from_request_blocking(cache, &body, &tenant, &kiro_model)
        })
        .await
        {
            Ok(accounting) => accounting,
            Err(e) => {
                // 记账失败不影响请求（回落到前缀估算），但要留线索
                tracing::warn!("[prompt-cache] 记账画像任务失败: {}", e);
                None
            }
        }
    }

    fn from_request_blocking(
        cache: Arc<BreakpointCache>,
        body: &[u8],
        tenant: &str,
        kiro_model: &str,
    ) -> Option<Self> {
        let raw: serde_json::Value = serde_json::from_slice(body).ok()?;
        let plan = cache.plan(&build_profile(&raw, tenant, kiro_model));
        tracing::info!(
            "[prompt-cache] plan: tenant={} model={} total={} read={} creation_5m={} creation_1h={} pending_writes={}",
            tenant,
            kiro_model,
            plan.total,
            plan.read,
            plan.creation_5m,
            plan.creation_1h,
            plan.pending.entries.len()
        );
        Some(Self { cache, plan })
    }

    pub fn usage_for(&self, final_input_tokens: i32) -> PromptCacheUsage {
        self.plan.usage_for(final_input_tokens)
    }

    /// 上游成功产出响应后提交缓存写入（幂等：重复调用只是刷新同一批条目）
    pub fn commit(&self) {
        self.cache.commit(&self.plan.pending);
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// 全局断点缓存表
#[derive(Debug)]
pub struct BreakpointCache {
    table: Mutex<HashMap<[u8; 32], Entry>>,
    /// 最小可缓存长度的全局覆盖；None 时按模型取官方值（`CacheProfile::min_tokens`）
    min_tokens_override: Option<i32>,
    snapshot_path: Option<PathBuf>,
    dirty: AtomicBool,
}

impl BreakpointCache {
    /// 创建并从快照恢复；`snapshot_path` 为 None 时纯内存（测试用）。
    /// `min_tokens_override <= 0` 表示按模型使用官方最小可缓存长度。
    pub fn new(min_tokens_override: i32, snapshot_path: Option<PathBuf>) -> Self {
        let table = snapshot_path
            .as_ref()
            .map(|p| load_snapshot(p))
            .unwrap_or_default();
        Self {
            table: Mutex::new(table),
            min_tokens_override: (min_tokens_override > 0).then_some(min_tokens_override),
            snapshot_path,
            dirty: AtomicBool::new(false),
        }
    }

    pub fn len(&self) -> usize {
        self.table.lock().len()
    }

    /// 计算本请求的缓存拆分。命中的条目立即刷新 TTL；新写入放进 `pending`，待 commit。
    pub fn plan(&self, profile: &CacheProfile) -> CachePlan {
        self.plan_at(profile, now_ms())
    }

    fn plan_at(&self, profile: &CacheProfile, now: u64) -> CachePlan {
        let blocks = &profile.blocks;
        let total = profile.total_tokens();
        let bps: Vec<usize> = (0..blocks.len())
            .filter(|&i| blocks[i].breakpoint.is_some())
            .collect();
        if bps.is_empty() {
            return CachePlan {
                total,
                ..Default::default()
            };
        }

        let mut table = self.table.lock();

        // 1. 读：断点从后往前，各自回看 LOOKBACK_BLOCKS 个 block，取第一个（即最长）命中
        let mut hit: Option<usize> = None;
        'outer: for &bp in bps.iter().rev() {
            let lo = bp.saturating_sub(LOOKBACK_BLOCKS);
            for i in (lo..=bp).rev() {
                if let Some(e) = table.get_mut(&blocks[i].hash)
                    && e.expires_at_ms > now
                {
                    e.expires_at_ms = now + e.tier.ttl_ms();
                    hit = Some(i);
                    break 'outer;
                }
            }
        }
        if hit.is_some() {
            self.dirty.store(true, Ordering::Relaxed);
        }
        drop(table);

        let read = hit.map_or(0, |i| blocks[i].cum_tokens);

        // 2. 写：命中位置之后、达到最小长度的断点逐段计入 creation
        let mut plan = CachePlan {
            read,
            total,
            ..Default::default()
        };
        let mut prev = read;
        let min_tokens = self.min_tokens_override.unwrap_or(profile.min_tokens);
        for &bp in bps.iter().filter(|&&bp| hit.is_none_or(|h| bp > h)) {
            let b = &blocks[bp];
            if b.cum_tokens < min_tokens || b.cum_tokens <= prev {
                continue;
            }
            let seg = b.cum_tokens - prev;
            let tier = b.breakpoint.unwrap_or(Tier::FiveM);
            match tier {
                Tier::FiveM => plan.creation_5m += seg,
                Tier::OneH => plan.creation_1h += seg,
            }
            plan.pending.entries.push((b.hash, tier));
            prev = b.cum_tokens;
        }
        plan
    }

    /// 上游成功产出响应后提交写入
    pub fn commit(&self, pending: &PendingWrites) {
        self.commit_at(pending, now_ms());
    }

    fn commit_at(&self, pending: &PendingWrites, now: u64) {
        if pending.entries.is_empty() {
            return;
        }
        let mut table = self.table.lock();
        for (hash, tier) in &pending.entries {
            table.insert(
                *hash,
                Entry {
                    tier: *tier,
                    expires_at_ms: now + tier.ttl_ms(),
                },
            );
        }
        if table.len() > MAX_ENTRIES {
            // 淘汰到上限的 90%：留出滞回，避免表满后每次 commit 都对全表排序（且持锁）
            evict_oldest(&mut table, MAX_ENTRIES / 10 * 9);
        }
        self.dirty.store(true, Ordering::Relaxed);
    }
}
impl BreakpointCache {
    /// 清理过期条目
    pub fn evict_expired(&self) {
        let now = now_ms();
        let mut table = self.table.lock();
        let before = table.len();
        table.retain(|_, e| e.expires_at_ms > now);
        if table.len() != before {
            self.dirty.store(true, Ordering::Relaxed);
        }
    }

    /// 有变更时把缓存表原子写入快照文件
    pub fn save_snapshot(&self) {
        let Some(path) = &self.snapshot_path else {
            return;
        };
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return;
        }
        // 持锁只做 Copy 级拷贝；hex 编码与序列化在锁外，避免大表时阻塞 plan/commit
        let raw: Vec<([u8; 32], Entry)> = {
            let table = self.table.lock();
            table.iter().map(|(k, v)| (*k, *v)).collect()
        };
        let snapshot: HashMap<String, Entry> =
            raw.into_iter().map(|(k, v)| (hex::encode(k), v)).collect();
        let bytes = match serde_json::to_vec(&snapshot) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("[prompt-cache] 快照序列化失败: {}", e);
                return;
            }
        };
        if let Err(e) = crate::common::fs::atomic_write(path, &bytes) {
            // 写失败下次重试
            self.dirty.store(true, Ordering::Relaxed);
            tracing::warn!("[prompt-cache] 快照写入失败 {}: {}", path.display(), e);
        }
    }

    /// 启动后台维护任务：周期清理过期条目并落盘快照
    pub fn start_maintenance(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(MAINTENANCE_INTERVAL);
            tick.tick().await;
            loop {
                tick.tick().await;
                let this = Arc::clone(&this);
                // 快照写盘是同步 IO，放到 blocking 线程池
                if let Err(e) = tokio::task::spawn_blocking(move || {
                    this.evict_expired();
                    this.save_snapshot();
                })
                .await
                {
                    tracing::warn!("[prompt-cache] 后台维护任务异常: {}", e);
                }
            }
        });
    }
}

fn load_snapshot(path: &std::path::Path) -> HashMap<[u8; 32], Entry> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        // 首次运行没有快照文件，属正常情况
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return HashMap::new(),
        Err(e) => {
            tracing::warn!("[prompt-cache] 快照读取失败 {}: {}", path.display(), e);
            return HashMap::new();
        }
    };
    let raw: HashMap<String, Entry> = match serde_json::from_slice(&bytes) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                "[prompt-cache] 快照解析失败，忽略 {}: {}",
                path.display(),
                e
            );
            return HashMap::new();
        }
    };
    let now = now_ms();
    raw.into_iter()
        .filter(|(_, e)| e.expires_at_ms > now)
        .filter_map(|(k, e)| {
            let bytes: [u8; 32] = hex::decode(k).ok()?.try_into().ok()?;
            Some((bytes, e))
        })
        .collect()
}

fn evict_oldest(table: &mut HashMap<[u8; 32], Entry>, keep: usize) {
    let mut by_expiry: Vec<([u8; 32], u64)> =
        table.iter().map(|(k, e)| (*k, e.expires_at_ms)).collect();
    by_expiry.sort_unstable_by_key(|(_, t)| *t);
    let drop_n = table.len().saturating_sub(keep);
    for (k, _) in by_expiry.into_iter().take(drop_n) {
        table.remove(&k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const T0: u64 = 1_000_000;

    fn long(word: &str, n: usize) -> String {
        std::iter::repeat_n(word, n).collect::<Vec<_>>().join(" ")
    }

    /// Claude Code 形态：system 末块断点 + 最后一条消息末块断点
    fn cc_body(turns: &[(&str, &str)], last_user: &str) -> serde_json::Value {
        let mut messages = Vec::new();
        for (u, a) in turns {
            messages.push(json!({"role":"user","content":[{"type":"text","text":u}]}));
            messages.push(json!({"role":"assistant","content":[{"type":"text","text":a}]}));
        }
        messages.push(json!({"role":"user","content":[
            {"type":"text","text":last_user,"cache_control":{"type":"ephemeral"}}
        ]}));
        json!({
            "model": "claude-opus-5-5",
            "system": [{"type":"text","text":long("system prompt", 800),"cache_control":{"type":"ephemeral"}}],
            "messages": messages,
        })
    }

    fn run(cache: &BreakpointCache, body: &serde_json::Value, now: u64) -> CachePlan {
        let profile = build_profile(body, "t", "claude-opus-5-5");
        let plan = cache.plan_at(&profile, now);
        cache.commit_at(&plan.pending, now);
        plan
    }

    #[test]
    fn first_request_writes_then_next_turn_reads() {
        let cache = BreakpointCache::new(1024, None);
        let b1 = cc_body(&[], "hello");
        let p1 = run(&cache, &b1, T0);
        assert_eq!(p1.read, 0);
        assert_eq!(p1.creation_5m, p1.total, "首轮全部写入");

        let b2 = cc_body(&[("hello", "hi there")], "next question");
        let p2 = run(&cache, &b2, T0 + 1000);
        assert_eq!(p2.read, p1.total, "上一轮整个前缀命中");
        assert_eq!(p2.read + p2.creation_5m, p2.total, "增量部分写入");
        assert!(p2.creation_5m > 0);
    }

    #[test]
    fn no_cache_control_is_plain_input() {
        let cache = BreakpointCache::new(1, None);
        let body = json!({"model":"m","system":"sys","messages":[{"role":"user","content":"hi"}]});
        let p = run(&cache, &body, T0);
        assert_eq!((p.read, p.creation_5m, p.creation_1h), (0, 0, 0));
        assert!(p.total > 0);
        let u = p.usage_for(p.total);
        assert_eq!(u.input_tokens, p.total);
    }

    #[test]
    fn below_min_tokens_not_cached() {
        let cache = BreakpointCache::new(1024, None);
        let body = json!({"model":"m","messages":[{"role":"user","content":[
            {"type":"text","text":"short","cache_control":{"type":"ephemeral"}}]}]});
        let p = run(&cache, &body, T0);
        assert_eq!(p.creation_5m, 0);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn expired_entry_misses() {
        let cache = BreakpointCache::new(1024, None);
        let b1 = cc_body(&[], "hello");
        run(&cache, &b1, T0);
        let b2 = cc_body(&[("hello", "hi")], "again");
        let p = run(&cache, &b2, T0 + 6 * 60 * 1000);
        // system 断点条目同样过期 → 全部重新写入
        assert_eq!(p.read, 0);
    }

    #[test]
    fn hit_refreshes_ttl() {
        let cache = BreakpointCache::new(1024, None);
        run(&cache, &cc_body(&[], "a"), T0);
        // 4 分钟后命中 system 前缀（不同的末条消息），刷新 TTL
        run(&cache, &cc_body(&[], "b"), T0 + 4 * 60 * 1000);
        // 距首次写入 8 分钟、距刷新 4 分钟：仍命中
        let p = run(&cache, &cc_body(&[], "c"), T0 + 8 * 60 * 1000);
        assert!(p.read > 0);
    }

    #[test]
    fn one_hour_tier_split() {
        let cache = BreakpointCache::new(1, None);
        let body = json!({"model":"m",
            "system":[{"type":"text","text":long("sys", 300),"cache_control":{"type":"ephemeral","ttl":"1h"}}],
            "messages":[{"role":"user","content":[{"type":"text","text":long("q", 50),"cache_control":{"type":"ephemeral"}}]}]});
        let p = run(&cache, &body, T0);
        assert!(p.creation_1h > 0 && p.creation_5m > 0);
        assert_eq!(p.creation_1h + p.creation_5m, p.total);
    }

    #[test]
    fn moving_breakpoint_does_not_change_hash() {
        let a = json!({"model":"m","messages":[{"role":"user","content":[{"type":"text","text":"x","cache_control":{"type":"ephemeral"}}]}]});
        let b = json!({"model":"m","messages":[{"role":"user","content":[{"type":"text","text":"x"}]}]});
        assert_eq!(
            build_profile(&a, "t", "m").blocks[0].hash,
            build_profile(&b, "t", "m").blocks[0].hash
        );
    }

    #[test]
    fn model_partitions_cache() {
        let body = cc_body(&[], "hello");
        assert_ne!(
            build_profile(&body, "t", "claude-opus-5-5").blocks[0].hash,
            build_profile(&body, "t", "claude-sonnet-5").blocks[0].hash
        );
    }

    #[test]
    fn tenant_partitions_cache() {
        // 评审意见：缓存按 API Key 隔离，B 不能通过 cache_read 探测 A 发过的前缀
        let cache = BreakpointCache::new(1, None);
        let body = cc_body(&[], "hello");
        let a = build_profile(&body, "key:1", "claude-opus-5.5");
        let pa = cache.plan_at(&a, T0);
        cache.commit_at(&pa.pending, T0);
        let b = build_profile(&body, "key:2", "claude-opus-5.5");
        assert_eq!(cache.plan_at(&b, T0 + 1000).read, 0, "其他 Key 不命中");
        assert!(cache.plan_at(&a, T0 + 1000).read > 0, "同一 Key 命中");
    }

    #[test]
    fn nested_cache_control_in_schema_is_content_not_breakpoint() {
        // 评审意见：input_schema 里名为 cache_control 的属性不是断点
        let schema = json!({"type":"object","properties":{"cache_control":{"type":"string"}}});
        let body = json!({"model":"m",
            "tools":[{"name":"t","description":"d","input_schema":schema}],
            "messages":[{"role":"user","content":"hi"}]});
        assert!(!has_cache_control(&body));
        let mut injected = body.clone();
        ensure_auto_cache_control(&mut injected);
        assert_eq!(injected["cache_control"]["type"], "ephemeral");

        // 嵌套同名键参与哈希：schema 不同则前缀不同
        let other = json!({"model":"m",
            "tools":[{"name":"t","description":"d","input_schema":{"type":"object","properties":{"cache_control":{"type":"number"}}}}],
            "messages":[{"role":"user","content":"hi"}]});
        assert_ne!(
            build_profile(&body, "t", "m").blocks[0].hash,
            build_profile(&other, "t", "m").blocks[0].hash
        );
    }

    #[test]
    fn object_keys_are_escaped_in_canonical_form() {
        // CR：键 "a:1,b" 与两个键 a、b 不能规范化成同一串
        let two_keys = json!({"type":"tool_use","id":"x","name":"n","input":{"a":1,"b":2}});
        let one_key = json!({"type":"tool_use","id":"x","name":"n","input":{"a:1,b":2}});
        assert_ne!(canonical_block(&two_keys), canonical_block(&one_key));
    }

    #[test]
    fn deferred_tools_do_not_enter_top_level_prefix() {
        // CR：defer_loading=true 的工具不进顶层前缀；中途新增延迟工具不应使旧前缀失效
        let cache = BreakpointCache::new(1, None);
        let tool = |name: &str, deferred: bool| {
            let mut t = json!({"name":name,"description":"d","input_schema":{"type":"object"}});
            if deferred {
                t["defer_loading"] = json!(true);
            }
            t
        };
        let mut b1 = cc_body(&[], "hello");
        b1["tools"] = json!([tool("Read", false)]);
        let p1 = run(&cache, &b1, T0);
        assert_eq!(p1.read, 0);

        // 次轮：追加一个延迟加载工具，其余前缀不变
        let mut b2 = cc_body(&[("hello", "hi there")], "next question");
        b2["tools"] = json!([tool("Read", false), tool("mcp__x__search", true)]);
        let p2 = run(&cache, &b2, T0 + 1000);
        assert!(p2.read >= p1.total, "新增延迟工具后上一轮前缀仍应命中");

        // 对照：追加的是非延迟工具则必须破坏前缀
        let mut b3 = cc_body(&[("hello", "hi there")], "next question");
        b3["tools"] = json!([tool("Read", false), tool("Write", false)]);
        let p3 = cache.plan_at(&build_profile(&b3, "t", "claude-opus-5-5"), T0 + 2000);
        assert_eq!(p3.read, 0, "非延迟工具变更应使前缀失效");
    }

    #[test]
    fn referenced_deferred_tool_definition_is_part_of_the_reference_block_key() {
        let build = |desc: &str, referenced: bool| {
            let mut ref_content = vec![json!({"type":"text","text":"loaded"})];
            if referenced {
                ref_content.push(json!({"type":"tool_reference","tool_name":"mcp__x__search"}));
            }
            let body = json!({
                "model": "claude-opus-5-5",
                "tools": [
                    {"name":"Read","description":"d","input_schema":{"type":"object"}},
                    {"name":"mcp__x__search","description":desc,"defer_loading":true,
                     "input_schema":{"type":"object"}}
                ],
                "messages": [
                    {"role":"user","content":[{"type":"text","text":"hi"}]},
                    {"role":"user","content":[
                        {"type":"tool_result","tool_use_id":"t1","content":ref_content}
                    ]}
                ],
            });
            build_profile(&body, "t", "claude-opus-5-5")
        };
        let hashes = |p: &CacheProfile| p.blocks.iter().map(|b| b.hash).collect::<Vec<_>>();

        // 被引用的延迟工具改了描述：引用块（最后一段）哈希必须变，之前的段不变
        let (a, b) = (build("v1", true), build("v2", true));
        let (ha, hb) = (hashes(&a), hashes(&b));
        assert_eq!(ha.len(), hb.len());
        let last = ha.len() - 1;
        assert_eq!(ha[..last], hb[..last]);
        assert_ne!(ha[last], hb[last], "引用处必须带上延迟工具定义");

        // 没被引用的延迟工具改动：不影响任何段（Tool Search 契约）
        assert_eq!(hashes(&build("v1", false)), hashes(&build("v2", false)));
    }

    #[test]
    fn min_cacheable_tokens_per_model() {
        assert_eq!(min_cacheable_tokens("claude-opus-5.5"), 512);
        assert_eq!(min_cacheable_tokens("claude-fable-5"), 512);
        assert_eq!(min_cacheable_tokens("claude-fable-5.1"), 512);
        assert_eq!(min_cacheable_tokens("claude-opus-4.7"), 2048);
        assert_eq!(min_cacheable_tokens("claude-opus-4.6"), 4096);
        assert_eq!(min_cacheable_tokens("claude-haiku-4.5"), 4096);
        assert_eq!(min_cacheable_tokens("claude-sonnet-4.6"), 1024);
        assert_eq!(min_cacheable_tokens("deepseek-3.2"), 1024);
    }

    #[test]
    fn per_model_min_applies_without_override() {
        // ~2400 token 前缀：Opus 5.5（512）可缓存，Haiku 4.5（4096）不可缓存
        let cache = BreakpointCache::new(0, None);
        let body = cc_body(&[], "hello");
        let opus = cache.plan_at(&build_profile(&body, "t", "claude-opus-5.5"), T0);
        assert!(opus.creation_5m > 0);
        let haiku = cache.plan_at(&build_profile(&body, "t", "claude-haiku-4.5"), T0);
        assert_eq!(haiku.creation_5m, 0);
        // 显式覆盖优先于按模型取值
        let forced = BreakpointCache::new(1, None);
        assert!(
            forced
                .plan_at(&build_profile(&body, "t", "claude-haiku-4.5"), T0)
                .creation_5m
                > 0
        );
    }

    #[test]
    fn top_level_cache_control_marks_last_block() {
        let body = json!({"model":"m","cache_control":{"type":"ephemeral"},
            "messages":[{"role":"user","content":"a"},{"role":"assistant","content":"b"},{"role":"user","content":"c"}]});
        let p = build_profile(&body, "t", "m");
        assert_eq!(p.blocks.last().unwrap().breakpoint, Some(Tier::FiveM));
        assert!(
            p.blocks[..p.blocks.len() - 1]
                .iter()
                .all(|b| b.breakpoint.is_none())
        );
    }

    #[test]
    fn auto_cache_control_injected_only_when_absent() {
        let mut plain = json!({"model":"m","messages":[{"role":"user","content":"hi"}]});
        ensure_auto_cache_control(&mut plain);
        assert_eq!(plain["cache_control"]["type"], "ephemeral");

        let mut explicit = json!({"model":"m","messages":[{"role":"user","content":[
            {"type":"text","text":"hi","cache_control":{"type":"ephemeral","ttl":"1h"}}]}]});
        ensure_auto_cache_control(&mut explicit);
        assert!(explicit.get("cache_control").is_none());
    }

    #[test]
    fn lookback_window_limits_hits() {
        let cache = BreakpointCache::new(1, None);
        // 首轮：单条消息带断点
        let mut msgs = vec![
            json!({"role":"user","content":[{"type":"text","text":long("w", 50),"cache_control":{"type":"ephemeral"}}]}),
        ];
        let p1 = run(&cache, &json!({"model":"m","messages":msgs.clone()}), T0);
        assert!(p1.creation_5m > 0);
        // 去掉旧断点，追加 25 个 block，只在末尾打断点 → 旧条目超出 20 block 回看窗口
        msgs[0] = json!({"role":"user","content":[{"type":"text","text":long("w", 50)}]});
        for i in 0..25 {
            let role = if i % 2 == 0 { "assistant" } else { "user" };
            msgs.push(json!({"role":role,"content":[{"type":"text","text":format!("t{i}")}]}));
        }
        let last = msgs.len() - 1;
        msgs[last]["content"][0]["cache_control"] = json!({"type":"ephemeral"});
        let p2 = run(&cache, &json!({"model":"m","messages":msgs}), T0 + 1000);
        assert_eq!(p2.read, 0);
    }

    #[test]
    fn plan_without_commit_does_not_write() {
        let cache = BreakpointCache::new(1, None);
        let profile = build_profile(&cc_body(&[], "hello"), "t", "m");
        let p = cache.plan_at(&profile, T0);
        assert!(!p.pending.entries.is_empty());
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn usage_for_keeps_invariants() {
        let plan = CachePlan {
            read: 800,
            creation_5m: 150,
            creation_1h: 0,
            total: 1000,
            ..Default::default()
        };
        let u = plan.usage_for(1000);
        assert_eq!(u.cache_read_input_tokens, 800);
        assert_eq!(u.cache_creation_input_tokens, 150);
        assert_eq!(u.input_tokens, 50);
        let scaled = plan.usage_for(2000);
        assert_eq!(scaled.total_input_tokens(), 2000);
        assert_eq!(scaled.cache_read_input_tokens, 1600);
    }

    #[test]
    fn snapshot_roundtrip() {
        let dir = std::env::temp_dir().join(format!("kiro-pc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prompt_cache.json");
        let _ = std::fs::remove_file(&path);
        {
            let cache = BreakpointCache::new(1024, Some(path.clone()));
            let profile = build_profile(&cc_body(&[], "hello"), "t", "claude-opus-5-5");
            let plan = cache.plan(&profile);
            cache.commit(&plan.pending);
            cache.save_snapshot();
        }
        let cache = BreakpointCache::new(1024, Some(path.clone()));
        assert!(cache.len() > 0, "重启后从快照恢复");
        let p = cache.plan(&build_profile(
            &cc_body(&[("hello", "hi")], "x"),
            "t",
            "claude-opus-5-5",
        ));
        assert!(p.read > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
