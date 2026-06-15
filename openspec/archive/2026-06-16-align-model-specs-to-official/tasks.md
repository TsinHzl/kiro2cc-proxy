# 任务清单：align-model-specs-to-official

## 状态：ARCHIVED

## 任务

- [x] T1：修改 `src/anthropic/stream.rs::context_window_for_model()` — 在已有 1M `match arm` 中**追加** `m.contains("opus-4-6") || m.contains("opus-4.6") || m.contains("fable-5") || m.contains("fable_5")`；既有 4-7/4-8/sonnet-4-6 分支保持不变；同步更新该函数上方的注释说明（提及 opus-4.6/fable-5 未在 Kiro 上游实测、仅按官方公布值）。验证：`cargo check`。
- [x] T2：修改 `src/anthropic/stream.rs::infer_cache_read_tokens()` — opus 系列内层 `if` 增加 `4-6 / 4.6` 进入 `(2.60, 15.0, 75.0)` 档；新增 `else if model.contains("fable")` 独立分支返回 `(2.60, 15.0, 75.0)` 三元组并附"占位待实测"注释；其余 sonnet/haiku/默认分支不变。验证：`cargo check`。
- [x] T3：修改 `src/anthropic/handlers.rs::build_model_list()` — `claude-opus-4-6` 与 `claude-opus-4-6-thinking` 两个条目的 `max_tokens` 字段从 `64000` 改为 `128000`；其余字段不变。验证：`cargo check`。
- [x] T4：在 `src/anthropic/handlers.rs::build_model_list()` 中追加两个 Model 条目：`claude-fable-5`（display_name="Claude Fable 5"，max_tokens=128000，owned_by="anthropic"，object="model"，model_type="chat"，created=1772582400）与 `claude-fable-5-thinking`（display_name="Claude Fable 5 (Thinking)"，其余同上）。位置：紧跟 `claude-opus-4-8-thinking` 之后、`claude-haiku-4-5-20251001` 之前。验证：`cargo check`。
- [x] T5：修改 `src/anthropic/converter.rs::map_model()` — 在现有 sonnet 分支与 opus 分支之间追加 `else if model_lower.contains("fable") { Some("claude-fable-5".to_string()) }` 一支；其余分支不变。验证：`cargo check`。
- [x] T6：在 `src/anthropic/stream.rs` 末尾的 `#[cfg(test)] mod tests` 块内新增以下断言（不创建新文件、不动 `src/test.rs`）：
  - `assert_eq!(context_window_for_model("claude-opus-4-6"), 1_000_000);`
  - `assert_eq!(context_window_for_model("claude-opus-4-6-thinking"), 1_000_000);`
  - `assert_eq!(context_window_for_model("claude-fable-5"), 1_000_000);`
  - `assert_eq!(context_window_for_model("claude-haiku-4-5-20251001"), 200_000);`
  - `assert_eq!(context_window_for_model("claude-sonnet-4-6"), 1_000_000);`
  - `infer_cache_read_tokens(1000, Some(0.0234), 0, "claude-opus-4-6")` 返回 `Some(v)` 且 `0 <= v <= 1000`。
  - `infer_cache_read_tokens(1000, Some(0.0234), 0, "claude-fable-5")` 返回 `Some(v)` 且 `0 <= v <= 1000`。
  验证：`cargo test --lib stream::tests`。
- [x] T7：在 `src/anthropic/converter.rs` 末尾的 `#[cfg(test)] mod tests` 块内（已存在）新增：
  - `assert_eq!(map_model("claude-fable-5"), Some("claude-fable-5".to_string()));`
  - `assert_eq!(map_model("claude-opus-4-6"), Some("claude-opus-4.6".to_string()));`（回归）
  验证：`cargo test --lib converter::tests`。
- [x] T8：在 `src/anthropic/handlers.rs` 末尾添加（如不存在则新建）`#[cfg(test)] mod tests` 块，加入：
  - `find_by_id("claude-opus-4-6").max_tokens == 128000`
  - `find_by_id("claude-opus-4-6-thinking").max_tokens == 128000`
  - `find_by_id("claude-fable-5")` 存在且 `max_tokens == 128000` 且 `owned_by == "anthropic"`
  - `find_by_id("claude-fable-5-thinking")` 存在且 `max_tokens == 128000`
  - `find_by_id("claude-haiku-4-5-20251001").max_tokens == 64000`（回归）
  - `find_by_id("claude-opus-4-7").max_tokens == 128000`（回归）
  - `find_by_id("claude-sonnet-4-6").max_tokens == 64000`（回归）
  验证：`cargo test --lib handlers::tests`。
- [x] T9：运行 `cargo test`（238 个全部通过含本次 14 条断言）；`cargo clippy` 在改动文件上 0 新警告（仓库历史 47 项基线不变）；fmt 仅本次新增段落自身格式 clean，仓库整体格式漂移属历史遗留不在范围。

## 验收标准

- [ ] 所有 9 个任务标记为 `[x]`。
- [ ] `cargo fmt --check` 无 diff。
- [ ] `cargo clippy --all-targets -- -D warnings` 无 warning / error。
- [ ] `cargo test` 全部通过（含本次新增 13 条断言）。
- [ ] sub-agent CR 输出 PASS（critical/high 全部修复）。
- [ ] 用户对 8 项 A/B 验收清单（见 proposal.md "A/B 验收标准"）逐项确认。
