//! 使用日志 CSV 导出：按 API Key / 账号 / CST 日期区间筛出全部明细并渲染为 CSV。
//!
//! 与分页接口的差异：导出不分页、不套用分页接口的 2000 / 10000 条截断，
//! 返回范围内仍留存于明细缓冲的全部记录（单 Key / 账号上限见 `MAX_RECORDS_PER_KEY`）。
use std::collections::HashMap;

use chrono::FixedOffset;

use super::aggregate::cst_date_of;
use super::{UsageRecord, UsageTracker, get_k_ref};

/// 导出范围
#[derive(Debug, Clone, Copy)]
pub enum ExportScope<'a> {
    /// 单个 API Key 的全部明细
    ApiKey(u32),
    /// 单个账号的全部明细
    Credential(u64),
    /// CST（UTC+8）日期闭区间 `[from, to]`（`YYYY-MM-DD`，两端含），覆盖所有 Key
    DateRange { from: &'a str, to: &'a str },
}

impl ExportScope<'_> {
    fn matches(&self, r: &UsageRecord) -> bool {
        match self {
            Self::ApiKey(id) => r.api_key_id == *id,
            Self::Credential(id) => r.credential_id == Some(*id),
            Self::DateRange { from, to } => {
                // YYYY-MM-DD 字典序即时间序
                let d = cst_date_of(&r.created_at);
                *from <= d.as_str() && d.as_str() <= *to
            }
        }
    }
}

/// 导出文件的读者：决定是否包含账号信息与失败原因
///
/// 与用户端分页接口同口径：账号标识、失败原因可能含上游 / 账号侧细节，不向 Key 持有者暴露。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportAudience {
    Admin,
    User,
}

/// 单元格内容
enum Cell {
    /// 来自客户端或上游的自由文本：需做 CSV 公式注入防护
    Text(String),
    /// 数字 / 时间 / 服务端枚举：原样输出
    Raw(String),
}

impl Cell {
    fn opt_text(v: Option<&str>) -> Self {
        Self::Text(v.unwrap_or_default().to_string())
    }

    fn opt_num<T: std::fmt::Display>(v: Option<T>) -> Self {
        Self::Raw(v.map(|n| n.to_string()).unwrap_or_default())
    }
}

/// 账号 ID -> 邮箱
type Emails = HashMap<u64, String>;

/// 序号列表头：位于所有数据列之前，从 1 递增，末行序号即导出总条数
const SEQ_HEADER: &str = "seq";

/// 一列的定义：表头与取值放在同一处，避免表头与数据行错位
struct Column {
    name: &'static str,
    /// 仅管理端导出包含
    admin_only: bool,
    value: fn(&UsageRecord, &Emails) -> Cell,
}

/// 非完成记录（失败 / 客户端取消）不计费，费用类列留空（与页面显示「—」一致）
fn billed(r: &UsageRecord) -> bool {
    r.is_completed()
}

/// 费用 / credits 保留 6 位小数：足以覆盖单次请求的最小量级，又避免浮点噪声（0.30000000000000004）
fn money(v: f64) -> Cell {
    Cell::Raw(format!("{v:.6}"))
}

