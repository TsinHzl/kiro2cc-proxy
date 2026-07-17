## 新增需求

### 需求：Admin 只读模型列表端点

Admin API 需提供一个只读端点，返回当前代理支持的完整模型列表，供 admin-ui 展示，且必须使用 admin 鉴权而非普通客户端 API Key 鉴权。

#### 场景：携带有效 admin key 请求模型列表

- **WHEN** 客户端携带有效的 `x-api-key`（等于 `adminApiKey`）请求 `GET /api/admin/models`
- **THEN** 返回 200，响应体为 `{ object: "list", data: [...] }`，`data` 中每一项包含 `id/object/created/owned_by/display_name/type/max_tokens` 字段（响应体结构与 `GET /v1/models` 一致；模型集合的实际来源与同步规则见"模型费率倍率展示"需求，两端点的模型集合不保证完全一致）

#### 场景：未携带或携带无效 admin key 请求模型列表

- **WHEN** 客户端未携带 `x-api-key`，或携带的值不等于 `adminApiKey`
- **THEN** 返回 401，不泄露模型列表内容

### 需求：模型费率倍率展示

`GET /api/admin/models` 响应的模型行数据须直接来源于 Kiro `ListAvailableModels` 接口的实时调用结果（不使用本地静态表、不使用缓存），仅在上游调用失败时回退到本地静态模型表。

#### 场景：上游调用成功

- **WHEN** 至少存在一个可用账号，且 Kiro `ListAvailableModels` 调用成功返回
- **THEN** `GET /api/admin/models` 的每一条模型行的 `id`/`display_name`/`max_tokens`/`rate_multiplier` 均直接取自本次接口响应对应模型的 `modelId`/`modelName`/`tokenLimits.maxOutputTokens`/`rateMultiplier`，模型集合与接口返回的 `models[]` 一一对应（包括接口新增/未在本地静态表出现过的模型，此时无需额外代码改动即可同步展示）；该集合不等于 `build_model_list()` 的客户端别名集合——不包含 `-thinking` 独立条目和历史日期式别名（如 `claude-3-5-sonnet-20241022`），这是预期行为

#### 场景：无可用账号或上游调用失败

- **WHEN** 当前没有可用账号，或调用 Kiro `ListAvailableModels` 失败（网络错误、非 2xx 响应、响应体解析失败）
- **THEN** `GET /api/admin/models` 仍返回 `200`，模型行回退为本地静态模型表（与 `/v1/models` 集合一致），所有条目的 `rate_multiplier` 均为 `null`

#### 场景：上游返回空模型列表

- **WHEN** Kiro `ListAvailableModels` 调用成功但 `models` 为空数组
- **THEN** `GET /api/admin/models` 返回 `200`，`data` 为空数组（如实反映接口结果，不触发静态列表回退）
