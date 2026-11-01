# 任务清单：enhance-balance-dialog-usage-detail

## 状态：DONE

## 任务

- [x] 任务 1：扩展 `src/kiro/model/usage_limits.rs` 数据模型——`Bonus` 增加 `bonus_code`/`display_name`/`expires_at`，`FreeTrialInfo` 确认 `free_trial_expiry`，`UsageBreakdown` 增加 `resource_type`/`display_name`/`display_name_plural`/`currency`/`unit`/`overage_rate`/`overage_cap`，`SubscriptionInfo` 增加 `subscription_type`/`upgrade_capability`/`overage_capability`，`UsageLimitsResponse` 增加 `user_info`/`overage_configuration`；全部 `#[serde(default)]`。验证：`cargo test usage_limits`（含新增反序列化用例：bonus 明细字段、freeTrial ACTIVE/EXPIRED、时间戳解析、缺失字段容忍）通过。
- [x] 任务 2：主链路 URL 补参数——`refresh.rs::get_usage_limits` 两个 URL 分支（带/不带 profileArn）追加 `&isEmailRequired=true`，与不带 ARN 补救路径（issue #43）口径统一，保障 `userInfo` 可达。验证：`cargo test usage_limits` 既有 URL 断言用例同步更新后通过。
- [x] 任务 3：凭据创建时间——`KiroCredentials` 增加 `created_at: Option<String>`（RFC3339，`#[serde(default)]`），新增账号写入、存量账号首次加载时补写并落盘；`CredentialStatusItem` 透出 `createdAt`。验证：`cargo test credentials` + 新增兼容性用例（旧 JSON 无字段可反序列化、新账号落盘有值）通过。
- [x] 任务 4：扩展 `src/admin/types.rs` 的 `BalanceResponse`（新增 `base_usage`/`base_limit`/`free_trial`/`bonuses`（含 status）/`resource_detail`/`subscription_type`/`upgrade_capable`/`user_id`/`user_email` 可选字段）与 `src/admin/service.rs::fetch_balance` 组装逻辑（对齐 KAM `parseUsageResponse` 口径：freeTrial/bonus 仅 ACTIVE 计入汇总，分解字段透传 status）。验证：`cargo test balance` 通过 + `cargo check` clean。
- [x] 任务 5：前端 `web-ui/admin-ui/src/types/api.ts` 补 `BalanceResponse` 新字段与 `CredentialStatusItem.createdAt` 类型；`balance-dialog.tsx` 新增：三卡片分解（主配额含重置日期；免费试用恒渲染、无额度 0/0 + 「无试用额度」占位；奖励总计含 N 个生效奖励）、生效奖励明细列表（名称 + ACTIVE 徽章 + Code + 过期时间 + 用量 + 百分比）、订阅详情（rawType/超额费率/资源类型/可升级徽章）、用户 ID、Token 到期时间（凭据 `expiresAt`）、添加日期（`createdAt`）；奖励明细无数据时整块隐藏。验证：`pnpm build` 通过，本地 UI 目视核对布局与降级态。
- [x] 任务 6：i18n 补全（`zh.json`/`en.json`）新增标签文案；`cargo fmt` + `cargo clippy` clean；全量 `cargo test` 无回归。

## 验收标准

- [x] 打开账号余额弹窗，能看到截图中的全部信息：主配额/免费试用/奖励总计三卡片（含重置日期、N 个生效奖励）、生效奖励明细（名称/ACTIVE 徽章/Code/过期时间/用量/百分比）、订阅详情（rawType/超额费率/资源类型/可升级）、用户 ID、Token 到期时间、添加日期
- [x] 汇总用量数字与改造前一致（口径不变，无 UI 回归）
- [x] 无 ACTIVE 奖励的账号奖励明细区块隐藏；免费试用卡片恒渲染，无额度时 0/0 + 占位文案
- [x] `cargo fmt` + `cargo clippy` + `cargo test` 全部通过；admin-ui `pnpm build` 通过
