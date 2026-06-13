> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# Log Viewer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Admin UI 侧边栏新增「查看日志」入口，通过 SSE 实时展示后端 Rust tracing 日志，并提供一键下载功能。

**Architecture:** 后端新增 `LogCapture` 模块作为 `tracing_subscriber::Layer`，将每条日志同时写入内存 ring buffer（1000 条）和 `tokio::sync::broadcast` 广播通道；`/api/admin/logs/stream` SSE 端点先 flush 历史快照再订阅新事件；前端用 `EventSource` 建立 SSE 连接，状态管理在自定义 hook `useLogStream` 中，终端暗黑风格组件 `LogViewerPage` 提供级别过滤、关键词过滤、自动滚动和下载。

**Tech Stack:** Rust/Axum 0.8 (SSE, tracing_subscriber Layer, tokio broadcast), React 18, TypeScript, Tailwind CSS, native EventSource API

---

## File Map

| 操作 | 路径 | 职责 |
|------|------|------|
| 新建 | `src/log_capture.rs` | `LogEntry` 结构体、`LogCapture`（ring buffer + broadcast sender）、`LogCaptureLayer`（tracing Layer 实现） |
| 修改 | `src/admin/middleware.rs` | `AdminState` 新增 `log_capture` 字段 + `with_log_capture` builder 方法 |
| 新建 | `src/admin/log_handler.rs` | `stream_logs` SSE handler、`download_logs` 下载 handler（均含 inline Query Param 认证） |
| 修改 | `src/admin/mod.rs` | 声明 `log_handler` submodule |
| 修改 | `src/admin/router.rs` | 新增 `/logs/stream` 和 `/logs/download` 路由（绕过 header auth 中间件） |
| 修改 | `src/main.rs` | 修改 tracing 初始化为 registry 风格、创建 `LogCapture`、传入 `AdminState` |
| 修改 | `Cargo.toml` | `tracing-subscriber` 添加 `registry` feature |
| 新建 | `admin-ui/src/hooks/use-log-stream.ts` | `useLogStream` hook：EventSource 连接管理、自动重连（指数退避） |
| 新建 | `admin-ui/src/components/log-viewer-page.tsx` | 终端暗黑风日志查看页，含工具栏（级别过滤/关键词/自动滚动/下载/清空）和底部状态栏 |
| 修改 | `admin-ui/src/components/dashboard.tsx` | `activeTab` 扩展 `'logs'`、侧边栏新增入口、主内容区新增分支 |

---

## Task 1: 创建 `src/log_capture.rs`

**Files:**
- Create: `src/log_capture.rs`

- [ ] **Step 1: 写 `src/log_capture.rs`**

```rust
// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 内存日志捕获层 — 将 tracing 事件存入 ring buffer 并通过 broadcast 广播

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;
use tokio::sync::broadcast;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

const BROADCAST_CAPACITY: usize = 256;

#[derive(Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

pub struct LogCapture {
    ring_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    sender: broadcast::Sender<LogEntry>,
    capacity: usize,
}

impl LogCapture {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            ring_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            sender,
            capacity,
        }
    }

    pub fn as_layer(&self) -> LogCaptureLayer {
        LogCaptureLayer {
            ring_buffer: self.ring_buffer.clone(),
            sender: self.sender.clone(),
            capacity: self.capacity,
        }
    }

    /// 返回当前 ring buffer 快照（全量复制）
    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.ring_buffer.lock().iter().cloned().collect()
    }

    /// 订阅后续新事件（返回 broadcast::Receiver）
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.sender.subscribe()
    }
}

pub struct LogCaptureLayer {
    ring_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    sender: broadcast::Sender<LogEntry>,
    capacity: usize,
}

impl<S: Subscriber> Layer<S> for LogCaptureLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let level = match *metadata.level() {
            Level::TRACE => "TRACE",
            Level::DEBUG => "DEBUG",
            Level::INFO => "INFO",
            Level::WARN => "WARN",
            Level::ERROR => "ERROR",
        };

        let mut message = String::new();
        event.record(&mut MessageVisitor(&mut message));

        let entry = LogEntry {
            timestamp: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
            level: level.to_string(),
            target: metadata.target().to_string(),
            message,
        };

        {
            let mut buf = self.ring_buffer.lock();
            if buf.len() >= self.capacity {
                buf.pop_front();
            }
            buf.push_back(entry.clone());
        }

        // 无接收者时 send 返回 Err，属正常情况，忽略
        let _ = self.sender.send(entry);
    }
}

struct MessageVisitor<'a>(&'a mut String);

impl<'a> tracing::field::Visit for MessageVisitor<'a> {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0.push_str(value);
        } else {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(&format!("{}={}", field.name(), value));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let s = format!("{:?}", value);
        if field.name() == "message" {
            // 去掉 Debug 输出的外层引号（字符串类型会被加引号）
            if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                self.0.push_str(&s[1..s.len() - 1]);
            } else {
                self.0.push_str(&s);
            }
        } else {
            if !self.0.is_empty() {
                self.0.push(' ');
            }
            self.0.push_str(&format!("{}={}", field.name(), s));
        }
    }
}
```

