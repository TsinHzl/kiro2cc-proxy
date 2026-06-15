# 变更提案：align-model-specs-to-official

## 背景

当前 `kiro2cc-proxy` 中的模型上下文窗口 / 最大输出 token / 路由判断与 Anthropic 官方 2026-06 规格（来源：`claude-api` skill 文档表）不一致。

| 模型 | 官方上下文 | 当前 `context_window_for_model` | 官方 max_output | 当前 `build_model_list` | 当前 `map_model` |
|------|-----------|----|------|----|----|
| `claude-fable-5` | 1,000,000 | **未识别 → 200K** | 128,000 | **未列出** | **未识别 → None** |
| `claude-opus-4-8` | 1,000,000 | 1,000,000 ✅ | 128,000 | 128,000 ✅ | 4-8 ✅ |
| `claude-opus-4-7` | 1,000,000 | 1,000,000 ✅ | 128,000 | 128,000 ✅ | 4-7 ✅ |
| `claude-opus-4-6` | 1,000,000 | **200,000 ❌** | 128,000 | **64,000 ❌** | 默认 4-6 ✅ |
| `claude-sonnet-4-6` | 1,000,000 | 1,000,000 ✅ | 64,000（保持） | 64,000 ✅ | 4-6 ✅ |
| `claude-haiku-4-5` | 200,000 | 200,000 ✅ | 64,000 | 64,000 ✅ | haiku ✅ |

来源说明：
- 上下文 / max_output 数据来自 `claude-api` skill 中"Current Models (cached: 2026-06-04)"表（位于 `~/.claude/plugins/cache/anthropic-agent-skills/document-skills/.../skills/claude-api/SKILL.md`，引用自 Anthropic 官方文档）。
- sonnet-4.6 / haiku-4.5 的 max_output（64K）与 Anthropic 官方表一致，本次保持不变。

错配会导致：
1. `cap_input_tokens()` 把 opus-4.6 合法大上下文请求截断到 200K。
2. `infer_cache_read_tokens()` 对 opus-4.6 走 k_ref=2.40，与 Anthropic 官方 .00/.00 与 opus-4.7/4.8 同价的事实不符（用户已确认同档合理）。
3. `/v1/models` 列表向客户端暴露 opus-4.6 max_tokens=64000，迫使上层 SDK 提前截断。
4. fable-5 完全无法被识别：`/v1/models` 不暴露、`context_window_for_model` 默认 200K、`map_model` 返回 None → 实际请求拒绝在代理本地。

## 目标范围

**在范围内：**
- `src/anthropic/stream.rs::context_window_for_model()` — 在已有 1M 分支基础上**追加** `opus-4-6/4.6` 与 `fable-5` 两个匹配条件，已有 4-7/4-8/sonnet-4-6 分支保持不变。
- `src/anthropic/stream.rs::infer_cache_read_tokens()` —
  - opus 系列 k_ref=2.60 分支条件追加 `4-6 / 4.6`（与 4-7/4-8 同档；依据：官方 input/output 单价 .00/.00 已对齐）；
  - 新增独立 `model.contains("fable")` 分支，沿用 `(2.60, 15.0, 75.0)` 三元组作占位（注释明确标记"待实测，可能存在偏差"）。
- `src/anthropic/handlers.rs::build_model_list()` —
  - 修改 `claude-opus-4-6` / `claude-opus-4-6-thinking` 的 `max_tokens` 从 64000 → 128000（其余字段不变）；
  - 追加 `claude-fable-5`（display="Claude Fable 5"，max_tokens=128000，owned_by="anthropic"，object="model"，model_type="chat"，created=1772582400 即 2026-03-04 UTC 占位）；
  - 追加 `claude-fable-5-thinking`（display="Claude Fable 5 (Thinking)"，其余同上）。
- `src/anthropic/converter.rs::map_model()` — 追加 `model_lower.contains("fable")` 分支返回 `Some("claude-fable-5".to_string())`，与现有 sonnet/opus/haiku 分支同级。
- 单元测试 —— 在 `src/anthropic/stream.rs` 末尾的 `#[cfg(test)] mod tests` 块内新增对 `context_window_for_model` 和 `infer_cache_read_tokens` 的断言；在 `src/anthropic/handlers.rs` 末尾新增（如不存在）`#[cfg(test)] mod tests` 块对 `build_model_list` 的断言；在 `src/anthropic/converter.rs` 末尾的 `#[cfg(test)] mod tests` 块内对 `map_model("claude-fable-5")` 的断言。
  - 不修改 `src/test.rs`（该文件仅是手动调试入口，非测试模块）。

