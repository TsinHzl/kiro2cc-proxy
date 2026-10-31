// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! API Key 用量追踪模块
//!
//! 记录每个 API Key 的请求用量（input/output tokens），并根据模型定价估算费用。
//! 数据持久化到 `api_key_usage.json`。

use crate::common::fs::atomic_write;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

/// 单条用量记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecord {
    /// API Key ID（0 = 主密钥）
    pub api_key_id: u32,
    /// 账号 ID（None 表示旧数据或未知）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<u64>,
    /// 模型名称
    pub model: String,
    /// 输入 tokens
    pub input_tokens: i32,
    /// 输出 tokens
    pub output_tokens: i32,
    /// 估算费用（美元）
    pub estimated_cost: f64,
    /// 真实 credits 消耗（来自 meteringEvent，None 表示旧数据）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_used: Option<f64>,
    /// 缓存命中的输入 token 数（来自 meteringEvent 或反推，None 表示旧数据）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i32>,
    /// 缓存创建的输入 token 数（来自 meteringEvent，None 表示旧数据）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i32>,
    /// 5m ephemeral tier 的 cache_creation 拆分（默认 0，向后兼容）
    #[serde(default)]
    pub cache_creation_5m_input_tokens: i32,
    /// 1h ephemeral tier 的 cache_creation 拆分（默认 0，向后兼容）
    #[serde(default)]
    pub cache_creation_1h_input_tokens: i32,
    /// 记录时间
    pub created_at: DateTime<Utc>,
    /// 客户端 IP（None 表示旧数据或未知）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
    /// 请求的 effort 级别（low/medium/high/xhigh/max；None 表示旧数据或客户端未传 output_config）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// 首 token 延迟（请求开始到首个上游业务数据事件，毫秒；None 表示旧数据或非流式请求）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_token_ms: Option<u64>,
    /// 服务端处理总耗时（请求开始到落库前，毫秒；None 表示旧数据）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// 深度思考文案 token 数（客户端实际收到的 thinking 内容；None 表示旧数据
    /// 或该请求未产生 thinking 内容）。流式与非流式均为 BPE 真值（count_tokens），
    /// 与客户端「Thought for Ns (N tokens)」显示同源同口径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_token_count: Option<i32>,
    /// 请求类型（true = 流式 SSE，false = 非流式；None 表示旧数据）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_stream: Option<bool>,
    /// 客户端 User-Agent（截断至 256 字符、最坏 1024 字节；None 表示旧数据或客户端未携带）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

/// `record()` 的可选参数集合：位置参数已达 14 个且多次追加（thinking/is_stream），
/// 收拢为具名结构体后调用点尾部不再出现裸 None/Some 字面量，新增字段也不会
/// 引发全量调用点改动（此前 thinking 字段追加时已三次被迫改动全部调用点）。
///
/// 生产调用点必须显式设置 is_stream——Default 的 None 仅表示「旧数据」语义，
/// 漏设会与新数据混淆；thinking_token_count 的 None 容忍度更高（未产生思考）。
#[derive(Debug, Clone, Default)]
pub struct RecordOptions {
    pub credential_id: Option<u64>,
    pub client_ip: Option<String>,
    pub credits_used: Option<f64>,
    pub cache_read_input_tokens: Option<i32>,
    pub cache_creation_input_tokens: Option<i32>,
    pub effort: Option<String>,
    pub first_token_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub thinking_token_count: Option<i32>,
    pub is_stream: Option<bool>,
    pub user_agent: Option<String>,
}

/// 单个 API Key 的用量汇总
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    /// API Key ID
    pub api_key_id: u32,
    /// 总请求次数
    pub total_requests: u64,
    /// 总输入 tokens
    pub total_input_tokens: i64,
    /// 总输出 tokens
    pub total_output_tokens: i64,
    /// 总估算费用（美元）
    pub total_cost: f64,
    /// 累计真实 credits 消耗（旧记录按 estimated_cost * k_ref 回退估算）
    pub total_credits: f64,
    /// 节省的 credits 总量（仅含有 credits_used 的记录）
    pub total_credits_saved: f64,
    /// 按模型分组的用量
    pub by_model: Vec<ModelUsage>,
}