- [ ] **Step 2: 编译验证**

```bash
cargo check 2>&1 | grep -E "^error"
```

预期输出：此时会有 `log_capture` 未被使用的警告，但无 error。如有 error，修复后继续。

- [ ] **Step 3: Commit**

```bash
git add src/log_capture.rs
git commit -m "feat(log-viewer): add LogCapture tracing layer with ring buffer and broadcast channel"
```

---

## Task 2: 更新 `AdminState` 添加 `log_capture` 字段

**Files:**
- Modify: `src/admin/middleware.rs`

- [ ] **Step 1: 在 `AdminState` struct 中添加字段**

在 `src/admin/middleware.rs` 的 `use` 块顶部添加导入：

```rust
use crate::log_capture::LogCapture;
```

在 `AdminState` struct 中 `throttle_log_store` 字段之后添加：

```rust
    /// 日志捕获器（可选）
    pub log_capture: Option<Arc<LogCapture>>,
```

- [ ] **Step 2: 在 `AdminState::new` 中初始化该字段**

在 `new` 方法的 `Self { ... }` 块中，`throttle_log_store: None,` 之后添加：

```rust
            log_capture: None,
```

- [ ] **Step 3: 添加 `with_log_capture` builder 方法**

在 `with_config_path` 方法之后添加：

```rust
    pub fn with_log_capture(mut self, capture: Arc<LogCapture>) -> Self {
        self.log_capture = Some(capture);
        self
    }
```

- [ ] **Step 4: 编译验证**

```bash
cargo check 2>&1 | grep -E "^error"
```

预期：无 error。

- [ ] **Step 5: Commit**

```bash
git add src/admin/middleware.rs
git commit -m "feat(log-viewer): add log_capture field to AdminState"
```

---

## Task 3: 创建 `src/admin/log_handler.rs`

**Files:**
- Create: `src/admin/log_handler.rs`

- [ ] **Step 1: 写 `src/admin/log_handler.rs`**

