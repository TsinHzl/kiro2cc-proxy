# 变更提案：enhance-balance-dialog-usage-detail

## 背景

Admin UI 账号余额弹窗目前只展示汇总用量（已用/上限/剩余/百分比/重置时间），而上游 `getUsageLimits` API 实际返回了远比这丰富的数据。参考 Kiro-account-manager（KAM）的账号详情弹窗（截图）：配额按「主配额 / 免费试用 / 奖励总计」三卡片分解、生效奖励逐条列出（名称/Code/过期时间/用量）、订阅详情含 rawType/超额费率/资源类型/可升级、基本信息含用户 ID。这些数据 kiro2cc-proxy 的 Rust 模型在反序列化时直接丢弃了，用户无法在本项目管理端看到账号的真实额度结构。

## 目标范围

**在范围内：**

1. **后端模型补全**（`src/kiro/model/usage_limits.rs`）：`UsageLimitsResponse` / `UsageBreakdown` / `Bonus` / `FreeTrialInfo` / `SubscriptionInfo` 补齐 KAM 已解析的字段——bonus 的 `bonusCode`/`displayName`/`expiresAt`/`status`、`freeTrialExpiry`（含秒级时间戳→秒的口径确认）、breakdown 级 `resourceType`/`displayName`/`currency`/`unit`/`overageRate`/`overageCap`、`subscriptionInfo.subscriptionType`/`upgradeCapability`/`overageCapability`、顶层 `userInfo`（email/userId）、`overageConfiguration`。
2. **主链路补 `isEmailRequired=true`**（`src/kiro/token_manager/refresh.rs::get_usage_limits`）：两个 URL 分支（带/不带 profileArn）均追加该参数，对齐 KAM REST 口径，保障 `userInfo`（用户 ID/邮箱）可达；上游不返回时降级隐藏，不影响其他字段。
3. **凭据创建时间**（`src/kiro/model/credentials/model.rs` + `src/kiro/token_manager` + `src/admin/types.rs`）：`KiroCredentials` 新增 `created_at`（RFC3339，`#[serde(default)]` 向后兼容），新增账号时写入、存量账号首次落盘时补写；`CredentialStatusItem` 透出 `createdAt`。
4. **Admin API 扩展**（`src/admin/types.rs` + `src/admin/service.rs`）：`BalanceResponse` 新增可选字段 `baseUsage`/`baseLimit`（主配额）、`freeTrial`（已用/上限/过期时间/激活态）、`bonuses[]`（code/名称/已用/上限/过期时间/status）、`resourceDetail`（resourceType/unit/overageRate）、`subscriptionType`/`upgradeCapable`、`userInfo`（email/userId）、`nextResetAt` 保持不变。全部字段 `Option`/默认空，旧客户端无感知。
5. **前端弹窗增强**（`web-ui/admin-ui/src/components/balance-dialog.tsx` + `types/api.ts` + i18n）：融入现有阶梯式设计——核心额度看板下方新增「主配额 / 免费试用 / 奖励总计」三卡片分解（恒渲染三卡栅格：主配额含重置日期；免费试用无额度时 0/0 + 「无试用额度」占位文案，与截图一致；奖励总计含 N 个生效奖励）；生效奖励明细列表（名称 + ACTIVE 状态徽章 + Code + 过期时间 + 用量 + 已用百分比）；订阅详情字段（rawType/超额费率/资源类型/可升级徽章）与用户 ID；Token 到期时间（凭据已有 `expiresAt`，纯前端展示行）；添加日期（`createdAt`）。无数据的区块优雅降级。
6. **单元测试**：模型层新增字段的反序列化用例（bonus 明细含 status、freeTrial 过期时间、subscriptionInfo 扩展、userInfo）、`created_at` 兼容性用例。

**不在范围内：**

- 不加「刷新数据」按钮（用户已确认，维持 react-query + 300s 后端缓存现状）
- 不复刻 KAM 的「账户可用模型」区块（本项目已有单账号模型查询入口）
- 不改余额缓存 TTL 与 `get_usage_limits_for` 的刷新/自愈链路
- 不动 user-ui（仅 admin-ui）
- 不新增外部 crate

## 技术方案

- 数据获取完全复用现有链路：`refresh.rs::get_usage_limits` → `admin_ops.rs::get_usage_limits_for`（含 IdC 自愈）→ `service.rs::get_balance`（300s 缓存）。除 URL 追加 `isEmailRequired=true` 查询参数外不发新请求、不改请求方法，只在模型层「多保留」上游已返回的字段。
- 字段口径对齐 KAM `main/index.ts::parseUsageResponse`：主配额取 `usageLimitWithPrecision ?? usageLimit`；freeTrial 仅 `freeTrialStatus === 'ACTIVE'` 时计入汇总；bonus 仅 `status === 'ACTIVE'` 计入；`expiresAt` 为秒级 Unix 时间戳时 ×1000 转 ISO（后端统一转秒级 f64 交前端格式化，与现有 `nextResetAt` 一致）。
- 汇总数字（`currentUsage`/`usageLimit`）计算口径不变，仅新增分解字段，现有 UI 不回归。
- 前端三卡片与奖励明细复用弹窗内已有的 `surface-2` 卡片视觉语言（InfoRow/CardHeader/QuotaPercentBadge），不引入新组件库元素。

## 预期影响

- Admin API `GET /api/admin/credentials/{id}/balance` 响应体向后兼容地新增字段；旧字段语义不变。
- 弹窗信息密度上升，需在 536px 宽度约束下排版（三卡片栅格 + 奖励明细紧凑行）。
- Rust 模型新增 `Option` 字段不反序列化失败（`#[serde(default)]`），对上游响应格式变化有容忍度。

## 风险

- **上游字段形态差异**：REST API 与 CBOR API 的字段名有历史差异（KAM 注释提到 `freeTrialUsage` vs `freeTrialInfo`、时间戳 number vs string）。应对：`Option` + `serde(default)` 双容忍，缺失即降级不渲染，不阻塞汇总展示。
- **`isEmailRequired=true` 兼容性**：该参数为 KAM REST 口径的既有参数，预期对响应结构无破坏；若上游对个别账号类型报错，回退方案是去掉该参数（userInfo 降级隐藏）。
- **弹窗信息过载**：区块多导致滚动过长。应对：奖励明细区块无 ACTIVE 奖励时整体隐藏；freeTrial 卡片恒渲染但无额度时 0/0 + 占位文案。
- **`created_at` 存量数据**：旧 credentials.json 无该字段，`#[serde(default)]` 反序列化为 None，首次加载/落盘时补写当前时间（与 KAM「添加日期」语义近似，非精确历史时间）。
