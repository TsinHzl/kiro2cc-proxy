# 任务清单：account-metrics-consumption-rebrand

## 状态：ARCHIVED

## 任务

- [x] **任务 1：账号行额度单元格改为消费口径**
  - 文件：`admin-ui/src/components/account-row.tsx`
  - 改动：新增 `usageTone(usedPct)` 配色函数（40/70 阈值，与现有 60/30 剩余阈值数学等价）；额度单元格已查询态从 `remaining/usageLimit + remainingPct% + 剩余进度条` 改为 `(usageLimit−remaining)/usageLimit + usagePercentage% + 已用进度条`；进度条宽度改用 `usedPct`，配色改用 `usageTone`；未查询/加载态不变
  - 验证：`cd admin-ui && npx tsc --noEmit` 零错误；视觉确认进度条与百分比语义一致（已用口径）

- [x] **任务 2：dashboard 新增 consumedCreditsTotal 派生并下传**
  - 文件：`admin-ui/src/components/dashboard.tsx`
  - 改动：在 `liveCreditsTotal` 派生附近新增 `consumedCreditsTotal`（`Σ(usageLimit − remaining)`，仅 `usageLimit > 0` 的账号）；`AccountMetrics` 调用处新增 `consumedCreditsTotal={consumedCreditsTotal}` prop
  - 验证：预期产生 `AccountMetrics` 缺 `consumedCreditsTotal` prop 的 TS 错误，属任务 3 未完成的正常中间态（非本任务缺陷）；dashboard 自身派生逻辑与类型应无 TS 错误

- [x] **任务 3：AccountMetrics 三卡改造 + Ring 中心文案**
  - 文件：`admin-ui/src/components/account-metrics.tsx`、`admin-ui/src/components/metrics.tsx`（不改）、`admin-ui/src/components/dashboard.tsx`（同步清理 lowestAccount）
  - 改动：`AccountMetricsProps` 新增 `consumedCreditsTotal: number | null`，**移除 `lowestAccount` prop 字段**（第三卡不再消费，死代码清理）；第二卡「全局剩余积分」新增 `MetricAside` 包裹 `Ring`（percent=avg，tone=remainingTone(avg).stroke）+ 中心绝对定位 `<span>` 显示 `Math.round(avg)%`；第三卡 label 改 `metricConsumedLabel`，主值改 `consumedCreditsTotal`，移除原 `MetricAside` 环形图与 lowestAccount 脚注，脚注改 `metricCreditsCovered`（`queried/total 已查询`）；同步在 `dashboard.tsx` 移除 `lowestAccount` 派生与 `lowestAccount={...}` prop 下传（`avgRemainingPercent` 派生保留，供第二卡环形图复用）
  - 验证：`npx tsc --noEmit` 零错误；`npm run build` 成功

- [x] **任务 4：i18n 文案新增/调整**
  - 文件：`admin-ui/src/i18n/locales/zh.json`、`admin-ui/src/i18n/locales/en.json`
  - 改动：新增 `metricConsumedLabel`（zh「全局消费积分」/ en「Global credits consumed」）；`metricAvgLabel` / `metricAvgLowest` 保留但不再被引用（留作历史键，避免误删依赖）；不删除任何现有键
  - 验证：`npx tsc --noEmit` 零错误；中英文键对齐

- [x] **任务 5：前端构建验证**
  - Run：`cd admin-ui && npm run build`
  - Expected：构建成功，`dist` 更新（供后续 `cargo build --release` 嵌入，本变更不强制嵌入验证）

## 验收标准

- [ ] 账号行额度单元格展示「已用值/总额 + 已用% + 已用进度条」，配色按已用阈值（<40 ok / <70 warn / ≥70 danger）
- [ ] 指标条第二卡「全局剩余积分」右侧出现环形图，中心显示剩余平均百分比
- [ ] 指标条第三卡标题为「全局消费积分」，主值为已消费积分合计，脚注为「X/X 已查询」
- [ ] 第三卡不再展示环形图与「启用中最低」脚注
- [ ] `npx tsc --noEmit` 零错误，`npm run build` 成功
- [ ] 中英文文案对齐，无硬编码中文
- [ ] 排序逻辑、后端接口、user-ui 不受影响
