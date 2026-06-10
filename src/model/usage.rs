// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! API Key 用量追踪模块
//!
//! 记录每个 API Key 的请求用量（input/output tokens），并根据模型定价估算费用。
//! 数据持久化到 `api_key_usage.json`。

use chrono::{DateTime, FixedOffset, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
    /// 记录时间
    pub created_at: DateTime<Utc>,
    /// 客户端 IP（None 表示旧数据或未知）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
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
}
/// 模型定价（每百万 tokens，美元）
/// 使用 200K context 标准定价
struct ModelPricing {
    input_per_mtok: f64,
    output_per_mtok: f64,
}

/// 根据模型名获取定价
fn get_model_pricing(model: &str) -> ModelPricing {
    let model_lower = model.to_lowercase();

    if model_lower.contains("opus") {
        // Opus 4.5+: $5 / $25
        ModelPricing {
            input_per_mtok: 5.0,
            output_per_mtok: 25.0,
        }
    } else if model_lower.contains("haiku") {
        // Haiku 4.5: $1 / $5
        ModelPricing {
            input_per_mtok: 1.0,
            output_per_mtok: 5.0,
        }
    } else {
        // Sonnet 4: $3 / $15（默认）
        ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        }
    }
}

/// 无缓存基准换算率（credits/$），按模型实测值
/// sonnet: 7.06，opus-4-6: 7.13，opus-4-7: 7.30，opus-4-8: 7.24，其余默认 7.06
fn get_k_ref(model: &str) -> f64 {
    let lower = model.to_lowercase();
    if lower.contains("opus") {
        if lower.contains("4-8") || lower.contains("4.8") {
            7.24
        } else if lower.contains("4-7") || lower.contains("4.7") {
            7.30
        } else {
            7.13
        }
    } else {
        7.06
    }
}

/// 计算单次请求的估算费用
fn calculate_cost(model: &str, input_tokens: i32, output_tokens: i32) -> f64 {
    let pricing = get_model_pricing(model);
    let input_cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * pricing.output_per_mtok;
    input_cost + output_cost
}

/// 每个 API Key / 账号的最大日志条数，超出时删除最老的记录
const MAX_RECORDS_PER_KEY: usize = 10_000;

