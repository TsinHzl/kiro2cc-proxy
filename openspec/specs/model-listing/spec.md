# 规范增量：model-listing

## 新增需求

### 需求：/v1/models 动态来源

`GET /v1/models` 返回的模型列表以上游 Kiro `ListAvailableModels` 实时响应为准，带 TTL 缓存与静态表回退，端点在任何情况下都不得失败。

#### 场景：缓存命中直接返回
- **WHEN** 收到 `GET /v1/models` 且缓存存在且未超过 `modelCacheTtlSecs`
- **THEN** 直接返回缓存的模型列表，不发起上游调用

#### 场景：缓存过期成功刷新
- **WHEN** 缓存不存在或已超过 `modelCacheTtlSecs`，且上游 `ListAvailableModels` 调用成功返回非空模型集
- **THEN** 将上游 `AvailableModelInfo` 映射为 Anthropic `Model`（`owned_by` 由 `guess_owned_by()` 按 ID 前缀推断，`max_tokens` 取 `token_limits.max_output_tokens`），写入缓存并返回

#### 场景：刷新失败且存在旧缓存
- **WHEN** 缓存已过期，但上游调用失败（无可用账号 / 网络错误 / 非 2xx / 反序列化失败）
- **THEN** 继续返回上一次成功的缓存内容，并记录 warn 日志，不向客户端暴露错误

#### 场景：刷新失败且无任何缓存
- **WHEN** 上游调用失败且此前从未成功缓存过
- **THEN** 回退返回 `build_model_list()` 静态表内容，记录 warn 日志，HTTP 状态仍为 200

#### 场景：GET /v1/models/{id} 与列表来源一致
- **WHEN** 收到 `GET /v1/models/{model_id}`
- **THEN** 从与 `/v1/models` 相同的动态来源中查找匹配 `id` 的模型；命中返回该 `Model`，未命中返回 404

### 需求：未知模型开放透传

`map_model()` 对未命中内置关键词规则的模型 ID 执行开放透传，而非一律拒绝。

#### 场景：命中内置规则的模型走规范化映射
- **WHEN** 请求模型命中现有内置关键词规则（如 `sonnet` / `opus` / `gpt` / `glm` 等）
- **THEN** 行为与现状完全一致，映射到对应 Kiro 规范化模型 ID（内置规则优先级不变）

#### 场景：未命中规则的非空模型 ID 透传
- **WHEN** 请求模型未命中任何内置规则，但 trim 后为非空字符串
- **THEN** 剥离可选 `-thinking` 后缀后，将该模型 ID 原样作为 Kiro 模型 ID 返回，并记录透传日志；实际可用性由上游判断

#### 场景：空模型 ID 仍被拒绝
- **WHEN** 请求模型 trim 后为空字符串
- **THEN** `convert_request` 返回 `UnsupportedModel` 错误（代理侧 400），不发往上游

## 配置增量

### 需求：modelCacheTtlSecs 配置项

#### 场景：默认 TTL
- **WHEN** 配置未显式设置 `modelCacheTtlSecs`
- **THEN** 使用默认值 3600 秒

#### 场景：env override
- **WHEN** 通过环境变量覆盖 `modelCacheTtlSecs`（经 `apply_env_overrides()`）
- **THEN** 覆盖值生效，与其他配置字段的 env 覆盖机制一致