```rust
// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 日志查看 API Handler — SSE 流 + 下载接口

use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response, sse::{Event, KeepAlive, Sse}},
};
use futures::stream::{self, StreamExt};
use tokio::sync::broadcast::error::RecvError;

use super::middleware::AdminState;
use crate::common::auth;

/// GET /api/admin/logs/stream?api_key=<key>
///
/// EventSource 不支持自定义 Header，因此通过 Query Param 认证。
/// 连接后先发送 history 事件（全量快照），随后持续推送 log 事件。
pub async fn stream_logs(
    State(state): State<AdminState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if !check_api_key(&state, &params) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let Some(log_capture) = &state.log_capture else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Log capture not enabled").into_response();
    };
    let log_capture = log_capture.clone();

    // 先订阅，再取快照，避免遗漏订阅后、快照前的新事件
    let rx = log_capture.subscribe();
    let history = log_capture.snapshot();
    let history_json = serde_json::to_string(&history).unwrap_or_default();

    let history_stream = stream::once(async move {
        Ok::<Event, Infallible>(Event::default().event("history").data(history_json))
    });

    let live_stream = stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(entry) => {
                    let json = serde_json::to_string(&entry).unwrap_or_default();
                    return Some((
                        Ok(Event::default().event("log").data(json)),
                        rx,
                    ));
                }
                Err(RecvError::Lagged(n)) => {
                    // 广播通道溢出，跳过丢失的消息继续
                    tracing::warn!("log SSE stream lagged by {} messages", n);
                    continue;
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });

    Sse::new(history_stream.chain(live_stream))
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(30))
                .text("ping"),
        )
        .into_response()
}

/// GET /api/admin/logs/download?api_key=<key>
///
/// 返回当前 ring buffer 内全部日志，以 .txt 文件形式下载。
pub async fn download_logs(
    State(state): State<AdminState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if !check_api_key(&state, &params) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let Some(log_capture) = &state.log_capture else {
        return (StatusCode::SERVICE_UNAVAILABLE, "Log capture not enabled").into_response();
    };

    let snapshot = log_capture.snapshot();
    let mut content = String::with_capacity(snapshot.len() * 120);
    for entry in &snapshot {
        content.push_str(&format!(
            "{} [{}] {} {}\n",
            entry.timestamp, entry.level, entry.target, entry.message
        ));
    }

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let disposition = format!(
        "attachment; filename=\"kiro2cc-proxy-logs-{}.txt\"",
        timestamp
    );

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/plain; charset=utf-8".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        disposition.parse().unwrap(),
    );

    (headers, content).into_response()
}

fn check_api_key(state: &AdminState, params: &HashMap<String, String>) -> bool {
    let key = params.get("api_key").map(|s| s.as_str()).unwrap_or("");
    auth::constant_time_eq(key, &state.admin_api_key.read())
}
```

- [ ] **Step 2: 编译验证**

```bash
cargo check 2>&1 | grep -E "^error"
```

预期：此步骤会报 `log_handler` 模块未声明的错误，Task 4 中修复。

- [ ] **Step 3: Commit**

```bash
git add src/admin/log_handler.rs
git commit -m "feat(log-viewer): add SSE stream and download handlers for log viewer"
```

---

## Task 4: 注册模块和路由

**Files:**
- Modify: `src/admin/mod.rs`
- Modify: `src/admin/router.rs`

- [ ] **Step 1: 在 `src/admin/mod.rs` 中声明 `log_handler` 模块**

在 `mod api_keys;` 这一行之后添加：

```rust
mod log_handler;
```

- [ ] **Step 2: 重构 `src/admin/router.rs` 使用双 Router 隔离认证**

> **重要**：Axum 0.8 中 `Router::layer()` 对该 Router 上的**所有路由**生效，包括调用 `.layer()` 之后添加的路由。因此不能靠顺序绕过中间件，必须使用两个独立 Router 然后 merge。

用以下内容完整替换 `src/admin/router.rs`：

```rust
// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin API 路由配置

use axum::{
    Router, middleware,
    routing::{delete, get, post, put},
};

use super::{
    api_keys::{
        create_api_key, delete_api_key, get_all_usage, get_credential_usage_records,
        get_daily_usage, get_daily_usage_records, get_key_usage, get_key_usage_records,
        get_rpm, get_server_info, get_throttle_logs, list_api_keys, reset_key_usage,
        update_api_key,
    },
    handlers::{
        add_credential, delete_credential, get_all_credentials, get_auth_keys,
        get_credential_balance, get_load_balancing_mode, reset_failure_count,
        set_auth_keys, set_credential_disabled, set_credential_priority,
        set_load_balancing_mode, update_credential,
    },
    log_handler::{download_logs, stream_logs},
    middleware::{AdminState, admin_auth_middleware},
};

/// 创建 Admin API 路由
pub fn create_admin_router(state: AdminState) -> Router {
    // 受 header 认证中间件保护的路由
    let protected = Router::new()
        .route(
            "/credentials",
            get(get_all_credentials).post(add_credential),
        )
        .route("/credentials/{id}", delete(delete_credential).put(update_credential))
        .route("/credentials/{id}/disabled", post(set_credential_disabled))
        .route("/credentials/{id}/priority", post(set_credential_priority))
        .route("/credentials/{id}/reset", post(reset_failure_count))
        .route("/credentials/{id}/balance", get(get_credential_balance))
        .route("/credentials/{id}/usage/records", get(get_credential_usage_records))
        .route("/credentials/{id}/throttle-logs", get(get_throttle_logs))
        .route(
            "/config/load-balancing",
            get(get_load_balancing_mode).put(set_load_balancing_mode),
        )
        .route(
            "/config/auth-keys",
            get(get_auth_keys).put(set_auth_keys),
        )
        .route("/server-info", get(get_server_info))
        .route("/api-keys", get(list_api_keys).post(create_api_key))
        .route("/api-keys/usage", get(get_all_usage))
        .route("/api-keys/{id}", put(update_api_key).delete(delete_api_key))
        .route("/api-keys/{id}/usage", get(get_key_usage).delete(reset_key_usage))
        .route("/api-keys/{id}/usage/records", get(get_key_usage_records))
        .route("/rpm", get(get_rpm))
        .route("/usage/daily", get(get_daily_usage))
        .route("/usage/daily/{date}/records", get(get_daily_usage_records))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            admin_auth_middleware,
        ))
        .with_state(state.clone());

    // 日志路由使用 Query Param 内联认证（EventSource API 不支持自定义 Header）
    let log_routes = Router::new()
        .route("/logs/stream", get(stream_logs))
        .route("/logs/download", get(download_logs))
        .with_state(state);

    protected.merge(log_routes)
}
```

