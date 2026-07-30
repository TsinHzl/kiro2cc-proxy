# 任务清单：enterprise-sso-azure-ad

## 状态：DONE

## 任务

- [x] T1：`KiroCredentials` 新增 `provider`、`token_endpoint`、`scopes` 三个 `Option<String>` 字段，`canonicalize_auth_method_value` 识别 `external_idp` / `AzureAD` / `EntraID`
- [x] T2：`validate_refresh_token` 对 `external_idp` 账号跳过 100 字符下限
- [x] T3：`token_manager.rs` 新增 `refresh_external_idp_token()` 函数，向 `token_endpoint` 发送 `refresh_token` grant（无 clientSecret），解析 access_token / refresh_token / expires_in 写回凭据
- [x] T4：`refresh_token()` 分发函数新增 `external_idp` 分支，调用 T3 函数
- [x] T5：`KiroProvider` 数据面请求头注入 `TokenType: EXTERNAL_IDP`（当 `auth_method == "external_idp"` 时）
- [x] T6：新增单元测试：T1 canonicalize、T2 validate 放宽、T3 刷新逻辑（mock HTTP）

## 验收标准

- [ ] `authMethod: "external_idp"` + `tokenEndpoint` + `clientId` 账号能成功刷新 token
- [ ] 刷新后 `access_token` / `expires_at` 正确回写 credentials.json
- [ ] 数据面请求携带 `TokenType: EXTERNAL_IDP` header
- [ ] 现有 social / idc 账号行为无回归（cargo test 全绿）
- [ ] `cargo clippy` 无新增 warning
