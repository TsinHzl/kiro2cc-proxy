# 设计文档：add-model-rate-multiplier

## 上下文

用户提供了 Kiro CLI `/model` 选择器背后真实接口 `AmazonCodeWhispererService.ListAvailableModels` 的完整抓包（`management.{region}.kiro.dev`，`x-amz-json-1.0` 协议，请求体 `{"origin":"KIRO_CLI","profileArn":...}`，响应含 `models[].rateMultiplier`）。用户明确要求"直接每次调用官方的那个获取支持的模型的列表即可"，即不做静态表、不做缓存，每次访问 admin-ui 支持模型页都实时查询。

## 目标 / 非目标

**目标：**
- 支持模型页展示每个模型的实时官方费率倍率
- 上游接口不可用时不影响模型列表本身的可用性

**非目标：**
- 不改变 `/v1/messages`、`/cc/v1/messages`、`/v1/models` 的行为
- 不引入缓存层（用户已明确拒绝缓存方案）
- 不新增账号选择/负载均衡逻辑

## 决策

1. **账号选择复用 `acquire_context(None)`**：`ListAvailableModels` 不依赖具体账号身份，只需要任意一个当前可用、已刷新 token 的账号即可。这与 `MultiTokenManager::get_usage_limits()`（无 id 版本）完全同构，不新增选择策略。
2. **响应类型用 `serde(flatten)` 包裹现有 `Model`**：新增 `AdminModelItem { #[serde(flatten)] model: Model, rate_multiplier: Option<f64> }`，而不是复制 `Model` 的 7 个字段。好处：`Model`/`ModelsResponse`（`/v1/models` 公开契约）完全不受触碰，新字段只出现在 admin-only 响应里。
3. **`converter` 模块可见性从 private 提升为 `pub(crate)`**：复用已验证的 `map_model()` 做内部 id → 真实 Kiro modelId 的归一化（含 `-thinking` 变体、新旧版本号拼写的兼容处理），不重新实现映射表，避免逻辑分裂。
4. **失败降级策略**：`AdminService::list_model_rates()` 捕获 `list_available_models()` 的 `Err`（无可用账号 / 网络错误 / 非 2xx），记录 `tracing::warn!` 后返回空 `HashMap`。`get_admin_models` 因此永远不会因为上游调用失败而 500——模型列表可用性优先于费率展示的完整性。
5. **超时设为 15s**（区别于 `getUsageLimits` 的 60s）：这是一次轻量元数据查询，且直接影响 admin-ui 页面加载体验，应该快速失败而不是长时间挂起。
6. **归一化后未命中的模型（包括账号侧未开放的模型，如某些 profile 下 `claude-fable-5` 可能不在 `ListAvailableModels` 返回列表中）显示为 `null`**：这如实反映该账号当前的真实可用目录，而不是伪造一个数值。

## 风险 / 权衡

- **无官方文档的私有协议**：`ListAvailableModels` 完全基于用户抓包还原，Kiro 后续变更协议会导致费率失效；已通过优雅降级保证不会影响核心功能。
- **额外网络往返**：每次打开/刷新支持模型页都会多一次上游请求；这是用户显式选择的行为（"每次调用"优先于缓存的实时性/一致性），不是性能疏漏。