- [ ] **Step 3: 编译验证**

```bash
cargo check 2>&1 | grep -E "^error"
```

预期：无 error（`main.rs` 尚未传入 `log_capture`，但字段是 `Option`，不影响编译）。

- [ ] **Step 4: Commit**

```bash
git add src/admin/mod.rs src/admin/router.rs
git commit -m "feat(log-viewer): register log_handler module and SSE/download routes"
```

---

## Task 5: 更新 `main.rs` 和 `Cargo.toml`

**Files:**
- Modify: `src/main.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: 在 `Cargo.toml` 中为 `tracing-subscriber` 添加 `registry` feature**

将：

```toml
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

改为：

```toml
tracing-subscriber = { version = "0.3", features = ["env-filter", "registry"] }
```

- [ ] **Step 2: 在 `src/main.rs` 顶部添加 `mod log_capture;`**

在 `mod admin;` 这行之后添加：

```rust
mod log_capture;
```

- [ ] **Step 3: 在 `src/main.rs` 中替换 tracing 初始化代码**

找到并删除：

```rust
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
```

替换为：

```rust
    // 初始化日志捕获器（在 tracing 初始化之前创建）
    let log_capture = std::sync::Arc::new(log_capture::LogCapture::new(1000));

    // 初始化日志（registry 风格，同时输出到控制台和 LogCapture）
    {
        use tracing_subscriber::prelude::*;
        let make_filter = || {
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        };
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_filter(make_filter()))
            .with(log_capture.as_layer().with_filter(make_filter()))
            .init();
    }
```

- [ ] **Step 4: 在 `AdminState` 构建处传入 `log_capture`**

找到 `admin_state = admin_state.with_throttle_log_store(throttle_log_store.clone());` 这一行，在其之后添加：

```rust
            admin_state = admin_state.with_log_capture(log_capture.clone());
```

- [ ] **Step 5: 编译验证**

```bash
cargo check 2>&1 | grep -E "^error"
```

预期：无 error。

- [ ] **Step 6: 运行冒烟测试**

```bash
cargo build 2>&1 | tail -5
```

预期：`Finished` 行，无 error。

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/main.rs
git commit -m "feat(log-viewer): wire LogCapture into tracing registry and AdminState"
```

---

## Task 6: 创建 `use-log-stream.ts` Hook

**Files:**
- Create: `admin-ui/src/hooks/use-log-stream.ts`

- [ ] **Step 1: 写 `admin-ui/src/hooks/use-log-stream.ts`**

```typescript
// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useEffect, useCallback, useRef } from 'react'
import { storage } from '@/lib/storage'

export interface LogEntry {
  timestamp: string
  level: 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR'
  target: string
  message: string
}

const MAX_FRONT_LOGS = 2000

