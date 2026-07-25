# Claude Opus 5 支持实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 kiro2cc-proxy 代理侧接入 Kiro 2026-07-25 上线的 `claude-opus-5` 模型（rateMultiplier 2.2、maxInputTokens 1M、maxOutputTokens 128K），与现有 `claude-opus-4.7/4.8` 全套接入路径对称。

**Architecture:** 6 处对称补丁 — `map_model` 别名、`model_max_output_tokens` 上限 128000、`is_large_window_model` 大窗口系数 0.15、`get_k_ref` 同档 2.36、`build_model_list` 静态表、`admin/service.rs` 抓包测试样例。每个改动点配独立单测，不引入新模块/接口/依赖。

**Tech Stack:** Rust 2024 edition、axum、tokio、serde_json、parking_lot；Rust 内置 `#[cfg(test)]`。

**Spec:** `docs/superpowers/specs/2026-07-25-claude-opus-5-support-design.md`

## Global Constraints

- Rust 2024 edition（来自 `Cargo.toml`）
- 仅修改既有 6 处函数/测试，不新增文件（除测试代码与既有同文件内追加）
- 不引入新 crate、不修改 `Cargo.toml`
- 必须 `cargo fmt --check` 与 `cargo clippy --all-targets -- -D warnings` 通过
- 必须 `cargo check` 与 `cargo test` 全部通过
- 不回归现有 opus-4.5/4.6/4.7/4.8/sonnet-5/fable-5/gpt-5.6-* 行为
- 所有 match-style 顺序敏感（opus-5 分支必须在 4-5 之前）
- `created` 字段使用 unix 时间戳；opus-5 条目使用 `1777500000`（约 2026-07-25）

---

## File Structure

| 文件 | 职责 | 改动类型 |
|---|---|---|
| `src/anthropic/converter.rs` | Anthropic ↔ Kiro 协议转换、模型映射 | 修改 2 处（`map_model`、`model_max_output_tokens`） |
| `src/anthropic/stream.rs` | SSE 流式状态机、上下文窗口/缩放 | 修改 1 处（`is_large_window_model`） |
| `src/model/usage.rs` | 用量追踪、定价、k_ref | 修改 1 处（`get_k_ref`） |
| `src/anthropic/handlers.rs` | HTTP 路由 + 静态模型列表 | 修改 1 处（`build_model_list`） |
| `src/admin/service.rs` | Admin REST 服务 + 抓包反序列化测试 | 修改 1 处（追加 opus-5 抓包样例） |
| `docs/代码速查表.md` + `docs/源码全景解析.md` | 项目文档 | 同步 opus-5 说明 |

---

## Task 1: `converter.rs::map_model` 新增 opus-5 别名

**Files:**
- Modify: `src/anthropic/converter.rs:412-477`（`map_model` 函数 + 顶部 doc comment）
- Test: 同文件内 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: 无外部依赖
- Produces: `pub fn map_model(model: &str) -> Option<String>` 增加 `opus-5/opus.5/opus-5` 字符串识别

- [ ] **Step 1: 写失败测试**

在 `src/anthropic/converter.rs` 文件尾部 `#[cfg(test)] mod tests` 模块中（紧邻 `test_map_model_thinking_suffix_sonnet` 之后），新增：

```rust
#[test]
fn test_map_model_opus_5_aliases() {
    assert_eq!(map_model("claude-opus-5").unwrap(), "claude-opus-5");
    assert_eq!(map_model("claude-opus-5-thinking").unwrap(), "claude-opus-5");
    assert_eq!(map_model("Claude Opus 5").unwrap(), "claude-opus-5");
    assert_eq!(map_model("claude-opus-5-20260101").unwrap(), "claude-opus-5");
    assert_eq!(
        map_model("claude-opus-5-thinking-20260101").unwrap(),
        "claude-opus-5"
    );
    assert_eq!(map_model("claude-Opus-5").unwrap(), "claude-opus-5");

    // 回归：现有 opus-4.7/4.8 不被 opus-5 分支误命中
    assert_eq!(map_model("claude-opus-4-7-20251115").unwrap(), "claude-opus-4.7");
    assert_eq!(map_model("claude-opus-4.8").unwrap(), "claude-opus-4.8");
    assert_eq!(map_model("claude-opus-4.6").unwrap(), "claude-opus-4.6");
    assert_eq!(map_model("claude-opus-4.5").unwrap(), "claude-opus-4.5");
    assert_eq!(map_model("claude-sonnet-5").unwrap(), "claude-sonnet-5");
}
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cargo test --lib anthropic::converter::tests::test_map_model_opus_5_aliases -- --nocapture
```

