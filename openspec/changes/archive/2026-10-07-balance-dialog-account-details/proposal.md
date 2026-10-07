# 变更提案：balance-dialog-account-details

## 背景

Admin 面板账号列表点击「钱包」图标弹出的「账号 #N 余额信息」弹窗，当前只展示订阅类型、额度进度、剩余额度、下次重置四项。用户无法在弹窗内确认「这是哪个账号、怎么登录的、当前健康状态如何」，必须关闭弹窗回到表格行才能核对邮箱/昵称/状态，核对成本高。

同时弹窗内订阅标题 `KIRO STUDENT` 为居中布局，与左侧的标题栏、下方的「已使用 / 限额」标签不构成对齐轴线，视觉上突兀。

需求来自用户对弹窗截图的三点明确诉求：信息更丰富、标题左对齐、展示登录来源。

## 目标范围

**在范围内：**

- 弹窗内新增账号信息区块，展示四组字段（不新增后端接口，但需在既有 `CredentialStatusItem` 上新增若干透传字段）：
  - **核心标识**：邮箱、昵称/备注、账号 ID、优先级
  - **认证与区域**：登录来源、认证方式、Auth Region、API Region
  - **状态与健康**：启用/禁用状态、健康状态、Token 过期时间、连续失败次数、7 天限流次数、最后使用时间
  - **运行配置与统计**：账号级代理、thinking adaptive 开关状态、成功调用次数
- 订阅标题由居中改为左对齐，与弹窗标题栏、下方额度标签形成统一左对齐轴线
- 登录来源（Google / GitHub / BuilderId）的采集与展示：复用现有 `provider` 字段
  - 批量导入（KAM 导出 JSON）时映射 `idp` 字段到 `provider`，写入前做归一化
  - **添加账号**对话框新增「登录来源」下拉（仅 social 认证时可选 Google / GitHub）
  - **编辑账号**对话框新增同款「登录来源」下拉，为存量 social 账号提供补录入口
  - 后端 `CredentialStatusItem` 透出 `provider` 与 `api_region`
  - 存量账号 `provider` 为空时，回退按 `authMethod` 推断：`idc` → BuilderId，`social` → 未知

**不在范围内：**

- 不新增后端接口或修改余额查询链路（`BalanceResponse` 保持不变）
- 不改动表格行 `account-row.tsx` 的既有展示
- 不做 `provider` 的存量数据自动回填迁移（Google/GitHub 在服务端不可推断，仅靠人工在编辑对话框补录）
- 不改动 `external_idp`（企业 IdC）写入 `provider` 的既有语义

## 技术方案

### 数据来源（关键约束）

服务端 `authMethod` 仅能区分 `social` / `idc` / `external_idp` 三类，**无法区分 Google 与 GitHub**——两者共用同一 `SOCIAL_PROFILE_ARN` 常量，refreshToken 与 accessToken 均为不透明字符串（非 JWT，无法解析 claim），`getUsageLimits` 响应也不含 IdP 信息。

因此登录来源只能来自**用户声明**（导入时或编辑时），落点选现有 `provider` 字段。

**`provider` 字段现状核对**（实测代码）：该字段仅在 `src/admin/service.rs:328` 添加账号时赋值、在 `src/kiro/token_manager/manager/admin_ops.rs:146` 参与身份一致性比较，**当前无任何业务消费方**；刷新链路的分支判据是 `auth_method`（`src/kiro/token_manager/refresh.rs:55-58,129-130`），不读 `provider`。因此为其补充新语义不会影响 token 刷新。

### provider 写入侧归一化规则（防止脏值入库）

KAM 导出 JSON 的 `idp` 取值包含 `Google` / `GitHub` / `BuilderId` 等。写入侧统一走前端归一化函数：

| 输入（大小写不敏感） | 写入值 |
|---|---|
| `google` | `Google` |
| `github` | `GitHub` |
| 其他（含 `BuilderId`、空、未知） | 不写入（`undefined`，字段保持原值） |

`BuilderId` 被显式排除：该来源已可由 `authMethod === 'idc'` 推断，写入 `provider` 会与 `external_idp` 的 IdP 名称语义混淆。

归一化仅在**前端**执行，后端 `AddCredentialRequest.provider` / `UpdateCredentialRequest.provider` 保持透传语义，不影响 `external_idp` 写入 `AzureAD` 等既有路径。

### 后端改动

| 文件 | 改动 |
|---|---|
| `src/kiro/token_manager/types.rs` | `CredentialEntrySnapshot` 新增 `provider: Option<String>`、`api_region: Option<String>`、`auth_region: Option<String>` |
| `src/kiro/token_manager/manager/admin_ops.rs` | `snapshot()` 填充上述三字段 |
| `src/admin/types.rs` | `CredentialStatusItem` 新增 `provider`、`api_region`、`auth_region` |
| `src/admin/service.rs` | `get_all_credentials()` 透传上述三字段 |
| `src/admin/types.rs` | `UpdateCredentialRequest` 新增 `provider: Option<String>` |
| `src/kiro/token_manager/manager/admin_ops.rs` | `apply_update_fields()` 处理 `provider`；纳入 `update_changes_subscription_identity()` |