/// 按模型分组的用量
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    pub model: String,
    pub requests: u64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost: f64,
    /// 累计真实 credits 消耗（旧记录按 estimated_cost * k_ref 回退估算）
    pub credits: f64,
    /// 节省的 credits 总量（仅含有 credits_used 的记录）
    pub credits_saved: f64,
}
/// 每个 API Key / 账号的最大日志条数，超出时删除最老的记录
const MAX_RECORDS_PER_KEY: usize = 10_000;
/// 测试专用再导出（tests.rs 为独立文件，需经模块路径访问私有常量）
#[cfg(test)]
pub(crate) const MAX_RECORDS_PER_KEY_FOR_TEST: usize = MAX_RECORDS_PER_KEY;

/// 生命周期累计计数的持久化结构（按 api_key_id 与 credential_id 分组）
///
/// 明细记录受 `MAX_RECORDS_PER_KEY` 裁剪，直接数记录条数会在超过 1 万次后
/// 封顶不再增长（历史缺陷：列表页请求数永远显示 10,000）。裁剪发生时把被
/// 删记录的贡献累加进此处的「裁剪前基数」，使
/// `裁剪前基数 + 现存记录求和` 恒等于真实累计值。
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LifetimeTotals {
    /// 裁剪掉的记录贡献的请求数（已从明细缓冲删除，只能从这里取）
    pruned_requests: u64,
    /// 裁剪掉的记录贡献的输入 tokens
    pruned_input_tokens: i64,
    /// 裁剪掉的记录贡献的输出 tokens
    pruned_output_tokens: i64,
}

impl std::ops::AddAssign for LifetimeTotals {
    fn add_assign(&mut self, rhs: Self) {
        self.pruned_requests += rhs.pruned_requests;
        self.pruned_input_tokens += rhs.pruned_input_tokens;
        self.pruned_output_tokens += rhs.pruned_output_tokens;
    }
}

/// 累计计数文件路径：与用量文件同目录的 `api_key_lifetime.json`
fn lifetime_path_for(usage_path: &Path) -> Option<PathBuf> {
    usage_path.parent().map(|d| d.join("api_key_lifetime.json"))
}

/// 用量追踪器（线程安全）
pub struct UsageTracker {
    pub(crate) records: Arc<RwLock<Vec<UsageRecord>>>,

    pub(crate) dirty_tx: mpsc::UnboundedSender<()>,

    /// 生命周期累计（裁剪前基数），按 api_key_id 分组
    lifetime_keys: Arc<RwLock<HashMap<u32, LifetimeTotals>>>,
    /// 生命周期累计（裁剪前基数），按 credential_id 分组
    lifetime_credentials: Arc<RwLock<HashMap<u64, LifetimeTotals>>>,
    /// 累计计数落盘路径（仅后台任务持有；None = 记忆态，如测试场景）
    #[allow(dead_code)]
    lifetime_path: Option<PathBuf>,
    /// 累计计数脏标记：仅裁剪发生（基数变化）时置位，由后台任务周期落盘
    lifetime_dirty: Arc<AtomicBool>,
}

