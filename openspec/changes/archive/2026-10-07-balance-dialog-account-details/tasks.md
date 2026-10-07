# 任务清单：balance-dialog-account-details

## 状态：ARCHIVED

## 任务

### 后端：字段透传

- [x] **T1** `src/kiro/token_manager/types.rs` 的 `CredentialEntrySnapshot` 新增 `provider: Option<String>`、`api_region: Option<String>`、`auth_region: Option<String>`，三者均带 `#[serde(skip_serializing_if = "Option::is_none")]`。验证：`cargo check` 通过。
- [x] **T2** `src/kiro/token_manager/manager/admin_ops.rs` 的 `snapshot()` 中填充 `provider: e.credentials.provider.clone()`、`api_region: e.credentials.api_region.clone()`、`auth_region: e.credentials.auth_region.clone()`。验证：`cargo check` 通过。
- [x] **T3** `src/admin/types.rs` 的 `CredentialStatusItem` 新增 `provider: Option<String>`、`api_region: Option<String>`、`auth_region: Option<String>`（三者均带 `#[serde(skip_serializing_if = "Option::is_none")]`，与该结构体既有可选字段风格一致）；新增单测断言三者出现在序列化 JSON 中。验证：`cargo test credential_status_item` 通过。
- [x] **T4** `src/admin/service.rs` 的 `get_all_credentials()` 中透传 `provider: entry.provider`、`api_region: entry.api_region`、`auth_region: entry.auth_region`。验证：`cargo test` 通过。
- [x] **T5** `src/admin/types.rs` 的 `UpdateCredentialRequest` 新增 `provider: Option<String>`。验证：`cargo check` 通过。
- [x] **T6** `src/kiro/token_manager/manager/admin_ops.rs` 的 `apply_update_fields()` 处理 `provider`（空字符串清空为 `None`）；新增单测断言空串清空与有值写入两种行为。验证：`cargo test apply_update_fields` 通过。
- [x] **T7** `src/kiro/token_manager/manager/admin_ops.rs` 的 `update_changes_subscription_identity()` 纳入 `update.provider.is_some()`；新增单测断言「仅更新 provider」返回 `true`（触发 `subscription_title` 重置）。验证：`cargo test` 通过。

### 前端：类型与派生函数

- [x] **T8** `web-ui/admin-ui/src/types/api.ts`：`CredentialStatusItem` 新增 `provider?: string`、`apiRegion?: string`、`authRegion?: string`；`AddCredentialRequest` / `UpdateCredentialRequest` 新增 `provider?: string`。验证：`pnpm build` 无 TS 错误。
- [x] **T9** `web-ui/admin-ui/src/lib/account-state.ts` 新增两个纯函数并导出：
  - `normalizeLoginSource(raw: string | undefined): 'Google' | 'GitHub' | undefined` —— 大小写不敏感匹配 `google` / `github`，其余（含 `BuilderId`、空、未知）返回 `undefined`
  - `loginSource(item: CredentialStatusItem): string | null` —— `authMethod === 'social'` 且 `normalizeLoginSource(item.provider)` 有值时返回该值；`authMethod === 'idc'` 返回 `'BuilderId'`；其余返回 `null`

  验证：`pnpm build` 通过，且按下列输入/输出清单逐一核对（该包无前端测试框架，以清单核对替代；核对方式：将两函数逐行等价的 JS 副本写入临时 `.mjs` 用 `node` 执行，8 项全部 PASS 后删除临时文件）：
  | 输入 | `loginSource()` 期望输出 |
  |---|---|
  | `social` + `provider='Google'` | `'Google'` |
  | `social` + `provider='github'`（小写） | `'GitHub'` |
  | `social` + `provider` 为空 | `null` |
  | `social` + `provider='AzureAD'` | `null` |
  | `social` + `provider='BuilderId'` | `null` |
  | `idc` + 任意 provider | `'BuilderId'` |
  | `external_idp` + `provider='AzureAD'` | `null` |
  | `authMethod` 为 `null` | `null` |

### 前端：弹窗改造

- [x] **T10** `web-ui/admin-ui/src/components/balance-dialog.tsx`：
  - props 新增 `credential: CredentialStatusItem | null`
  - 订阅标题容器由 `text-center` 改为左对齐（`getSubscriptionColor` 逻辑保持不变）
  - 新增账号信息区块（四组字段：核心标识 / 认证与区域 / 状态与健康 / 运行配置与统计），置于额度详情下方以分隔线隔开，`grid-cols-1 sm:grid-cols-2` + `max-h-[40vh] overflow-y-auto`
  - `credential` 为 `null` 时该区块渲染占位态「账号信息加载中」，不隐藏

  验证：`pnpm build` 通过 + 本地运行目视核对四组字段齐全、左对齐生效。