export function useLogStream(enabled: boolean): {
  logs: LogEntry[]
  connected: boolean
} {
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [connected, setConnected] = useState(false)
  const esRef = useRef<EventSource | null>(null)
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const reconnectDelay = useRef(1000)

  const connect = useCallback(() => {
    const apiKey = storage.getApiKey()
    if (!apiKey) return

    const es = new EventSource(
      `/api/admin/logs/stream?api_key=${encodeURIComponent(apiKey)}`
    )
    esRef.current = es

    es.onopen = () => {
      setConnected(true)
      reconnectDelay.current = 1000
    }

    es.addEventListener('history', (e: MessageEvent) => {
      try {
        const entries: LogEntry[] = JSON.parse(e.data)
        setLogs(entries)
      } catch {
        // ignore malformed history payload
      }
    })

    es.addEventListener('log', (e: MessageEvent) => {
      try {
        const entry: LogEntry = JSON.parse(e.data)
        setLogs((prev) => {
          const next = [...prev, entry]
          return next.length > MAX_FRONT_LOGS
            ? next.slice(next.length - MAX_FRONT_LOGS)
            : next
        })
      } catch {
        // ignore malformed log entry
      }
    })

    es.onerror = () => {
      setConnected(false)
      es.close()
      esRef.current = null
      const delay = reconnectDelay.current
      reconnectDelay.current = Math.min(delay * 2, 30000)
      reconnectTimer.current = setTimeout(connect, delay)
    }
  }, [])

  useEffect(() => {
    if (!enabled) {
      esRef.current?.close()
      esRef.current = null
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current)
      setConnected(false)
      setLogs([])
      return
    }

    connect()

    return () => {
      esRef.current?.close()
      esRef.current = null
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current)
    }
  }, [enabled, connect])

  return { logs, connected }
}
```

- [ ] **Step 2: Commit**

```bash
git add admin-ui/src/hooks/use-log-stream.ts
git commit -m "feat(log-viewer): add useLogStream SSE hook with auto-reconnect"
```

---

## Task 7: 创建 `log-viewer-page.tsx` 组件

**Files:**
- Create: `admin-ui/src/components/log-viewer-page.tsx`

- [ ] **Step 1: 写 `admin-ui/src/components/log-viewer-page.tsx`**

```tsx
// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useRef, useEffect, useCallback } from 'react'
import { useLogStream, type LogEntry } from '@/hooks/use-log-stream'
import { storage } from '@/lib/storage'

type LevelFilter = 'ALL' | 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR'

const LEVEL_FILTERS: LevelFilter[] = ['ALL', 'DEBUG', 'INFO', 'WARN', 'ERROR']

function levelColor(level: string): string {
  switch (level) {
    case 'TRACE': return '#6e7681'
    case 'DEBUG': return '#6e7681'
    case 'INFO':  return '#58a6ff'
    case 'WARN':  return '#f0b429'
    case 'ERROR': return '#f85149'
    default:      return '#e6edf3'
  }
}

function rowBackground(level: string): string {
  if (level === 'WARN')  return 'rgba(240,180,41,0.08)'
  if (level === 'ERROR') return 'rgba(248,81,73,0.08)'
  return 'transparent'
}

function formatTimestamp(ts: string): string {
  // "2026-06-08T10:23:44.123Z" → "2026-06-08 10:23:44.123"
  return ts.replace('T', ' ').replace('Z', '')
}

