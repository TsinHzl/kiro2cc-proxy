// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! API Key 用量追踪模块
//!
//! 记录每个 API Key 的请求用量（input/output tokens），并根据模型定价估算费用。
//! 数据持久化到 `api_key_usage.json`。

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
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

/// 用量追踪器（线程安全）
pub struct UsageTracker {
    pub(crate) records: Arc<RwLock<Vec<UsageRecord>>>,

    pub(crate) dirty_tx: mpsc::UnboundedSender<()>,
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
        let records_clone = records.clone();
        let path_clone = path.clone();

        // 启动后台异步写入任务，避免同步文件写阻塞请求线程
        tokio::spawn(async move {
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
                                break;
                            }
                        }
                    }
                    _ = interval.tick() => {
                        if dirty {
                            if let Err(e) = Self::save_internal(&records_clone, &path_clone).await {
                                tracing::error!("Failed to save usage: {}", e);
                            } else {
                                dirty = false;
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            records,

            dirty_tx: tx,
        })
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
    #[allow(clippy::too_many_arguments)]
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
        effort: Option<String>,
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
            cache_creation_5m_input_tokens: 0,
            cache_creation_1h_input_tokens: 0,
            created_at: Utc::now(),
            client_ip,
            effort,
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
                let cred_count = records
                    .iter()
                    .filter(|r| r.credential_id == Some(cid))
                    .count();
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
            total_requests: filtered.len() as u64,
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
        let records = self.records.read();
        let mut key_ids: Vec<u32> = records.iter().map(|r| r.api_key_id).collect();
        key_ids.sort();
        key_ids.dedup();
        drop(records);

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
        let mut records = self.records.write();
        records.retain(|r| r.api_key_id != api_key_id);
        drop(records);
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
pub(crate) use daily::{CredentialDaySummary, DailySummary};
#[allow(unused_imports)]
pub(crate) use pagination::{UsageRecordItem, UsageRecordsPage};
#[allow(unused_imports)]
pub(crate) use pricing::{ModelPricing, get_model_pricing};
pub(crate) use pricing::{calculate_cost, get_k_ref};
