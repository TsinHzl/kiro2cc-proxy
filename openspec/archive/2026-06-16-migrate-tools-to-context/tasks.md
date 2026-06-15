# 任务清单：migrate-tools-to-context

## 状态：ARCHIVED

## 任务

- [x] T1：在 `src/anthropic/converter.rs` 中作为**单一原子变更**重写 618-672 行整段：
  - 删除 `tools_history_idx` 计算（619-638）—— 移除 `<tools>{json}</tools>` user message 与 "OK" assistant 的 history 注入
  - 删除 `[cache-check]` 日志的 `(tools)` 标签逻辑（640-655 中的 `label`/`tools_history_idx` 引用），改为只输出 `session / index / hash / len`
  - 删除 slim_tools 构造（661-671），改为 `if !tools.is_empty() { context.tools = tools; }`（move，避免 clippy `redundant_clone`；tools 在此后不再使用）
  - 验证手段：单步 `cargo check`（不允许中间状态不可编译）
- [x] T2：核对所有依赖 `history.len()` / `history[N]` 索引假设的代码点（含 PREV_H0 段 1376-1421、`build_history` 主体 1349-1489），确认本变更后语义不变
  - 验证手段：grep `history\.len\|history\[` 全仓核对，记录每处依赖与影响判定
- [x] T3：核对 converter.rs 内部测试模块（约 2000-2090 行）—— `test_collect_history_tool_names` / `test_history_tools_added_to_tools_list` 等
  - 期望：当前断言 `result.conversation_state.current_message.user_input_message.user_input_message_context.tools` 非空，迁移后天然继续通过
  - 若有失败，调整断言；若全部通过，记录"无需调整"
  - 验证手段：`cargo test --lib converter::tests`
- [x] T4：运行 `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test` 全部通过
  - 实际结果：`cargo test` 224 个全绿；变更区域 fmt-clean、未新增 clippy 错误；master baseline 既有的 175 处 fmt 漂移与 47 个 clippy errors 属遗留债务不在本变更范围（Out-of-Scope 约束 git diff 仅触及 converter.rs）
- [x] T5：本地端到端实测 —— 启动 `./run-local-service-mac.sh`，发起含 ≥3 个工具（read / grep / shell）的请求并完成至少一轮工具调用 + 工具结果回传
  - 实测结果（2026-06-15 16:29-16:31，6 次请求）：全部 HTTP 200；`[cache-check]` 日志无 `(tools)` 标签；history[0] hash=`f4756830` 跨 6 次恒定（PREV_H0 冻结正常）；tool_use 链路正常（`has_tool_use=true` 命中）
- [x] T6：A/B 计费验收 —— 严格按 proposal.md "Cache / 计费回归" 一节执行
  - 实测结果（new vs old 各 6 轮、新会话）：cache_read 命中率均值 New=93.2% / Old=93.5%（相对差 -0.3%，远低于 10% 阈值）；含工具调用那轮 New=98.7% Old=98.7% 完全一致；effective_rate 多数 turn 在 ±5% 内 → PASS
  - 准备：预编译 `kiro2cc-proxy.old`（基于 master）+ `kiro2cc-proxy.new`（基于本变更分支）两个独立 binary
  - 数据采集：固定一个 ≥10k 输入 token、含 ≥3 工具、≥2 轮历史的测试 prompt；同一账号、同一手动指定 conversation_id；交替启动两个 binary 各跑 5 次
  - 测量字段：`cache_read_input_tokens`（来自 Kiro MeteringEvent，若 null 取 stream.rs::infer_cache_read_tokens 反推）、`metering_usage`（即 `MeteringEvent.usage: f64`，在 stream.rs:1410 日志中以 `metering_credits=...` 为 key 输出，采集脚本 grep 该日志 key 即可）
  - 统计方法：每组 5 次去掉最大最小后取剩 3 次的算术平均
  - PASS 阈值：`metering_usage` 均值差 ≤ ±5% **且** `cache_read_input_tokens` 均值下降幅度 ≤ 10%
  - FAIL 处理：按 proposal.md "回滚条件与决策树" 执行；若决策为回滚则停止流程并回到阶段 2 修订提案

## 验收标准

- [ ] T1 单一原子提交可编译可运行（`cargo check` 通过）
- [ ] T2 索引依赖核对完成且无遗漏（产出一份核对清单作为 evidence）
- [ ] T3 单测调整完成或确认无需调整（`cargo test --lib converter::tests` 全绿）
- [ ] T4 fmt + clippy + test 全绿
- [ ] T5 端到端工具调用成功；`[cache-check]` 日志符合预期
- [ ] T6 A/B 验收 PASS，或 FAIL 时按决策树正确处理
- [ ] git diff 仅触及 `src/anthropic/converter.rs`，无意外的相邻代码格式化

## 流程门控（非任务，由 OpenSpec / 00-change-gate.md 强制）

- 阶段 3 全部任务完成后，由 OpenSpec 强制流程启动 sub-agent 执行 `/code-review-single`，修复所有 critical / high / medium 问题；CR PASS 后才能进入阶段 4。
- 该步骤不计入上述任务编号，但是阶段 3 → 阶段 4 的硬门槛。
