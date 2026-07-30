# Design：enterprise-sso-azure-ad

## 上下文

现有认证分支只有 `social`（Kiro OAuth）和 `idc`（AWS SSO OIDC）。external_idp 是第三条路径，
刷新逻辑与 idc 相似（都是 OIDC `refresh_token` grant），但端点和认证客户端模型不同：
- idc：端点固定为 `oidc.{region}.amazonaws.com`，有 clientSecret
- external_idp：端点由用户在凭据中提供，公共客户端无 clientSecret，需携带 scope

## 目标 / 非目标

- **目标**：最小化新代码，复用现有 refresh 响应解析逻辑，不引入新 crate
- **非目标**：不实现 interactive re-login / PKCE 流，不自动发现 token_endpoint（不做 OIDC Discovery）

## 决策

### D1：字段扩展方式 — 追加到现有 KiroCredentials，不新建结构体
复用现有 `KiroCredentials` 结构，追加 `provider`（`Option<String>`）、`token_endpoint`（`Option<String>`）、
`scopes`（`Option<String>`）。替代方案（枚举变体）会破坏 JSON 反序列化的向后兼容性。

### D2：刷新函数位置 — `token_manager.rs` 新增 `refresh_external_idp_token()`
与 `refresh_social_token` / `refresh_idc_token` 并列，保持文件内一致的函数命名模式。

### D3：请求体格式 — `application/x-www-form-urlencoded`（公共客户端）
Azure AD / Entra ID 支持公共客户端不传 `client_secret`；`scope` 字段使用凭据中的 `scopes`，
若为空则省略（依赖 IdP 默认 scope）。

### D4：TokenType header — 仅在 `build_headers()` 判断 auth_method，不改动 body 注入路径
`TokenType` 是请求头而非 body 字段，单点修改 `KiroProvider` 的 header 构建逻辑即可，
不影响 `rewrite_profile_arn` 等 body 处理。

## 风险 / 权衡

- external_idp 的 `expires_in` 有时不随响应返回（Entra ID 可能省略），此时回退为 +1h 估算（与 social 分支一致）
- `validate_refresh_token` 长度下限 100 是针对 Kiro social token 特征的启发式检查，external_idp token 绕过此检查是合理的
