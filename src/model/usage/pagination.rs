//! 分页查询：API Key / 账号维度的原始请求记录分页

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;

use super::{UsageRecord, UsageTracker, get_k_ref};

impl UsageTracker {
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
        sorted.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        let total_pages = total.div_ceil(page_size);
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
                let credits_saved = r
                    .credits_used
                    .map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu);
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
                    effort: r.effort,
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
    /// 请求的 effort 级别（None 表示旧数据或客户端未传 output_config）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
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
        sorted.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        let total_pages = total.div_ceil(page_size);
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
                let credits_saved = r
                    .credits_used
                    .map(|cu| r.estimated_cost * get_k_ref(&r.model) - cu);
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
                    effort: r.effort,
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