预期：FAIL，第一个断言实际返回 `Some("claude-opus-4.6")`（兜底分支）而非 `Some("claude-opus-5")`。

- [ ] **Step 3: 修改 `map_model` 函数**

修改 `src/anthropic/converter.rs:412-477`：

```rust
/// 模型映射：将 Anthropic 模型名映射到 Kiro 模型 ID
///
/// 按照用户要求：
/// - opus 5/5 → claude-opus-5
/// - opus 4.5/4-5 → claude-opus-4.5
/// - opus 4.8/4-8 → claude-opus-4.8
/// - opus 4.7/4-7 → claude-opus-4.7
/// - 其他 opus → claude-opus-4.6
/// - sonnet 4.6/4-6 → claude-sonnet-4.6
/// - 其他 sonnet → claude-sonnet-4.5
/// - 所有 haiku → claude-haiku-4.5
pub fn map_model(model: &str) -> Option<String> {
    let model_lower = model.to_lowercase();

    if model_lower.contains("sonnet") {
        if model_lower.contains("4-6") || model_lower.contains("4.6") {
            Some("claude-sonnet-4.6".to_string())
        } else if model_lower.contains("sonnet-5") || model_lower.contains("sonnet.5") {
            // claude-sonnet-5: Max Input 1M, Max Output 64K, Rate 1.3 Credit（与 sonnet-4.x 同档）
            Some("claude-sonnet-5".to_string())
        } else {
            Some("claude-sonnet-4.5".to_string())
        }
    } else if model_lower.contains("fable") {
        Some("claude-fable-5".to_string())
    } else if model_lower.contains("opus") {
        if model_lower.contains("opus-5") || model_lower.contains("opus.5") {
            // claude-opus-5: Max Input 1M, Max Output 128K, Rate 2.2 Credit（与 4.7/4.8 同档）
            Some("claude-opus-5".to_string())
        } else if model_lower.contains("4-5") || model_lower.contains("4.5") {
            Some("claude-opus-4.5".to_string())
        } else if model_lower.contains("4-8") || model_lower.contains("4.8") {
            Some("claude-opus-4.8".to_string())
        } else if model_lower.contains("4-7") || model_lower.contains("4.7") {
            Some("claude-opus-4.7".to_string())
        } else {
            Some("claude-opus-4.6".to_string())
        }
    } else if model_lower.contains("haiku") {
        Some("claude-haiku-4.5".to_string())
    } else if model_lower == "auto" {
        Some("auto".to_string())
    } else if model_lower.contains("deepseek") {
        Some("deepseek-3.2".to_string())
    } else if model_lower.contains("glm") {
        Some("glm-5".to_string())
    } else if model_lower.contains("minimax") {
        if model_lower.contains("2.5") || model_lower.contains("2-5") {
            Some("minimax-m2.5".to_string())
        } else {
            Some("minimax-m2.1".to_string())
        }
    } else if model_lower.contains("qwen") {
        Some("qwen3-coder-next".to_string())
    } else if model_lower.contains("gpt") {
        if model_lower.contains("terra") {
            Some("gpt-5.6-terra".to_string())
        } else if model_lower.contains("luna") {
            Some("gpt-5.6-luna".to_string())
        } else if model_lower.contains("sol")
            || model_lower.contains("5.6")
            || model_lower.contains("5-6")
        {
            Some("gpt-5.6-sol".to_string())
        } else {
            None
        }
    } else {
        None
    }
}
```

关键修改：
1. 顶部注释 `opus 4.5/4-5` 前插入 `opus 5/5 → claude-opus-5`
2. opus 分支最前插入 `opus-5/opus.5/opus-5` 判断（必须在 4-5 之前）

- [ ] **Step 4: 运行测试验证通过**

