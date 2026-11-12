// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! API Key 用量追踪模块
//!
//! 记录每个 API Key 的请求用量（input/output tokens），并根据模型定价估算费用。
//! 数据持久化到两份文件：
//! - `api_key_usage.json`：请求明细，每个 Key / 账号最多保留 `MAX_RECORDS_PER_KEY` 条，
//!   仅用于「使用日志」分页展示；
//! - `api_key_daily_usage.json`：按 `(Key, CST 日期, 账号, 模型)` 聚合的日汇总，不裁剪，
//!   日报、账号汇总、Key 累计费用/credits 等所有统计都读这份（见 `aggregate.rs`）。

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

use aggregate::{DailyStore, DayTotals};

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
/// 每个 API Key / 账号的最大明细条数，超出时删除最老的记录
///
/// 仅限制「使用日志」明细；统计口径读 `DailyStore`，不受此上限影响。
#[cfg(not(test))]
const MAX_RECORDS_PER_KEY: usize = 100_000;
/// 测试构建使用小上限：`record()` 每次全量扫描明细，写满生产上限的用例耗时随条数平方增长
/// （10 万条时 `model::usage` 测试约 6 分钟）。裁剪逻辑与上限数值无关，用小值即可覆盖。
#[cfg(test)]
const MAX_RECORDS_PER_KEY: usize = 2_000;
/// 测试专用再导出（tests.rs 为独立文件，需经模块路径访问私有常量）
#[cfg(test)]
pub(crate) const MAX_RECORDS_PER_KEY_FOR_TEST: usize = MAX_RECORDS_PER_KEY;

/// 引入日汇总之前已被裁剪掉的历史（只读遗留基数，按 api_key_id 分组）
///
/// 旧版本在明细裁剪时把被删记录的请求数/token 累加到 `api_key_lifetime.json`。
/// 日汇总上线后裁剪不再写入此处（新请求已在日汇总里），否则会重复计数；
/// 这份基数冻结为升级时刻的值，仅在 `reset` 时清除。
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LifetimeTotals {
    /// 升级前裁剪掉的记录贡献的请求数
    pruned_requests: u64,
    /// 升级前裁剪掉的记录贡献的输入 tokens
    pruned_input_tokens: i64,
    /// 升级前裁剪掉的记录贡献的输出 tokens
    pruned_output_tokens: i64,
}

/// 遗留基数文件路径：与用量文件同目录的 `api_key_lifetime.json`
fn lifetime_path_for(usage_path: &Path) -> Option<PathBuf> {
    usage_path.parent().map(|d| d.join("api_key_lifetime.json"))
}

/// 日汇总文件路径：与用量文件同目录的 `api_key_daily_usage.json`
fn daily_path_for(usage_path: &Path) -> Option<PathBuf> {
    usage_path
        .parent()
        .map(|d| d.join("api_key_daily_usage.json"))
}

/// 用量追踪器（线程安全）
pub struct UsageTracker {
    /// 请求明细（受 `MAX_RECORDS_PER_KEY` 裁剪，仅供使用日志分页）
    pub(crate) records: Arc<RwLock<Vec<UsageRecord>>>,
    /// 日汇总（不裁剪，所有统计的唯一数据源）
    pub(crate) daily: Arc<RwLock<DailyStore>>,

    pub(crate) dirty_tx: mpsc::UnboundedSender<()>,

    /// 升级前已裁剪历史的遗留基数（只读，仅 `reset` 时清除），按 api_key_id 分组
    lifetime_keys: Arc<RwLock<HashMap<u32, LifetimeTotals>>>,
    /// 遗留基数落盘路径（None = 记忆态，如测试场景）
    #[allow(dead_code)]
    lifetime_path: Option<PathBuf>,
    /// 遗留基数脏标记：仅 `reset` 清除基数时置位，由后台任务周期落盘
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

        // 日汇总：文件缺失/为空而明细非空 = 存量部署首次升级，用现存明细回填
        // （已被旧版本裁剪的历史无法恢复）。文件存在但损坏时向上传播错误，
        // 禁止以空汇总启动：静默归零后再落盘会覆盖掉历史汇总。
        let daily_path = daily_path_for(&path);
        let mut daily = match &daily_path {
            Some(p) if p.exists() => {
                let content = fs::read_to_string(p)?;
                if content.trim().is_empty() {
                    DailyStore::default()
                } else {
                    DailyStore::from_json(&content).map_err(|e| {
                        anyhow::anyhow!("解析日汇总文件 {} 失败: {}", p.display(), e)
                    })?
                }
            }
            _ => DailyStore::default(),
        };
        let backfilled = daily.is_empty() && !records.is_empty();
        if backfilled {
            daily = DailyStore::from_records(&records);
            tracing::info!(
                "日汇总文件缺失，已由 {} 条现存明细回填（更早已被裁剪的历史无法恢复）",
                records.len()
            );
        }
        let daily = Arc::new(RwLock::new(daily));