/// 用量追踪器（线程安全）
pub struct UsageTracker {
    records: RwLock<Vec<UsageRecord>>,
    file_path: PathBuf,
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
        Ok(Self {
            records: RwLock::new(records),
            file_path: path,
        })
    }

    /// 持久化到文件
    fn save(&self) -> anyhow::Result<()> {
        let records = self.records.read();
        let content = serde_json::to_string(&*records)?;
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.file_path, content)?;
        Ok(())
    }

    /// 记录一次请求用量
    pub fn record(
        &self,
        api_key_id: u32,
        credential_id: Option<u64>,
        model: String,
        input_tokens: i32,
        output_tokens: i32,
        client_ip: Option<String>,
        credits_used: Option<f64>,
        cache_read_input_tokens: Option<i32>,
        cache_creation_input_tokens: Option<i32>,
    ) {
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
            created_at: Utc::now(),
            client_ip,
        };
        {
            let mut records = self.records.write();
            records.push(record);

            // 按 api_key_id 裁剪：保留最新的 MAX_RECORDS_PER_KEY 条
            let key_count = records.iter().filter(|r| r.api_key_id == api_key_id).count();
            if key_count > MAX_RECORDS_PER_KEY {
                let excess = key_count - MAX_RECORDS_PER_KEY;
                let mut removed = 0;
                records.retain(|r| {
                    if removed < excess && r.api_key_id == api_key_id {
                        removed += 1;
                        false
                    } else {
                        true
                    }
                });
            }

            // 按 credential_id 裁剪
            if let Some(cid) = credential_id {
                let cred_count = records.iter().filter(|r| r.credential_id == Some(cid)).count();
                if cred_count > MAX_RECORDS_PER_KEY {
                    let excess = cred_count - MAX_RECORDS_PER_KEY;
                    let mut removed = 0;
                    records.retain(|r| {
                        if removed < excess && r.credential_id == Some(cid) {
                            removed += 1;
                            false
                        } else {
                            true
                        }
                    });
                }
            }
        }
        if let Err(e) = self.save() {
            tracing::warn!("保存用量记录失败: {}", e);
        }
    }
    /// 获取单个 API Key 的用量汇总
    pub fn get_summary(&self, api_key_id: u32) -> UsageSummary {
        let records = self.records.read();
        let filtered: Vec<&UsageRecord> = records
            .iter()
            .filter(|r| r.api_key_id == api_key_id)
            .collect();

        let mut by_model: HashMap<String, (u64, i64, i64, f64)> = HashMap::new();
        for r in &filtered {
            let entry = by_model.entry(r.model.clone()).or_default();
            entry.0 += 1;
            entry.1 += r.input_tokens as i64;
            entry.2 += r.output_tokens as i64;
            entry.3 += r.estimated_cost;
        }

        let total_credits_saved: f64 = filtered
            .iter()
            .filter_map(|r| r.credits_used.map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu))
            .sum();

        UsageSummary {
            api_key_id,
            total_requests: filtered.len() as u64,
            total_input_tokens: filtered.iter().map(|r| r.input_tokens as i64).sum(),
            total_output_tokens: filtered.iter().map(|r| r.output_tokens as i64).sum(),
            total_cost: filtered.iter().map(|r| r.estimated_cost).sum(),
            total_credits_saved,
            by_model: by_model
                .into_iter()
                .map(|(model, (requests, input, output, cost))| ModelUsage {
                    model,
                    requests,
                    input_tokens: input,
                    output_tokens: output,
                    cost,
                })
                .collect(),
        }
    }

    /// 获取所有 API Key 的用量概览
    pub fn get_all_summaries(&self) -> Vec<UsageSummary> {
        let records = self.records.read();
        let mut key_ids: Vec<u32> = records.iter().map(|r| r.api_key_id).collect();
        key_ids.sort();
        key_ids.dedup();
        drop(records);

        key_ids.iter().map(|&id| self.get_summary(id)).collect()
    }

    /// 重置指定 API Key 的用量记录
    pub fn reset(&self, api_key_id: u32) -> anyhow::Result<()> {
        let mut records = self.records.write();
        records.retain(|r| r.api_key_id != api_key_id);
        drop(records);
        self.save()
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

    /// 分页查询指定 API Key 的原始请求记录（按 created_at 降序）
    /// page 从 1 开始，小于 1 的值视为 1
    /// credential_labels: 账号 ID -> 显示标签（email 或 nickname）
    pub fn get_records_paged(
        &self,
        api_key_id: u32,
        page: usize,
        page_size: usize,
        credential_labels: &HashMap<u64, String>,
    ) -> UsageRecordsPage {
        if page_size == 0 {
            return UsageRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size: 0,
                total_pages: 0,
            };
        }

        // 在锁内只做过滤和克隆，不做排序
        let owned: Vec<UsageRecord> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| r.api_key_id == api_key_id)
                .cloned()
                .collect()
        };

        let total = owned.len();
        if total == 0 {
            return UsageRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size,
                total_pages: 0,
            };
        }

        // 锁已释放，在锁外排序
        let mut sorted = owned;
        sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let total_pages = (total + page_size - 1) / page_size;
        let page = page.max(1).min(total_pages);
        let start = (page - 1) * page_size;

        let items: Vec<UsageRecordItem> = sorted
            .into_iter()
            .skip(start)
            .take(page_size)
            .map(|r| {
                let credential_label = r.credential_id.and_then(|cid| credential_labels.get(&cid).cloned());
                let credits_saved = r.credits_used.map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu);
                UsageRecordItem {
                    model: r.model,
                    input_tokens: r.input_tokens,
                    output_tokens: r.output_tokens,
                    estimated_cost: r.estimated_cost,
                    credits_used: r.credits_used,
                    credits_saved,
                    cache_read_input_tokens: r.cache_read_input_tokens,
                    cache_creation_input_tokens: r.cache_creation_input_tokens,
                    created_at: r.created_at,
                    credential_id: r.credential_id,
                    credential_label,
                    client_ip: r.client_ip,
                }
            })
            .collect();

        UsageRecordsPage {
            records: items,
            total,
            page,
            page_size,
            total_pages,
        }
    }
}

/// 分页查询结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecordsPage {
    pub records: Vec<UsageRecordItem>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// 对外暴露的单条记录
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRecordItem {
    pub model: String,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub estimated_cost: f64,
    /// 真实 credits 消耗（来自 meteringEvent，None 表示旧数据）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_used: Option<f64>,
    /// 缓存命中的输入 token 数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i32>,
    /// 缓存创建的输入 token 数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i32>,
    /// 节省的 credits（与无缓存对比）= estimated_cost * get_k_ref(model) - credits_used
    /// 仅当 credits_used 有值时才有值
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_saved: Option<f64>,
    pub created_at: DateTime<Utc>,
    /// 使用的账号 ID（None 表示旧数据或主密钥请求）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<u64>,
    /// 账号账号（email 或 nickname，用于前端显示）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential_label: Option<String>,
    /// 客户端 IP（None 表示旧数据或未知）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<String>,
}

impl UsageTracker {
    /// 分页查询指定账号的原始请求记录（按 created_at 降序）
    pub fn get_records_paged_by_credential(
        &self,
        credential_id: u64,
        page: usize,
        page_size: usize,
        credential_labels: &HashMap<u64, String>,
    ) -> UsageRecordsPage {
        if page_size == 0 {
            return UsageRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size: 0,
                total_pages: 0,
            };
        }

