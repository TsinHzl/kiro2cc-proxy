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
