# 变更提案：add-model-rate-multiplier

## 背景

admin-ui 已有"支持模型"只读列表页（`add-model-list-panel`），但只展示模型 ID/显示名称/提供方/Max Tokens，没有官方 credits 费率倍率（如 Kiro CLI `/model` 选择器中 `claude-opus-4.8 = 2.20x`）。用户提供了真实抓包，确认 Kiro 后端存在 `AmazonCodeWhispererService.ListAvailableModels` 接口（`management.{region}.kiro.dev`，`x-amz-json-1.0` 协议），并明确要求"直接每次调用官方接口获取"，不做静态表、不做本地缓存。

## 目标范围

**在范围内：**
- 新增对 Kiro `ListAvailableModels` 接口的封装调用（复用现有 Bearer Token / profileArn / region / HTTP client 基础设施）
- `MultiTokenManager` 新增 `list_available_models()`，复用 `acquire_context(None)`（"任意可用账号"策略，与 `get_usage_limits()` 无 id 版本同模式）
- `AdminService` 新增方法，对上游调用失败做优雅降级（无可用账号 / 网络失败 / 非 2xx → 记录日志，返回空结果，不阻断整个模型列表接口）
- `GET /api/admin/models` 响应的每个模型条目新增 `rate_multiplier: number | null`，通过现有 `map_model()` 将内部 id 归一化后与实时返回的 Kiro `modelId` 集合关联
- admin-ui 支持模型页表格新增"费率倍率"列，格式如 `1.30x`；无法匹配时显示 `—`

**不在范围内：**
- 不修改 `/v1/models`（客户端可见协议保持不变）；`rate_multiplier` 只出现在新的 admin-only 响应结构中
- 不做任何本地缓存/TTL —— 每次请求都实时调用上游（用户明确要求）
- 不新增账号选择策略；仍复用"任意可用账号"逻辑

## 技术方案

- 新增 `src/kiro/model/available_models.rs`：`AvailableModelsResponse { models: Vec<AvailableModelInfo> }`，`AvailableModelInfo { model_id, rate_multiplier }`，仅反序列化实际用到的字段
- `src/kiro/token_manager.rs` 新增模块级 `list_available_models(credentials, config, token, proxy)` 函数（对齐 `get_usage_limits` 的写法），POST `https://management.{region}.kiro.dev/?origin=KIRO_CLI&profileArn=...`，header `content-type: application/x-amz-json-1.0`、`x-amz-target: AmazonCodeWhispererService.ListAvailableModels`；`MultiTokenManager::list_available_models(&self)` 包装方法用 `acquire_context(None)` 取任意账号
- `src/anthropic/mod.rs` 将 `mod converter;` 提升为 `pub(crate) mod converter;`，复用已有 `map_model()` 做 id 归一化（不新写映射逻辑）
- `src/admin/service.rs` 新增 `AdminService::list_model_rates(&self) -> HashMap<String, f64>`：调用 `list_available_models()`，成功则以 `model_id → rate_multiplier` 建表；失败则 `tracing::warn!` 并返回空表
- `src/admin/types.rs` 新增 `AdminModelItem`（`#[serde(flatten)]` 包裹现有 `Model` + `rate_multiplier: Option<f64>`，不重复定义字段）与 `AdminModelsResponse`
- `src/admin/api_keys.rs::get_admin_models` 改为接受 `State(AdminState)`，对 `build_model_list()` 的每一项调用 `map_model(&model.id)` 归一化后查表得到 `rate_multiplier`

## 预期影响

- 管理员打开"支持模型"页时会新增一次到 Kiro 官方接口的实时网络请求；该接口是轻量级元数据查询（非 `generateAssistantResponse` 生成请求），不消耗 `AGENTIC_REQUEST` 额度
- 无可用账号或上游报错时，模型列表接口仍返回 200、模型条目完整，仅 `rate_multiplier` 全部为 `null`，不影响页面核心可用性
- `/v1/models`、现有 `Model`/`ModelsResponse` 类型均不受影响

## 风险

- `ListAvailableModels` 是私有协议，无官方文档，依赖用户抓包还原；若 Kiro 后续调整该接口的 URL/字段结构会导致费率显示失效（已做优雅降级为 `null`，不会导致模型列表页崩溃或整体报错）
- **region 取值空间未跨账号验证**：新接口 host `management.{region}.kiro.dev` 与现有 `getUsageLimits` 的 `q.{region}.amazonaws.com` 复用同一个 `credentials.effective_api_region()`，但抓包只覆盖了单一账号/单一 region。若某些账号的 region 命名在两套域名下不一致，会导致该账号的费率查询系统性失败——降级机制保证不影响列表可用性，但会让"费率倍率"列整体失效且不易察觉。已在 tasks.md 中加入归档前的真实账号验证步骤作为兜底
- 每次访问支持模型页都会触发一次额外的上游调用，属于用户明确要求的行为（"直接每次调用"），非缺陷
