// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 限流事件日志模块
//!
//! 记录每个 credential 收到 429 响应的事件详情，持久化到 `throttle_log.json`。

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 单条限流事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThrottleEvent {
    pub credential_id: u64,
    /// "api" 或 "mcp"
    pub request_type: String,
    pub status_code: u16,
    /// 响应体摘要（截取前 200 字符）
    pub response_body: String,
    pub created_at: DateTime<Utc>,
}

/// 每个 credential 的最大限流记录数
const MAX_EVENTS_PER_CREDENTIAL: usize = 500;

/// 限流日志存储
pub struct ThrottleLogStore {
    events: Arc<RwLock<Vec<ThrottleEvent>>>,

    dirty_tx: Option<mpsc::UnboundedSender<()>>,
}

impl ThrottleLogStore {
    /// 创建空的 store（用于加载失败时降级）
    pub fn empty<P: AsRef<Path>>(_path: P) -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),

            dirty_tx: None,
        }
    }

    /// 从文件加载，文件不存在则创建空列表
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let events = if path.exists() {
            let content = fs::read_to_string(&path)?;
            if content.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&content)?
            }
        } else {
            Vec::new()
        };
        let events = Arc::new(RwLock::new(events));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let events_clone = events.clone();
        let path_clone = path.clone();

        tokio::spawn(async move {
            let mut dirty = false;
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                tokio::select! {
                    res = rx.recv() => {
                        match res {
                            Some(_) => dirty = true,
                            None => {
                                if dirty
                                    && let Err(e) = Self::save_internal(&events_clone, &path_clone).await {
                                        tracing::error!("Graceful shutdown throttle log save failed: {}", e);
                                    }
                                break;
                            }
                        }
                    }
                    _ = interval.tick() => {
                        if dirty {
                            if let Err(e) = Self::save_internal(&events_clone, &path_clone).await {
                                tracing::error!("Failed to save throttle log: {}", e);
                            } else {
                                dirty = false;
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            events,

            dirty_tx: Some(tx),
        })
    }

    async fn save_internal(
        events: &Arc<RwLock<Vec<ThrottleEvent>>>,
        file_path: &Path,
    ) -> anyhow::Result<()> {
        let content = {
            let e = events.read();
            serde_json::to_string(&*e)?
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

    /// 记录一次限流事件
    pub fn record(
        &self,
        credential_id: u64,
        request_type: &str,
        status_code: u16,
        response_body: &str,
    ) {
        let body_summary = if response_body.len() > 200 {
            let boundary = response_body
                .char_indices()
                .take_while(|(i, _)| *i < 200)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(0);
            format!("{}...", &response_body[..boundary])
        } else {
            response_body.to_string()
        };

        let event = ThrottleEvent {
            credential_id,
            request_type: request_type.to_string(),
            status_code,
            response_body: body_summary,
            created_at: Utc::now(),
        };

        {
            let mut events = self.events.write();
            events.push(event);

            // 按 credential_id 裁剪
            let count = events
                .iter()
                .filter(|e| e.credential_id == credential_id)
                .count();
            if count > MAX_EVENTS_PER_CREDENTIAL {
                let excess = count - MAX_EVENTS_PER_CREDENTIAL;
                let mut removed = 0;
                events.retain(|e| {
                    if removed < excess && e.credential_id == credential_id {
                        removed += 1;
                        false
                    } else {
                        true
                    }
                });
            }
        }

        if let Some(tx) = &self.dirty_tx {
            let _ = tx.send(());
        }
    }

    /// 分页查询指定 credential 的限流日志（按 created_at 降序）
    pub fn get_paged(&self, credential_id: u64, page: usize, page_size: usize) -> ThrottleLogsPage {
        if page_size == 0 {
            return ThrottleLogsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size: 0,
                total_pages: 0,
            };
        }

        let owned: Vec<ThrottleEvent> = {
            let events = self.events.read();
            events
                .iter()
                .filter(|e| e.credential_id == credential_id)
                .cloned()
                .collect()
        };

        let total = owned.len();
        if total == 0 {
            return ThrottleLogsPage {
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

        let records: Vec<ThrottleLogItem> = sorted
            .into_iter()
            .skip(start)
            .take(page_size)
            .map(|e| ThrottleLogItem {
                credential_id: e.credential_id,
                request_type: e.request_type,
                status_code: e.status_code,
                response_body: e.response_body,
                created_at: e.created_at,
            })
            .collect();

        ThrottleLogsPage {
            records,
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
pub struct ThrottleLogsPage {
    pub records: Vec<ThrottleLogItem>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}

/// 对外暴露的单条限流记录
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThrottleLogItem {
    pub credential_id: u64,
    pub request_type: String,
    pub status_code: u16,
    pub response_body: String,
    pub created_at: DateTime<Utc>,
}
