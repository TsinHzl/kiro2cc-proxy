# 变更提案：add-throttle-log

## 背景
账号管理页面的卡片中展示了"限流：N"累计计数，但管理员无法查看具体的限流事件详情（何时发生、哪个请求类型触发等）。需要让该文案可点击，进入限流日志详情页查看历史记录。

## 目标范围
**在范围内：**
- 后端新增 ThrottleEvent 记录结构，在 429 响应时写入事件日志
- 后端新增 API 端点 `GET /credentials/{id}/throttle-logs` 返回分页限流日志
- 前端新增 ThrottleLogPage 组件展示限流日志（时间、请求类型、响应体摘要）
- 前端 credential-card 中"限流：N"文案改为可点击，点击导航到限流日志页

**不在范围内：**
- 修改现有 UsageRecord 结构
- 限流事件的报警/通知功能
- 限流日志导出功能
- User UI（仅 Admin UI）

## 技术方案

### 后端
1. **新增 `ThrottleEvent` 结构**（`src/model/throttle_log.rs`）：
   - `credential_id: u64` — 被限流的账号 ID
   - `request_type: String` — "api" 或 "mcp"（区分两个调用路径）
   - `status_code: u16` — HTTP 状态码（429）
   - `response_body: String` — 响应体摘要（截取前 200 字符）
   - `created_at: DateTime<Utc>` — 事件时间

2. **新增 `ThrottleLogStore`**：与 UsageTracker 类似，基于 `Vec<ThrottleEvent>` + RwLock + JSON 文件持久化。每个 credential 最多保留 500 条。

3. **记录时机**：在 `provider.rs` 两处 429 处理逻辑中（line 429 和 line 625），调用 `throttle_log_store.record(...)` 记录事件。

4. **新增 API 端点**：`GET /api/admin/credentials/{id}/throttle-logs?page=1&page_size=50`，返回与 `UsageRecordsPage` 类似的分页结构。

### 前端
1. **新增类型** `ThrottleLogRecord` 和 `ThrottleLogsResponse`（`types/api.ts`）
2. **新增 API 函数** `getThrottleLogs(id, page, pageSize)`（`api/credentials.ts`）
3. **新增 hook** `useThrottleLogs(id, page, pageSize)`（`hooks/use-credentials.ts`）
4. **新增组件** `ThrottleLogPage`（`components/throttle-log-page.tsx`）— 表格展示时间、请求类型、响应摘要
5. **dashboard.tsx** 新增 `throttleLogCredentialId` 状态和路由分支
6. **credential-card.tsx** 将"限流：N"改为可点击 span，点击调用新的 `onViewThrottleLog(id)` 回调

## 预期影响
- 新增独立的 throttle_log.json 持久化文件（每 credential 上限 500 条，总体内存占用可控）
- provider.rs 每次 429 增加一次写操作（非热路径，对性能影响可忽略）
- 前端增加一个新页面，不影响现有页面

## 风险
- **磁盘写入频率**：高并发限流场景下频繁写 JSON 文件。应对：复用 UsageTracker 的 debounce 策略或简单 append-save。
- **文件膨胀**：上限 500 条/credential，清理最旧记录。
- **重试循环多条记录**：同一客户端请求在重试循环中可能连续触发多个 credential 的 429，每次都会独立记录一条 ThrottleEvent。这是预期行为——记录维度是 per-credential（哪个账号在何时被限流），而非 per-request。
