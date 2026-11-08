// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 模型「首次出现」日期自检
//!
//! 上游 `ListAvailableModels` 不提供发布时间字段。这里改为由服务自己记录：
//! 每次成功拉到实时模型列表，就把「之前没见过的模型 ID」连同当天日期写入
//! `kiro_model_seen.json`（与 credentials.json 同目录）。后续新增的模型因此无需改代码
//! 即可带上 `released_at`，前端据此在窗口期内展示 NEW 徽章。
//!
//! 「是否最近新增」仍由前端按当前时间实时判定（避免页面长时间停留后徽章不消失），
//! 窗口天数见 `web-ui/admin-ui/src/components/model-list-page.tsx` 的 `RECENT_WINDOW_DAYS`。
//!
//! # 首次运行的基线
//! 记录文件不存在（首次升级到本版本，或文件被删/损坏）时，当前已有的全部模型只登记为
//! 「基线」（日期为 `null`），不算新增，否则升级当天整张表都会亮 NEW。
//! 基线之后才出现的模型才记当天日期。
//!
//! # 手动覆盖
//! `MODEL_RELEASE_DATES` 仍保留，用于：升级前已手动登记、需要保持徽章的模型，
//! 或想把日期改成实际上线日。覆盖优先于自检记录。
//!
//! # 局限
//! 记录的是「本代理第一次看到该模型的日期」，不是官方发布日期；
//! 新增账号若带来了其他账号没有的模型，也会被当作新增。

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::NaiveDate;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

/// 记录文件名（位于 credentials.json 同目录）
pub(crate) const SEEN_FILE_NAME: &str = "kiro_model_seen.json";

/// 手动覆盖的发布日期（模型 ID → `YYYY-MM-DD`），优先于自检记录
///
/// 日期取该模型在本代理中「可调用」的日期（上线日），非官方发布日期。
/// 一般无需再追加；新增模型由自检自动记录。
const MODEL_RELEASE_DATES: &[(&str, &str)] = &[
    ("claude-opus-5.5", "2026-10-12"),
    ("claude-sonnet-5.5", "2026-10-12"),
    ("claude-haiku-5.5", "2026-11-08"),
    ("kimi-k3", "2026-11-08"),
];

/// 查询手动覆盖的发布日期（未登记返回 `None`）
fn override_date(model_id: &str) -> Option<&'static str> {
    MODEL_RELEASE_DATES
        .iter()
        .find(|(id, _)| *id == model_id)
        .map(|(_, date)| *date)
}

/// 持久化内容：模型 ID → 首次出现日期（`None` 表示基线，不算新增）
#[derive(Debug, Default, Serialize, Deserialize)]
struct SeenState {
    models: BTreeMap<String, Option<String>>,
}

impl SeenState {
    /// 登记本次看到的模型，返回是否有变化（需要落盘）
    ///
    /// `baseline` 为 true 时，新模型登记为 `None`（不算新增）。
    fn observe(&mut self, ids: &[&str], today: NaiveDate, baseline: bool) -> bool {
        let today = today.format("%Y-%m-%d").to_string();
        let mut changed = false;
        for id in ids {
            if id.is_empty() || self.models.contains_key(*id) {
                continue;
            }
            let first_seen = if baseline { None } else { Some(today.clone()) };
            self.models.insert((*id).to_string(), first_seen);
            changed = true;
        }
        changed
    }
}

/// 模型发布日期追踪器
pub(crate) struct ModelReleaseTracker {
    path: Option<PathBuf>,
    /// `None` 表示尚未建立基线
    state: Mutex<Option<SeenState>>,
}

