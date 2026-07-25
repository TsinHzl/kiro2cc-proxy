# 设计：Claude Opus 5 支持

## 背景

2026-07-25 Kiro 上线 `claude-opus-5` 模型（rateMultiplier 2.2、maxInputTokens 1M、maxOutputTokens 128K，与 4.7/4.8 同档），但代理侧未识别该模型：当前 `claude-opus-5*` 请求会被 `map_model` 兜底映射到 `claude-opus-4.6`，导致 `max_tokens` 上限错（64K 而非 128K）、流式 compact 阈值错（按 200K 窗口而非 1M 计算）、计费 `k_ref` 错（1.90 而非 2.36）、`/v1/models` 回退分支缺失 id、Admin 面板抓包结构兼容性未覆盖。

本文档对齐 `claude-opus-5` 与现有 `claude-opus-4.7`/`claude-opus-4.8` 的全套接入路径，与已上线的 `claude-sonnet-5` 风格保持对称。

## 目标范围

**在范围内：**
- `map_model` 新增 opus-5 别名
- `model_max_output_tokens` opus-5 → 128000
- `is_large_window_model` 加入 opus-5（系数 0.15）
- `get_k_ref` opus-5 → 2.36
- `build_model_list` 静态表新增 `claude-opus-5` + `claude-opus-5-thinking`
- `admin/service.rs` 抓包反序列化测试追加 opus-5 样例
- 对应单测覆盖

**不在范围内：**
- 不引入新模块、新接口、新 crate
- 不修改 token_manager / provider 账号选择逻辑（opus-5 自动复用既有 9-retry/3-per-account 路径）
- 不修改 admin/user-ui 前端（实时模型列表已自动包含 opus-5）
- 不修改 converter.rs thinking/output_config 构建逻辑（opus-5 schema 与 4.6/4.7/4.8 同构，沿用既有路径）
- 不引入 opus-5 专用 max_tokens 下限保护、effort 枚举裁剪（与 4.6/4.7/4.8 行为一致）

## 技术方案

### 改动点 1 — `src/anthropic/converter.rs::map_model`

在 opus 分支最前置 opus-5 分支（必须放在 4-5/4.5 之前），并更新顶部注释。

```rust
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
}
```

同步更新顶部注释：

```rust
/// - opus 5/5 → claude-opus-5
/// - opus 4.5/4-5 → claude-opus-4.5
/// - opus 4.8/4-8 → claude-opus-4.8
/// - opus 4.7/4-7 → claude-opus-4.7
/// - 其他 opus → claude-opus-4.6
```

### 改动点 2 — `src/anthropic/converter.rs::model_max_output_tokens`

将 opus-5 加入 128000 分支：