**不在范围内：**
- pricing / usage 显示子系统的单价表更新。
- Kiro 上游对 opus-4.6 1M 窗口与 fable-5 是否实际启用的实测验证（用户已选择"直接对齐官方，不验证"）。
- 其他模型（haiku-4-5、sonnet-4-6、claude-3.x、claude-4 / 4.5、deepseek、glm 等）的规格不动。
- `/v1/models` 中已存在的旧版模型 ID（claude-3-5-sonnet-20241022 等）不动。

## 技术方案

### 1. `context_window_for_model()` 扩展（追加，不替换）
当前匹配 4-7/4-8/sonnet-4-6 → 1M。本次仅**追加**两个匹配条件：
```rust
m if m.contains("opus-4-8") || m.contains("opus-4.8")
   || m.contains("opus-4-7") || m.contains("opus-4.7")
   || m.contains("opus-4-6") || m.contains("opus-4.6")     // 新增
   || m.contains("sonnet-4-6") || m.contains("sonnet-4.6")
   || m.contains("fable-5") || m.contains("fable_5") => 1_000_000,
_ => 200_000,
```
注释从「opus-4.7/4.8/sonnet-4.6 实测按 1M 窗口」更新为「opus-4.6/4.7/4.8、sonnet-4.6、fable-5 按 1M 窗口（官方公布值；fable-5 与 opus-4.6 未在 Kiro 上游实测）」。

### 2. `infer_cache_read_tokens()` 调整
```rust
let (k_ref, input_price, output_price): (f64, f64, f64) = if model.contains("opus") {
    if model.contains("4-6") || model.contains("4.6")
        || model.contains("4-7") || model.contains("4.7")
        || model.contains("4-8") || model.contains("4.8") {
        (2.60, 15.0, 75.0)   // 4.6/4.7/4.8 同档：官方均 .00/.00
    } else {
        (2.40, 15.0, 75.0)   // 4.5 及更早
    }
} else if model.contains("fable") {
    // 占位：fable-5 单价未实测，临时沿用 opus 顶端档位
    // 影响：cache_read_input_tokens 反推值进入 usage 上报，存在估算偏差
    // 后续行动：抓 Kiro fable-5 metering_credits 后修正
    (2.60, 15.0, 75.0)
} else if model.contains("haiku") {
    return None;
} else {
    (7.06, 3.0, 15.0)
};
```

### 3. `build_model_list()` 改三处
- `claude-opus-4-6` 行：`max_tokens: 64000` → `128000`。
- `claude-opus-4-6-thinking` 行：同上。
- 在 opus-4-8-thinking 与 haiku-4-5 之间追加 fable-5 / fable-5-thinking 两条（字段值见上文）。

### 4. `map_model()` 路由扩展
在现有 sonnet / opus / haiku 三个 `else if` 之间追加：
```rust
} else if model_lower.contains("fable") {
    Some("claude-fable-5".to_string())
}
```
位置：sonnet 分支后、opus 分支前（避免被 opus 含 "5" 字面意外吞噬；实际 sonnet/opus 均不含 "fable"，顺序不强约束，但保持紧凑）。

### 5. 单元测试（新增断言，目标文件正确性已 sub-agent 复核）
- `src/anthropic/stream.rs` 内 `mod tests`：
  - `context_window_for_model("claude-opus-4-6") == 1_000_000`
  - `context_window_for_model("claude-opus-4-6-thinking") == 1_000_000`
  - `context_window_for_model("claude-fable-5") == 1_000_000`
  - `context_window_for_model("claude-haiku-4-5-20251001") == 200_000`（回归）
  - `context_window_for_model("claude-sonnet-4-6") == 1_000_000`（回归）
  - `infer_cache_read_tokens(1000, Some(0.0234), 0, "claude-opus-4-6")` 返回 `Some(_)` 且数值落在 `[0, 1000]`（确保进入新分支不 panic / 不返回 None）。
  - `infer_cache_read_tokens(1000, Some(0.0234), 0, "claude-fable-5")` 同上。
