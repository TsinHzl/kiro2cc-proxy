// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin UI 界面偏好持久化存储
//!
//! 保存在配置目录的 `ui_preferences.json` 中，用于在服务端本地持久化
//! 侧边栏折叠等 UI 状态，使同一部署实例跨设备访问时保持一致。

use std::fs;
use std::path::{Path, PathBuf};

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use crate::common::fs::atomic_write;

fn default_true() -> bool {
    true
}

/// Admin UI 界面偏好（落盘格式与 API 响应一致）
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UiPreferences {
    /// Admin 主侧边栏是否收起（默认 false，即展开）
    #[serde(default)]
    pub admin_sidebar_collapsed: bool,
    /// 对话页历史会话侧边栏是否展开（默认 true，即展开）
    #[serde(default = "default_true")]
    pub chat_sidebar_open: bool,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            admin_sidebar_collapsed: false,
            chat_sidebar_open: true,
        }
    }
}

/// 线程安全的 UI 偏好持久化存储
pub struct UiPreferencesStore {
    path: PathBuf,
    /// 内存快照锁：仅在纯内存读写瞬间持有，不在磁盘 I/O 期间持锁
    state: RwLock<UiPreferences>,
    /// 串行化并发更新与原子写盘，避免交错写入互相覆盖
    write_lock: Mutex<()>,
}

impl UiPreferencesStore {
    /// 从磁盘加载偏好文件；文件不存在或内容损坏时回退默认值，不阻止服务启动
    pub fn load(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let prefs = match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<UiPreferences>(&content) {
                Ok(parsed) => parsed,
                Err(e) => {
                    tracing::warn!(
                        "解析 UI 偏好文件 {} 失败，已回退默认值: {}",
                        path.display(),
                        e
                    );
                    UiPreferences::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => UiPreferences::default(),
            Err(e) => {
                tracing::warn!(
                    "读取 UI 偏好文件 {} 失败，已回退默认值: {}",
                    path.display(),
                    e
                );
                UiPreferences::default()
            }
        };
        Self {
            path,
            state: RwLock::new(prefs),
            write_lock: Mutex::new(()),
        }
    }

    /// 读取当前偏好快照
    pub fn get(&self) -> UiPreferences {
        *self.state.read()
    }

    /// 局部更新并原子写盘（调用方应在阻塞线程池中执行）
    pub fn update(
        &self,
        admin_sidebar_collapsed: Option<bool>,
        chat_sidebar_open: Option<bool>,
    ) -> anyhow::Result<UiPreferences> {
        if admin_sidebar_collapsed.is_none() && chat_sidebar_open.is_none() {
            return Ok(self.get());
        }

        let _write_guard = self.write_lock.lock();
        let current = *self.state.read();
        let mut next = current;
        if let Some(v) = admin_sidebar_collapsed {
            next.admin_sidebar_collapsed = v;
        }
        if let Some(v) = chat_sidebar_open {
            next.chat_sidebar_open = v;
        }
        if next == current && self.path.exists() {
            return Ok(next);
        }

        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&next)?;
        atomic_write(&self.path, &bytes)?;
        *self.state.write() = next;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_prefs_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("kiro-ui-prefs-{}-{}", name, uuid::Uuid::new_v4()));
        dir.join("ui_preferences.json")
    }

    #[test]
    fn load_returns_defaults_when_missing() {
        let path = temp_prefs_path("missing");
        let store = UiPreferencesStore::load(&path);
        assert_eq!(store.get(), UiPreferences::default());
        assert!(!store.get().admin_sidebar_collapsed);
        assert!(store.get().chat_sidebar_open);
    }

    #[test]
    fn update_persists_partial_fields_across_reload() {
        let path = temp_prefs_path("persist");
        let store = UiPreferencesStore::load(&path);

        let updated = store.update(Some(true), None).unwrap();
        assert!(updated.admin_sidebar_collapsed);
        assert!(updated.chat_sidebar_open);

        let updated = store.update(None, Some(false)).unwrap();
        assert!(updated.admin_sidebar_collapsed);
        assert!(!updated.chat_sidebar_open);

        let reloaded = UiPreferencesStore::load(&path);
        assert_eq!(reloaded.get(), updated);

        if let Some(parent) = path.parent() {
            fs::remove_dir_all(parent).ok();
        }
    }

    #[test]
    fn load_falls_back_to_defaults_on_corrupted_json() {
        let path = temp_prefs_path("corrupt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not-valid-json").unwrap();

        let store = UiPreferencesStore::load(&path);
        assert_eq!(store.get(), UiPreferences::default());

        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn empty_update_returns_snapshot_without_creating_file() {
        let path = temp_prefs_path("empty-update");
        let store = UiPreferencesStore::load(&path);
        let res = store.update(None, None).unwrap();
        assert_eq!(res, UiPreferences::default());
        assert!(!path.exists());
    }
}
