//! 用量日报：按日期（CST）汇总的单日/单账号用量结构与日报查询。
use chrono::FixedOffset;
use serde::Serialize;

use super::aggregate::{DailyKey, DayTotals, cst_date_of};
use super::pagination::{UsageRecordItem, UsageRecordsPage};
use super::{UsageRecord, UsageTracker, get_k_ref};

/// 单日明细查询的截断上限（既有行为，保持不变）
const MAX_DAILY_TOTAL: usize = 2000;
/// 区间明细查询的截断上限：降序保留最新 10000 条（OpenSpec T6 用户裁决值；
/// 独立于单 Key 明细存量上限 MAX_RECORDS_PER_KEY）
const MAX_RANGE_TOTAL: usize = 10_000;

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
    /// 累计输入 tokens（含 cache 命中部分）
    pub total_input_tokens: u64,
    /// 累计输出 tokens
    pub total_output_tokens: u64,
    /// 累计 cache 命中的输入 tokens（None 按 0 计入）
    pub total_cache_read_tokens: u64,
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
    /// 累计 cache 命中的输入 tokens（None 按 0 计入）
    pub total_cache_read_tokens: u64,
}

/// 指定账号在 CST 最近 N 天（含今日）的用量汇总
///
/// 数据源为日汇总，不受明细裁剪影响。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialRangeSummary {
    pub credential_id: u64,
    /// 区间起始 CST 日期（含），格式 YYYY-MM-DD
    pub from_date: String,
    /// 区间结束 CST 日期（含），恒为今日
    pub to_date: String,
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost: f64,
    pub total_credits: f64,
    /// 节省的 credits 总量（仅含有 credits_used 的记录）
    pub total_credits_saved: f64,
    /// 累计 cache 命中的输入 tokens（None 按 0 计入）
    pub total_cache_read_tokens: u64,
}

/// 指定账号全部留存统计的用量汇总
///
/// 数据源为日汇总（不受明细裁剪影响），覆盖日汇总上线以来的全部请求；
/// 更早且已被旧版本裁剪的历史无法恢复。费用为估算 USD，Credits 真实用量优先、缺失按模型回退估算。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialUsageSummary {
    pub credential_id: u64,
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost: f64,
    pub total_credits: f64,
    /// 节省的 credits 总量（仅含有 credits_used 的记录）
    pub total_credits_saved: f64,
    /// 累计 cache 命中的输入 tokens（None 按 0 计入）
    pub total_cache_read_tokens: u64,
}

impl UsageTracker {
    /// 汇总指定账号满足 `keep` 条件的日汇总行
    fn sum_credential(&self, credential_id: u64, keep: impl Fn(&DailyKey) -> bool) -> DayTotals {
        let mut total = DayTotals::default();
        for (k, t) in self.daily.read().iter() {
            if k.credential_id == Some(credential_id) && keep(k) {
                total.merge(t);
            }
        }
        total
    }

    /// 按账号聚合全部留存统计（不限日期）。
    ///
    /// 无记录时返回零值汇总（不报错），与 `get_today_summary_for_credential` 一致。
    pub fn get_summary_for_credential(&self, credential_id: u64) -> CredentialUsageSummary {
        let t = self.sum_credential(credential_id, |_| true);
        CredentialUsageSummary {
            credential_id,
            total_requests: t.requests,
            total_input_tokens: t.input_tokens,
            total_output_tokens: t.output_tokens,
            total_cache_read_tokens: t.cache_read_tokens,
            total_cost: t.cost,
            total_credits: t.credits,
            total_credits_saved: t.credits_saved,
        }
    }

    /// 按 CST（UTC+8）当前日期聚合指定 credential 的用量。
    ///
    /// 返回结构包含今日的请求数、输入/输出 token、估算费用、credits 用量及节省值。
    /// 当 credential 在今日没有记录时返回零值汇总（不报错）。
    pub fn get_today_summary_for_credential(&self, credential_id: u64) -> CredentialDaySummary {
        let today = cst_date_of(&chrono::Utc::now());
        let t = self.sum_credential(credential_id, |k| k.date == today);
        CredentialDaySummary {
            date: today,
            credential_id,
            total_requests: t.requests,
            total_input_tokens: t.input_tokens,
            total_output_tokens: t.output_tokens,
            total_cache_read_tokens: t.cache_read_tokens,
            total_cost: t.cost,
            total_credits: t.credits,
            total_credits_saved: t.credits_saved,
        }
    }

