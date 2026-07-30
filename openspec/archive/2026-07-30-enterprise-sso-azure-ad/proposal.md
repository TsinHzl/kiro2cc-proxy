# 变更提案：enterprise-sso-azure-ad

## 背景

Kiro 支持企业用户通过 Microsoft Entra ID / Azure AD 进行 SSO 认证（auth_method = `external_idp`）。
这类账号既不是 AWS Builder ID（social），也不是 IAM Identity Center（idc），其 token 刷新走
IdP 的 OAuth2 `refresh_token` grant（公共客户端，无 clientSecret），并需要在数据面请求头中注入
`TokenType: EXTERNAL_IDP`。当前代理不支持该认证方式，添加此类账号后会走 social 刷新路径导致失败。

## 目标范围

**在范围内：**
- `KiroCredentials` 新增 `external_idp` 相关字段：`provider`、`token_endpoint`、`issuer_url`、`scopes`
- `canonicalize_auth_method_value` 识别 `external_idp`
- `refresh_token()` 新增 `external_idp` 分支：向 `token_endpoint` 发送 `refresh_token` grant（无 clientSecret）
- `KiroProvider` 数据面请求头注入 `TokenType: EXTERNAL_IDP`（当 auth_method 为 external_idp 时）
- `validate_refresh_token` 对 external_idp 账号放宽长度限制（Azure refresh_token 可能较短）
- Admin API 的账号字段透传（add/update 时允许新字段写入 credentials.json）

**不在范围内：**
- 浏览器登录 / OIDC 授权码流（interactive login）— 用户自行从 Kiro IDE 提取 token 后手动添加
- `ListAvailableProfiles` 懒解析回填 profileArn — 当前 profileArn 已由用户手动配置，暂不自动推导
- Kiro API Key 认证方式（issue #20 的另一半，独立变更）
- 前端 Admin UI 增加专属 external_idp 账号添加弹窗（现有通用添加面板已可填写字段）

## 技术方案

1. **凭据字段扩展**：在 `KiroCredentials` 追加三个可选字段（`provider`、`token_endpoint`、`scopes`），
   `canonicalize_auth_method_value` 将 `external_idp`/`AzureAD`/`EntraID` 统一归一为 `external_idp`。

2. **刷新逻辑**：`refresh_token()` 在现有 `idc` / `social` 分支之外新增 `external_idp` 分支，
   向 `token_endpoint` POST `application/x-www-form-urlencoded`，字段为
   `{grant_type, refresh_token, client_id, scope}`（无 client_secret，公共客户端）。

3. **请求头注入**：`KiroProvider::build_headers()` 检测 `auth_method == "external_idp"`，
   追加 `TokenType: EXTERNAL_IDP` header，其余头部与 social 账号保持一致。

4. **validate 放宽**：`validate_refresh_token` 对 external_idp 跳过 100 字符下限检查（Azure token 可能更短）。

## 预期影响

- 不影响现有 social / idc 账号的任何逻辑
- `credentials.json` 向后兼容：新字段均为 `Option`，旧格式文件读取不受影响
- 需要新增 3 个单元测试覆盖 external_idp 刷新路径

## 风险

- Azure token_endpoint 格式多样（v1/v2，tenantId 变量），由用户正确填写，代理不做额外校验
- 若 external_idp refresh_token 过期，当前无法走 interactive re-login，账号会进入失败状态（与现有 social/idc 行为一致）