- [x] **T11** 同步更新调用链 `web-ui/admin-ui/src/components/dashboard.tsx` 与 `dashboard/dialogs.tsx`：按 `selectedCredentialId` 从 `data.credentials` 查找并透传 `credential` prop（查不到时传 `null`）。验证：`pnpm build` 通过，打开弹窗显示对应账号信息。

### 前端：登录来源采集

- [x] **T12** `web-ui/admin-ui/src/components/batch-import-dialog.tsx`：`CredentialInput` 接口新增 `provider?: string`；KAM 导出格式映射新增 `provider: normalizeLoginSource(a.credentials?.provider) ?? normalizeLoginSource(a.idp)`，并在 `addCredential` 载荷中透传 `provider: cred.provider`。验证：粘贴含 `idp: "Google"` 的 KAM JSON 导入后，`app/config/credentials.json` 中新账号 `provider` 为 `"Google"`；含 `idp: "BuilderId"` 时不写入该字段。
- [x] **T13** `web-ui/admin-ui/src/components/add-credential-dialog.tsx`：`authMethod === 'social'` 时新增「登录来源」下拉（Google / GitHub / 不指定），值经 `normalizeLoginSource()` 后随 `addCredential` 请求提交为 `provider`；`idc` 时不下发该字段。验证：手动添加 social 账号并选 GitHub 后，弹窗登录来源显示 GitHub。
- [x] **T14** `web-ui/admin-ui/src/components/edit-credential-dialog.tsx`：新增「登录来源」下拉（仅 `!isIdc` 渲染），初值由 `loginSource(credential)` 推导；仅在值发生变化时写入 `data.provider`（沿用该对话框「只提交有变更字段」的既有约定）。验证：对 `provider` 为空的存量 social 账号补录 GitHub 后，弹窗登录来源显示 GitHub。
- [x] **T15** `web-ui/admin-ui/src/i18n/locales/zh.json` 与 `en.json` 新增：账号信息区块全部字段标签、登录来源选项与展示文案（含「社交登录（来源未知）」及说明性 `title`）、认证方式展示映射（`social` → 社交登录、`idc` → BuilderId、`external_idp` → 企业 IdP）。验证：中英文切换下弹窗文案完整无缺失 key，`external_idp` 不显示原始英文值。

### 构建与验收

- [x] **T16** `cargo fmt` + `cargo clippy` clean；`cargo test` 全绿；`cd web-ui/admin-ui && pnpm build` 成功。验证：三条命令均无错误输出。

## 验收标准

- [ ] 弹窗内可见 17 项账号信息：邮箱、昵称、账号 ID、优先级、登录来源、认证方式、Auth Region、API Region、启用状态、健康状态、Token 过期时间、连续失败次数、7 天限流次数、最后使用时间、代理配置、thinking adaptive 开关、成功调用次数
- [ ] 订阅标题与弹窗标题、额度标签行左边缘对齐
- [ ] `authMethod === 'idc'` 的账号登录来源显示 BuilderId
- [ ] `authMethod === 'social'` 且 `provider` 为空的账号显示「社交登录（来源未知）」
- [ ] `provider` 为 `Google` / `GitHub`（大小写不敏感）的 social 账号显示对应来源
- [ ] 粘贴 KAM 导出 JSON 批量导入后，新账号的 `provider` 按归一化规则落盘到 `app/config/credentials.json`；`idp: "BuilderId"` 不写入该字段
- [ ] 通过编辑对话框为存量 social 账号补录登录来源后，弹窗展示同步更新
- [ ] 编辑登录来源后该账号 `subscription_title` 被清空，下次额度查询后重新填充
- [ ] 账号信息区块在 `credential` 为 `null` 时显示占位态且不隐藏，弹窗高度不跳变
- [ ] 余额数据（已使用 / 限额 / 百分比 / 剩余额度 / 下次重置）展示与改动前完全一致，无回归
- [ ] 表格行 `account-row.tsx` 展示与交互与改动前完全一致，无回归
- [ ] `cargo fmt` / `cargo clippy` / `cargo test` / `pnpm build` 全部通过
