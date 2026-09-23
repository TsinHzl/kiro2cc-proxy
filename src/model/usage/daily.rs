//! 按日期（CST）汇总与日报查询

use chrono::FixedOffset;
use serde::Serialize;

use super::pagination::{UsageRecordItem, UsageRecordsPage};
use super::{UsageRecord, UsageTracker, get_k_ref};

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

/// 指定账号在指定 CST 日期的用量汇总
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialDaySummary {
    pub date: String,
    pub credential_id: u64,
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost: f64,
    pub total_credits: f64,
    /// 节省的 credits 总量（仅含有 credits_used 的记录）
    pub total_credits_saved: f64,
}

impl UsageTracker {
    /// 按 CST（UTC+8）当前日期聚合指定 credential 的用量。
    ///
    /// 返回结构包含今日的请求数、输入/输出 token、估算费用、credits 用量及节省值。
    /// 当 credential 在今日没有记录时返回零值汇总（不报错）。
    pub fn get_today_summary_for_credential(&self, credential_id: u64) -> CredentialDaySummary {
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let today = chrono::Utc::now()
            .with_timezone(&cst)
            .format("%Y-%m-%d")
            .to_string();

        let mut requests: u64 = 0;
        let mut input_tokens: u64 = 0;
        let mut output_tokens: u64 = 0;
        let mut cost: f64 = 0.0;
        let mut credits: f64 = 0.0;
        let mut credits_saved: f64 = 0.0;

        let records = self.records.read();
        for r in records.iter() {
            if r.credential_id != Some(credential_id) {
                continue;
            }
            let date = r
                .created_at
                .with_timezone(&cst)
                .format("%Y-%m-%d")
                .to_string();
            if date != today {
                continue;
            }
            requests += 1;
            input_tokens = input_tokens.saturating_add(r.input_tokens.max(0) as u64);
            output_tokens = output_tokens.saturating_add(r.output_tokens.max(0) as u64);
            cost += r.estimated_cost;
            let k_ref = get_k_ref(&r.model);
            credits += r.credits_used.unwrap_or(r.estimated_cost * k_ref);
            if let Some(cu) = r.credits_used {
                credits_saved += r.estimated_cost * k_ref - cu;
            }
        }

        CredentialDaySummary {
            date: today,
            credential_id,
            total_requests: requests,
            total_input_tokens: input_tokens,
            total_output_tokens: output_tokens,
            total_cost: cost,
            total_credits: credits,
            total_credits_saved: credits_saved,
        }
    }

    /// 按 CST（UTC+8）日期聚合所有记录，返回按日期降序的汇总列表
    pub fn get_daily_summaries(&self) -> Vec<DailySummary> {
        use std::collections::BTreeMap;
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let records = self.records.read();
        let mut map: BTreeMap<String, (u64, f64, f64, f64)> = BTreeMap::new();
        for r in records.iter() {
            let date = r
                .created_at
                .with_timezone(&cst)
                .format("%Y-%m-%d")
                .to_string();
            let entry = map.entry(date).or_default();
            entry.0 += 1;
            entry.1 += r.estimated_cost;
            entry.2 += r
                .credits_used
                .unwrap_or(r.estimated_cost * get_k_ref(&r.model));
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
        let page_size = page_size.clamp(1, 500);
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();

        let owned: Vec<UsageRecord> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| {
                    r.created_at
                        .with_timezone(&cst)
                        .format("%Y-%m-%d")
                        .to_string()
                        == date
                })
                .cloned()
                .collect()
        };

        let mut sorted = owned;
        sorted.sort_by_key(|b| std::cmp::Reverse(b.created_at));
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
