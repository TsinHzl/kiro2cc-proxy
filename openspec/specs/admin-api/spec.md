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

### 需求：批量 IP 归属地查询接口

Admin API 与 User API 各提供一个受鉴权保护的端点，输入一批 IP，返回每个 IP 的归属地信息（国家/省份/城市），分别供 admin-ui 日志详情页与 user-ui 用量日志页展示。两个端点共享同一份服务端 ip2region 离线索引，行为完全一致，仅鉴权方式和挂载路径不同。

#### 场景：查询公网 IP 的归属地（Admin）

- **WHEN** 管理员已鉴权，请求 `GET /api/admin/geo/batch?ips=222.35.11.141`
- **THEN** 响应包含 `"222.35.11.141"` 对应的对象，字段为 `country`（国家）、`regionName`（省份）、`city`（城市），值来自本地 ip2region 库的离线查询结果

#### 场景：查询公网 IP 的归属地（User）

- **WHEN** 用户已通过 `x-api-key` 鉴权，请求 `GET /api/user/geo/batch?ips=222.35.11.141`
- **THEN** 响应格式与结果与 Admin 端点一致

#### 场景：查询私有/环回地址

- **WHEN** 请求的 `ips` 参数中包含 `127.0.0.1`、`10.x.x.x`、`172.16.x.x`、`192.168.x.x` 或 `::1`
- **THEN** 该 IP 对应的值为 `null`，不进入 ip2region 查询（这类地址本身没有有效归属地）

#### 场景：查询格式非法的 IP

- **WHEN** `ips` 参数中的某一项不是合法的 IPv4/IPv6 字符串
- **THEN** 该项对应的值为 `null`，不影响批次中其他合法 IP 的正常返回（不整体报错）

#### 场景：批量查询包含重复 IP

- **WHEN** `ips` 参数中同一个 IP 出现多次
- **THEN** 响应中该 IP 只出现一次，对应一次查询结果（去重）

#### 场景：未鉴权访问

- **WHEN** 请求缺少有效的 Admin 鉴权凭据（访问 `/api/admin/geo/batch`）或有效的 `x-api-key`（访问 `/api/user/geo/batch`）
- **THEN** 分别返回现有 admin / user 鉴权中间件统一的 401/403 响应，不执行归属地查询

## 修改的行为

### 需求：`useIpGeo` 前端 hook 的数据来源

`admin-ui` 与 `user-ui` 中 `useIpGeo(ips)` 的对外类型（`GeoInfo { country, regionName, city, displayIp? }`）与调用方式保持不变，但内部实现分别改为请求各自后端新增的端点（`/api/admin/geo/batch` 或 `/api/user/geo/batch`），不再请求 `http://ip-api.com` 与 `https://api.ipify.org`。

#### 场景：管理员在 HTTPS 部署下查看日志详情

- **WHEN** admin-ui 部署在 HTTPS 域名下，管理员打开任意一个日志详情页（daily/credential/api-key）
- **THEN** IP 归属地正常显示（此前因 Mixed Content 被浏览器拦截而始终为空的问题被消除）

#### 场景：普通用户在 HTTPS 部署下查看自己的用量日志

- **WHEN** user-ui 部署在 HTTPS 域名下，用户打开用量日志页（`usage-log-page.tsx`）
- **THEN** IP 归属地正常显示，问题根因与效果与 Admin 侧一致

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