```rust
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

### 改动点 3 — `src/anthropic/stream.rs::is_large_window_model`

opus-5 窗口 1M，加入大窗口分支：

```rust
fn is_large_window_model(model: &str) -> bool {
    model.contains("opus-4-7")
        || model.contains("opus-4-8")
        || model.contains("opus-5")
        || model.contains("opus.5")
        || model.contains("claude-4-7")
        || model.contains("claude-4-8")
}
```

### 改动点 4 — `src/model/usage.rs::get_k_ref`

opus-5 与 4.7/4.8 同档 2.36：

```rust
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
        1.90
    } else if m.contains("opus") || m.contains("fable") {
        2.36
    } else if m.contains("sonnet-5") || m.contains("sonnet.5") {
        1.43
    } else {
        1.43
    }
}
```

### 改动点 5 — `src/anthropic/handlers.rs::build_model_list`

在 opus-4.8 条目之后插入 opus-5 + opus-5-thinking 两个 `Model` 结构：

```rust
Model {
    id: "claude-opus-5".to_string(),
    object: "model".to_string(),
    created: 1777500000,   // 2026-07-25 Kiro 上线日附近
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
```

### 改动点 6 — `src/admin/service.rs` 抓包测试

在 `test_available_models_response_deserializes_real_capture_shape` 的 `models[]` 字面量追加 opus-5 完整条目（基于本次 2026-07-25 抓包），并在断言中追加 `claude-opus-5` 存在性检查：

```rust
{
    "additionalModelRequestFieldsSchema": {
        "type": "object",
        "properties": {
            "thinking": { "type": "object", "properties": {
                "type": { "type": "string", "enum": ["adaptive", "disabled"] }
            }, "required": ["type"] },
            "output_config": { "type": "object", "properties": {
                "effort": { "type": "string", "enum": ["low","medium","high","xhigh","max"], "default": "high" }
            } },
            "max_tokens": { "type": "integer", "minimum": 1024, "maximum": 128000 }
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

### 新增测试

| # | 文件 | 测试名 | 断言 |
|---|---|---|---|
| 1 | `src/anthropic/converter.rs` | `test_map_model_opus_5_aliases` | 6 个 opus-5 别名（含 thinking/年份戳/混合写法）均 → `claude-opus-5`；`claude-opus-4-7-20251115` 仍 → `claude-opus-4.7` |
| 2 | `src/anthropic/converter.rs` | `test_model_max_output_tokens_opus_5` | `claude-opus-5` / `-thinking` → 128000；`claude-opus-4-6` 仍 → 64000 |
| 3 | `src/anthropic/stream.rs` | `test_is_large_window_model_includes_opus_5` | `scale_for_client(100_000, "claude-opus-5")` → 15_000；回归 `claude-sonnet-5` → 66_570、`claude-opus-4-5` → 66_570 |
| 4 | `src/model/usage.rs` | `test_get_k_ref_opus_5` | `get_k_ref("claude-opus-5")` → 2.36；回归 `claude-opus-4-6` → 1.90、`claude-sonnet-5` → 1.43 |
| 5 | `src/anthropic/handlers.rs` | `test_build_model_list_includes_opus_5` | 列表含 `claude-opus-5` 与 `claude-opus-5-thinking` |

## 预期影响

- `/v1/messages` 与 `/cc/v1/messages` 调用 `claude-opus-5*` 模型成功（当前会因 `map_model` 兜底到 4.6 而走 64K 上限 + 错误计费）
- `list_available_models` 实时路径自动包含 opus-5（无改动）
- 静态回退路径补齐 opus-5（账号查询失败时仍可枚举）
- Admin 模型列表面板展示 opus-5 + rateMultiplier 2.2 + max_tokens 128000
- 现有 opus-4.6/4.7/4.8 调用路径不变

## 风险

- **协议兼容性**：opus-5 `additionalModelRequestFieldsSchema` 与 4.6/4.7/4.8 同构（均含 thinking/output_config/max_tokens），复用既有 `build_additional_model_request_fields` 路径；schema 内 `effort` 5 档（low/medium/high/xhigh/max）由 Kiro 服务端校验，客户端可透传任何字符串（与 4.7 一致）。
- **k_ref 实测未对齐**：opus-5 官方 rateMultiplier=2.2，但 `get_k_ref` 返回 2.36（沿用 4.7/4.8 实测档位）；差异由 `credits_saved` 字段显示时吸收，不影响输入输出计费正确性（`calculate_cost` 走官方 USD 定价 $5/$25）。后续若需要精确，按 `get_k_ref` 加显式分支 `2.20` 即可。
- **cache checkpoint 阈值**：opus-5 `minimumTokensPerCacheCheckpoint=1024`（vs 4.7/4.8 = 4096），但代理转换层不读该字段（Kiro 服务端按字段值校验），无需任何改动。

## 验收标准

- [ ] `cargo fmt --check` 与 `cargo clippy --all-targets -- -D warnings` 通过
- [ ] `cargo check` 与 `cargo test` 全部通过
- [ ] 新增 5 个测试 + admin 抓包测试追加 opus-5 样例，全部通过
- [ ] `map_model("claude-opus-5")` 返回 `Some("claude-opus-5")`
- [ ] `model_max_output_tokens("claude-opus-5")` 返回 128000
- [ ] `scale_for_client(100_000, "claude-opus-5")` 返回 15_000
- [ ] `get_k_ref("claude-opus-5")` 返回 2.36
- [ ] `build_model_list()` 包含 `claude-opus-5` 与 `claude-opus-5-thinking`
- [ ] 现有 opus-4.6/4.7/4.8/sonnet-5 测试无回归