# 任务清单：add-model-rate-multiplier

## 状态：ARCHIVED

## 任务

- [x] 新增 `src/kiro/model/available_models.rs`：定义 `AvailableModelsResponse { models: Vec<AvailableModelInfo> }` / `AvailableModelInfo { model_id, rate_multiplier }`，`#[serde(rename_all = "camelCase")]`——根据用户抓包，响应体实际字段为 `modelId`（string）/ `rateMultiplier`（f64），其余字段（`modelName`/`description`/`rateUnit`/`promptCaching`/`supportedInputTypes`/`tokenLimits`/`additionalModelRequestFieldsSchema` 等）均不反序列化，serde 默认忽略未知字段。在 `src/kiro/model/mod.rs` 注册 `pub mod available_models;`
- [x] `src/kiro/token_manager.rs` 新增模块级函数 `list_available_models(credentials, config, token, proxy) -> anyhow::Result<AvailableModelsResponse>`——**HTTP method/headers/body 编码遵循抓包还原的 AWS JSON RPC 格式**：POST `https://management.{region}.kiro.dev/?origin=KIRO_CLI`，`content-type: application/x-amz-json-1.0`，`x-amz-target: AmazonCodeWhispererService.ListAvailableModels`，超时 15s（此格式与 `get_usage_limits` 的 GET 骨架无关，代码库中无现存先例，不可照抄 `get_usage_limits` 的请求结构）。**`profile_arn` 的 `Some`/`None` 处理逻辑对齐 `get_usage_limits`（URL 条件追加）与 `provider.rs::rewrite_profile_arn`（body 字段增删）——仅当 `credentials.profile_arn` 为 `Some` 时才向 URL 追加 `&profileArn=...` 且向请求体写入 `"profileArn"` 字段；为 `None` 时 URL 不带该参数、请求体也不包含该 key（不写 `null`）**。`region` 直接复用 `credentials.effective_api_region(config)`——与现有 `getUsageLimits`（`q.{region}.amazonaws.com`）用的是同一个 region 取值，未单独验证 `management.{region}.kiro.dev` 是否对所有 region 都可解析（见风险条目），实现时用 `tracing::debug!` 打印实际请求的 host，便于上线后排查
- [x] `src/kiro/token_manager.rs` 新增 `MultiTokenManager::list_available_models(&self) -> anyhow::Result<AvailableModelsResponse>`，用 `acquire_context(None)` 取任意可用账号（对齐 `get_usage_limits()` 无 id 版本）
- [x] `src/anthropic/mod.rs`：`mod converter;` → `pub(crate) mod converter;`，使 `map_model()` 可被 `src/admin/` 复用
- [x] `src/admin/service.rs` 新增 `AdminService::list_model_rates(&self) -> HashMap<String, f64>`：调用 `token_manager.list_available_models()`，成功建 `model_id → rate_multiplier` 表；失败 `tracing::warn!` 后返回空表（不向上传播错误）
- [x] `src/admin/types.rs` 新增 `AdminModelItem { #[serde(flatten)] model: Model, rate_multiplier: Option<f64> }` 与 `AdminModelsResponse { object, data: Vec<AdminModelItem> }`
- [x] `src/admin/api_keys.rs::get_admin_models` 签名改为 `State(state): State<AdminState>`；调用 `state.service.list_model_rates().await`，对 `build_model_list()` 每项用 `map_model(&model.id)` 归一化后查表填充 `rate_multiplier`，返回 `AdminModelsResponse`
- [x] 更新 `src/admin/api_keys.rs` 现有单测 `test_get_admin_models_matches_build_model_list` 以适配新签名（零账号 `AdminState` 构造，验证 200 + 模型 id 集合不变 + 所有 `rate_multiplier` 为 `None`，覆盖优雅降级路径）
- [x] `admin-ui/src/types/api.ts`：`ModelItem` 新增 `rate_multiplier?: number | null`
- [x] `admin-ui/src/components/model-list-page.tsx`：表格新增"费率倍率"列，`rate_multiplier` 非空时格式化为 `${value.toFixed(2)}x`，为空时显示 `—`
- [x] `cargo fmt` + `cargo clippy` + `cargo test` 全部通过；`admin-ui` 构建无 TS 报错
- [x] 归档前用至少一个真实账号手动验证一次 `GET /api/admin/models`，确认 `rate_multiplier` 非全部为 `null`（用于排除 region host 不可达等实现期无法用单测覆盖的问题）——本地起服务（2 个真实账号）实测：Sonnet 系列 1.3x、Opus 系列 2.2x、Haiku 系列 0.4x 等均正确回填，仅 `claude-fable-5` 未命中返回 `null`（符合归一化未命中的预期降级）

## 验收标准

- [x] `GET /api/admin/models` 每个模型条目包含 `rate_multiplier` 字段（`number` 或 `null`）
- [x] 无可用账号或上游 `ListAvailableModels` 调用失败时，接口仍返回 200、模型列表完整，`rate_multiplier` 全部为 `null`
- [x] admin-ui 支持模型页新增"费率倍率"列，格式与 Kiro CLI `/model` 选择器一致（如 `1.30x`）
- [x] `/v1/models` 端点行为、响应结构不变
- [x] `cargo test` / `cargo clippy` / `cargo fmt --check` 全部通过
