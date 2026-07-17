# 任务清单：model-list-live-source

## 状态：ARCHIVED

## 任务

- [x] `src/kiro/model/available_models.rs`：`AvailableModelInfo` 新增 `#[serde(default)] model_name: String`、`#[serde(default)] token_limits: TokenLimits`；新增 `TokenLimits { max_input_tokens: u64, max_output_tokens: u64 }`（`#[serde(rename_all = "camelCase")]`，派生 `Default`），确保上游单个字段缺失时该模型条目仍能反序列化成功，不触发整个 `models[]` 解析失败
- [x] `src/admin/service.rs`：新增 `guess_owned_by(model_id: &str) -> &'static str` 前缀启发式函数（覆盖 `claude`/`gpt`/`auto`/`deepseek`/`minimax`/`glm`/`qwen`，默认 `"unknown"`）
- [x] `src/admin/service.rs`：新增两个纯函数（不依赖网络，可直接单测）：`fn live_model_to_admin_item(info: &AvailableModelInfo) -> AdminModelItem`（`owned_by` 走启发式，`rate_multiplier: Some(...)`）与 `fn fallback_model_to_admin_item(model: Model) -> AdminModelItem`（`rate_multiplier: None`）
- [x] `src/admin/service.rs`：新增 `AdminService::list_admin_models(&self) -> Vec<AdminModelItem>`——仅做"调用 + 分支"：成功路径对每个 `AvailableModelInfo` 调用 `live_model_to_admin_item`；失败路径（`Err`）`tracing::warn!` 后对 `build_model_list()` 每项调用 `fallback_model_to_admin_item`；移除旧的 `list_model_rates`
- [x] `src/admin/api_keys.rs::get_admin_models` 简化为调用 `state.service.list_admin_models().await`；删除 `build_admin_model_items` 及对 `map_model`/`build_model_list` 的 import（两者均不再被本文件直接使用，回退逻辑已内移至 `service.rs::list_admin_models`）
- [x] 更新 `src/admin/api_keys.rs` 现有单测：移除已失效的 `test_build_admin_model_items_fills_rate_multiplier_on_match`；`test_get_admin_models_matches_build_model_list` 适配新签名（零账号场景验证回退路径：200 + 模型 id 集合与 `build_model_list()` 一致 + 所有 `rate_multiplier` 为 `None`）
- [x] 新增单测覆盖 `live_model_to_admin_item`：构造 fake `AvailableModelInfo`（含已知前缀如 `claude-sonnet-4.6`、未知前缀如 `foo-model`），断言 `id`/`display_name`/`max_tokens`/`rate_multiplier` 正确映射，且 `owned_by` 对已知前缀返回预期厂商、未知前缀返回 `"unknown"`
- [x] 新增单测覆盖 `fallback_model_to_admin_item`：断言 `rate_multiplier` 为 `None`，其余字段与传入 `Model` 一致
- [x] 新增单测：基于真实抓包字段结构构造 JSON 字面量反序列化为 `AvailableModelsResponse`，断言 `model_id`/`model_name`/`rate_multiplier`/`token_limits.{max_input_tokens,max_output_tokens}` 映射正确；并额外覆盖一条缺失 `modelName`/`tokenLimits` 的条目，验证 `#[serde(default)]` 容错不会导致整个 `models[]` 解析失败
- [x] 确认 `map_model` 在 admin 模块清除依赖后无其他外部调用方，将 `src/anthropic/mod.rs` 的 `mod converter;` 可见性从 `pub(crate)` 收回为 private
- [x] `cargo fmt` + `cargo clippy` + `cargo test` 全部通过（`cargo fmt` 仅对本次改动文件执行，不做全量格式化）
- [x] 归档前用至少一个真实账号手动验证 `GET /api/admin/models`：确认返回的模型行数量/id 与最新一次真实抓包的 `models[]` 一致（尤其确认接口若已新增/下架模型时页面能同步展示，而非仍显示旧的本地静态表）——实测本地服务加载 2 个真实账号后返回 18 条模型，`rate_multiplier` 全部非空（确认走实时路径而非回退）

## 验收标准

- [x] 上游调用成功时，`GET /api/admin/models` 的模型集合与实时 `ListAvailableModels` 响应一致（而非本地 `build_model_list()`）——用户已确认接受集合从客户端别名（约 32 条）变为 Kiro 原生家族（约 16 条，不含 `-thinking` 独立条目和历史日期式别名）——实测返回 18 条
- [x] 上游调用失败/无可用账号时，回退本地静态列表，`rate_multiplier` 全部为 `null`，行为与当前一致——单测 `test_get_admin_models_matches_build_model_list` 覆盖
- [x] `owned_by` 对已知模型前缀（`claude`/`gpt`/`deepseek`/`minimax`/`glm`/`qwen`/`auto`）返回预期厂商名，未知前缀返回 `"unknown"`（单测覆盖，非人工抽查）——单测 + 真实账号验证均通过
- [x] `/v1/models` 端点行为、响应结构不变（本次未改动其实现）
- [x] admin-ui 前端代码无需改动（`AdminModelItem` JSON 形状不变）
- [x] `cargo test` / `cargo clippy` / `cargo fmt --check` 全部通过
- [x] 基于真实抓包 JSON 字面量的反序列化单测通过，且新增字段缺失时不会导致整个 `models[]` 解析失败（`#[serde(default)]` 容错覆盖）
