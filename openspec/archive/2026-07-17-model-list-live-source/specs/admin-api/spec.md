## 修改需求

### 需求：模型费率倍率展示

`GET /api/admin/models` 响应的模型行数据须直接来源于 Kiro `ListAvailableModels` 接口的实时调用结果（不使用本地静态表、不使用缓存），仅在上游调用失败时回退到本地静态模型表。

#### 场景：上游调用成功

- **WHEN** 至少存在一个可用账号，且 Kiro `ListAvailableModels` 调用成功返回
- **THEN** `GET /api/admin/models` 的每一条模型行的 `id`/`display_name`/`max_tokens`/`rate_multiplier` 均直接取自本次接口响应对应模型的 `modelId`/`modelName`/`tokenLimits.maxOutputTokens`/`rateMultiplier`，模型集合与接口返回的 `models[]` 一一对应（包括接口新增/未在本地静态表出现过的模型）；该集合不等于 `build_model_list()` 的客户端别名集合——不包含 `-thinking` 独立条目和历史日期式别名（如 `claude-3-5-sonnet-20241022`），这是预期行为

#### 场景：无可用账号或上游调用失败

- **WHEN** 当前没有可用账号，或调用 Kiro `ListAvailableModels` 失败（网络错误、非 2xx 响应、响应体解析失败）
- **THEN** `GET /api/admin/models` 仍返回 `200`，模型行回退为本地静态模型表（与 `/v1/models` 集合一致），所有条目的 `rate_multiplier` 均为 `null`

#### 场景：上游返回空模型列表

- **WHEN** Kiro `ListAvailableModels` 调用成功但 `models` 为空数组
- **THEN** `GET /api/admin/models` 返回 `200`，`data` 为空数组（如实反映接口结果，不触发静态列表回退）