```bash
cargo test --lib anthropic::converter::tests::test_map_model_opus_5_aliases
```

预期：PASS。

- [ ] **Step 5: 全测试 + lint 验证无回归**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
```

预期：全部通过。

- [ ] **Step 6: 提交**

```bash
git add src/anthropic/converter.rs
git commit -m "feat(model): map_model 接入 claude-opus-5 别名

与 4.7/4.8/sonnet-5 风格对齐，新增 opus-5/opus.5/opus-5 字符串
识别，统一映射到 claude-opus-5。opus 分支顺序敏感：opus-5
必须在 4-5 之前以避免被吞并。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

## Task 2: `converter.rs::model_max_output_tokens` opus-5 → 128000

**Files:**
- Modify: `src/anthropic/converter.rs:1356-1367`（`model_max_output_tokens` 函数）
- Test: 同文件内 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: 无外部依赖
- Produces: `fn model_max_output_tokens(model: &str) -> i32` 对 opus-5 字符串返回 128000

- [ ] **Step 1: 写失败测试**

在 `src/anthropic/converter.rs` 文件尾部 `#[cfg(test)] mod tests` 中新增：

```rust
#[test]
fn test_model_max_output_tokens_opus_5() {
    assert_eq!(model_max_output_tokens("claude-opus-5"), 128000);
    assert_eq!(model_max_output_tokens("claude-opus-5-thinking"), 128000);
    assert_eq!(model_max_output_tokens("Claude-Opus-5"), 128000);

    // 回归：其他档位不变
    assert_eq!(model_max_output_tokens("claude-opus-4.6"), 64000);
    assert_eq!(model_max_output_tokens("claude-opus-4.5"), 64000);
    assert_eq!(model_max_output_tokens("claude-sonnet-5"), 64000);
    assert_eq!(model_max_output_tokens("claude-haiku-4.5"), 64000);
}
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cargo test --lib anthropic::converter::tests::test_model_max_output_tokens_opus_5
```

预期：FAIL，opus-5 返回 64000 而非 128000。

- [ ] **Step 3: 修改 `model_max_output_tokens`**

修改 `src/anthropic/converter.rs:1356-1367`：

```rust
/// 根据模型返回 Kiro 允许的 max_tokens 上限
/// claude-opus-5 / claude-opus-4.7 / claude-opus-4.8 Max Output = 128K（1M 窗口代际）
/// claude-sonnet-5 Max Output = 64K，与 sonnet-4.x 同档，走默认分支即可
fn model_max_output_tokens(model: &str) -> i32 {
    let m = model.to_lowercase();
    if m.contains("opus-4-7") || m.contains("opus-4.7")
        || m.contains("opus-4-8") || m.contains("opus-4.8")
        || m.contains("opus-5") || m.contains("opus.5")
    {
        128000
    } else {
        64000
    }
}
```

- [ ] **Step 4: 运行测试验证通过**

```bash
cargo test --lib anthropic::converter::tests::test_model_max_output_tokens_opus_5
```

预期：PASS。

- [ ] **Step 5: 全测试 + lint 验证无回归**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
```

预期：全部通过。

- [ ] **Step 6: 提交**

```bash
git add src/anthropic/converter.rs
git commit -m "feat(model): model_max_output_tokens 接入 opus-5 上限 128K

与 opus-4.7/4.8 同分支（128000 输出上限），对应 Kiro
tokenLimits.maxOutputTokens=128000。文档注释更新。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

## Task 3: `stream.rs::is_large_window_model` 加入 opus-5

**Files:**
- Modify: `src/anthropic/stream.rs:585-591`（`is_large_window_model` 函数）
- Test: 同文件内 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: 无外部依赖
- Produces: `fn is_large_window_model(model: &str) -> bool` 对 opus-5 系列返回 true

- [ ] **Step 1: 写失败测试**

在 `src/anthropic/stream.rs` 文件尾部 `#[cfg(test)] mod tests` 中（紧邻 `test_scale_for_client_large_window_model` 之后）新增：