        let records = Arc::new(RwLock::new(records));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let lifetime_dirty = Arc::new(AtomicBool::new(false));
        let lifetime_dirty_clone = lifetime_dirty.clone();
        let records_clone = records.clone();
        let daily_clone = daily.clone();
        let path_clone = path.clone();
        // 遗留基数：文件存在即以文件为准，保证重启幂等；缺失 = 从未发生过裁剪
        let lifetime_path = lifetime_path_for(&path);
        let lifetime_keys = match &lifetime_path {
            Some(p) if p.exists() => {
                // 文件存在但不可读/不可解析时向上传播错误，禁止以零基数启动：
                // 静默归零后一旦再落盘，会把空基数写回并覆盖历史累计量
                let content = fs::read_to_string(p)?;
                if content.trim().is_empty() {
                    HashMap::new()
                } else {
                    let raw = serde_json::from_str::<HashMap<String, LifetimeTotals>>(&content)
                        .map_err(|e| {
                            anyhow::anyhow!("解析累计基数文件 {} 失败: {}", p.display(), e)
                        })?;
                    raw.into_iter()
                        .filter_map(|(k, v)| k.parse::<u32>().ok().map(|id| (id, v)))
                        .collect()
                }
            }
            _ => HashMap::new(),
        };
        let lifetime_keys = Arc::new(RwLock::new(lifetime_keys));
        let lifetime_keys_task = lifetime_keys.clone();
        let lifetime_path_task = lifetime_path.clone();
        let daily_path_task = daily_path;

