## 新增需求

### 需求：Admin 只读模型列表端点

Admin API 需提供一个只读端点，返回当前代理支持的完整模型列表，供 admin-ui 展示，且必须使用 admin 鉴权而非普通客户端 API Key 鉴权。

#### 场景：携带有效 admin key 请求模型列表

- **WHEN** 客户端携带有效的 `x-api-key`（等于 `adminApiKey`）请求 `GET /api/admin/models`
- **THEN** 返回 200，响应体为 `{ object: "list", data: [...] }`，`data` 中每一项包含 `id/object/created/owned_by/display_name/type/max_tokens` 字段，且模型集合与 `GET /v1/models` 返回的集合完全一致

#### 场景：未携带或携带无效 admin key 请求模型列表

- **WHEN** 客户端未携带 `x-api-key`，或携带的值不等于 `adminApiKey`
- **THEN** 返回 401，不泄露模型列表内容

#### 场景：新增模型后模型列表端点自动同步

- **WHEN** `build_model_list()` 未来新增或修改模型条目（如新增一个模型系列）
- **THEN** `GET /api/admin/models` 无需额外代码改动即返回更新后的集合（因为该端点直接复用 `build_model_list()` 的返回值，不维护独立的模型数据副本）

### 需求：模型费率倍率展示

`GET /api/admin/models` 响应的每个模型条目须包含官方实时费率倍率（`rate_multiplier`），来源于 Kiro `ListAvailableModels` 接口的实时调用，不使用缓存。

#### 场景：上游调用成功且模型可映射

- **WHEN** 至少存在一个可用账号，且 Kiro `ListAvailableModels` 调用成功返回，且某内部模型 id 经 `map_model()` 归一化后的目标 id 出现在本次返回的模型目录中
- **THEN** 该模型条目的 `rate_multiplier` 字段为对应的数值（如 `1.3`）

#### 场景：无可用账号或上游调用失败

- **WHEN** 当前没有可用账号，或调用 Kiro `ListAvailableModels` 失败（网络错误、非 2xx 响应）
- **THEN** `GET /api/admin/models` 仍返回 `200`，模型列表完整（与不含费率信息时一致），所有条目的 `rate_multiplier` 均为 `null`

#### 场景：归一化后未命中当前账号的模型目录

- **WHEN** 上游调用成功，但某内部模型 id 经 `map_model()` 归一化后的目标 id 不在本次返回的模型目录中
- **THEN** 该模型条目的 `rate_multiplier` 为 `null`