```rust
#[test]
fn test_is_large_window_model_includes_opus_5() {
    // opus-5 走大窗口分支（× 0.15）
    assert_eq!(scale_for_client(100_000, "claude-opus-5"), 15_000);
    assert_eq!(scale_for_client(100_000, "claude-opus-5-thinking"), 15_000);
    assert_eq!(scale_for_client(200_000, "Claude-Opus-5"), 30_000);
    assert_eq!(scale_for_client(1, "claude-opus-5"), 1);

    // 回归：sonnet-5 不归入大窗口分支
    assert_eq!(scale_for_client(100_000, "claude-sonnet-5"), 66_570);
    // 回归：opus-4.5/4.6 不归入大窗口分支
    assert_eq!(scale_for_client(100_000, "claude-opus-4-5"), 66_570);
    assert_eq!(scale_for_client(100_000, "claude-opus-4-6"), 66_570);
    // 回归：opus-4.7/4.8 仍走大窗口分支
    assert_eq!(scale_for_client(100_000, "claude-opus-4-7"), 15_000);
    assert_eq!(scale_for_client(100_000, "claude-opus-4-8"), 15_000);
}
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cargo test --lib anthropic::stream::tests::test_is_large_window_model_includes_opus_5
```

预期：FAIL，opus-5 返回 66_570 而非 15_000。

- [ ] **Step 3: 修改 `is_large_window_model`**

修改 `src/anthropic/stream.rs:585-591`：

```rust
/// 4.7/4.8/opus-5 模型的缩放系数（窗口 1M，需更低系数避免过早触发 compact）。
fn is_large_window_model(model: &str) -> bool {
    model.contains("opus-4-7")
        || model.contains("opus-4-8")
        || model.contains("opus-5")
        || model.contains("opus.5")
        || model.contains("claude-4-7")
        || model.contains("claude-4-8")
}
```

- [ ] **Step 4: 运行测试验证通过**

```bash
cargo test --lib anthropic::stream::tests::test_is_large_window_model_includes_opus_5
```

预期：PASS。

- [ ] **Step 5: 全测试 + lint 验证无回归**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
```

预期：全部通过。

- [ ] **Step 6: 提交**

```bash
git add src/anthropic/stream.rs
git commit -m "feat(stream): is_large_window_model 接入 opus-5（系数 0.15）

opus-5 窗口 1M，与 4.7/4.8 同归大窗口分支，避免过早触发
Claude Code compact。文档注释更新。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

## Task 4: `usage.rs::get_k_ref` opus-5 → 2.36

**Files:**
- Modify: `src/model/usage.rs:124-146`（`get_k_ref` 函数）
- Test: 同文件内 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: 无外部依赖
- Produces: `fn get_k_ref(model: &str) -> f64` 对 opus-5 字符串返回 2.36

- [ ] **Step 1: 写失败测试**

在 `src/model/usage.rs` 文件尾部 `#[cfg(test)] mod tests` 中新增：

```rust
#[test]
fn test_get_k_ref_opus_5() {
    assert_eq!(get_k_ref("claude-opus-5"), 2.36);
    assert_eq!(get_k_ref("claude-opus-5-thinking"), 2.36);
    assert_eq!(get_k_ref("Claude-Opus-5"), 2.36);

    // 回归：其他档位不变
    assert_eq!(get_k_ref("claude-opus-4-7"), 2.36);
    assert_eq!(get_k_ref("claude-opus-4-8"), 2.36);
    assert_eq!(get_k_ref("claude-opus-4-6"), 1.90);
    assert_eq!(get_k_ref("claude-opus-4-5"), 1.90);
    assert_eq!(get_k_ref("claude-sonnet-5"), 1.43);
    assert_eq!(get_k_ref("claude-sonnet-4.6"), 1.43);
    assert_eq!(get_k_ref("claude-haiku-4.5"), 1.43);
}
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cargo test --lib model::usage::tests::test_get_k_ref_opus_5
```

预期：FAIL，opus-5 返回 1.90（兜底命中 4-5/4-6 分支）而非 2.36。

- [ ] **Step 3: 修改 `get_k_ref`**

修改 `src/model/usage.rs:124-146`：

