// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin「对话」页的会话存储
//!
//! 每个会话一个文件：`<dir>/<uuid>.json`，用原子写入落盘。会话之间互不影响：
//! 保存一个会话不会重写其他会话，单个文件损坏也只影响它自己。
//! 内存里只保留元数据索引（标题、时间、消息条数），消息正文按需从磁盘读取。
//!
//! 消息内容对后端是不透明的 JSON（由前端定义结构），这里只校验它是对象数组。

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::common::fs::atomic_write;

/// 标题最多保留的字符数
pub const MAX_TITLE_CHARS: usize = 120;
/// 单个会话最多保留的消息条数
pub const MAX_MESSAGES: usize = 5000;
/// 最多保留的会话数
pub const MAX_SESSIONS: usize = 1000;

/// 完整会话（磁盘格式，同时是 GET 单个会话的响应）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    /// 创建时间（Unix 毫秒）
    pub created_at: i64,
    /// 最近一次保存时间（Unix 毫秒）
    pub updated_at: i64,
    pub messages: Vec<Value>,
}

/// 会话元数据（列表项）
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: usize,
}

/// 启动扫描用：只解析元数据字段，消息数组用 IgnoredAny 跳过，避免为每个会话分配完整消息树
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionHeader {
    id: String,
    title: String,
    created_at: i64,
    updated_at: i64,
    messages: Vec<IgnoredAny>,
}

#[derive(Debug)]
pub enum ChatSessionError {
    NotFound,
    /// 请求内容不合法（id 格式、消息结构、数量超限等）
    Invalid(String),
    Io(String),
}

impl fmt::Display for ChatSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "会话不存在"),
            Self::Invalid(msg) => write!(f, "{}", msg),
            Self::Io(msg) => write!(f, "会话读写失败: {}", msg),
        }
    }
}

impl std::error::Error for ChatSessionError {}

pub struct ChatSessionStore {
    dir: PathBuf,
    /// 元数据索引。同时充当写锁：持锁期间完成「写文件 + 更新索引」，保证两者一致
    index: Mutex<HashMap<String, ChatSessionMeta>>,
}

/// 只接受标准小写带连字符的 UUID，既是校验也是防路径穿越
fn valid_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id)
        .map(|u| u.hyphenated().to_string() == id)
        .unwrap_or(false)
}

fn normalize_title(title: &str) -> String {
    title.trim().chars().take(MAX_TITLE_CHARS).collect()
}

