# 规范增量：模型规格对齐 Anthropic 官方

## 修改需求

### 需求：模型上下文窗口判断（`src/anthropic/stream.rs::context_window_for_model`）

#### 场景：opus-4.6 走 1M 窗口
- **WHEN** 调用 `context_window_for_model` 传入包含子串 `"opus-4-6"` 或 `"opus-4.6"` 的模型 ID
- **THEN** 返回 `1_000_000`

#### 场景：fable-5 走 1M 窗口
- **WHEN** 调用 `context_window_for_model` 传入包含子串 `"fable-5"` 或 `"fable_5"` 的模型 ID
- **THEN** 返回 `1_000_000`

#### 场景：opus-4.7 / opus-4.8 / sonnet-4.6 行为不变
- **WHEN** 调用 `context_window_for_model` 传入包含 `"opus-4-7"` / `"opus-4-8"` / `"sonnet-4-6"` 的模型 ID
- **THEN** 返回 `1_000_000`

#### 场景：haiku-4-5 / 旧版 / 未识别模型走默认 200K
- **WHEN** 调用 `context_window_for_model` 传入 `"claude-haiku-4-5-20251001"` 或不在 1M 列表的任何模型 ID
- **THEN** 返回 `200_000`

### 需求：缓存命中 token 反推（`src/anthropic/stream.rs::infer_cache_read_tokens`）

#### 场景：opus-4.6 使用与 opus-4.7/4.8 同档 k_ref
- **WHEN** 调用 `infer_cache_read_tokens` 传入 `model` 包含子串 `"opus-4-6"` 或 `"opus-4.6"`
- **THEN** 内部 `(k_ref, input_price, output_price)` 取 `(2.60, 15.0, 75.0)`，与 opus-4.7/4.8 同（依据：官方单价均 .00 / .00）

#### 场景：fable-5 沿用 opus 顶端 k_ref（占位）
- **WHEN** 调用 `infer_cache_read_tokens` 传入 `model` 包含子串 `"fable"`
- **THEN** 内部 `(k_ref, input_price, output_price)` 取 `(2.60, 15.0, 75.0)` 作为占位值
- **AND** 函数返回 `Some(v)` 且 `v` 落在 `[0, total]` 区间，**不返回 None**

#### 场景：opus-4.5 及更早 / sonnet / haiku 行为不变
- **WHEN** 调用 `infer_cache_read_tokens` 传入 model 不含 4-6/4.6/4-7/4.7/4-8/4.8/fable 子串
- **THEN** 沿用本变更前的 k_ref 表（opus 4.5 → 2.40；sonnet → 7.06；haiku → None）

### 需求：模型路由（`src/anthropic/converter.rs::map_model`）

#### 场景：fable-5 路由到 Kiro fable-5 ID
- **WHEN** 调用 `map_model` 传入小写化后包含子串 `"fable"` 的模型 ID（如 `"claude-fable-5"`、`"claude-fable-5-thinking"`）
- **THEN** 返回 `Some("claude-fable-5".to_string())`

#### 场景：原有 sonnet / opus / haiku 路由不变
- **WHEN** 调用 `map_model` 传入不含 `"fable"` 的模型 ID
- **THEN** 返回值与本变更前一致（sonnet-4.6 → claude-sonnet-4.6、opus-4-6 默认 → claude-opus-4.6、opus-4-7 → claude-opus-4.7、opus-4-8 → claude-opus-4.8、haiku → claude-haiku-4.5）

### 需求：`/v1/models` 暴露的模型清单（`src/anthropic/handlers.rs::build_model_list`）

#### 场景：opus-4.6 暴露 128K max_tokens
- **WHEN** GET `/v1/models`
- **THEN** 响应 `data[]` 中存在 `id="claude-opus-4-6"` 与 `id="claude-opus-4-6-thinking"` 两条目
- **AND** 两者 `max_tokens` 字段值均为 `128000`

#### 场景：fable-5 出现在模型列表
- **WHEN** GET `/v1/models`
- **THEN** 响应 `data[]` 中存在 `id="claude-fable-5"` 条目
- **AND** 该条目 `max_tokens=128000`、`owned_by="anthropic"`、`object="model"`、`model_type="chat"`、`display_name="Claude Fable 5"`

#### 场景：fable-5-thinking 同步出现
- **WHEN** GET `/v1/models`
- **THEN** 响应 `data[]` 中存在 `id="claude-fable-5-thinking"` 条目
- **AND** 该条目 `max_tokens=128000`、`display_name="Claude Fable 5 (Thinking)"`

#### 场景：其他模型规格保持不变
- **WHEN** GET `/v1/models`
- **THEN** 以下条目的 `max_tokens` 字段与本变更前完全一致：
  - `claude-sonnet-4-6` → `64000`
  - `claude-sonnet-4-6-thinking` → `64000`
  - `claude-opus-4-7` → `128000`
  - `claude-opus-4-7-thinking` → `128000`
  - `claude-opus-4-8` → `128000`
  - `claude-opus-4-8-thinking` → `128000`
  - `claude-haiku-4-5-20251001` → `64000`
  - `claude-haiku-4-5-20251001-thinking` → `64000`
  - 旧版 3.x / 4 / 4.5 / auto / deepseek / glm 条目全部保持原值
