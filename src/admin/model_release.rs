// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 模型「新增」自检
//!
//! 记录文件 `kiro_model_seen.json`（与 credentials.json 同目录）保存「已见过的模型 ID」
//! 及其登记日期。每次成功拉到实时模型列表时：
//! - 文件中已有的模型保持不变；
//! - 文件中没有的模型，以当天日期写入文件，前端在窗口期内展示 NEW 徽章。
//!
//! 「是否仍在窗口期内」由前端按当前时间实时判定（避免页面长时间停留后徽章不消失），
//! 窗口天数见 `web-ui/admin-ui/src/components/model-list-page.tsx` 的 `RECENT_WINDOW_DAYS`。
//!
//! # 首次建立记录
//! 记录文件不存在（首次升级，或文件被删/损坏）时，当前列表中的模型登记为「基线」（日期为 `null`，不算新增），
//! 否则升级当天整张表都会亮 NEW。`ALWAYS_NEW_IDS` 中的模型不参与基线。
//!
//! # 强制新增
//! `ALWAYS_NEW_IDS` 中的模型不进入基线：无论何时首次出现，都以登记当天的日期写入，从而展示 NEW。
//! 升级前旧记录里这些模型的条目会在加载时被移除一次（由记录格式版本号 `SEEN_VERSION` 控制），
//! 之后按实际拉取日期重新登记。
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

/// 当前记录格式版本。缺失版本号的旧文件视为 0，加载时执行一次迁移
const SEEN_VERSION: u32 = 3;

/// 强制按首次出现日期登记 NEW 的模型，不参与基线
const ALWAYS_NEW_IDS: &[&str] = &[
    "claude-sonnet-5.5",
    "claude-opus-5.5",
    "claude-haiku-5.5",
    "kimi-k3",
    "grok-4.7",
];

/// 持久化内容：模型 ID → 登记日期（`None` 表示基线，不算新增）
#[derive(Debug, Serialize, Deserialize)]
struct SeenState {
    /// 记录格式版本；缺失即旧格式（0）
    #[serde(default)]
    version: u32,
    models: BTreeMap<String, Option<String>>,
}

impl SeenState {
    /// 新建的当前版本记录（尚无任何模型）
    fn fresh() -> Self {
        Self {
            version: SEEN_VERSION,
            models: BTreeMap::new(),
        }
    }

    /// 旧版本记录迁移：移除 `ALWAYS_NEW_IDS` 的旧条目，使其按当前拉取日期重新登记。
    /// 返回是否有变化（需要落盘）
    fn migrate(&mut self) -> bool {
        if self.version >= SEEN_VERSION {
            return false;
        }
        for id in ALWAYS_NEW_IDS {
            self.models.remove(*id);
        }
        self.version = SEEN_VERSION;
        true
    }

    /// 登记本次看到的模型，返回是否有变化（需要落盘）
    ///
    /// `baseline` 为 true 时，新模型登记为 `None`（不算新增），`ALWAYS_NEW_IDS` 除外。
    fn observe(&mut self, ids: &[&str], today: NaiveDate, baseline: bool) -> bool {
        let today = today.format("%Y-%m-%d").to_string();
        let mut changed = false;
        for id in ids {
            if id.is_empty() || self.models.contains_key(*id) {
                continue;
            }
            let always_new = ALWAYS_NEW_IDS.contains(id);
            let first_seen = if baseline && !always_new {
                None
            } else {
                Some(today.clone())
            };
            self.models.insert((*id).to_string(), first_seen);
            changed = true;
        }
        changed
    }
}

/// 模型新增日期追踪器
pub(crate) struct ModelReleaseTracker {
    path: Option<PathBuf>,
    /// `None` 表示尚未建立基线
    state: Mutex<Option<SeenState>>,
}