fn validate_messages(messages: &[Value]) -> Result<(), ChatSessionError> {
    if messages.len() > MAX_MESSAGES {
        return Err(ChatSessionError::Invalid(format!(
            "单个会话最多保存 {} 条消息",
            MAX_MESSAGES
        )));
    }
    if messages.iter().any(|m| !m.is_object()) {
        return Err(ChatSessionError::Invalid("messages 必须是对象数组".into()));
    }
    Ok(())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl ChatSessionStore {
    /// 加载目录（不存在则创建），扫描已有会话建立索引。无法解析的文件跳过并告警，不阻止启动
    pub fn load(dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;

        let mut index = HashMap::new();
        for entry in fs::read_dir(&dir)? {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("读取对话会话目录条目失败，已跳过: {}", e);
                    continue;
                }
            };
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let stem_ok = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(valid_id)
                .unwrap_or(false);
            if !stem_ok {
                continue;
            }
            let header = fs::read(&path)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| {
                    serde_json::from_slice::<SessionHeader>(&bytes).map_err(Into::into)
                });
            match header {
                Ok(h) if path.file_stem().and_then(|s| s.to_str()) == Some(h.id.as_str()) => {
                    index.insert(
                        h.id.clone(),
                        ChatSessionMeta {
                            id: h.id,
                            title: h.title,
                            created_at: h.created_at,
                            updated_at: h.updated_at,
                            message_count: h.messages.len(),
                        },
                    );
                }
                Ok(_) => {
                    tracing::warn!("对话会话文件 id 与文件名不一致，已跳过: {}", path.display())
                }
                Err(e) => tracing::warn!("对话会话文件解析失败，已跳过 {}: {}", path.display(), e),
            }
        }
        tracing::info!("已加载 {} 个对话会话", index.len());

        Ok(Self {
            dir,
            index: Mutex::new(index),
        })
    }

    fn path_of(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.json", id))
    }

    fn write_session(&self, session: &ChatSession) -> Result<(), ChatSessionError> {
        let bytes = serde_json::to_vec(session).map_err(|e| ChatSessionError::Io(e.to_string()))?;
        atomic_write(&self.path_of(&session.id), &bytes)
            .map_err(|e| ChatSessionError::Io(e.to_string()))
    }

    /// 按最近更新时间倒序
    pub fn list(&self) -> Vec<ChatSessionMeta> {
        let mut items: Vec<_> = self.index.lock().values().cloned().collect();
        items.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        items
    }

    pub fn get(&self, id: &str) -> Result<ChatSession, ChatSessionError> {
        if !valid_id(id) {
            return Err(ChatSessionError::NotFound);
        }
        if !self.index.lock().contains_key(id) {
            return Err(ChatSessionError::NotFound);
        }
        let bytes = match fs::read(self.path_of(id)) {
            Ok(bytes) => bytes,
            // 与 delete/clear 对齐：文件已被外部删除按「不存在」处理，并清掉索引里的残留项
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.index.lock().remove(id);
                return Err(ChatSessionError::NotFound);
            }
            Err(e) => return Err(ChatSessionError::Io(e.to_string())),
        };
        serde_json::from_slice(&bytes).map_err(|e| ChatSessionError::Io(e.to_string()))
    }

    pub fn create(
        &self,
        title: &str,
        messages: Vec<Value>,
    ) -> Result<ChatSessionMeta, ChatSessionError> {
        validate_messages(&messages)?;
        let mut index = self.index.lock();
        if index.len() >= MAX_SESSIONS {
            return Err(ChatSessionError::Invalid(format!(
                "最多保存 {} 个会话，请先删除部分历史会话",
                MAX_SESSIONS
            )));
        }
        let now = now_ms();
        let session = ChatSession {
            id: uuid::Uuid::new_v4().hyphenated().to_string(),
            title: normalize_title(title),
            created_at: now,
            updated_at: now,
            messages,
        };
        self.write_session(&session)?;
        let meta = meta_of(&session);
        index.insert(session.id.clone(), meta.clone());
        Ok(meta)
    }

    /// 整体替换消息；`title` 为 None 时保留原标题
    pub fn update(
        &self,
        id: &str,
        title: Option<&str>,
        messages: Vec<Value>,
    ) -> Result<ChatSessionMeta, ChatSessionError> {
        if !valid_id(id) {
            return Err(ChatSessionError::NotFound);
        }
        validate_messages(&messages)?;
        let mut index = self.index.lock();
        let existing = index.get(id).ok_or(ChatSessionError::NotFound)?;
        let session = ChatSession {
            id: id.to_string(),
            title: title
                .map(normalize_title)
                .unwrap_or_else(|| existing.title.clone()),
            created_at: existing.created_at,
            updated_at: now_ms(),
            messages,
        };
        self.write_session(&session)?;
        let meta = meta_of(&session);
        index.insert(session.id.clone(), meta.clone());
        Ok(meta)
    }

    pub fn delete(&self, id: &str) -> Result<(), ChatSessionError> {
        if !valid_id(id) {
            return Err(ChatSessionError::NotFound);
        }
        let mut index = self.index.lock();
        if !index.contains_key(id) {
            return Err(ChatSessionError::NotFound);
        }
        match fs::remove_file(self.path_of(id)) {
            Ok(()) => {}
            // 文件已被外部删除：索引里的残留项一并清掉即可
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(ChatSessionError::Io(e.to_string())),
        }
        index.remove(id);
        Ok(())
    }

    /// 删除全部会话，返回删除的个数。中途失败时已删除的保持删除，其余仍在索引中
    pub fn clear(&self) -> Result<usize, ChatSessionError> {
        let mut index = self.index.lock();
        let ids: Vec<String> = index.keys().cloned().collect();
        let mut removed = 0;
        for id in ids {
            match fs::remove_file(self.path_of(&id)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(ChatSessionError::Io(e.to_string())),
            }
            index.remove(&id);
            removed += 1;
        }
        Ok(removed)
    }
}