`auth_region` 与 `api_region` 是账号级区域配置（`KiroCredentials` 已有字段），仅新增透传路径，不改变其解析优先级（`effective_auth_region` / `effective_api_region` 逻辑不变）。

`UpdateCredentialRequest.provider` 需纳入 `update_changes_subscription_identity` 判定：修改登录来源改变账号身份语义，与修改 `auth_method` 同类，应触发 `subscription_title` 重置。

### 前端改动

| 文件 | 改动 |
|---|---|
| `web-ui/admin-ui/src/types/api.ts` | `CredentialStatusItem` 新增 `provider?`、`apiRegion?`、`authRegion?`；`AddCredentialRequest` / `UpdateCredentialRequest` 新增 `provider?` |
| `web-ui/admin-ui/src/lib/account-state.ts` | 新增 `normalizeLoginSource()`（写入侧归一化）与 `loginSource()`（展示侧派生） |
| `web-ui/admin-ui/src/components/balance-dialog.tsx` | 新增账号信息区块；订阅标题改左对齐 |
| `web-ui/admin-ui/src/components/batch-import-dialog.tsx` | KAM 映射新增归一化后的 `provider` |
| `web-ui/admin-ui/src/components/add-credential-dialog.tsx` | social 认证时新增「登录来源」下拉 |
| `web-ui/admin-ui/src/components/edit-credential-dialog.tsx` | 新增「登录来源」下拉（补录入口） |
| `web-ui/admin-ui/src/i18n/locales/zh.json` / `en.json` | 新增字段标签、登录来源文案、认证方式展示映射 |

`BalanceDialog` 当前 props 只有 `credentialId`，需新增 `credential: CredentialStatusItem | null`。调用链 `dashboard.tsx` → `dashboard/dialogs.tsx` → `BalanceDialog` 同步透传；`dashboard.tsx` 已有 `data.credentials` 列表，按 `selectedCredentialId` 查找。

**降级行为**：`credential` 为 `null` 时（列表未加载完 / 账号已删除 / 刷新间隙），账号信息区块渲染为占位态「账号信息加载中」，不隐藏区块——避免内容出现时弹窗高度跳变。余额区块行为不变。

### 布局

订阅标题左对齐后，弹窗内所有元素共享同一左侧起始边距。账号信息区块置于额度详情下方，以分隔线隔开，采用「标签在上、值在下」的双列网格（与现有「剩余额度 / 下次重置」网格同构）：默认 `grid-cols-1`，`sm` 断点（640px）起 `sm:grid-cols-2`，与弹窗现有 `sm:max-w-md` 断点一致。区块外层加 `max-h-[40vh] overflow-y-auto`，防止字段增多后弹窗溢出视口。

## 预期影响

- 展示层增强 + 三个字段透传（`provider` / `auth_region` / `api_region`），不改动请求转发、协议转换、账号选择等核心链路
- `CredentialStatusItem` 新增三个可选字段，前端旧版本忽略即可，无破坏性
- **行为变更（需显式知悉）**：`UpdateCredentialRequest` 新增 `provider` 且纳入 `update_changes_subscription_identity` 后，编辑登录来源会清空该账号的 `subscription_title`，并在下次额度查询时重新填充（弹窗订阅名短暂显示「未知订阅类型」）。这是为保持身份变更后订阅信息一致性的有意取舍，与编辑 `auth_method` 的既有行为对齐
- 未传 `provider` 的既有 update 调用行为完全不变（`Option` 语义）

## 风险

| 风险 | 应对 |
|---|---|
| 存量账号 `provider` 为空，登录来源显示「未知」，用户期望落空 | 弹窗显示「社交登录（来源未知）」并在 `title` 说明「可通过编辑账号补录来源」；编辑对话框提供补录入口 |
| KAM `idp` 脏值（如 `BuilderId`、任意字符串）写入 `provider`，与 `external_idp` 的 IdP 名称语义混淆 | 写入侧 `normalizeLoginSource()` 白名单归一化，非 Google/GitHub 一律不写入 |
| 展示侧误把 `provider` 中的 `AzureAD` 当成登录来源 | `loginSource()` 仅在 `authMethod === 'social'` 时按 Google/GitHub 白名单映射，其余返回 `null` 由调用方回退到「来源未知」 |
| 弹窗信息量增加导致高度溢出 | 账号信息区块 `max-h-[40vh] overflow-y-auto` + 双列网格 |
| 编辑登录来源触发 `subscription_title` 清空，用户误认为数据丢失 | 该行为与编辑 `auth_method` 一致，属既有模式；如需提示可在编辑对话框文案中说明 |
| 后端新增字段未同步测试 | 同步补 `provider` / `api_region` 序列化断言与 `apply_update_fields` 行为断言 |
