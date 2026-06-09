# 任务清单：add-failure-log-store

## 状态：ARCHIVED

## 任务

### 后端（执行顺序：T1 → T2 → T3 → T4 → T5 → T6 → T7）

- [x] T1：新建 `src/model/failure_log.rs`，实现 `FailureLogStore`（含 `FailureEvent`、`FailureLogItem`、`FailureLogsPage`，结构与 `ThrottleLogStore` 对等）
- [x] T2：在 `src/model/mod.rs` 中添加 `pub mod failure_log;`（依赖 T1）
- [x] T3：在 `src/admin/middleware.rs` 的 `AdminState` 中添加 `failure_log_store: Option<Arc<FailureLogStore>>` 及 `with_failure_log_store` 方法（依赖 T1）
- [x] T4：在 `src/kiro/provider.rs` 中添加 `failure_log_store: Option<Arc<FailureLogStore>>` 字段及 `with_failure_log_store` 方法（参照 `with_throttle_log_store` builder 模式），在 API + MCP 两路的 401/403 分支记录事件（依赖 T1；对现有多账号逻辑影响评估：仅在 401/403 分支追加记录调用，不改变 `report_failure`、重试逻辑及账号切换行为）
- [x] T5：在 `src/admin/api_keys.rs` 中添加 `get_failure_logs` handler（复用 `get_throttle_logs` 模式；依赖 T1）
- [x] T6：在 `src/admin/router.rs` 中注册路由 `GET /credentials/{id}/failure-logs`（依赖 T5）
- [x] T7：在 `src/main.rs` 中初始化 `FailureLogStore`（复用 `throttle_data_dir`，文件名 `failure_log.json`，load/empty 同 throttle_log），注入 `provider` 和 `admin_state`，输出 `tracing::info!` 确认注入成功（依赖 T2、T3、T4）

### 前端（各任务独立，可并行；T11 依赖 T8/T9/T10）

- [x] T8：在 `admin-ui/src/types/api.ts` 中添加 `FailureLogRecord`、`FailureLogsResponse` 类型
- [x] T9：在 `admin-ui/src/api/credentials.ts` 中添加 `getFailureLogs` 函数（依赖 T8）
- [x] T10：在 `admin-ui/src/hooks/use-credentials.ts` 中添加 `useFailureLogs` hook（依赖 T9）
- [x] T11：重写 `admin-ui/src/components/failure-log-page.tsx`，改用专属分页接口，移除 `LogViewerPage` 依赖，移除旧的「过滤条件」卡片，参照 `ThrottleLogPage` 布局（依赖 T10）

## 验收标准

- [ ] 对被封禁账号（返回 401/403）发起请求后，点击"失败"进入失败日志页，能看到 ≥1 条记录（含时间、类型、状态码、响应摘要）；每次外部请求因重试最多产生 3 条记录
- [ ] 无失败记录时页面正常显示空状态提示，不报错
- [ ] 失败日志在服务重启后仍保留（持久化至 `failure_log.json`）
- [ ] `cargo check` + `cargo clippy` clean，`cargo test` 全部通过
- [ ] API `/credentials/{id}/failure-logs` 返回正确分页结构（`records`、`total`、`page`、`page_size`、`totalPages`）
- [ ] 429 限流场景手工复现：`ThrottleLogPage` 限流条目数量无变化，`failureCount` 不受影响