/// 列顺序即 CSV 列顺序。时间用 CST（与日期区间筛选、日报同一时区），表头已标注。
static COLUMNS: &[Column] = &[
    Column {
        name: "time_cst",
        admin_only: false,
        value: |r, _| {
            let cst = FixedOffset::east_opt(8 * 3600).unwrap();
            Cell::Raw(
                r.created_at
                    .with_timezone(&cst)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
            )
        },
    },
    Column {
        name: "api_key_id",
        admin_only: true,
        value: |r, _| Cell::Raw(r.api_key_id.to_string()),
    },
    Column {
        name: "credential_id",
        admin_only: true,
        value: |r, _| Cell::opt_num(r.credential_id),
    },
    Column {
        name: "email",
        admin_only: true,
        // 账号无邮箱（或账号已删除）时留空，账号仍可由上一列 credential_id 定位
        value: |r, emails| {
            Cell::opt_text(
                r.credential_id
                    .and_then(|id| emails.get(&id))
                    .map(String::as_str),
            )
        },
    },
    Column {
        name: "model",
        admin_only: false,
        value: |r, _| Cell::Text(r.model.clone()),
    },
    Column {
        name: "effort",
        admin_only: false,
        value: |r, _| Cell::opt_text(r.effort.as_deref()),
    },
    Column {
        name: "request_type",
        admin_only: false,
        value: |r, _| {
            Cell::Raw(
                match r.is_stream {
                    Some(true) => "stream",
                    Some(false) => "non_stream",
                    None => "",
                }
                .to_string(),
            )
        },
    },
    Column {
        name: "result",
        admin_only: false,
        value: |r, _| Cell::Raw(r.outcome.as_deref().unwrap_or("completed").to_string()),
    },
    Column {
        name: "error_message",
        admin_only: true,
        value: |r, _| Cell::opt_text(r.error_message.as_deref()),
    },
    Column {
        name: "input_tokens_total",
        admin_only: false,
        value: |r, _| Cell::Raw(r.input_tokens.to_string()),
    },
    Column {
        name: "cache_read_tokens",
        admin_only: false,
        value: |r, _| Cell::opt_num(r.cache_read_input_tokens),
    },
    Column {
        name: "cache_creation_tokens",
        admin_only: false,
        value: |r, _| Cell::opt_num(r.cache_creation_input_tokens),
    },
    Column {
        name: "output_tokens",
        admin_only: false,
        value: |r, _| Cell::Raw(r.output_tokens.to_string()),
    },
    Column {
        name: "thinking_tokens",
        admin_only: false,
        value: |r, _| Cell::opt_num(r.thinking_token_count),
    },
    Column {
        name: "first_token_ms",
        admin_only: false,
        value: |r, _| Cell::opt_num(r.first_token_ms),
    },
    Column {
        name: "duration_ms",
        admin_only: false,
        value: |r, _| Cell::opt_num(r.duration_ms),
    },
    Column {
        name: "cost_usd",
        admin_only: false,
        value: |r, _| {
            if billed(r) {
                money(r.estimated_cost)
            } else {
                Cell::Raw(String::new())
            }
        },
    },
    Column {
        name: "credits",
        admin_only: false,
        // 与页面同口径：优先真实值，缺失时按模型档位估算
        value: |r, _| {
            if billed(r) {
                money(
                    r.credits_used
                        .unwrap_or_else(|| r.estimated_cost * get_k_ref(&r.model)),
                )
            } else {
                Cell::Raw(String::new())
            }
        },
    },
    Column {
        name: "credits_saved",
        admin_only: false,
        value: |r, _| match r.credits_used {
            Some(cu) if billed(r) => money(r.estimated_cost * get_k_ref(&r.model) - cu),
            _ => Cell::Raw(String::new()),
        },
    },
    Column {
        name: "client_ip",
        admin_only: false,
        value: |r, _| Cell::opt_text(r.client_ip.as_deref()),
    },
    Column {
        name: "user_agent",
        admin_only: false,
        value: |r, _| Cell::opt_text(r.user_agent.as_deref()),
    },
];

/// 追加一个 CSV 字段（RFC 4180 转义）。
///
/// `guard_formula` 为真时，对以 `= + - @ \t \r` 开头的文本前置单引号：model / user_agent /
/// error_message 等来自客户端或上游，直接交给 Excel / WPS 打开会被当作公式执行。
fn push_field(out: &mut String, value: &str, guard_formula: bool) {
    let needs_guard = guard_formula
        && value
            .chars()
            .next()
            .is_some_and(|c| matches!(c, '=' | '+' | '-' | '@' | '\t' | '\r'));
    let needs_quote = needs_guard || value.contains([',', '"', '\r', '\n']);
    if !needs_quote {
        out.push_str(value);
        return;
    }
    out.push('"');
    if needs_guard {
        out.push('\'');
    }
    for c in value.chars() {
        if c == '"' {
            out.push('"');
        }
        out.push(c);
    }
    out.push('"');
}

