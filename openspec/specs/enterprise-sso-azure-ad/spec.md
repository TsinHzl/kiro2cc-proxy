# Spec：enterprise-sso-azure-ad

## 新增需求

### 需求 1：external_idp 账号识别

#### 场景：canonicalize 多种写法
- **WHEN** credentials.json 中 `authMethod` 为 `"external_idp"` / `"AzureAD"` / `"EntraID"`（大小写不敏感）
- **THEN** 内存中统一规范为 `"external_idp"`，序列化回文件保持原值

#### 场景：validate 放宽
- **WHEN** auth_method 为 `"external_idp"` 且 refresh_token 非空（无论长度）
- **THEN** `validate_refresh_token` 返回 `Ok(())`，不抛 "refresh_token 过短" 错误

---

### 需求 2：external_idp token 刷新

#### 场景：刷新成功
- **WHEN** auth_method = `external_idp`，`token_endpoint` 和 `client_id` 均存在，IdP 返回 200 含 `access_token`
- **THEN** 凭据 `access_token` 更新，`expires_at` 按 `expires_in` 秒重新计算；若响应含 `refresh_token` 则回写，否则保留原值

#### 场景：token_endpoint 缺失
- **WHEN** auth_method = `external_idp` 但 `token_endpoint` 为 None
- **THEN** 返回错误 `"external_idp 刷新需要 tokenEndpoint"`，不发起 HTTP 请求

#### 场景：client_id 缺失
- **WHEN** auth_method = `external_idp` 但 `client_id` 为 None
- **THEN** 返回错误 `"external_idp 刷新需要 clientId"`

#### 场景：IdP 返回 4xx
- **WHEN** IdP 返回 401 / 400
- **THEN** 返回包含 HTTP 状态码的错误信息，账号进入失败状态（与 idc/social 行为一致）

---

### 需求 3：数据面请求头

#### 场景：external_idp 账号发起请求
- **WHEN** 选中账号 `auth_method == "external_idp"`
- **THEN** 数据面（GenerateAssistantResponse）请求头包含 `TokenType: EXTERNAL_IDP`

#### 场景：非 external_idp 账号
- **WHEN** 选中账号 `auth_method` 为 `social` 或 `idc`
- **THEN** 请求头不包含 `TokenType` 字段（行为不变）
