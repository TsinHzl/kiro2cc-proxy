# 任务清单：streamify-cc-v1-endpoint

## 状态：ARCHIVED

## 任务

- [x] **T1 — 为流式路径引入可选 deadline（行为不变）**
  `handlers.rs`：新增 `async fn wait_deadline(deadline: Option<Instant>)`（`None` 时
  `std::future::pending::<()>().await`）；`create_sse_stream` 新增
  `deadline: Option<Instant>` 参数并纳入 `unfold` state（5 元组 → 6 元组），
  在 `select!` 中新增 `_ = wait_deadline(deadline) => {...}` 分支发
  `overloaded_error` 后终止；**不加 `biased`**。`handle_stream_request` 新增
  `stream_deadline: Option<Duration>` 参数，`/v1` 调用点（handlers.rs:897-912）
  传 `None`。
  验证：`cargo check` + `cargo clippy` clean；`cargo test` 全绿（基线 412 passed）；
  `/v1` 无任何行为变化（deadline 恒 `None`）。
  spec.md 引用：需求「流式路径可选全局 deadline」全部场景 + 需求「/v1/messages 行为不变」

- [x] **T2 — 补 deadline 语义单元测试**
  在 `handlers.rs` 的 `#[cfg(test)]` 模块新增两个 `#[tokio::test]`：
  ① `wait_deadline(None)` 在 `tokio::time::timeout(50ms, _)` 下超时（永不就绪）；
  ② `wait_deadline(Some(Instant::now()))` 立即就绪。
  验证：`cargo test` 新增 2 个测试通过，总数 414 passed。
  spec.md 引用：场景「deadline 为 None 时不注册超时分支」

- [x] **T3 — 切换 /cc/v1 流式分派为实时转发**
  `handlers.rs:1721-1736`：`handle_stream_request_buffered(...)` 改为
  `handle_stream_request(...)`，11 个参数原样传递，末尾新增
  `Some(Duration::from_secs(300))`。
  验证：`cargo check` 通过（此时 `handle_stream_request_buffered` 出现
  dead_code 警告，属预期中间态）；`cargo test` 全绿。
  spec.md 引用：需求「/cc/v1/messages 流式响应实时转发」全部场景 + 场景「/cc/v1 撞 300s deadline」

- [x] **T4 — 删除缓冲实现（三处必须同批，否则无法编译）**
  删除 `handlers.rs` 的 `create_buffered_sse_stream`(1800-1937) 与
  `handle_stream_request_buffered`(1758-1799)，删除 `stream.rs` 的
  `BufferedStreamContext`(1533-1695) 全体；清理随之孤立的 import。
  验证：`cargo build --release` 零 warning；三个符号 grep 零命中；
  `cargo test` 全绿；`cargo clippy` clean。
  spec.md 引用：需求「移除 /cc/v1 缓冲模式」全部场景

- [x] **T5 — 更新代码内文档注释**
  `src/anthropic/mod.rs:14`（去掉「等待 contextUsageEvent 后再发送 message_start」）；
  `src/anthropic/handlers.rs:1586-1590`（`post_messages_cc` 的 doc comment 两条
  contextUsageEvent 描述 —— **T3+T4 的 CR 发现的清单遗漏**，原 grep 用「缓冲」
  关键字未命中该处）；
  `CLAUDE.md:42`（handlers.rs 行的「缓冲模式」）与 `CLAUDE.md:79`（`/cc/v1 vs /v1`
  整段改写为「流式转发 + 300s deadline + 25s ping」）。
  验证：四处 grep 不再出现「缓冲」「contextUsageEvent」描述。

- [x] **T6 — 更新 README.md**
  `README.md:628`（表格「缓冲模式，`input_tokens` 更准确」）与 `:631`（说明段）
  改为描述实时转发与 300s deadline。
  验证：`grep -n "缓冲" README.md` 零命中。