        let owned: Vec<UsageRecord> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| r.credential_id == Some(credential_id))
                .cloned()
                .collect()
        };

        let total = owned.len();
        if total == 0 {
            return UsageRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size,
                total_pages: 0,
            };
        }

        let mut sorted = owned;
        sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let total_pages = (total + page_size - 1) / page_size;
        let page = page.max(1).min(total_pages);
        let start = (page - 1) * page_size;

        let items: Vec<UsageRecordItem> = sorted
            .into_iter()
            .skip(start)
            .take(page_size)
            .map(|r| {
                let credential_label = r.credential_id.and_then(|cid| credential_labels.get(&cid).cloned());
                let credits_saved = r.credits_used.map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu);
                UsageRecordItem {
                    model: r.model,
                    input_tokens: r.input_tokens,
                    output_tokens: r.output_tokens,
                    estimated_cost: r.estimated_cost,
                    credits_used: r.credits_used,
                    credits_saved,
                    cache_read_input_tokens: r.cache_read_input_tokens,
                    cache_creation_input_tokens: r.cache_creation_input_tokens,
                    created_at: r.created_at,
                    credential_id: r.credential_id,
                    credential_label,
                    client_ip: r.client_ip,
                }
            })
            .collect();

        UsageRecordsPage {
            records: items,
            total,
            page,
            page_size,
            total_pages,
        }
    }
}

/// 按日期汇总的用量
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySummary {
    pub date: String,
    pub total_requests: u64,
    pub total_cost: f64,
    pub total_credits: f64,
    /// 节省的 credits 总量（仅含有 credits_used 的记录）
    pub total_credits_saved: f64,
}

impl UsageTracker {
    /// 按 CST（UTC+8）日期聚合所有记录，返回按日期降序的汇总列表
    pub fn get_daily_summaries(&self) -> Vec<DailySummary> {
        use std::collections::BTreeMap;
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let records = self.records.read();
        let mut map: BTreeMap<String, (u64, f64, f64, f64)> = BTreeMap::new();
        for r in records.iter() {
            let date = r.created_at.with_timezone(&cst).format("%Y-%m-%d").to_string();
            let entry = map.entry(date).or_default();
            entry.0 += 1;
            entry.1 += r.estimated_cost;
            entry.2 += r.credits_used.unwrap_or(r.estimated_cost / 0.72);
            if let Some(cu) = r.credits_used {
                entry.3 += r.estimated_cost * get_k_ref(&r.model) - cu;
            }
        }
        let mut result: Vec<DailySummary> = map
            .into_iter()
            .map(|(date, (reqs, cost, credits, saved))| DailySummary {
                date,
                total_requests: reqs,
                total_cost: cost,
                total_credits: credits,
                total_credits_saved: saved,
            })
            .collect();
        result.sort_by(|a, b| b.date.cmp(&a.date));
        result
    }

    /// 分页查询指定 CST（UTC+8）日期的原始记录，硬限总量 2000 条
    pub fn get_records_paged_by_date(
        &self,
        date: &str,
        page: usize,
        page_size: usize,
        credential_labels: &std::collections::HashMap<u64, String>,
    ) -> UsageRecordsPage {
        const MAX_TOTAL: usize = 2000;
        let page_size = page_size.min(500).max(1);
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();

        let owned: Vec<UsageRecord> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| r.created_at.with_timezone(&cst).format("%Y-%m-%d").to_string() == date)
                .cloned()
                .collect()
        };

        let mut sorted = owned;
        sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        sorted.truncate(MAX_TOTAL);

        let total = sorted.len();
        if total == 0 {
            return UsageRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size,
                total_pages: 0,
            };
        }

        let total_pages = (total + page_size - 1) / page_size;
        let page = page.max(1).min(total_pages);
        let start = (page - 1) * page_size;

        let items: Vec<UsageRecordItem> = sorted
            .into_iter()
            .skip(start)
            .take(page_size)
            .map(|r| {
                let credential_label = r
                    .credential_id
                    .and_then(|cid| credential_labels.get(&cid).cloned());
                let credits_saved = r.credits_used.map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu);
                UsageRecordItem {
                    model: r.model,
                    input_tokens: r.input_tokens,
                    output_tokens: r.output_tokens,
                    estimated_cost: r.estimated_cost,
                    credits_used: r.credits_used,
                    credits_saved,
                    cache_read_input_tokens: r.cache_read_input_tokens,
                    cache_creation_input_tokens: r.cache_creation_input_tokens,
                    created_at: r.created_at,
                    credential_id: r.credential_id,
                    credential_label,
                    client_ip: r.client_ip,
                }
            })
            .collect();

        UsageRecordsPage {
            records: items,
            total,
            page,
            page_size,
            total_pages,
        }
    }
}