impl ModelReleaseTracker {
    /// 从 `path` 加载记录；文件不存在或损坏时视为尚未建立基线
    pub(crate) fn load(path: Option<PathBuf>) -> Self {
        let state = path.as_ref().and_then(|p| {
            let content = std::fs::read_to_string(p).ok()?;
            match serde_json::from_str::<SeenState>(&content) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::warn!("解析模型首次出现记录失败，将重新建立基线: {}", e);
                    None
                }
            }
        });
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    /// 仅内存、不落盘（测试用）
    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        Self::load(None)
    }

    /// 登记一次成功拉取到的实时模型列表
    ///
    /// 只应在上游实时响应成功时调用；列表为空时忽略，避免异常空响应建立出空基线，
    /// 导致之后所有模型都被当成新增。
    pub(crate) fn observe(&self, ids: &[&str]) {
        self.observe_on(ids, chrono::Utc::now().date_naive());
    }

    fn observe_on(&self, ids: &[&str], today: NaiveDate) {
        if ids.is_empty() {
            return;
        }
        let mut guard = self.state.lock();
        let baseline = guard.is_none();
        let state = guard.get_or_insert_with(SeenState::default);
        // 建立基线时即使没有任何新条目也要落盘（ids 非空，必有变化）
        if state.observe(ids, today, baseline) {
            self.save(state);
        }
    }

    /// 查询模型发布日期：手动覆盖优先，其次自检记录；基线或未见过返回 `None`
    pub(crate) fn release_date(&self, model_id: &str) -> Option<String> {
        if let Some(date) = override_date(model_id) {
            return Some(date.to_string());
        }
        self.state
            .lock()
            .as_ref()
            .and_then(|s| s.models.get(model_id))
            .and_then(|d| d.clone())
    }

    /// 先写临时文件再重命名，避免写到一半崩溃留下损坏的记录
    fn save(&self, state: &SeenState) {
        let Some(path) = &self.path else { return };
        let json = match serde_json::to_string_pretty(state) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("序列化模型首次出现记录失败: {}", e);
                return;
            }
        };
        let tmp = path.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, path)) {
            tracing::warn!("保存模型首次出现记录失败: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn override_date_returns_registered_date() {
        assert_eq!(override_date("claude-opus-5.5"), Some("2026-10-12"));
        assert_eq!(override_date("claude-sonnet-5.5"), Some("2026-10-12"));
        assert_eq!(override_date("claude-haiku-5.5"), Some("2026-11-08"));
        assert_eq!(override_date("kimi-k3"), Some("2026-11-08"));
        assert_eq!(override_date("claude-opus-5"), None);
        assert_eq!(override_date(""), None);
    }

    #[test]
    fn all_overrides_use_iso_date_format() {
        // 前端 Date.parse 依赖 `YYYY-MM-DD`，格式错误会静默失效
        for (id, date) in MODEL_RELEASE_DATES {
            assert_eq!(date.len(), 10, "模型 {id} 的日期格式非法: {date}");
            assert!(
                NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok(),
                "模型 {id} 的日期无法解析: {date}"
            );
        }
    }

    #[test]
    fn first_observation_is_baseline_not_new() {
        let t = ModelReleaseTracker::in_memory();
        t.observe_on(&["a", "b"], day("2026-10-13"));
        assert_eq!(t.release_date("a"), None);
        assert_eq!(t.release_date("b"), None);
    }

    #[test]
    fn model_appearing_after_baseline_gets_first_seen_date() {
        let t = ModelReleaseTracker::in_memory();
        t.observe_on(&["a", "b"], day("2026-10-13"));
        t.observe_on(&["a", "b", "c"], day("2026-10-20"));
        assert_eq!(t.release_date("a"), None);
        assert_eq!(t.release_date("c").as_deref(), Some("2026-10-20"));
    }

    #[test]
    fn first_seen_date_is_not_overwritten() {
        let t = ModelReleaseTracker::in_memory();
        t.observe_on(&["a"], day("2026-10-13"));
        t.observe_on(&["a", "c"], day("2026-10-20"));
        t.observe_on(&["a", "c"], day("2026-11-30"));
        assert_eq!(t.release_date("c").as_deref(), Some("2026-10-20"));
    }

    #[test]
    fn empty_list_does_not_establish_baseline() {
        let t = ModelReleaseTracker::in_memory();
        t.observe_on(&[], day("2026-10-13"));
        // 空响应之后的第一次真实列表仍应作为基线，而不是全部算新增
        t.observe_on(&["a"], day("2026-10-14"));
        assert_eq!(t.release_date("a"), None);
    }

    #[test]
    fn override_takes_priority_over_recorded_baseline() {
        let t = ModelReleaseTracker::in_memory();
        t.observe_on(&["claude-opus-5.5"], day("2026-10-13"));
        assert_eq!(
            t.release_date("claude-opus-5.5").as_deref(),
            Some("2026-10-12")
        );
    }

    #[test]
    fn persists_and_reloads_across_restarts() {
        let dir = std::env::temp_dir().join(format!("kiro_model_seen_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SEEN_FILE_NAME);
        let _ = std::fs::remove_file(&path);

        let t = ModelReleaseTracker::load(Some(path.clone()));
        t.observe_on(&["a"], day("2026-10-13"));
        t.observe_on(&["a", "c"], day("2026-10-20"));

        // 重启后：不再重建基线，c 的日期保留，新出现的 d 记当天
        let t2 = ModelReleaseTracker::load(Some(path.clone()));
        assert_eq!(t2.release_date("c").as_deref(), Some("2026-10-20"));
        t2.observe_on(&["a", "c", "d"], day("2026-10-25"));
        assert_eq!(t2.release_date("a"), None);
        assert_eq!(t2.release_date("d").as_deref(), Some("2026-10-25"));

        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupted_file_falls_back_to_new_baseline() {
        let dir = std::env::temp_dir().join(format!("kiro_model_seen_bad_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SEEN_FILE_NAME);
        std::fs::write(&path, "not json").unwrap();

        let t = ModelReleaseTracker::load(Some(path));
        t.observe_on(&["a"], day("2026-10-13"));
        assert_eq!(t.release_date("a"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