impl UsageTracker {
    /// 从文件加载，文件不存在则创建空列表
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let records = if path.exists() {
            let content = fs::read_to_string(&path)?;
            if content.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&content)?
            }
        } else {
            Vec::new()
        };
        let records = Arc::new(RwLock::new(records));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let lifetime_dirty = Arc::new(AtomicBool::new(false));
        let lifetime_dirty_clone = lifetime_dirty.clone();
        let records_clone = records.clone();
        let path_clone = path.clone();
        // 以现存明细为基数迁移。迁移只发生在文件缺失时 —— 文件存在即以文件为准，
        // 保证重启幂等。token 基数迁移为 0：明细裁剪从部署升级后才计入，
        // 请求数优先修复（token 侧误差仅剩「升级前已裁剪的部分」，不可恢复）。
        let lifetime_path = lifetime_path_for(&path);
        let (lifetime_keys, lifetime_credentials) = match &lifetime_path {
            Some(p) if p.exists() => {
                // 文件存在但不可读/不可解析时向上传播错误，禁止以零基数启动：
                // 静默归零后一旦再发生裁剪，会把小基数写回并覆盖历史累计量
                let content = fs::read_to_string(p)?;
                match serde_json::from_str::<HashMap<String, LifetimeTotals>>(&content) {
                    // 文件存在但为空：零基数起步（现存明细仍由 get_summary
                    // 现存求和计入，基数若也计现存记录会导致请求数翻倍）
                    Ok(raw) if raw.is_empty() || content.trim().is_empty() => {
                        (HashMap::new(), HashMap::new())
                    }
                    Ok(raw) if !raw.is_empty() => {
                        let keys = raw
                            .iter()
                            .filter_map(|(k, v)| k.parse::<u32>().ok().map(|id| (id, v.clone())))
                            .collect::<HashMap<_, _>>();
                        // credential 与 key 共用同一文件，键空间可能重叠（如 key #3
                        // 与 credential #3）；当前 credential 侧仅内存态，不落盘恢复
                        (keys, HashMap::new())
                    }
                    // 文件存在但格式损坏：向上传播错误，禁止以零基数启动（同上）
                    Err(e) => {
                        return Err(anyhow::anyhow!(
                            "解析累计基数文件 {} 失败: {}",
                            p.display(),
                            e
                        ));
                    }
                    Ok(_) => (HashMap::new(), HashMap::new()),
                }
            }
            // 存量部署首次升级：零基数迁移（同上，现存记录由明细求和计入）
            _ => (HashMap::new(), HashMap::new()),
        };
        let lifetime_keys = Arc::new(RwLock::new(lifetime_keys));
        let lifetime_credentials = Arc::new(RwLock::new(lifetime_credentials));
        let lifetime_keys_task = lifetime_keys.clone();
        let lifetime_credentials_task = lifetime_credentials.clone();
        let lifetime_path_task = lifetime_path.clone();

        // 启动后台异步写入任务，避免同步文件写阻塞请求线程
        tokio::spawn(async move {
            let lifetime_keys = lifetime_keys_task;
            let lifetime_credentials = lifetime_credentials_task;
            let lifetime_path = lifetime_path_task;
            let mut dirty = false;
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                tokio::select! {
                    res = rx.recv() => {
                        match res {
                            Some(_) => dirty = true,
                        None => {
                                // 通道已关闭（系统退出），执行 Graceful Shutdown 刷盘
                                if dirty
                                    && let Err(e) = Self::save_internal(&records_clone, &path_clone).await {
                                        tracing::error!("Graceful shutdown usage save failed: {}", e);
                                    }
                                if lifetime_dirty.swap(false, Ordering::Relaxed)
                                    && let Err(e) = Self::save_lifetime_internal(
                                        &lifetime_keys, &lifetime_credentials, &lifetime_path,
                                    ).await
                                {
                                    tracing::error!("Graceful shutdown lifetime save failed: {}", e);
                                }
                                break;
                            }
                        }
                    }
                    _ = interval.tick() => {
                        // 明细刷盘成功才清 dirty，失败则下个周期重试
                        if dirty {
                            if let Err(e) = Self::save_internal(&records_clone, &path_clone).await {
                                tracing::error!("Failed to save usage: {}", e);
                            } else {
                                dirty = false;
                            }
                        }
                        // 累计计数仅在发生裁剪（基数变化）时落盘，独立于明细 dirty：
                        // 明细长时间无新增时基数变化也要能落盘；写盘失败恢复脏标记，
                        // 由下个周期重试（否则该次基数变化将永久丢失）
                        if lifetime_dirty.swap(false, Ordering::Relaxed)
                            && let Err(e) = Self::save_lifetime_internal(
                                &lifetime_keys, &lifetime_credentials, &lifetime_path,
                            ).await
                        {
                            lifetime_dirty.store(true, Ordering::Relaxed);
                            tracing::error!("Failed to save lifetime totals: {}", e);
                        }
                    }
                }
            }
        });

        Ok(Self {
            records,

            dirty_tx: tx,
            lifetime_keys,
            lifetime_credentials,
            lifetime_path,
            lifetime_dirty: lifetime_dirty_clone,
        })
    }

    /// 生命周期累计落盘（异步）：仅记录裁剪前基数，不含现存明细
    async fn save_lifetime_internal(
        lifetime_keys: &Arc<RwLock<HashMap<u32, LifetimeTotals>>>,
        _lifetime_credentials: &Arc<RwLock<HashMap<u64, LifetimeTotals>>>,
        path: &Option<PathBuf>,
    ) -> anyhow::Result<()> {
        let Some(path) = path else {
            return Ok(());
        };
        // JSON 对象键不支持数字，存为 str(id)；credential 侧暂不落盘
        // （credential 仅用于账号维度统计，key 侧请求数才是 UI 展示口径）
        let map = lifetime_keys.read().clone();
        let raw: HashMap<String, LifetimeTotals> =
            map.into_iter().map(|(id, v)| (id.to_string(), v)).collect();
        let content = serde_json::to_string(&raw)?;
        let path = path.clone();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            atomic_write(&path, content.as_bytes())?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    /// 内部真正的异步落地方法
    async fn save_internal(
        records: &Arc<RwLock<Vec<UsageRecord>>>,
        file_path: &Path,
    ) -> anyhow::Result<()> {
        let content = {
            let r = records.read();
            serde_json::to_string(&*r)?
        };
        let path = file_path.to_path_buf();
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, content)?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    /// 记录一次请求用量
    pub fn record(
        &self,
        api_key_id: u32,
        model: String,
        input_tokens: i32,
        output_tokens: i32,
        options: RecordOptions,
    ) {
        let RecordOptions {
            credential_id,
            client_ip,
            credits_used,
            cache_read_input_tokens,
            cache_creation_input_tokens,
            effort,
            first_token_ms,
            duration_ms,
            thinking_token_count,
            is_stream,
            user_agent,
        } = options;
        let cost = calculate_cost(&model, input_tokens, output_tokens);
        let record = UsageRecord {
            api_key_id,
            credential_id,
            model,
            input_tokens,
            output_tokens,
            estimated_cost: cost,
            credits_used,
            cache_read_input_tokens,
            cache_creation_input_tokens,
            cache_creation_5m_input_tokens: 0,
            cache_creation_1h_input_tokens: 0,
            created_at: Utc::now(),
            client_ip,
            effort,
            first_token_ms,
            duration_ms,
            thinking_token_count,
            is_stream,
            user_agent,
        };
        {
            let mut records = self.records.write();
            records.push(record);

            // 按 api_key_id 裁剪：保留最新的 MAX_RECORDS_PER_KEY 条
            let key_count = records
                .iter()
                .filter(|r| r.api_key_id == api_key_id)
                .count();
            if key_count > MAX_RECORDS_PER_KEY {
                let excess = key_count - MAX_RECORDS_PER_KEY;
                let mut removed = 0;
                let mut pruned = LifetimeTotals::default();
                records.retain(|r| {
                    if r.api_key_id == api_key_id && removed < excess {
                        removed += 1;
                        pruned.pruned_requests += 1;
                        pruned.pruned_input_tokens += r.input_tokens as i64;
                        pruned.pruned_output_tokens += r.output_tokens as i64;
                        false
                    } else {
                        true
                    }
                });
                *self.lifetime_keys.write().entry(api_key_id).or_default() += pruned;
                self.lifetime_dirty.store(true, Ordering::Relaxed);
            }

            // 按 credential_id 裁剪
            if let Some(cid) = credential_id {
                let cred_count = records
                    .iter()
                    .filter(|r| r.credential_id == Some(cid))
                    .count();
                if cred_count > MAX_RECORDS_PER_KEY {
                    let excess = cred_count - MAX_RECORDS_PER_KEY;
                    let mut removed = 0;
                    // credential 维度裁剪删掉的记录可能属于其他 api_key_id，
                    // 需按记录各自的 api_key_id 分桶补偿 key 侧基数，
                    // 否则对应 key 的请求数会因记录消失而倒退
                    let mut by_key: HashMap<u32, LifetimeTotals> = HashMap::new();
                    records.retain(|r| {
                        if r.credential_id == Some(cid) && removed < excess {
                            removed += 1;
                            let e = by_key.entry(r.api_key_id).or_default();
                            e.pruned_requests += 1;
                            e.pruned_input_tokens += r.input_tokens as i64;
                            e.pruned_output_tokens += r.output_tokens as i64;
                            false
                        } else {
                            true
                        }
                    });
                    {
                        // credential 侧基数 = 各 key 分桶贡献之和（仅内存态）
                        let mut cred = LifetimeTotals::default();
                        let mut keys = self.lifetime_keys.write();
                        for (id, v) in by_key {
                            cred.pruned_requests += v.pruned_requests;
                            cred.pruned_input_tokens += v.pruned_input_tokens;
                            cred.pruned_output_tokens += v.pruned_output_tokens;
                            *keys.entry(id).or_default() += v;
                        }
                        drop(keys);
                        *self.lifetime_credentials.write().entry(cid).or_default() += cred;
                    }
                    self.lifetime_dirty.store(true, Ordering::Relaxed);
                }
            }
        }
        let _ = self.dirty_tx.send(());
    }
    /// 获取单个 API Key 的用量汇总
    pub fn get_summary(&self, api_key_id: u32) -> UsageSummary {
        let records = self.records.read();
        let filtered: Vec<&UsageRecord> = records
            .iter()
            .filter(|r| r.api_key_id == api_key_id)
            .collect();

        let mut by_model: HashMap<String, (u64, i64, i64, f64, f64, f64)> = HashMap::new();
        for r in &filtered {
            let credits = r
                .credits_used
                .unwrap_or_else(|| r.estimated_cost * get_k_ref(&r.model));
            let credits_saved = r
                .credits_used
                .map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu)
                .unwrap_or(0.0);
            let entry = by_model.entry(r.model.clone()).or_default();
            entry.0 += 1;
            entry.1 += r.input_tokens as i64;
            entry.2 += r.output_tokens as i64;
            entry.3 += r.estimated_cost;
            entry.4 += credits;
            entry.5 += credits_saved;
        }

        let total_credits_saved: f64 = filtered
            .iter()
            .filter_map(|r| {
                r.credits_used
                    .map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu)
            })
            .sum();

        let total_credits: f64 = filtered
            .iter()
            .map(|r| {
                r.credits_used
                    .unwrap_or_else(|| r.estimated_cost * get_k_ref(&r.model))
            })
            .sum();

        UsageSummary {
            api_key_id,
            // 基数（已裁剪部分）+ 现存明细求和 = 真实累计请求数；
            // 不加基数会导致超过 MAX_RECORDS_PER_KEY 后请求数封顶不再增长
            total_requests: self
                .lifetime_keys
                .read()
                .get(&api_key_id)
                .map_or(0, |t| t.pruned_requests)
                + filtered.len() as u64,
            total_input_tokens: filtered.iter().map(|r| r.input_tokens as i64).sum(),
            total_output_tokens: filtered.iter().map(|r| r.output_tokens as i64).sum(),
            total_cost: filtered.iter().map(|r| r.estimated_cost).sum(),
            total_credits,
            total_credits_saved,
            by_model: by_model
                .into_iter()
                .map(
                    |(model, (requests, input, output, cost, credits, credits_saved))| ModelUsage {
                        model,
                        requests,
                        input_tokens: input,
                        output_tokens: output,
                        cost,
                        credits,
                        credits_saved,
                    },
                )
                .collect(),
        }
    }

    /// 获取所有 API Key 的用量概览
    pub fn get_all_summaries(&self) -> Vec<UsageSummary> {
        // 枚举明细与累计基数的 ID 并集：仅剩基数（明细被全部裁剪）的 Key
        // 也要出现在概览中，否则管理 UI 会把该 Key 显示为 0
        let mut key_ids: Vec<u32> = {
            let records = self.records.read();
            let lifetime = self.lifetime_keys.read();
            let mut ids: Vec<u32> = records.iter().map(|r| r.api_key_id).collect();
            ids.extend(lifetime.keys().copied());
            ids
        };
        key_ids.sort();
        key_ids.dedup();

        key_ids.iter().map(|&id| self.get_summary(id)).collect()
    }

    /// 历史用量记录中出现过的最大 API Key ID
    ///
    /// 用于 API Key ID 计数器的种子：计数器文件首次不存在时（所有存量部署），
    /// `ApiKeyManager::load` 只能按当前 key 列表推算，会漏掉已删除 key 曾用过的
    /// 高位 id，导致新 key 复用后继承其用量与累计消费额。
    pub fn max_api_key_id(&self) -> u32 {
        self.records
            .read()
            .iter()
            .map(|r| r.api_key_id)
            .max()
            .unwrap_or(0)
    }

    /// 重置指定 API Key 的用量记录
    pub fn reset(&self, api_key_id: u32) -> anyhow::Result<()> {
        // 与 record 一致的锁顺序：保持明细写锁直至基数清零，避免并发查询
        // 在两段之间看到「仅剩基数」的不一致中间态（如 10,007 → 7 → 0）
        let mut records = self.records.write();
        records.retain(|r| r.api_key_id != api_key_id);
        // 基数同步清零，否则重置后请求数会直接回到重置前的累计值
        self.lifetime_keys.write().remove(&api_key_id);
        drop(records);
        self.lifetime_dirty.store(true, Ordering::Relaxed);
        let _ = self.dirty_tx.send(());
        Ok(())
    }

    /// 获取指定 API Key 的累计费用（轻量版，仅算总费用）
    pub fn get_total_cost(&self, api_key_id: u32) -> f64 {
        let records = self.records.read();
        records
            .iter()
            .filter(|r| r.api_key_id == api_key_id)
            .map(|r| r.estimated_cost)
            .sum()
    }

    /// 获取指定 API Key 的累计真实 credits 消耗（轻量版）
    /// 旧记录无 credits_used 时按 estimated_cost * k_ref 回退估算（与日报汇总口径一致）
    pub fn get_total_credits(&self, api_key_id: u32) -> f64 {
        let records = self.records.read();
        records
            .iter()
            .filter(|r| r.api_key_id == api_key_id)
            .map(|r| {
                r.credits_used
                    .unwrap_or_else(|| r.estimated_cost * get_k_ref(&r.model))
            })
            .sum()
    }
}

mod daily;
mod pagination;
mod pricing;

#[cfg(test)]
mod tests;

#[allow(unused_imports)] // 类型归属 daily.rs，经 mod 再导出保持原模块路径
pub(crate) use daily::{CredentialDaySummary, CredentialRangeSummary, DailySummary};
#[allow(unused_imports)]
pub(crate) use pagination::{UsageRecordItem, UsageRecordsPage};
#[allow(unused_imports)]
pub(crate) use pricing::{
    MIN_INFER_INPUT_TOKENS, ModelPricing, get_model_pricing, infer_cache_read_from_credits,
};
pub(crate) use pricing::{calculate_cost, get_k_ref, refine_prefix_cache_read};
#[cfg(test)]
pub(crate) use pricing::{credits_for_cache_read, get_claude_credit_rates};
