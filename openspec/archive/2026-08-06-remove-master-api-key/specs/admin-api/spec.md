# Admin API 变更规范：移除主 API Key

## 修改的需求

### 需求：`GET /api/admin/server-info` 响应体不再包含主 API Key

#### 场景：查询服务器信息
- **WHEN** 客户端请求 `GET /api/admin/server-info`
- **THEN** 响应体仅包含 `{ version: string }`，不再包含 `masterApiKey` 字段

### 需求：`GET /api/admin/config/auth-keys` 响应体不再包含主 API Key

#### 场景：查询认证密钥
- **WHEN** 客户端请求 `GET /api/admin/config/auth-keys`
- **THEN** 响应体仅包含 `{ adminApiKey: string }`（脱敏），不再包含 `apiKey` 字段

### 需求：`PUT /api/admin/config/auth-keys` 不再支持修改主 API Key

#### 场景：修改认证密钥请求体包含 apiKey 字段
- **WHEN** 客户端 `PUT /api/admin/config/auth-keys` 请求体携带 `apiKey` 字段
- **THEN** 该字段被忽略（因请求体类型中已移除该字段，序列化时静默丢弃），不产生错误，仅 `adminApiKey` 生效

#### 场景：仅修改 Admin Password
- **WHEN** 客户端 `PUT /api/admin/config/auth-keys` 请求体为 `{ adminApiKey: "new-value" }`
- **THEN** 运行时 `admin_api_key` 更新生效，且持久化写入 `config.json` 的 `adminApiKey` 字段；行为与改动前一致

## 移除的需求

### 需求：主 API Key 全局兜底认证

移除 `/v1/messages`、`/cc/v1/messages` 等 Anthropic 兼容接口中的主 API Key 校验分支。

#### 场景（移除前）：主 Key 匹配直接放行
- **WHEN**（移除前）请求头 `x-api-key`/`Authorization: Bearer` 与 `config.apiKey` 相等
- **THEN**（移除前）直接放行，`ApiKeyContext.id = 0`，不检查启用/过期/额度

该场景在本次变更后不再存在：鉴权仅通过 `ApiKeyManager`（子 Key）校验，任何请求都必须携带已在 Admin 后台创建且启用、未过期、未超额度的子 Key，否则返回 401。
