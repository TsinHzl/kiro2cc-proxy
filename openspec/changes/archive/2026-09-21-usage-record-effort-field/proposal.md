# 变更提案：usage-record-effort-field

## 背景
使用日志每条记录已展示模型名，但缺少该请求使用的 effort 级别（low/medium/high/xhigh/max）。
用户无法从日志中判断某次请求用了哪个推理力度，不利于排查 TTFB 异常或 credits 消耗对比。

## 目标范围
**在范围内：**
- `UsageRecord`（内存存储结构）新增 `effort: Option<String>` 字段
- `UsageRecordItem`（API 响应结构）新增 `effort: Option<String>` 字段
- `UsageTracker::record()` 函数签名新增 `effort: Option<String>` 参数并写入记录
- 流式路径（`stream.rs` StreamContext）：新增 `effort` 字段，`new_with_thinking()` 和 `handle_stream_request()` 函数签名各加一个 `effort: Option<String>` 参数，流结束时随 `tracker.record()` 写入
- 非流式路径（`handlers.rs`）：`tracker.record()` 调用处补传 effort 参数
- 前端 `admin-ui/src/types/api.ts`：`UsageRecord` 接口加 `effort?: string`
- `admin-ui/src/components/usage-log-table.tsx`：模型名单元格下方渲染 effort 标签（橙色小 badge，无 effort 时不渲染）

**不在范围内：**
- 不新增额外的持久化机制（effort 字段将随现有 UsageTracker 后台刷盘机制自动落地，无需额外工作）
- 按 effort 筛选或统计
- throttle log、failure log 等其他日志页面
- `user-ui`（无 usage log 页面，不涉及）

## 技术方案
- **effort 来源**：请求的 `output_config` 字段整体存在时取 `effort` 字符串（serde 默认值为 `"high"`，不会出现空串）；`output_config` 整体为 `None` 时（客户端未传该字段）存 `None`
- **流式路径传递链**：`post_messages()` → 提取 effort → 传入 `handle_stream_request()` → 传入 `StreamContext::new_with_thinking()` → 存为 `self.effort` → 流结束时传入 `tracker.record()`
- **非流式路径**：`handlers.rs` 完成响应后直接从请求体取值传入 `tracker.record()`
- **向后兼容**：`Option<String>` + `#[serde(skip_serializing_if = "Option::is_none")]`，旧记录 effort 为 None，前端不渲染标签

## 预期影响
- `tracker.record()`、`handle_stream_request()`、`new_with_thinking()` 函数签名各新增一个参数，所有调用方需同步修改
- 前端模型名单元格新增 effort badge（橙色，字号与现有 badge 一致，换行显示于模型名下方）

## 风险
- `Option<String>` 字段向后兼容，无现有行为破坏
- 函数签名变更影响编译，须一次性改完所有调用方，`cargo check` 可即时发现遗漏