- `src/anthropic/converter.rs` 内 `mod tests`（已存在）：
  - `map_model("claude-fable-5") == Some("claude-fable-5".to_string())`
  - `map_model("claude-opus-4-6") == Some("claude-opus-4.6".to_string())`（回归）
- `src/anthropic/handlers.rs`：
  - 若已有 `mod tests`，添加；否则新建 `#[cfg(test)] mod tests { use super::*; ... }`。
  - `build_model_list().iter().find(|m| m.id == "claude-opus-4-6").unwrap().max_tokens == 128000`
  - `build_model_list().iter().find(|m| m.id == "claude-opus-4-6-thinking").unwrap().max_tokens == 128000`
  - `build_model_list().iter().any(|m| m.id == "claude-fable-5")`
  - `build_model_list().iter().any(|m| m.id == "claude-fable-5-thinking")`
  - `build_model_list().iter().find(|m| m.id == "claude-haiku-4-5-20251001").unwrap().max_tokens == 64000`（回归）
  - `build_model_list().iter().find(|m| m.id == "claude-opus-4-7").unwrap().max_tokens == 128000`（回归）

## 预期影响

- **行为变更：** opus-4.6 大上下文请求不再被代理截断；客户端通过 `/v1/models` 看到 opus-4.6 / fable-5 真实输出能力；fable-5 请求能被路由到上游。
- **计费精度：** opus-4.6 的 `cache_read_input_tokens` 反推从 k_ref=2.40 改为 2.60，与官方价格档对齐。
- **兼容性：** 纯增强 + 修正，不删除任何模型 ID；旧客户端无破坏。
- **风险承担：** 用户已接受不对 Kiro 上游实测验证 — 若 Kiro 实际不支持，回滚为单 commit `git revert`。

## 风险

| 风险 | 概率 | 影响 | 应对 |
|------|------|------|------|
| Kiro 上游 opus-4.6 实际仍是 200K/64K | 中 | 客户端长上下文请求得 502/400 | 用户已接受；回滚 `git revert <commit>` |
| Kiro 上游未启用 fable-5 | 高 | 客户端选 fable-5 时 Kiro 返回 model_not_found | 错误透传；用户可手动切换 opus-4.8；map_model 已修，不构成本地拦截 |
| `infer_cache_read_tokens` fable-5 占位偏差 | 中 | usage 上报中 `cache_read_input_tokens` 反推值与实际偏差，可能影响计费展示 | 注释明确"待实测"；本次不修，后续抓包后单独 PR 修正 |
| opus-4.6 k_ref 从 2.40 升到 2.60 反推数值变化 | 低 | 历史用量统计前后不可比 | 在 commit message 中说明；usage 仅作展示，不影响实际计费 |

## A/B 验收标准（可量化）

1. **静态：** `cargo fmt --check` 无 diff。
2. **静态：** `cargo clippy --all-targets -- -D warnings` 无 warning / error。
3. **静态：** `cargo test` 全部通过，含本次新增的 13 条断言。
4. **接口（启动后人工核验）：** `curl -s http://localhost:8080/v1/models | jq '.data[] | select(.id=="claude-opus-4-6") | .max_tokens'` 输出 `128000`。
5. **接口：** `curl -s http://localhost:8080/v1/models | jq '.data[] | select(.id=="claude-fable-5") | {id, max_tokens, owned_by}'` 输出 `{"id":"claude-fable-5","max_tokens":128000,"owned_by":"anthropic"}`。
6. **回归：** `curl -s http://localhost:8080/v1/models | jq '.data[] | select(.id=="claude-haiku-4-5-20251001") | .max_tokens'` 输出 `64000`（不变）。
7. **回归：** `curl -s http://localhost:8080/v1/models | jq '.data[] | select(.id=="claude-opus-4-7") | .max_tokens'` 输出 `128000`（不变）。
8. **回归：** `curl -s http://localhost:8080/v1/models | jq '.data[] | select(.id=="claude-sonnet-4-6") | .max_tokens'` 输出 `64000`（不变）。
