# 变更提案：model-list-live-source

## 背景

`add-model-rate-multiplier` 已实现"支持模型"页费率倍率的实时查询，但模型行本身仍来自本地硬编码的 `build_model_list()`——Kiro 官方新增/下架模型时，页面无法感知，必须改代码才能同步。用户明确要求："这个页面显示的所有支持的模型都是通过接口返回"，并提供了 `ListAvailableModels` 响应体的完整真实抓包，确认字段结构为 `modelId`/`modelName`/`rateMultiplier`/`tokenLimits.{maxInputTokens,maxOutputTokens}` 等。

## 目标范围

**在范围内：**
- `GET /api/admin/models` 成功调用上游时，模型行数据（id/显示名称/max_tokens/费率倍率）完全来自本次 `ListAvailableModels` 响应，不再与本地 `build_model_list()` 做归一化 join
- 上游调用失败或当前无可用账号时，按用户明确选择：回退到本地 `build_model_list()` 静态列表（`rate_multiplier` 全部为 `null`），与当前行为一致——保证页面不会空白
- `owned_by`（提供方）由 `modelId` 前缀做最小启发式推断（接口不返回该字段）
- 移除本次改造前为"归一化 join"引入的 `map_model()` 依赖及 `pub(crate) mod converter;`（若确认无其他调用方，收回为 private）

**不在范围内：**
- 不修改 `/v1/messages`、`/cc/v1/messages`、`/v1/models`（客户端可见协议与其模型集合不变，仍用 `build_model_list()`）
- 不引入缓存（延续 `add-model-rate-multiplier` 已确认的"每次实时调用"原则）
- 不改动 admin-ui 前端类型/表格结构（`AdminModelItem` 对外 JSON 形状保持不变，前端零改动）

## 已知且用户已确认接受的行为变化

`ListAvailableModels` 返回的是 Kiro 原生模型家族集合（如 `claude-sonnet-4.6`、`claude-opus-4.8`，抓包约 16 条），**不等于**本地 `build_model_list()` 面向客户端维护的别名集合（约 32 条，含 `-thinking` 独立条目、历史日期式别名如 `claude-3-5-sonnet-20241022`）。切换后"支持模型"页展示的将是接口原生集合，不再包含 `-thinking` 变体和旧版别名——该页面的用途从"客户端可传入的模型字符串大全"变为"Kiro 后端当前开放的模型家族总览"。此权衡已向用户说明并经其明确选择"完全改用接口原生集合"，非实施疏漏。

## 技术方案

- `src/kiro/model/available_models.rs`：`AvailableModelInfo` 新增字段 `model_name: String`、`token_limits: TokenLimits { max_input_tokens: u64, max_output_tokens: u64 }`（`#[serde(rename_all = "camelCase")]`；`TokenLimits` 派生 `Default`，两个新字段均加 `#[serde(default)]`，避免上游单个字段缺失/字段名不符导致整条数组反序列化失败而静默触发全局回退；其余字段如 `description`/`promptCaching`/`rateUnit`/`supportedInputTypes`/`additionalModelRequestFieldsSchema` 继续不反序列化）
- `src/admin/service.rs`：`list_model_rates() -> HashMap<String, f64>` 替换为 `list_admin_models() -> Vec<AdminModelItem>`：
  - 上游调用成功 → 将每个 `AvailableModelInfo` 映射为 `AdminModelItem`（`id`/`display_name` 取 `model_id`/`model_name`，`max_tokens` 取 `token_limits.max_output_tokens`，`owned_by` 走启发式推断，`rate_multiplier: Some(rate_multiplier)`，`object`/`type` 沿用固定字面量，`created: 0`）
  - 上游调用失败（`Err`）→ 回退：`build_model_list()` 逐项映射为 `AdminModelItem`，`rate_multiplier: None`（即 `add-model-rate-multiplier` 之前的行为）
- `src/admin/api_keys.rs::get_admin_models`：简化为直接调用 `state.service.list_admin_models().await` 并包一层 `AdminModelsResponse`；移除 `build_admin_model_items` 及其对 `map_model`/`rates` 的依赖
- 移除 `map_model`/`build_model_list` 在 `api_keys.rs` 的 import（两者均不再被 `get_admin_models` 直接使用，回退逻辑已内移至 `service.rs`）；确认无其他调用方后将 `src/anthropic/mod.rs` 的 `pub(crate) mod converter;` 收回为 `mod converter;`
- `src/admin/types.rs::AdminModelItem`/`AdminModelsResponse` 结构不变（保持 `#[serde(flatten)] model: Model` + `rate_multiplier`），admin-ui 无需改动

## 预期影响

- Kiro 官方新增/下架模型后，"支持模型"页无需改代码即可同步展示（含新模型的费率倍率）
- 上游调用失败/无可用账号时行为与当前一致（回退静态列表，`rate_multiplier` 全 `null`），不引入新的空白态或错误态
- `提供方` 列对官方新模型可能显示不准确的启发式猜测（如未知前缀显示 `unknown`），非接口真实数据——已在范围内明确告知用户此权衡
- `/v1/models`、`Model`/`ModelsResponse` 公开契约不受影响

## 风险

- **上游字段名/结构变化导致反序列化失败**：`ListAvailableModels` 为私有协议，若某条模型缺少 `modelName`/`tokenLimits` 或字段名不符，`serde` 默认会使整条数组反序列化失败。已通过 `#[serde(default)]` 容错（缺字段时取默认值而非整体失败）+ 基于真实抓包 JSON 字面量的反序列化单测（验证字段名映射符合当前抓包结构）缓解，但无法覆盖"字段值语义变化但类型/名称不变"的场景
- **`owned_by` 是本地启发式猜测，不是接口数据**：与"完全通过接口返回"的表述有一处必要的例外（接口本身不提供厂商归属字段），已默认按启发式推断处理，非阻塞项
- **`modelName` 即为 `modelId` 原样字符串（非人类友好名称，如 `claude-opus-4.8` 而非 "Claude Opus 4.8"）**："显示名称"列会比此前的手工命名列表更"技术化"，是真实接口数据如实展示的直接结果，非缺陷
- 复用 `add-model-rate-multiplier` 已识别的风险：`ListAvailableModels` 为私有协议，无官方文档，后续可能变更导致解析失败——已有的 15s 超时 + 优雅降级（回退静态列表）机制继续覆盖此风险