```rust
/// 平台级 credits/USD 换算率，按模型档位差异化（代理实测 2026-06-25）。
/// 仅 usage 报表 credits_saved 字段使用（estimated_cost × k_ref - credits_used）。
/// cache_read 派生已切换为前缀估算路径，不再依赖此值。
/// 2026-06-30 重校：按 opus 版本分档，基于实测 d=0.50 缓存折扣反推。
/// 2026-07-25 追加 opus-5 与 4.7/4.8 同档。
fn get_k_ref(model: &str) -> f64 {
    let m = model.to_lowercase();
    if m.contains("opus-4-7") || m.contains("opus-4.7")
        || m.contains("opus-4-8") || m.contains("opus-4.8")
        || m.contains("opus-5") || m.contains("opus.5")
    {
        // opus 4.7/4.8/5 共用同档（实测 4.8 ≈ 2.36，5 沿用 4.8 档位）
        2.36
    } else if m.contains("opus-4-5") || m.contains("opus-4.5")
        || m.contains("opus-4-6") || m.contains("opus-4.6")
    {
        // 旧 opus 4.5/4.6（实测 4.6 ≈ 1.90）
        1.90
    } else if m.contains("opus") || m.contains("fable") {
        // 未知 opus / fable 兜底沿用最新档
        2.36
    } else if m.contains("sonnet-5") || m.contains("sonnet.5") {
        // claude-sonnet-5: Rate = 1.3 Credit，与 sonnet-4.5/4.6 同档（实测确认）
        1.43
    } else {
        // sonnet 系列 / haiku 默认
        1.43
    }
}
```

- [ ] **Step 4: 运行测试验证通过**

```bash
cargo test --lib model::usage::tests::test_get_k_ref_opus_5
```

预期：PASS。

- [ ] **Step 5: 全测试 + lint 验证无回归**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
```

预期：全部通过。

- [ ] **Step 6: 提交**

```bash
git add src/model/usage.rs
git commit -m "feat(model): get_k_ref 接入 opus-5 同档（2.36）

与 opus-4.7/4.8 同一分支，credits_saved 显示一致性。注释更新
到 2026-07-25。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

## Task 5: `handlers.rs::build_model_list` 新增 opus-5 静态表

**Files:**
- Modify: `src/anthropic/handlers.rs:373-381`（opus-4.8 之后、fable-5 之前）
- Test: 同文件内 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: 无外部依赖
- Produces: `pub fn build_model_list() -> Vec<Model>` 返回值含 `claude-opus-5` 与 `claude-opus-5-thinking`

- [ ] **Step 1: 写失败测试**

在 `src/anthropic/handlers.rs` 文件 `#[cfg(test)] mod tests` 中（紧邻 `find_by_id` 测试附近）新增：

```rust
#[test]
fn test_build_model_list_includes_opus_5() {
    let list = build_model_list();
    let ids: std::collections::HashSet<&str> =
        list.iter().map(|m| m.id.as_str()).collect();

    assert!(ids.contains("claude-opus-5"), "缺 claude-opus-5 静态表项");
    assert!(
        ids.contains("claude-opus-5-thinking"),
        "缺 claude-opus-5-thinking 静态表项"
    );

    let opus5 = list.iter().find(|m| m.id == "claude-opus-5").unwrap();
    assert_eq!(opus5.owned_by, "anthropic");
    assert_eq!(opus5.display_name, "Claude Opus 5");
    assert_eq!(opus5.max_tokens, 128000);

    let opus5t = list
        .iter()
        .find(|m| m.id == "claude-opus-5-thinking")
        .unwrap();
    assert_eq!(opus5t.owned_by, "anthropic");
    assert_eq!(opus5t.display_name, "Claude Opus 5 (Thinking)");
    assert_eq!(opus5t.max_tokens, 128000);

    // 回归：sonnet-5 仍在
    assert!(ids.contains("claude-sonnet-5"));
    assert!(ids.contains("claude-sonnet-5-thinking"));
    // 回归：opus-4.7/4.8 仍在
    assert!(ids.contains("claude-opus-4-7"));
    assert!(ids.contains("claude-opus-4-8"));
}
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cargo test --lib anthropic::handlers::tests::test_build_model_list_includes_opus_5
```

预期：FAIL，`ids.contains("claude-opus-5")` 返回 false。

- [ ] **Step 3: 修改 `build_model_list`**

