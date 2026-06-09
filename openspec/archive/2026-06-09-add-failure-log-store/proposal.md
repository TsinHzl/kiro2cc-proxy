# 变更提案：add-failure-log-store

## 背景

失败日志页（`FailureLogPage`）当前复用实时日志查看器（`LogViewerPage`），以账号的 email/nickname 作为关键词过滤 ERROR 级日志。这个设计有两个根本性断裂：

1. **日志级别断裂**：`token_manager.report_failure` 每次调用只发出 WARN 级日志 `"账号 #N API 调用失败（M/3）"`；`provider.rs` MCP 路径的 401/403 完全不发日志。前端按 ERROR 过滤，所有逐次失败事件都被过滤掉。
2. **关键词断裂**：唯一的 ERROR 级日志是 `"账号 #N 已连续失败 3 次，已被禁用"`（只含数字 ID），前端关键词是 `email/nickname`，无法匹配 → 日志页面始终为空。

对比 `ThrottleLogPage`：429 限流有专属持久化存储（`ThrottleLogStore` + `throttle_log.json`），每次事件独立记录，API 分页返回。失败日志没有对等机制。

## 目标范围

**在范围内：**
- 新建 `src/model/failure_log.rs`：`FailureLogStore`，结构与 `ThrottleLogStore` 完全对等
- `provider.rs` 的 401/403 分支（API + MCP 两路）记录失败事件到 `FailureLogStore`
- 新增 Admin API `GET /credentials/{id}/failure-logs?page=1&page_size=50`
- `FailureLogPage` 改用专属分页接口，移除对 `LogViewerPage` 的依赖
- 持久化文件名：`failure_log.json`，与 `throttle_log.json` 并列

**不在范围内：**
- 不改动 `token_manager.report_failure` 的日志级别或内容
- 不改动 `ThrottleLogStore` 及限流相关逻辑
- 不改动实时日志查看器 `LogViewerPage`（仍作为独立页面使用）
- 不在 `CredentialStatusItem` 添加 `failureLogCount` 等新计数字段（`failureCount` 已足够）

## 技术方案

`FailureLogStore` 直接复用 `ThrottleLogStore` 的存储模式（`parking_lot::RwLock<Vec<FailureEvent>>` + 每次 `record()` 调用后立即同步落盘 JSON）。每个 credential_id 上限 500 条，超出时删最旧记录。**持久化策略**：与 `ThrottleLogStore` 一致，每次 `record()` 调用结束时立即调用 `self.save()`，同步写文件。高频 401 场景（如账号配置错误）理论上每次请求写 1–3 次磁盘，实测场景下频次低，不另加缓冲。

`response_body` 截取：复用 `ThrottleLogStore` 的 `char_indices().take_while(|(i, _)| *i < 200)` 安全截取方式，确保 UTF-8 字符边界对齐，不产生乱码。

记录时机：`provider.rs` 内 `matches!(status.as_u16(), 401 | 403)` 分支，在调用 `self.token_manager.report_failure(ctx.id)` 之后立即记录，传入 `credential_id`、`request_type`（"api"/"mcp"）、`status_code`、`response_body`（安全截取前 200 字符）。

**注入方式**：`Option<Arc<FailureLogStore>>`，与 `ThrottleLogStore` 保持一致。`main.rs` 初始化后注入时，同时输出 `tracing::info!("failure_log_store 已启用: {:?}", path)` 确认注入成功。若注入为 `None`（理论上不应出现），401/403 事件静默丢失，不影响主流程。

前端 `FailureLogPage` 完全重写，参照 `ThrottleLogPage` 布局：顶部汇总卡片（累计失败次数）+ 分页表格（时间、类型、状态码、响应摘要）。

## 预期影响

- 对现有多账号故障转移逻辑：零影响。`FailureLogStore.record` 是纯追加写入，不修改任何账号状态。
- 对 `provider.rs` 401/403 分支：追加一行 `store.record(...)` 调用，不影响重试/切换逻辑。
- 对 `token_manager.report_failure`：不触碰。
- 对 `ThrottleLogStore`：不触碰。
- 数据持久化：新增 `failure_log.json` 文件，旧部署无此文件时自动创建空列表（同 throttle_log.json 行为）。

## 风险

- **持久化文件增长**：每个账号最多 500 条，多账号场景下文件大小可控（每条约 300 字节，500 条 ≈ 150KB/账号）。
- **重复记录**：provider.rs 的重试循环中每次 401/403 都会记录，同一请求可能记录 1–3 条（因重试）。这是预期行为，与 throttle_log 一致。
- **并发写入**：多账号并发时多线程同时调用 `store.record()`，`RwLock` 写锁确保线程安全，写操作耗时约为 JSON 序列化 + 磁盘写，正常情况下不成为瓶颈（与 throttle_log 同样模式）。
- **注入遗漏静默失效**：`Option<Arc>` 注入为 `None` 时事件静默丢失。缓解措施：main.rs 初始化后打印确认日志。