impl UsageTracker {
    /// 导出范围内的全部明细为 CSV 文本（UTF-8，带 BOM 以便 Excel 正确识别中文，CRLF 换行，
    /// 按时间降序）。范围内没有记录时只含表头。
    pub fn export_records_csv(
        &self,
        scope: ExportScope<'_>,
        audience: ExportAudience,
        credential_emails: &Emails,
    ) -> String {
        // 锁内只过滤与克隆；排序与渲染都在锁外
        let mut matched: Vec<UsageRecord> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| scope.matches(r))
                .cloned()
                .collect()
        };
        matched.sort_by_key(|r| std::cmp::Reverse(r.created_at));

        let columns: Vec<&Column> = COLUMNS
            .iter()
            .filter(|c| audience == ExportAudience::Admin || !c.admin_only)
            .collect();

        let mut out = String::with_capacity(64 + matched.len() * 220);
        out.push('\u{FEFF}');
        out.push_str(SEQ_HEADER);
        for col in &columns {
            out.push(',');
            out.push_str(col.name);
        }
        out.push_str("\r\n");

        for (n, record) in matched.iter().enumerate() {
            out.push_str(&(n + 1).to_string());
            for col in &columns {
                out.push(',');
                match (col.value)(record, credential_emails) {
                    Cell::Text(s) => push_field(&mut out, &s, true),
                    Cell::Raw(s) => push_field(&mut out, &s, false),
                }
            }
            out.push_str("\r\n");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn utc_at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn record(api_key_id: u32, credential_id: Option<u64>, created_at: &str) -> UsageRecord {
        UsageRecord {
            api_key_id,
            credential_id,
            model: "claude-opus-4.6".to_string(),
            input_tokens: 100,
            output_tokens: 10,
            estimated_cost: 0.01,
            credits_used: Some(0.02),
            cache_read_input_tokens: Some(40),
            cache_creation_input_tokens: None,
            cache_creation_5m_input_tokens: 0,
            cache_creation_1h_input_tokens: 0,
            created_at: utc_at(created_at),
            client_ip: Some("1.2.3.4".to_string()),
            effort: None,
            first_token_ms: Some(500),
            duration_ms: Some(2000),
            thinking_token_count: None,
            is_stream: Some(true),
            user_agent: Some("claude-cli/2.1.5 (external, cli)".to_string()),
            outcome: None,
            error_message: None,
        }
    }

    async fn tracker_with(records: Vec<UsageRecord>) -> (UsageTracker, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("kiro2cc_usage_export_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let tracker = UsageTracker::load(dir.join("api_key_usage.json")).unwrap();
        tracker.records.write().extend(records);
        (tracker, dir)
    }

    /// 去掉 BOM 后按 CRLF 拆行
    fn lines(csv: &str) -> Vec<&str> {
        csv.strip_prefix('\u{FEFF}')
            .expect("应以 BOM 开头")
            .split("\r\n")
            .filter(|l| !l.is_empty())
            .collect()
    }

    #[test]
    fn test_push_field_escaping() {
        let mut s = String::new();
        push_field(&mut s, "plain", true);
        assert_eq!(s, "plain");

        let mut s = String::new();
        push_field(&mut s, "a,b", false);
        assert_eq!(s, "\"a,b\"");

        let mut s = String::new();
        push_field(&mut s, "say \"hi\"\nbye", false);
        assert_eq!(s, "\"say \"\"hi\"\"\nbye\"");
    }

    #[test]
    fn test_push_field_formula_guard_only_for_text() {
        for evil in ["=1+1", "+1", "-1", "@SUM(A1)", "\tcmd", "\rcmd"] {
            let mut s = String::new();
            push_field(&mut s, evil, true);
            assert!(
                s.starts_with("\"'"),
                "{evil:?} 应被前置单引号并加引号，得到 {s:?}"
            );
        }
        // 数字列（如负的 credits_saved）不能被改写
        let mut s = String::new();
        push_field(&mut s, "-0.100000", false);
        assert_eq!(s, "-0.100000");
    }

    #[tokio::test]
    async fn test_export_api_key_scope_and_order() {
        let (tracker, dir) = tracker_with(vec![
            record(1, None, "2026-10-01T00:00:00Z"),
            record(2, None, "2026-10-02T00:00:00Z"),
            record(1, None, "2026-10-03T00:00:00Z"),
        ])
        .await;
        let csv = tracker.export_records_csv(
            ExportScope::ApiKey(1),
            ExportAudience::Admin,
            &HashMap::new(),
        );
        let rows = lines(&csv);
        assert_eq!(rows.len(), 3, "表头 + 该 Key 的 2 条");
        // 降序：10-03 在前（UTC 00:00 = CST 08:00）
        assert!(rows[0].starts_with("seq,time_cst,"));
        assert!(rows[1].starts_with("1,2026-10-03 08:00:00,"));
        assert!(rows[2].starts_with("2,2026-10-01 08:00:00,"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_export_credential_scope_includes_label() {
        let (tracker, dir) = tracker_with(vec![
            record(1, Some(7), "2026-10-01T00:00:00Z"),
            record(1, Some(8), "2026-10-01T00:00:01Z"),
            record(1, None, "2026-10-01T00:00:02Z"),
        ])
        .await;
        let emails = HashMap::from([(7u64, "a@example.com".to_string())]);
        let csv =
            tracker.export_records_csv(ExportScope::Credential(7), ExportAudience::Admin, &emails);
        let rows = lines(&csv);
        assert_eq!(rows.len(), 2);
        assert!(rows[1].contains(",7,a@example.com,"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 日期区间按 CST 归属日，两端含；不套用分页接口的截断上限
    #[tokio::test]
    async fn test_export_date_range_uses_cst_and_is_inclusive() {
        let (tracker, dir) = tracker_with(vec![
            record(1, None, "2026-10-01T18:00:00Z"), // CST 10-02 02:00（区间首日）
            record(2, None, "2026-10-03T18:00:00Z"), // CST 10-04 02:00（区间末日，其他 Key 也应包含）
            record(1, None, "2026-10-01T10:00:00Z"), // CST 10-01 18:00（区间外）
            record(1, None, "2026-10-04T18:00:00Z"), // CST 10-05（区间外）
        ])
        .await;
        let csv = tracker.export_records_csv(
            ExportScope::DateRange {
                from: "2026-10-02",
                to: "2026-10-04",
            },
            ExportAudience::Admin,
            &HashMap::new(),
        );
        let rows = lines(&csv);
        assert_eq!(rows.len(), 3);
        assert!(rows[1].starts_with("1,2026-10-04 02:00:00,"));
        assert!(rows[2].starts_with("2,2026-10-02 02:00:00,"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn test_export_empty_scope_has_only_header() {
        let (tracker, dir) = tracker_with(vec![]).await;
        let csv = tracker.export_records_csv(
            ExportScope::ApiKey(1),
            ExportAudience::Admin,
            &HashMap::new(),
        );
        let rows = lines(&csv);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].starts_with("seq,time_cst,api_key_id,"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 用户端导出不含账号标识、api_key_id 与失败原因，且表头与数据行列数一致
    #[tokio::test]
    async fn test_export_user_audience_hides_account_and_error() {
        let mut failed = record(1, Some(7), "2026-10-01T00:00:00Z");
        failed.outcome = Some("failed".to_string());
        failed.error_message = Some("upstream secret detail".to_string());
        failed.credits_used = None;
        failed.user_agent = None; // 默认 UA 含逗号会被引号包裹，影响下方按逗号数列数的断言
        let (tracker, dir) = tracker_with(vec![failed]).await;
        let emails = HashMap::from([(7u64, "a@example.com".to_string())]);

        let user =
            tracker.export_records_csv(ExportScope::ApiKey(1), ExportAudience::User, &emails);
        assert!(!user.contains("a@example.com"));
        assert!(!user.contains("upstream secret detail"));
        for h in [
            "api_key_id",
            "credential_id",
            "account",
            "email",
            "error_message",
        ] {
            assert!(
                !lines(&user)[0].split(',').any(|c| c == h),
                "用户端不应含 {h} 列"
            );
        }
        let user_rows = lines(&user);
        assert_eq!(
            user_rows[0].split(',').count(),
            user_rows[1].split(',').count(),
            "用户端表头与数据行列数应一致（本例数据行无需转义的逗号）"
        );

        let admin =
            tracker.export_records_csv(ExportScope::ApiKey(1), ExportAudience::Admin, &emails);
        assert!(admin.contains("a@example.com"));
        assert!(admin.contains("upstream secret detail"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 序号从 1 连续递增，末行序号等于导出总条数；邮箱缺失时该列留空
    #[tokio::test]
    async fn test_export_seq_is_consecutive_and_email_blank_when_unknown() {
        let (tracker, dir) = tracker_with(vec![
            record(1, Some(7), "2026-10-01T00:00:00Z"),
            record(1, Some(8), "2026-10-01T00:00:01Z"),
            record(1, Some(7), "2026-10-01T00:00:02Z"),
        ])
        .await;
        let emails = HashMap::from([(7u64, "a@example.com".to_string())]);
        let csv =
            tracker.export_records_csv(ExportScope::ApiKey(1), ExportAudience::Admin, &emails);
        let rows = lines(&csv);
        assert_eq!(rows.len(), 4);
        let header: Vec<&str> = rows[0].split(',').collect();
        let email_idx = header.iter().position(|h| *h == "email").unwrap();
        for (i, row) in rows[1..].iter().enumerate() {
            let cells: Vec<&str> = row.split(',').collect();
            assert_eq!(cells[0], (i + 1).to_string());
            // 时间降序：第 2 条是 credential 8（无邮箱）
            let expected = if i == 1 { "" } else { "a@example.com" };
            assert_eq!(cells[email_idx], expected);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 用户端同样带序号列
    #[tokio::test]
    async fn test_export_user_audience_has_seq() {
        let (tracker, dir) = tracker_with(vec![record(1, None, "2026-10-01T00:00:00Z")]).await;
        let csv = tracker.export_records_csv(
            ExportScope::ApiKey(1),
            ExportAudience::User,
            &HashMap::new(),
        );
        let rows = lines(&csv);
        assert!(rows[0].starts_with("seq,time_cst,model,"));
        assert!(rows[1].starts_with("1,"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 失败 / 取消记录的费用类列留空；完成记录 credits 缺失时按档位回退
    #[tokio::test]
    async fn test_export_cost_columns_blank_for_non_completed() {
        let mut aborted = record(1, None, "2026-10-01T00:00:00Z");
        aborted.outcome = Some("client_aborted".to_string());
        aborted.estimated_cost = 0.0;
        aborted.credits_used = None;
        let mut legacy = record(1, None, "2026-10-01T00:00:01Z");
        legacy.credits_used = None;
        legacy.estimated_cost = 1.0;
        let (tracker, dir) = tracker_with(vec![aborted, legacy]).await;
        let csv = tracker.export_records_csv(
            ExportScope::ApiKey(1),
            ExportAudience::User,
            &HashMap::new(),
        );
        let rows = lines(&csv);
        let header: Vec<&str> = rows[0].split(',').collect();
        let col = |row: &str, name: &str| -> String {
            let idx = header.iter().position(|h| *h == name).unwrap();
            row.split(',').nth(idx).unwrap().to_string()
        };
        // rows[1] 是时间更晚的 legacy 记录，rows[2] 是 aborted
        assert_eq!(col(rows[1], "result"), "completed");
        assert_eq!(
            col(rows[1], "credits"),
            format!("{:.6}", get_k_ref("claude-opus-4.6"))
        );
        assert_eq!(col(rows[1], "credits_saved"), "");
        assert_eq!(col(rows[2], "result"), "client_aborted");
        assert_eq!(col(rows[2], "cost_usd"), "");
        assert_eq!(col(rows[2], "credits"), "");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 来自客户端的 user_agent 以 = 开头时，导出文件里必须是不可执行的文本
    #[tokio::test]
    async fn test_export_neutralizes_formula_in_user_agent() {
        let mut r = record(1, None, "2026-10-01T00:00:00Z");
        r.user_agent = Some("=HYPERLINK(\"http://evil\")".to_string());
        let (tracker, dir) = tracker_with(vec![r]).await;
        let csv = tracker.export_records_csv(
            ExportScope::ApiKey(1),
            ExportAudience::Admin,
            &HashMap::new(),
        );
        assert!(csv.contains("\"'=HYPERLINK(\"\"http://evil\"\")\""));
        let _ = std::fs::remove_dir_all(dir);
    }
}