- [x] **T7 — 更新 docs/代码速查表.md**
  `:55`（流程图 `post_messages_cc` 描述）、`:375`（表格行描述 + 行号 1302→1593）、
  删除 `:376` `handle_stream_request_buffered` 与 `:377` `create_buffered_sse_stream`
  两个表格行、删除 `:420` `pub struct BufferedStreamContext` 表格行（**实施时
  核实发现的清单遗漏**）、补入 `wait_deadline` 表格行、修正 `estimate_tokens`
  因本次删除而漂移的行号（1696→1547）。
  `:354` 经核实为 `/v1` vs `/cc/v1` 路由差异说明，措辞正确无需改（原清单此项判断有误）。
  验证：`grep -n "缓冲\|buffered" docs/代码速查表.md` 仅剩 `:301`、`:411` 两处
  「解码器缓冲区」「thinking 标签缓冲区」的无关同形词。

- [x] **T8 — 更新 docs/源码全景解析.md**
  共 9 处：`:201`（contextUsageEvent 用途改为「设 stop_reason + 用量诊断」）、
  `:244-246`（调用链图 `StreamContext / BufferedStreamContext` 双路径改为单路径）、
  `:649`（特性列表「缓冲模式」改为「300s 上游挂起保护」）、`:966-967`（时序图
  `BufferedStreamContext 等待 contextUsageEvent`）、`:1065-1087`（整节重写为
  「/cc/v1 与 /v1 的唯一差异 — 300s 全局 deadline」，含删除对已不存在的
  `infer_cache_read_tokens()` 的引用）、`:1361`（message_start usage 注释）、
  `:1628`（ContextUsageEvent 帧格式说明的 `input_tokens = percentage × 1M / 100`）、
  `:1898-1899`（决策二问题/方案）、`:1921-1923`（风险1 描述与规避手段）。
  **原清单遗漏 `:244-246`、`:966-967` 两处（实施时 grep 核实发现）。**
  **原清单要求「决策二改写为历史决策 + 已废止说明」，但「已废止说明」本身含
  「缓冲模式」字样会撞验证 grep —— 用户裁定改为直接删除历史说明，文档只描述当前行为。**
  验证：`grep -n "缓冲模式\|infer_cache_read" docs/源码全景解析.md` 零命中 ✅
  （剩余 `:637` bytes crate「字节缓冲」、`:1737` 解码器「缓冲区」为无关同形词；
  `:179`/`:1612-1616`/`:1837`/`:1841` 的 contextUsageEvent 为 Kiro 协议本身与
  prompt caching 变更史的正确描述，保留）。

## 验收标准

- [x] `cargo build --release` 零 warning
- [x] `cargo fmt --check` 通过；`cargo clippy` **本次改动文件（handlers.rs / stream.rs / mod.rs）零 warning**
      （原标准「clippy 零 warning」立项时即不成立：仓库既存 66 个 warning —— `--all-targets` 下测试代码
      61 个 `field_reassign_with_default`、`src/token.rs:309` items-after-test-module、
      `src/cache/fingerprint.rs:203` needless_range_loop、`src/kiro/token_manager.rs:418` 等 4 个
      collapsible_if。本次未新增，也未减少）
- [x] `cargo test` 全绿，测试总数 ≥ 414（实测 414 passed；基线 412 + T2 新增 2）
- [x] `grep -rn "BufferedStreamContext\|handle_stream_request_buffered\|create_buffered_sse_stream" --include="*.rs" src/` 零命中
- [x] `grep -rn "缓冲模式\|infer_cache_read" README.md CLAUDE.md docs/代码速查表.md docs/源码全景解析.md src/anthropic/mod.rs` 零命中
      （范围较原标准收窄：`docs/Kiro缓存与状态管理解析.md:133,199` 的 `infer_cache_read_tokens`
      是明确标注「已于 2026-06-25 移除」的变更史记录，不属本次范围，保留）
- [x] `/v1/messages` 流式请求的 `create_sse_stream` 调用传入 `None`，行为与变更前逐字节一致
      （`handlers.rs:909` 传 `None`；`wait_deadline(None)` 走 `std::future::pending()` 永不就绪，
      `select!` 该分支等价于不存在，已由 T2 单元测试 `test_wait_deadline_none_never_resolves` 覆盖）
- [ ] 真机：Claude Code 指向 `/cc/v1`，回答逐字增量出现（非一次性整段）
- [ ] 真机：同一 turn 内含多个工具调用时，首个 `tool_use` 在后续内容生成期间即开始执行
- [ ] 真机：长上下文会话中 `/context` 显示百分比与后端 `[input_tokens] 本地化: estimated=N final=M` 日志一致
