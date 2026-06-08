> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# 查看日志功能设计文档

## 概述

在 Admin UI 侧边栏新增「查看日志」入口，实时展示后端服务运行日志（等同于 `docker logs -f --timestamps kiro2cc-proxy`），并提供日志下载功能。

---

## 需求

- **实时性**：SSE 流式推送，毫秒级延迟
- **历史回放**：连接时加载最近 1000 条历史日志，随后持续追加新日志
- **级别过滤**：ALL / DEBUG / INFO / WARN / ERROR 按钮切换，前端本地过滤
- **关键词过滤**：前端本地实时过滤
- **下载**：一键下载当前缓冲区全部日志为 `.txt` 文件
- **认证**：复用现有 Admin API Key 认证中间件

---

## 架构

### 后端（Rust / Axum）

#### 新模块 `src/log_capture.rs`

```rust
pub struct LogCapture {
    ring_buffer: Arc<Mutex<VecDeque<LogEntry>>>,  // 容量上限 1000
    sender: broadcast::Sender<LogEntry>,
}

pub struct LogEntry {
    pub timestamp: String,   // ISO 8601，UTC
    pub level: String,       // "DEBUG" | "INFO" | "WARN" | "ERROR"
    pub target: String,      // tracing target（模块路径）
    pub message: String,
}

impl<S: Subscriber> Layer<S> for LogCaptureLayer { ... }
```

#### 初始化（`main.rs` 改动）

```rust
let log_capture = Arc::new(LogCapture::new(1000));

tracing_subscriber::registry()
    .with(fmt::layer())                   // 原有控制台输出保留
    .with(log_capture.as_layer())         // 新增捕获层
    .init();

// 传入 AdminState
admin_state = admin_state.with_log_capture(log_capture);
```

#### 新增 Admin API 路由

| 方法 | 路径 | 说明 |
|------|------|------|
| `GET` | `/api/admin/logs/stream?api_key=<key>` | SSE 流（Query Param 认证） |
| `GET` | `/api/admin/logs/download?api_key=<key>` | 下载缓冲区日志（Query Param 认证） |

> **认证说明**：浏览器原生 `EventSource` API 不支持自定义 Header，因此这两个端点改用 Query Parameter `api_key` 传递 Admin Key。后端中间件对这两个路由单独读取 `?api_key` 参数而非 `Authorization` Header。

**SSE 协议：**

1. 连接建立 → 立即发送 `event: history`，data 为 JSON 数组（当前缓冲全量）
2. 后续每条新日志 → `event: log`，data 为单条 `LogEntry` JSON
3. 每 30s 发送 `event: ping`（防连接超时）

**下载接口：**

返回 `Content-Disposition: attachment; filename="kiro2cc-proxy-logs-<timestamp>.txt"`，每行格式：`<timestamp> [<LEVEL>] <target> <message>`

### 前端（React / TypeScript）

#### 新文件 `src/hooks/use-log-stream.ts`

```typescript
export function useLogStream(enabled: boolean): {
  logs: LogEntry[];
  connected: boolean;
  error: string | null;
}
```

- `enabled=false` 时不建立 SSE 连接（切换到其他 tab 时断开）
- 连接断开后自动重连（指数退避，最长 30s）
- 接收 `history` 事件时用数组整体替换 `logs`
- 接收 `log` 事件时 append（前端最多保留 2000 条，超出时裁剪最旧的）

#### 新文件 `src/components/log-viewer-page.tsx`

**工具栏（从左到右）：**

1. 级别切换按钮组：`ALL` / `DEBUG` / `INFO` / `WARN` / `ERROR`
2. 关键词过滤输入框（flex: 1）
3. 自动滚动 toggle（默认开启；用户手动向上滚动时自动关闭，点击 toggle 恢复）
4. `清空` 按钮（仅清空前端展示，不影响后端缓冲）
5. `⬇ 下载日志` 按钮（触发 `/api/admin/logs/download`）
6. 连接状态指示（`● 已连接` / `◌ 重连中`）

**日志行格式：**

```
<timestamp>  <LEVEL>  <target>  <message>
```

- DEBUG 行：低对比度（`#6e7681`）
- INFO 行：正常白色（`#e6edf3`）
- WARN 行：黄色文字 + 全行黄色背景
- ERROR 行：红色文字 + 全行红色背景

**底部状态栏：**

`已显示 N 条（缓冲 M 条）| 过滤: <keyword> | 级别: <level>`

#### `src/components/dashboard.tsx` 改动

- `activeTab` 类型从 `'credentials' | 'apikeys' | 'settings'` 扩展为 `+ 'logs'`
- 侧边栏「系统」分组新增「查看日志」条目（`ScrollText` 图标）
- 主内容区新增 `activeTab === 'logs'` 分支渲染 `<LogViewerPage />`
- 切换到非 logs tab 时，`useLogStream` 的 `enabled` 变为 `false`，断开 SSE

---

## 数据流

```
Rust tracing events
    │
    ▼ LogCaptureLayer.on_event()
    ├── push_back → VecDeque<LogEntry> (ring buffer, max 1000)
    └── send      → broadcast::Sender<LogEntry>
                        │
                   SSE handler
                        ├── 连接时：flush ring_buffer → history event
                        └── 持续：broadcast::Receiver.recv() → log event

前端 useLogStream
    ├── EventSource('/api/admin/logs/stream')
    ├── history event → setLogs([...])
    ├── log event → setLogs(prev => [...prev, entry].slice(-2000))
    └── 级别/关键词过滤在 render 时 Array.filter()
```

---

## 不在范围内

- 日志持久化到磁盘（服务重启后历史丢失）
- 日志搜索历史记录
- 多实例/分布式日志聚合
- 日志告警/通知

---

## 文件变更列表

| 操作 | 路径 |
|------|------|
| 新建 | `src/log_capture.rs` |
| 修改 | `src/admin/middleware.rs`（AdminState 加 log_capture 字段） |
| 修改 | `src/admin/router.rs`（注册两个新路由） |
| 新建 | `src/admin/log_handler.rs`（SSE + 下载 handler） |
| 修改 | `src/main.rs`（初始化 LogCapture，传入 AdminState） |
| 新建 | `admin-ui/src/hooks/use-log-stream.ts` |
| 新建 | `admin-ui/src/components/log-viewer-page.tsx` |
| 修改 | `admin-ui/src/components/dashboard.tsx` |