        // 启动后台异步写入任务，避免同步文件写阻塞请求线程
        tokio::spawn(async move {
            let lifetime_keys = lifetime_keys_task;
            let lifetime_path = lifetime_path_task;
            let daily_path = daily_path_task;
            let mut dirty = false;
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                tokio::select! {
                    res = rx.recv() => {
                        match res {
                            Some(_) => dirty = true,
                            None => {
                                // 通道已关闭（系统退出），执行 Graceful Shutdown 刷盘
                                if dirty {
                                    // 日汇总先落盘，理由见下方周期刷盘处
                                    if let Err(e) = Self::save_daily_internal(&daily_clone, &daily_path).await {
                                        tracing::error!("Graceful shutdown daily usage save failed: {}", e);
                                    }
                                    if let Err(e) = Self::save_internal(&records_clone, &path_clone).await {
                                        tracing::error!("Graceful shutdown usage save failed: {}", e);
                                    }
                                }
                                if lifetime_dirty.swap(false, Ordering::Relaxed)
                                    && let Err(e) = Self::save_lifetime_internal(
                                        &lifetime_keys, &lifetime_path,
                                    ).await
                                {
                                    tracing::error!("Graceful shutdown lifetime save failed: {}", e);
                                }
                                break;
                            }
                        }
                    }
                    _ = interval.tick() => {
                        // 明细与日汇总都刷盘成功才清 dirty，失败则下个周期重试。
                        // 日汇总先落盘：两次写之间崩溃时最坏是「汇总有、明细无」（仅使用日志少几条）；
                        // 反序则明细多出的请求不会再被补回汇总（汇总非空不触发回填），统计永久漏计
                        if dirty {
                            let day = Self::save_daily_internal(&daily_clone, &daily_path).await;
                            let usage = Self::save_internal(&records_clone, &path_clone).await;
                            if let Err(e) = &day {
                                tracing::error!("Failed to save daily usage: {}", e);
                            }
                            if let Err(e) = &usage {
                                tracing::error!("Failed to save usage: {}", e);
                            }
                            if day.is_ok() && usage.is_ok() {
                                dirty = false;
                            }
                        }
                        // 遗留基数仅在 reset 清除时落盘，独立于明细 dirty；
                        // 写盘失败恢复脏标记，由下个周期重试
                        if lifetime_dirty.swap(false, Ordering::Relaxed)
                            && let Err(e) = Self::save_lifetime_internal(
                                &lifetime_keys, &lifetime_path,
                            ).await
                        {
                            lifetime_dirty.store(true, Ordering::Relaxed);
                            tracing::error!("Failed to save lifetime totals: {}", e);
                        }
                    }
                }
            }
        });

        // 回填的汇总需要落盘，否则下次启动还会重新回填
        if backfilled {
            let _ = tx.send(());
        }

        Ok(Self {
            records,
            daily,
            dirty_tx: tx,
            lifetime_keys,
            lifetime_path,
            lifetime_dirty: lifetime_dirty_clone,
        })
    }

    /// 遗留基数落盘（异步）
    async fn save_lifetime_internal(
        lifetime_keys: &Arc<RwLock<HashMap<u32, LifetimeTotals>>>,
        path: &Option<PathBuf>,
    ) -> anyhow::Result<()> {
        let Some(path) = path else {
            return Ok(());
        };
        // JSON 对象键不支持数字，存为 str(id)
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

    /// 日汇总落盘（异步，原子写）
    async fn save_daily_internal(
        daily: &Arc<RwLock<DailyStore>>,
        path: &Option<PathBuf>,
    ) -> anyhow::Result<()> {
        let Some(path) = path else {
            return Ok(());
        };
        let content = daily.read().to_json()?;
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
        // 先累加日汇总再入明细：明细随后可能被裁剪，汇总不受影响
        self.daily.write().add(&record);
        {
            let mut records = self.records.write();
            records.push(record);

            // 按 api_key_id 裁剪：保留最新的 MAX_RECORDS_PER_KEY 条
            let key_count = records
                .iter()
                .filter(|r| r.api_key_id == api_key_id)
                .count();
            if key_count > MAX_RECORDS_PER_KEY {
                let mut excess = key_count - MAX_RECORDS_PER_KEY;
                records.retain(|r| {
                    if r.api_key_id == api_key_id && excess > 0 {
                        excess -= 1;
                        false
                    } else {
                        true
                    }
                });
            }

            // 按 credential_id 裁剪
            if let Some(cid) = credential_id {
                let cred_count = records
                    .iter()
                    .filter(|r| r.credential_id == Some(cid))
                    .count();
                if cred_count > MAX_RECORDS_PER_KEY {
                    let mut excess = cred_count - MAX_RECORDS_PER_KEY;
                    records.retain(|r| {
                        if r.credential_id == Some(cid) && excess > 0 {
                            excess -= 1;
                            false
                        } else {
                            true
                        }
                    });
                }
            }
        }
        let _ = self.dirty_tx.send(());
    }

    /// 获取单个 API Key 的用量汇总（读日汇总，不受明细裁剪影响）
    pub fn get_summary(&self, api_key_id: u32) -> UsageSummary {
        let mut total = DayTotals::default();
        let mut by_model: HashMap<String, DayTotals> = HashMap::new();
        {
            let daily = self.daily.read();
            for (k, t) in daily.iter_key(api_key_id) {
                total.merge(t);
                by_model.entry(k.model.clone()).or_default().merge(t);
            }
        }
        // 升级前已裁剪的历史（只有请求数与 token，没有费用/credits）
        let legacy = self
            .lifetime_keys
            .read()
            .get(&api_key_id)
            .cloned()
            .unwrap_or_default();

        UsageSummary {
            api_key_id,
            total_requests: legacy.pruned_requests + total.requests,
            total_input_tokens: legacy.pruned_input_tokens + total.input_tokens as i64,
            total_output_tokens: legacy.pruned_output_tokens + total.output_tokens as i64,
            total_cost: total.cost,
            total_credits: total.credits,
            total_credits_saved: total.credits_saved,
            by_model: by_model
                .into_iter()
                .map(|(model, t)| ModelUsage {
                    model,
                    requests: t.requests,
                    input_tokens: t.input_tokens as i64,
                    output_tokens: t.output_tokens as i64,
                    cost: t.cost,
                    credits: t.credits,
                    credits_saved: t.credits_saved,
                })
                .collect(),
        }
    }

    /// 获取所有 API Key 的用量概览
    pub fn get_all_summaries(&self) -> Vec<UsageSummary> {
        // 枚举日汇总与遗留基数的 ID 并集：仅剩遗留基数的 Key 也要出现在概览中
        let mut key_ids: Vec<u32> = self.daily.read().key_ids();
        key_ids.extend(self.lifetime_keys.read().keys().copied());
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
        let from_records = self
            .records
            .read()
            .iter()
            .map(|r| r.api_key_id)
            .max()
            .unwrap_or(0);
        let from_daily = self.daily.read().max_api_key_id();
        let from_legacy = self.lifetime_keys.read().keys().copied().max().unwrap_or(0);
        from_records.max(from_daily).max(from_legacy)
    }

    /// 重置指定 API Key 的用量（明细、日汇总、遗留基数一并清除）
    pub fn reset(&self, api_key_id: u32) -> anyhow::Result<()> {
        // 保持明细写锁直至汇总与基数清零（锁顺序 records → daily → lifetime，
        // 与其他路径一致），避免并发查询看到只清了一部分的中间态
        let mut records = self.records.write();
        records.retain(|r| r.api_key_id != api_key_id);
        self.daily.write().remove_key(api_key_id);
        self.lifetime_keys.write().remove(&api_key_id);
        drop(records);
        self.lifetime_dirty.store(true, Ordering::Relaxed);
        let _ = self.dirty_tx.send(());
        Ok(())
    }

    /// 获取指定 API Key 的累计费用（轻量版，仅算总费用；读日汇总）
    pub fn get_total_cost(&self, api_key_id: u32) -> f64 {
        self.daily
            .read()
            .iter_key(api_key_id)
            .map(|(_, t)| t.cost)
            .sum()
    }

    /// 获取指定 API Key 的累计真实 credits 消耗（轻量版；读日汇总）
    /// 旧记录无 credits_used 时按 estimated_cost * k_ref 回退估算（与日报汇总口径一致）
    pub fn get_total_credits(&self, api_key_id: u32) -> f64 {
        self.daily
            .read()
            .iter_key(api_key_id)
            .map(|(_, t)| t.credits)
            .sum()
    }
}

mod aggregate;
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