fn meta_of(s: &ChatSession) -> ChatSessionMeta {
    ChatSessionMeta {
        id: s.id.clone(),
        title: s.title.clone(),
        created_at: s.created_at,
        updated_at: s.updated_at,
        message_count: s.messages.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let dir =
                std::env::temp_dir().join(format!("chat-session-test-{}", uuid::Uuid::new_v4()));
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn msgs(n: usize) -> Vec<Value> {
        (0..n)
            .map(|i| json!({ "id": i.to_string(), "role": "user", "content": "hi" }))
            .collect()
    }

    #[test]
    fn create_get_update_delete_roundtrip() {
        let tmp = TempDir::new();
        let store = ChatSessionStore::load(&tmp.0).unwrap();

        let meta = store.create("  你好  ", msgs(2)).unwrap();
        assert_eq!(meta.title, "你好");
        assert_eq!(meta.message_count, 2);

        let got = store.get(&meta.id).unwrap();
        assert_eq!(got.messages.len(), 2);

        let updated = store.update(&meta.id, None, msgs(4)).unwrap();
        assert_eq!(updated.title, "你好", "title 为 None 时应保留原标题");
        assert_eq!(updated.message_count, 4);
        assert_eq!(updated.created_at, meta.created_at);
        assert!(updated.updated_at >= meta.updated_at);

        let renamed = store.update(&meta.id, Some("新标题"), msgs(4)).unwrap();
        assert_eq!(renamed.title, "新标题");

        store.delete(&meta.id).unwrap();
        assert!(matches!(
            store.get(&meta.id),
            Err(ChatSessionError::NotFound)
        ));
        assert!(matches!(
            store.delete(&meta.id),
            Err(ChatSessionError::NotFound)
        ));
        assert!(store.list().is_empty());
    }

    #[test]
    fn get_after_external_file_removal_is_not_found_and_drops_index_entry() {
        let tmp = TempDir::new();
        let store = ChatSessionStore::load(&tmp.0).unwrap();
        let meta = store.create("a", msgs(1)).unwrap();

        fs::remove_file(tmp.0.join(format!("{}.json", meta.id))).unwrap();

        assert!(matches!(
            store.get(&meta.id),
            Err(ChatSessionError::NotFound)
        ));
        assert!(store.list().is_empty(), "残留的索引项应被清掉");
    }

    #[test]
    fn reload_restores_index_from_disk() {
        let tmp = TempDir::new();
        let id = {
            let store = ChatSessionStore::load(&tmp.0).unwrap();
            store.create("a", msgs(3)).unwrap().id
        };
        let store = ChatSessionStore::load(&tmp.0).unwrap();
        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].message_count, 3);
    }

    #[test]
    fn load_skips_corrupt_and_foreign_files() {
        let tmp = TempDir::new();
        fs::create_dir_all(&tmp.0).unwrap();
        let bad = uuid::Uuid::new_v4().hyphenated().to_string();
        fs::write(tmp.0.join(format!("{}.json", bad)), b"{not json").unwrap();
        fs::write(tmp.0.join("notes.json"), b"{}").unwrap();
        fs::write(tmp.0.join("x.tmp.123"), b"junk").unwrap();

        let store = ChatSessionStore::load(&tmp.0).unwrap();
        assert!(store.list().is_empty());
    }

    #[test]
    fn rejects_invalid_ids_and_path_traversal() {
        let tmp = TempDir::new();
        let store = ChatSessionStore::load(&tmp.0).unwrap();
        for id in [
            "../config",
            "..%2Fconfig",
            "",
            "abc",
            "A0EEBC99-9C0B-4EF8-BB6D-6BB9BD380A11",
        ] {
            assert!(
                matches!(store.get(id), Err(ChatSessionError::NotFound)),
                "{id}"
            );
            assert!(
                matches!(store.delete(id), Err(ChatSessionError::NotFound)),
                "{id}"
            );
            assert!(
                matches!(
                    store.update(id, None, vec![]),
                    Err(ChatSessionError::NotFound)
                ),
                "{id}"
            );
        }
    }

    #[test]
    fn rejects_bad_messages() {
        let tmp = TempDir::new();
        let store = ChatSessionStore::load(&tmp.0).unwrap();
        assert!(matches!(
            store.create("t", vec![json!("not an object")]),
            Err(ChatSessionError::Invalid(_))
        ));
        assert!(matches!(
            store.create("t", msgs(MAX_MESSAGES + 1)),
            Err(ChatSessionError::Invalid(_))
        ));
    }

    #[test]
    fn title_is_truncated_by_chars() {
        let tmp = TempDir::new();
        let store = ChatSessionStore::load(&tmp.0).unwrap();
        let long = "你".repeat(MAX_TITLE_CHARS + 50);
        let meta = store.create(&long, vec![]).unwrap();
        assert_eq!(meta.title.chars().count(), MAX_TITLE_CHARS);
    }

    #[test]
    fn list_is_sorted_by_updated_desc_and_clear_removes_all() {
        let tmp = TempDir::new();
        let store = ChatSessionStore::load(&tmp.0).unwrap();
        let a = store.create("a", vec![]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = store.create("b", vec![]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        store.update(&a.id, None, msgs(1)).unwrap();

        let ids: Vec<_> = store.list().into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![a.id.clone(), b.id.clone()]);

        assert_eq!(store.clear().unwrap(), 2);
        assert!(store.list().is_empty());
        assert!(!store.path_of(&a.id).exists());
    }
}
