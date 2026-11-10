//! 按日聚合的用量汇总存储。
//!
//! 明细缓冲（`UsageTracker::records`）受 `MAX_RECORDS_PER_KEY` 裁剪，只适合做「使用日志」
//! 分页展示；日报、账号区间汇总、Key 累计费用/credits 等统计必须读这里的聚合，
//! 否则高流量 Key 的历史日期会随新请求到来被逐条挤掉（数字倒退直至为 0）。
//!
//! 聚合粒度为 `(api_key_id, CST 日期, credential_id, 模型)`：每天每个 Key 每个账号
//! 每个模型最多一行，行数通常与请求量无关（只随天数、账号数、模型数增长），不做裁剪：
//! 配额检查与累计统计都依赖完整历史，裁剪会让已用量回落。
//! 持久化到与 `api_key_usage.json` 同目录的 `api_key_daily_usage.json`。

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};

use super::{UsageRecord, get_k_ref};

/// 聚合键。字段顺序即 `Ord` 顺序：按 `api_key_id` 优先，使单 Key 查询可走 `range`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DailyKey {
    pub api_key_id: u32,
    /// CST（UTC+8）日期，`YYYY-MM-DD`（字典序即时间序）
    pub date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<u64>,
    pub model: String,
}

/// 聚合值。token 数按 `max(0)` 钳制后累加，与此前日报口径一致
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DayTotals {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cost: f64,
    /// 真实 credits 优先，缺失按 `estimated_cost * k_ref` 回退
    pub credits: f64,
    /// 节省的 credits（仅含有 `credits_used` 的记录）
    pub credits_saved: f64,
}

impl DayTotals {
    fn add_record(&mut self, r: &UsageRecord) {
        let k_ref = get_k_ref(&r.model);
        self.requests += 1;
        self.input_tokens = self
            .input_tokens
            .saturating_add(r.input_tokens.max(0) as u64);
        self.output_tokens = self
            .output_tokens
            .saturating_add(r.output_tokens.max(0) as u64);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(r.cache_read_input_tokens.unwrap_or(0).max(0) as u64);
        self.cost += r.estimated_cost;
        self.credits += r.credits_used.unwrap_or(r.estimated_cost * k_ref);
        if let Some(cu) = r.credits_used {
            self.credits_saved += r.estimated_cost * k_ref - cu;
        }
    }

    pub fn merge(&mut self, other: &DayTotals) {
        self.requests += other.requests;
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(other.cache_read_tokens);
        self.cost += other.cost;
        self.credits += other.credits;
        self.credits_saved += other.credits_saved;
    }
}

/// 落盘行：键与值平铺，避免 JSON 对象不支持结构体作键
#[derive(Debug, Serialize, Deserialize)]
struct DailyEntry {
    #[serde(flatten)]
    key: DailyKey,
    #[serde(flatten)]
    totals: DayTotals,
}

/// `DailyEntry` 的借用版本，仅用于序列化
#[derive(Serialize)]
struct DailyEntryRef<'a> {
    #[serde(flatten)]
    key: &'a DailyKey,
    #[serde(flatten)]
    totals: &'a DayTotals,
}

/// 记录所属的 CST（UTC+8）日期
pub(crate) fn cst_date_of(t: &DateTime<Utc>) -> String {
    let cst = FixedOffset::east_opt(8 * 3600).unwrap();
    t.with_timezone(&cst).format("%Y-%m-%d").to_string()
}

#[derive(Debug, Default)]
pub(crate) struct DailyStore {
    map: BTreeMap<DailyKey, DayTotals>,
}

impl DailyStore {
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// 累加一条记录
    pub fn add(&mut self, r: &UsageRecord) {
        let key = DailyKey {
            api_key_id: r.api_key_id,
            date: cst_date_of(&r.created_at),
            credential_id: r.credential_id,
            model: r.model.clone(),
        };
        self.map.entry(key).or_default().add_record(r);
    }

    /// 由现存明细重建（升级后首次启动的回填；已被裁剪的历史无法恢复）
    pub fn from_records(records: &[UsageRecord]) -> Self {
        let mut store = Self::default();
        for r in records {
            store.add(r);
        }
        store
    }

    pub fn iter(&self) -> impl Iterator<Item = (&DailyKey, &DayTotals)> {
        self.map.iter()
    }

    /// 单个 API Key 的全部聚合行
    pub fn iter_key(&self, api_key_id: u32) -> impl Iterator<Item = (&DailyKey, &DayTotals)> {
        let start = DailyKey {
            api_key_id,
            date: String::new(),
            credential_id: None,
            model: String::new(),
        };
        self.map
            .range(start..)
            .take_while(move |(k, _)| k.api_key_id == api_key_id)
    }

    /// 聚合中出现过的最大 API Key ID
    pub fn max_api_key_id(&self) -> u32 {
        self.map.keys().next_back().map_or(0, |k| k.api_key_id)
    }

    pub fn key_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.map.keys().map(|k| k.api_key_id).collect();
        ids.dedup();
        ids
    }

    pub fn remove_key(&mut self, api_key_id: u32) {
        self.map.retain(|k, _| k.api_key_id != api_key_id);
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        // 借用视图序列化，刷盘路径上不克隆键值；与 DailyEntry 的平铺格式一致
        let entries: Vec<DailyEntryRef<'_>> = self
            .map
            .iter()
            .map(|(k, v)| DailyEntryRef { key: k, totals: v })
            .collect();
        serde_json::to_string(&entries)
    }

    pub fn from_json(content: &str) -> serde_json::Result<Self> {
        let entries: Vec<DailyEntry> = serde_json::from_str(content)?;
        let mut map = BTreeMap::new();
        for e in entries {
            // 重复键（手工编辑所致）合并而非覆盖
            map.entry(e.key)
                .or_insert_with(DayTotals::default)
                .merge(&e.totals);
        }
        Ok(Self { map })
    }
}