    /// 按 CST（UTC+8）聚合指定 credential 最近 N 天（含今日）的用量。
    ///
    /// 区间为 [今日 - (days-1), 今日]，闭区间按 CST 归属日比较；
    /// 聚合口径与 `get_today_summary_for_credential` 完全一致。
    /// 无记录时返回零值汇总（不报错）。
    pub fn get_range_summary_for_credential(
        &self,
        credential_id: u64,
        days: u32,
    ) -> CredentialRangeSummary {
        // 防御极大入参：NaiveDate 减法越界会 panic，区间查询无理由超过留存窗口
        const MAX_RANGE_DAYS: u32 = 366;
        let days = days.clamp(1, MAX_RANGE_DAYS);
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let today = chrono::Utc::now().with_timezone(&cst).date_naive();
        let from = today - chrono::Duration::days((days - 1) as i64);
        let from_s = from.format("%Y-%m-%d").to_string();
        let to_s = today.format("%Y-%m-%d").to_string();

        // YYYY-MM-DD 字典序即时间序
        let t = self.sum_credential(credential_id, |k| {
            from_s.as_str() <= k.date.as_str() && k.date.as_str() <= to_s.as_str()
        });
        CredentialRangeSummary {
            credential_id,
            from_date: from_s,
            to_date: to_s,
            total_requests: t.requests,
            total_input_tokens: t.input_tokens,
            total_output_tokens: t.output_tokens,
            total_cache_read_tokens: t.cache_read_tokens,
            total_cost: t.cost,
            total_credits: t.credits,
            total_credits_saved: t.credits_saved,
        }
    }

    /// 按 CST（UTC+8）日期聚合所有请求，返回按日期降序的汇总列表
    ///
    /// 读日汇总而非明细缓冲：已结束的日期不会随新请求到来而减少。
    pub fn get_daily_summaries(&self) -> Vec<DailySummary> {
        use std::collections::BTreeMap;
        let mut map: BTreeMap<String, DayTotals> = BTreeMap::new();
        for (k, t) in self.daily.read().iter() {
            map.entry(k.date.clone()).or_default().merge(t);
        }
        let mut result: Vec<DailySummary> = map
            .into_iter()
            .map(|(date, t)| DailySummary {
                date,
                total_requests: t.requests,
                total_cost: t.cost,
                total_credits: t.credits,
                total_credits_saved: t.credits_saved,
                total_input_tokens: t.input_tokens,
                total_output_tokens: t.output_tokens,
                total_cache_read_tokens: t.cache_read_tokens,
            })
            .collect();
        result.sort_by(|a, b| b.date.cmp(&a.date));
        result
    }

    /// 分页查询指定 CST（UTC+8）日期的原始记录，硬限总量 2000 条
    ///（from == to == date 的薄包装，与区间版共用管线）
    pub fn get_records_paged_by_date(
        &self,
        date: &str,
        page: usize,
        page_size: usize,
        credential_labels: &std::collections::HashMap<u64, String>,
    ) -> UsageRecordsPage {
        self.paginate_records_in_range(
            date,
            date,
            page,
            page_size,
            credential_labels,
            MAX_DAILY_TOTAL,
        )
    }

    /// 分页查询 CST（UTC+8）闭区间 [from, to]（含两端）的原始记录，
    /// 按时间降序保留最新 10000 条后再分页
    pub fn get_records_paged_by_date_range(
        &self,
        from: &str,
        to: &str,
        page: usize,
        page_size: usize,
        credential_labels: &std::collections::HashMap<u64, String>,
    ) -> UsageRecordsPage {
        self.paginate_records_in_range(
            from,
            to,
            page,
            page_size,
            credential_labels,
            MAX_RANGE_TOTAL,
        )
    }

    /// 单日/区间明细共用管线：CST 日期闭区间过滤（YYYY-MM-DD 字典序即时间序）
    /// → 降序排序 → 按上限截断 → 分页 → DTO 映射
    fn paginate_records_in_range(
        &self,
        from: &str,
        to: &str,
        page: usize,
        page_size: usize,
        credential_labels: &std::collections::HashMap<u64, String>,
        max_total: usize,
    ) -> UsageRecordsPage {
        let page_size = page_size.clamp(1, 500);
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();

        let owned: Vec<UsageRecord> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| {
                    let d = r
                        .created_at
                        .with_timezone(&cst)
                        .format("%Y-%m-%d")
                        .to_string();
                    from <= d.as_str() && d.as_str() <= to
                })
                .cloned()
                .collect()
        };

        let mut sorted = owned;
        sorted.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        sorted.truncate(max_total);

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
                    first_token_ms: r.first_token_ms,
                    duration_ms: r.duration_ms,
                    thinking_token_count: r.thinking_token_count,
                    is_stream: r.is_stream,
                    user_agent: r.user_agent,
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