修改 `src/anthropic/handlers.rs:373-381`（opus-4.8 之后）：

```rust
        Model {
            id: "claude-opus-4-8-thinking".to_string(),
            object: "model".to_string(),
            created: 1775600000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.8 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-5".to_string(),
            object: "model".to_string(),
            created: 1777500000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-5-thinking".to_string(),
            object: "model".to_string(),
            created: 1777500000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-fable-5".to_string(),
```

- [ ] **Step 4: 运行测试验证通过**

```bash
cargo test --lib anthropic::handlers::tests::test_build_model_list_includes_opus_5
```

预期：PASS。

- [ ] **Step 5: 全测试 + lint 验证无回归**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
```

预期：全部通过。

- [ ] **Step 6: 提交**

```bash
git add src/anthropic/handlers.rs
git commit -m "feat(handlers): build_model_list 静态表接入 opus-5

账号查询失败回退分支需包含 claude-opus-5 与 -thinking，
max_tokens=128000，created=1777500000。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

## Task 6: `admin/service.rs` 抓包测试追加 opus-5 样例

**Files:**
- Modify: `src/admin/service.rs:641-693`（`test_available_models_response_deserializes_real_capture_shape`）
- Test: 既有测试函数内补充

**Interfaces:**
- Consumes: 既有 `AvailableModelsResponse` 结构
- Produces: 既有测试函数追加 opus-5 抓包样例与字段断言

- [ ] **Step 1: 修改既有测试的 JSON 字面量 + 断言**

在 `test_available_models_response_deserializes_real_capture_shape` 函数体内，将 `models[]` JSON 字面量的第二个条目（sonnet-5）之后、第三个 `legacy-drift-model` 之前，插入完整 opus-5 抓包条目：

```json
{
    "additionalModelRequestFieldsSchema": {
        "type": "object",
        "properties": {
            "thinking": {
                "type": "object",
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["adaptive", "disabled"]
                    },
                    "display": {
                        "type": "string",
                        "enum": ["summarized", "omitted"]
                    }
                },
                "required": ["type"]
            },
            "output_config": {
                "type": "object",
                "properties": {
                    "effort": {
                        "type": "string",
                        "enum": ["low", "medium", "high", "xhigh", "max"],
                        "default": "high"
                    }
                }
            },
            "max_tokens": {
                "type": "integer",
                "minimum": 1024,
                "maximum": 128000
            }
        },
        "additionalProperties": false
    },
    "description": "Experimental preview of Claude Opus 5 model with 1M context window",
    "modelId": "claude-opus-5",
    "modelName": "claude-opus-5",
    "promptCaching": {
        "maximumCacheCheckpointsPerRequest": 4,
        "minimumTokensPerCacheCheckpoint": 1024,
        "supportsPromptCaching": true
    },
    "rateMultiplier": 2.2,
    "rateUnit": "Credit",
    "supportedInputTypes": ["TEXT", "IMAGE"],
    "tokenLimits": { "maxInputTokens": 1000000, "maxOutputTokens": 128000 }
}
```

将末尾 `assert_eq!(parsed.models.len(), 3);` 改为 `assert_eq!(parsed.models.len(), 4);`。

在该断言之后新增 opus-5 字段对齐断言：

```rust
        let opus5 = parsed
            .models
            .iter()
            .find(|m| m.model_id == "claude-opus-5")
            .expect("claude-opus-5 抓包条目缺失");
        assert_eq!(opus5.model_name.as_deref(), Some("claude-opus-5"));
        assert_eq!(opus5.rate_multiplier, Some(2.2));
        assert_eq!(opus5.token_limits.max_input_tokens, 1_000_000);
        assert_eq!(opus5.token_limits.max_output_tokens, 128_000);
        assert!(opus5.additional_model_request_fields_schema.is_some());
```

- [ ] **Step 2: 运行测试验证失败**

```bash
cargo test --lib admin::service::tests::test_available_models_response_deserializes_real_capture_shape
```

预期：FAIL，`assert_eq!(parsed.models.len(), 4)` 返回 3 而非 4。

- [ ] **Step 3: 修改既有测试函数**

