# 任务清单：add-throttle-log

## 状态：ARCHIVED

## 任务
- [x] 后端：新建 `src/model/throttle_log.rs`，定义 ThrottleEvent 结构和 ThrottleLogStore
- [x] 后端：在 `src/model/mod.rs` 中注册新模块
- [x] 后端：在应用初始化时加载 ThrottleLogStore 并注入共享状态
- [x] 后端：修改 `src/kiro/provider.rs` 两处 429 处理，写入限流事件
- [x] 后端：新增 API handler `get_throttle_logs` 并注册路由
- [x] 前端：新增 `ThrottleLogRecord` 类型和 API 函数
- [x] 前端：新增 `useThrottleLogs` hook
- [x] 前端：新建 `throttle-log-page.tsx` 组件
- [x] 前端：修改 `dashboard.tsx` 添加限流日志页导航状态
- [x] 前端：修改 `credential-card.tsx` 让"限流：N"可点击

## 验收标准
- [ ] 当账号收到 429 响应时，限流事件被持久化记录
- [ ] API 端点 `GET /api/admin/credentials/{id}/throttle-logs` 返回正确分页 JSON 响应
- [ ] 点击卡片中的"限流：N"文案，导航到该账号的限流日志详情页
- [ ] 限流日志页展示分页列表（时间、请求类型、响应摘要）
- [ ] 每个 credential 最多保留 500 条限流记录，超出淘汰最旧记录
- [ ] `cargo build` 和前端 `npm run build` 均通过