export function LogViewerPage() {
  const [levelFilter, setLevelFilter] = useState<LevelFilter>('ALL')
  const [keyword, setKeyword] = useState('')
  const [autoScroll, setAutoScroll] = useState(true)
  const [localLogs, setLocalLogs] = useState<LogEntry[]>([])

  const logEndRef = useRef<HTMLDivElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const autoScrollRef = useRef(true)

  const { logs, connected } = useLogStream(true)

  // Sync hook logs into local state (allows "clear" to work independently)
  useEffect(() => {
    setLocalLogs(logs)
  }, [logs])

  // Auto-scroll to bottom when new logs arrive
  useEffect(() => {
    if (autoScrollRef.current && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: 'auto' })
    }
  }, [localLogs])

  // Keep ref in sync so scroll handler doesn't close over stale state
  useEffect(() => {
    autoScrollRef.current = autoScroll
  }, [autoScroll])

  const handleScroll = useCallback(() => {
    const el = containerRef.current
    if (!el) return
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 50
    if (atBottom !== autoScrollRef.current) {
      setAutoScroll(atBottom)
    }
  }, [])

  const filteredLogs = localLogs.filter((entry) => {
    if (levelFilter !== 'ALL' && entry.level !== levelFilter) return false
    if (keyword) {
      const lower = keyword.toLowerCase()
      return (
        entry.message.toLowerCase().includes(lower) ||
        entry.target.toLowerCase().includes(lower)
      )
    }
    return true
  })

  const handleDownload = () => {
    const apiKey = storage.getApiKey()
    if (!apiKey) return
    window.open(
      `/api/admin/logs/download?api_key=${encodeURIComponent(apiKey)}`,
      '_blank'
    )
  }

  const handleClear = () => setLocalLogs([])

  const toggleAutoScroll = () => {
    setAutoScroll((prev) => {
      if (!prev && logEndRef.current) {
        logEndRef.current.scrollIntoView({ behavior: 'auto' })
      }
      return !prev
    })
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: 'calc(100vh - 56px)' }}>
      {/* Page Header */}
      <div className="mb-4">
        <h1 className="text-[22px] font-bold tracking-[-0.02em]">实时日志</h1>
        <p className="text-[13px] text-muted-foreground mt-0.5">
          实时查看服务运行日志，最近 1000 条
        </p>
      </div>

      {/* Terminal area */}
      <div
        style={{
          flex: 1,
          background: '#0d1117',
          borderRadius: 8,
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
          border: '1px solid #21262d',
          minHeight: 0,
        }}
      >
        {/* Toolbar */}
        <div
          style={{
            padding: '10px 16px',
            background: '#161b22',
            borderBottom: '1px solid #21262d',
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            flexWrap: 'wrap',
            flexShrink: 0,
          }}
        >
          {/* Level filter buttons */}
          <div style={{ display: 'flex', gap: 4 }}>
            {LEVEL_FILTERS.map((level) => (
              <button
                key={level}
                onClick={() => setLevelFilter(level)}
                style={{
                  padding: '3px 10px',
                  borderRadius: 4,
                  fontSize: 11,
                  border: '1px solid #30363d',
                  background: levelFilter === level ? '#388bfd20' : '#21262d',
                  color:
                    levelFilter === level
                      ? '#388bfd'
                      : levelColor(level === 'ALL' ? 'INFO' : level),
                  cursor: 'pointer',
                  fontWeight: levelFilter === level ? 600 : 400,
                }}
              >
                {level}
              </button>
            ))}
          </div>

          {/* Keyword filter */}
          <input
            type="text"
            placeholder="关键词过滤..."
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            style={{
              flex: 1,
              minWidth: 120,
              background: '#21262d',
              border: '1px solid #30363d',
              borderRadius: 4,
              padding: '4px 10px',
              fontSize: 11,
              color: '#e6edf3',
              outline: 'none',
            }}
          />

          {/* Auto-scroll toggle */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <span style={{ fontSize: 11, color: '#8b949e' }}>自动滚动</span>
            <div
              onClick={toggleAutoScroll}
              style={{
                width: 28,
                height: 16,
                borderRadius: 8,
                cursor: 'pointer',
                background: autoScroll ? '#388bfd' : '#30363d',
                position: 'relative',
                transition: 'background 0.2s',
              }}
            >
              <div
                style={{
                  width: 12,
                  height: 12,
                  background: 'white',
                  borderRadius: '50%',
                  position: 'absolute',
                  top: 2,
                  left: autoScroll ? 14 : 2,
                  transition: 'left 0.2s',
                }}
              />
            </div>
          </div>

          <button
            onClick={handleClear}
            style={{
              padding: '4px 10px',
              borderRadius: 4,
              fontSize: 11,
              border: '1px solid #30363d',
              background: '#21262d',
              color: '#8b949e',
              cursor: 'pointer',
            }}
          >
            清空
          </button>

          <button
            onClick={handleDownload}
            style={{
              padding: '4px 12px',
              borderRadius: 4,
              fontSize: 11,
              border: '1px solid #388bfd',
              background: '#388bfd20',
              color: '#388bfd',
              cursor: 'pointer',
              fontWeight: 600,
            }}
          >
            ⬇ 下载日志
          </button>

          {/* Connection status */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
            <div
              style={{
                width: 6,
                height: 6,
                borderRadius: '50%',
                background: connected ? '#3fb950' : '#f0b429',
              }}
            />
            <span
              style={{
                fontSize: 11,
                color: connected ? '#3fb950' : '#f0b429',
              }}
            >
              {connected ? '已连接' : '重连中...'}
            </span>
          </div>
        </div>

        {/* Log lines */}
        <div
          ref={containerRef}
          onScroll={handleScroll}
          style={{
            flex: 1,
            overflowY: 'auto',
            padding: '8px 0',
            fontFamily: "'SF Mono', 'Fira Code', 'Courier New', monospace",
            fontSize: 11,
            lineHeight: '1.7',
            minHeight: 0,
          }}
        >
          {filteredLogs.map((entry, i) => (
            <div
              key={i}
              style={{
                padding: '1px 16px',
                display: 'flex',
                gap: 10,
                alignItems: 'baseline',
                background: rowBackground(entry.level),
              }}
            >
              <span
                style={{ color: '#8b949e', minWidth: 170, flexShrink: 0, whiteSpace: 'nowrap' }}
              >
                {formatTimestamp(entry.timestamp)}
              </span>
              <span
                style={{
                  color: levelColor(entry.level),
                  background: `${levelColor(entry.level)}22`,
                  padding: '0 4px',
                  borderRadius: 2,
                  fontSize: 9,
                  minWidth: 40,
                  textAlign: 'center',
                  flexShrink: 0,
                }}
              >
                {entry.level}
              </span>
              <span
                style={{
                  color: '#6e7681',
                  flexShrink: 0,
                  fontSize: 9,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                  maxWidth: 280,
                }}
              >
                {entry.target}
              </span>
              <span
                style={{
                  color: entry.level === 'DEBUG' || entry.level === 'TRACE'
                    ? '#6e7681'
                    : '#e6edf3',
                  flex: 1,
                  wordBreak: 'break-all',
                }}
              >
                {entry.message}
              </span>
            </div>
          ))}
          <div ref={logEndRef} />
        </div>

        {/* Footer status bar */}
        <div
          style={{
            padding: '4px 16px',
            background: '#161b22',
            borderTop: '1px solid #21262d',
            display: 'flex',
            justifyContent: 'space-between',
            fontSize: 10,
            color: '#8b949e',
            flexShrink: 0,
          }}
        >
          <span>
            已显示 {filteredLogs.length} 条（缓冲 {localLogs.length} 条）
          </span>
          <span>
            {keyword ? `过滤: "${keyword}" | ` : ''}级别: {levelFilter}
          </span>
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Commit**

```bash
git add admin-ui/src/components/log-viewer-page.tsx
git commit -m "feat(log-viewer): add LogViewerPage component with dark terminal style"
```

---

## Task 8: 更新 `dashboard.tsx` 添加侧边栏入口和路由

**Files:**
- Modify: `admin-ui/src/components/dashboard.tsx`

- [ ] **Step 1: 添加图标和组件导入**

在 `dashboard.tsx` 顶部的 import 区域，找到 lucide-react 的图标导入行：

```tsx
import { RefreshCw, LogOut, Server, Plus, Upload, FileUp, Trash2, RotateCcw, CheckCircle2, Key, Settings, BarChart2 } from 'lucide-react'
```

在行尾的 `}` 之前添加 `, ScrollText`：

```tsx
import { RefreshCw, LogOut, Server, Plus, Upload, FileUp, Trash2, RotateCcw, CheckCircle2, Key, Settings, BarChart2, ScrollText } from 'lucide-react'
```

在 `import { DailyDetailPage }` 那一行之后添加：

```tsx
import { LogViewerPage } from '@/components/log-viewer-page'
```

- [ ] **Step 2: 扩展 `activeTab` 类型**

找到：

```tsx
  const [activeTab, setActiveTab] = useState<'credentials' | 'apikeys' | 'settings'>('credentials')
```

改为：

```tsx
  const [activeTab, setActiveTab] = useState<'credentials' | 'apikeys' | 'settings' | 'logs'>('credentials')
```

同时找到：

```tsx
  const prevTabRef = useRef<'credentials' | 'apikeys' | 'settings' | null>(null)
```

改为：

```tsx
  const prevTabRef = useRef<'credentials' | 'apikeys' | 'settings' | 'logs' | null>(null)
```

- [ ] **Step 3: 在侧边栏「系统」分组中新增「查看日志」按钮**

找到侧边栏「系统」分组中 `设置` 按钮对应的 `<button>` 元素（从 `onClick={() => { setActiveTab('settings')` 开始到对应的 `</button>` 结束），在该 `<button>` 之前插入「查看日志」按钮：

```tsx
            <button
              onClick={() => { setActiveTab('logs'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) }}
              className={`flex w-full items-center gap-2.5 px-3 py-2 text-[13px] font-medium rounded-md transition-all mb-0.5 ${activeTab === 'logs' ? 'text-foreground bg-secondary' : 'text-muted-foreground hover:text-foreground hover:bg-secondary'}`}
              style={activeTab === 'logs' ? { boxShadow: 'inset 2px 0 0 hsl(var(--primary))' } : undefined}
            >
              <ScrollText className="w-4 h-4 shrink-0" />
              <span>查看日志</span>
            </button>
```

- [ ] **Step 4: 在主内容区添加 `logs` 分支**

找到主内容区的条件渲染链（`activeTab === 'settings' ? ... : activeTab === 'apikeys' ? ...`），在最外层开头插入 `logs` 分支：

```tsx
        {activeTab === 'logs' ? (
          <LogViewerPage />
        ) : activeTab === 'settings' ? (
          // ... 原有内容不变
```

即将原来的：

```tsx
        {activeTab === 'settings' ? (
          <SettingsPanel />
        ) : ...
```

改为：

```tsx
        {activeTab === 'logs' ? (
          <LogViewerPage />
        ) : activeTab === 'settings' ? (
          <SettingsPanel />
        ) : ...
```

- [ ] **Step 5: 前端构建验证**

```bash
cd admin-ui && npm run build 2>&1 | tail -20
```

预期：`✓ built in` 行，无 TypeScript error。

- [ ] **Step 6: Commit**

```bash
git add admin-ui/src/components/dashboard.tsx
git commit -m "feat(log-viewer): add 查看日志 sidebar entry and route in dashboard"
```

---

## Task 9: 后端运行测试（端到端冒烟）

- [ ] **Step 1: 本地启动服务**

```bash
cargo run -- --config app/config/config.json 2>&1 &
sleep 3
```

预期日志中包含 `Admin UI 已启用: /admin`。

- [ ] **Step 2: 验证 SSE 端点**

将 `<YOUR_ADMIN_KEY>` 替换为 `app/config/config.json` 中的 `adminApiKey`：

```bash
curl -N "http://localhost:8080/api/admin/logs/stream?api_key=<YOUR_ADMIN_KEY>" 2>&1 | head -20
```

预期：收到 `event: history` 和 `data: [...]` 行。

- [ ] **Step 3: 验证下载端点**

```bash
curl -I "http://localhost:8080/api/admin/logs/download?api_key=<YOUR_ADMIN_KEY>" 2>&1 | grep -i content
```

预期：包含 `Content-Disposition: attachment; filename="kiro2cc-proxy-logs-*.txt"`。

- [ ] **Step 4: 验证未认证请求被拒绝**

```bash
curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/api/admin/logs/stream"
```

预期输出：`401`

- [ ] **Step 5: 前端验证（浏览器）**

在浏览器打开 `http://localhost:8080/admin`，登录后点击侧边栏「查看日志」，确认：
- 连接状态显示「已连接」（绿点）
- 能看到服务启动时的历史日志
- 在终端触发一条请求，日志实时出现在页面
- 级别过滤按钮可正常切换
- 关键词过滤实时生效
- 下载按钮触发文件下载

- [ ] **Step 6: 停止测试服务**

```bash
kill %1 2>/dev/null || true
```

- [ ] **Step 7: 最终提交（如有未提交修改）**

```bash
git status
# 如有未提交文件：
git add -p
git commit -m "feat(log-viewer): complete log viewer feature"
```