按 Step 1 内容修改 `src/admin/service.rs:641-693`。保留原 sonnet-5 条目，仅追加 opus-5 完整条目与 length/字段断言。

- [ ] **Step 4: 运行测试验证通过**

```bash
cargo test --lib admin::service::tests::test_available_models_response_deserializes_real_capture_shape
```

预期：PASS。

- [ ] **Step 5: 全测试 + lint 验证无回归**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib
```

预期：全部通过。

- [ ] **Step 6: 提交**

```bash
git add src/admin/service.rs
git commit -m "test(admin): 抓包反序列化测试追加 claude-opus-5 样例

基于 2026-07-25 Kiro ListAvailableModels 真实抓包，追加 opus-5
完整条目（含 additionalModelRequestFieldsSchema、rateMultiplier
2.2、maxInputTokens 1M、maxOutputTokens 128K），验证字段对齐。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

## Task 7: 文档同步

**Files:**
- Modify: `docs/代码速查表.md` `get_k_ref` 行
- Modify: `docs/源码全景解析.md` `map_model` 章节注释 + 表格

**Interfaces:**
- Consumes: 前 6 个任务的最终代码
- Produces: 文档与代码对齐

- [ ] **Step 1: 修改 `docs/代码速查表.md`**

定位 L496 行（`get_k_ref` 行），将当前描述：

```
计费系数：opus-4.7/opus-4.8/未知 opus·fable 兜底 = 2.36；opus-4.5/opus-4.6 = 1.90；sonnet 系列/sonnet-5/haiku 默认档 = 1.43
```

改为：

```
计费系数：opus-4.7/opus-4.8/opus-5/未知 opus·fable 兜底 = 2.36；opus-4.5/opus-4.6 = 1.90；sonnet 系列/sonnet-5/haiku 默认档 = 1.43
```

- [ ] **Step 2: 修改 `docs/源码全景解析.md` `map_model` 章节**

定位 L269 `map_model` 注释块区域，在 opus 分支注释之前新增 opus-5 说明：

```rust
} else if model_lower.contains("opus") {
    if model_lower.contains("opus-5") || model_lower.contains("opus.5") {
        // claude-opus-5：Max Input 1M、Max Output 128K、Rate 2.2 Credit（与 4.7/4.8 同档）
        Some("claude-opus-5".to_string())
    } else if model_lower.contains("4-5") || model_lower.contains("4.5") {
```

定位 L328-329 表格，新增 opus-5 行：

```
| `claude-opus-5` | contains "opus" + contains "opus-5"/"opus.5" | `claude-opus-5` | Claude Opus 5 |
```

- [ ] **Step 3: 提交**

```bash
git add docs/代码速查表.md docs/源码全景解析.md
git commit -m "docs: 同步 opus-5 说明至代码速查表与源码全景解析

get_k_ref 行追加 opus-5 档位（2.36 同 4.7/4.8）；map_model 注释
补全 opus-5 分支与表格行。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>"
```

---

## Acceptance Verification（最终验证）

在所有 Task 完成后执行：

- [ ] **Step 1: 全量单测**

```bash
cargo test --lib
```

预期：所有测试通过（含新增 5 个 opus-5 单测 + 1 个 admin 抓包补充断言）。

- [ ] **Step 2: 静态检查**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

预期：全部 clean，无 warning。

- [ ] **Step 3: 端到端手工验证（需真实 Kiro 账号 + credentials.json 已配置）**

```bash
./build-mac.sh
./run-local-service-mac.sh &
sleep 2

curl -s http://localhost:8000/v1/messages \
  -H "x-api-key: <your-api-key>" \
  -H "anthropic-version: 2023-06-01" \
  -H "content-type: application/json" \
  -d '{"model":"claude-opus-5","max_tokens":1024,"messages":[{"role":"user","content":"Hello"}]}' \
  | head -c 500

curl -s http://localhost:8000/admin/api/models \
  -H "Authorization: Bearer <admin-token>" \
  | jq '.data[] | select(.model.id | contains("opus-5"))'
```

预期：`/v1/messages` 返回正常 SSE 流；`/admin/api/models` 返回 `claude-opus-5` 条目，`rate_multiplier` 接近 2.2（实时来源）或本地静态表兜底。