impl ModelReleaseTracker {
    /// 从 `path` 加载记录；文件不存在或损坏时视为尚未建立基线。
    /// 旧格式记录会在此完成一次迁移并立即落盘
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
        let tracker = Self {
            path,
            state: Mutex::new(state),
        };
        {
            let mut guard = tracker.state.lock();
            if let Some(state) = guard.as_mut()
                && state.migrate()
            {
                tracker.save(state);
            }
        }
        tracker
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
        let state = guard.get_or_insert_with(SeenState::fresh);
        // 建立基线时即使没有任何新条目也要落盘（ids 非空，必有变化）
        if state.observe(ids, today, baseline) {
            self.save(state);
        }
    }

    /// 查询模型登记日期（未见过或属于基线返回 `None`）
    pub(crate) fn release_date(&self, model_id: &str) -> Option<String> {
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

    fn temp_path(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("kiro_model_seen_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(SEEN_FILE_NAME)
    }

    #[test]
    fn first_observation_is_baseline_not_new() {
        let t = ModelReleaseTracker::in_memory();
        t.observe_on(&["a", "b"], day("2026-10-13"));
        assert_eq!(t.release_date("a"), None);
        assert_eq!(t.release_date("b"), None);
    }

    #[test]
    fn always_new_models_get_date_even_on_baseline() {
        let t = ModelReleaseTracker::in_memory();
        t.observe_on(
            &["a", "kimi-k3", "claude-haiku-5.5", "grok-4.7"],
            day("2026-11-08"),
        );
        assert_eq!(t.release_date("a"), None);
        assert_eq!(t.release_date("grok-4.7").as_deref(), Some("2026-11-08"));
        assert_eq!(t.release_date("kimi-k3").as_deref(), Some("2026-11-08"));
        assert_eq!(
            t.release_date("claude-haiku-5.5").as_deref(),
            Some("2026-11-08")
        );
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
        t.observe_on(&["a", "c", "kimi-k3"], day("2026-10-20"));
        t.observe_on(&["a", "c", "kimi-k3"], day("2026-11-30"));
        assert_eq!(t.release_date("c").as_deref(), Some("2026-10-20"));
        assert_eq!(t.release_date("kimi-k3").as_deref(), Some("2026-10-20"));
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
    fn persists_and_reloads_across_restarts() {
        let path = temp_path("persist");

        let t = ModelReleaseTracker::load(Some(path.clone()));
        t.observe_on(&["a"], day("2026-10-13"));
        t.observe_on(&["a", "c"], day("2026-10-20"));
        t.observe_on(&["a", "c", "kimi-k3"], day("2026-10-21"));

        // 重启后：不再重建基线，c 与 kimi-k3 的日期保留，新出现的 d 记当天
        let t2 = ModelReleaseTracker::load(Some(path.clone()));
        assert_eq!(t2.release_date("c").as_deref(), Some("2026-10-20"));
        assert_eq!(t2.release_date("kimi-k3").as_deref(), Some("2026-10-21"));
        t2.observe_on(&["a", "c", "d", "kimi-k3"], day("2026-10-25"));
        assert_eq!(t2.release_date("a"), None);
        assert_eq!(t2.release_date("d").as_deref(), Some("2026-10-25"));

        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn legacy_file_is_migrated_once_and_always_new_entries_are_redated() {
        let path = temp_path("migrate");
        // 旧格式：无 version 字段，kimi-k3 / haiku 带旧日期，sonnet 为基线
        std::fs::write(
            &path,
            r#"{"models":{"a":null,"kimi-k3":"2026-10-16","claude-haiku-5.5":"2026-10-24","claude-sonnet-5.5":null}}"#,
        )
        .unwrap();

        let t = ModelReleaseTracker::load(Some(path.clone()));
        assert_eq!(t.release_date("a"), None);
        assert_eq!(t.release_date("kimi-k3"), None);
        assert_eq!(t.release_date("claude-haiku-5.5"), None);
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["version"], SEEN_VERSION);
        assert!(saved["models"].get("kimi-k3").is_none());
        assert!(saved["models"].get("claude-sonnet-5.5").is_none());

        // 迁移后按拉取日期登记，且重启不会再次清空
        t.observe_on(&["a", "kimi-k3", "claude-haiku-5.5"], day("2026-11-08"));
        let t2 = ModelReleaseTracker::load(Some(path.clone()));
        assert_eq!(t2.release_date("kimi-k3").as_deref(), Some("2026-11-08"));
        assert_eq!(
            t2.release_date("claude-haiku-5.5").as_deref(),
            Some("2026-11-08")
        );
        assert_eq!(t2.release_date("a"), None);

        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn corrupted_file_falls_back_to_new_baseline() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "not json").unwrap();

        let t = ModelReleaseTracker::load(Some(path.clone()));
        t.observe_on(&["a"], day("2026-10-13"));
        assert_eq!(t.release_date("a"), None);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
