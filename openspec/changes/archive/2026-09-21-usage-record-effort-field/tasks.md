# 任务清单：usage-record-effort-field

## 状态：ARCHIVED

## 任务
- [x] 1. `src/model/usage.rs`：`UsageRecord` 和 `UsageRecordItem` 各加 `effort: Option<String>` 字段（`#[serde(skip_serializing_if = "Option::is_none")]`）；`UsageTracker::record()` 函数签名加 `effort: Option<String>` 参数并写入记录；同步修改 `get_records_paged()`、`get_records_paged_by_credential()`、`get_records_paged_by_date()` 三处 `UsageRecord → UsageRecordItem` 映射，补传 `effort` 字段；更新现有 `tracker.record()` 测试调用处签名，并补充验证 effort 写入及 None 兜底的单元测试
- [x] 2. `src/anthropic/stream.rs`：`StreamContext` 加 `effort: Option<String>` 字段；`new_with_thinking()` 加 `effort` 参数；`handle_stream_request()` 函数签名加 `effort: Option<String>` 参数并传入 `new_with_thinking()`；流结束时 `tracker.record()` 传入 `self.effort.clone()`；同步更新 `stream.rs` 测试模块中约 30 处 `new_with_thinking()` 调用（`cargo check` 可发现遗漏）
- [x] 3. `src/anthropic/handlers.rs`：`post_messages()` 中提取 `effort`（`payload.output_config.as_ref().map(|c| c.effort.clone())`）并传入 `handle_stream_request()` 调用处；非流式路径的 `tracker.record()` 调用处（`handle_non_stream_request` 内）同样补传 effort
- [x] 4. `admin-ui/src/types/api.ts`：`UsageRecord` 接口加 `effort?: string`
- [x] 5. `admin-ui/src/components/usage-log-table.tsx`：模型名单元格改为 flex 列，model 名下方条件渲染 effort badge（样式参照页面现有 badge 变量，字号 10px，橙色背景，无 effort 时不渲染）

## 验收标准
- [ ] `cargo check` + `cargo test` 全部通过，无新增 warning
- [ ] 流式请求完成后，usage log API 返回的记录含 `effort` 字段且值正确
- [ ] 前端模型名下方显示橙色 effort badge（如 `high`），badge 样式为圆角小标签，字号 10px
- [ ] `output_config` 整体缺失的请求，记录 effort 为 None，前端不渲染 badge
