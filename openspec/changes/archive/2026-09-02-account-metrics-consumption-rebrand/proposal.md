# 变更提案：account-metrics-consumption-rebrand

## 背景

账号管理页面当前的额度展示口径与命名存在三处与用户心智不一致的问题：

1. **账号行单元格**展示的是「剩余额度」（`remaining / usageLimit` + 剩余百分比 + 剩余进度条），但用户关注的核心指标是「消费额度」（已用多少 / 占比多少），剩余口径让用户需要心算才能得出消费量。
2. **指标条第三卡**「启用账号平均剩余」同时承载了平均剩余百分比主值、最低账号脚注、右侧环形图三段信息，信息密度过高且与第二卡「全局剩余积分」割裂——环形图本应直观表达「全局剩余比例」，却被放在了「平均剩余」卡里。
3. **指标条第二卡**「全局剩余积分」只有积分合计数字 + `{{queried}}/{{total}} 已查询` 脚注，缺少一眼可读的比例可视化（环形图）。

## 目标范围

**在范围内：**
- 账号行（`account-row.tsx`）额度单元格：剩余口径 → 消费口径（已用值 + 已用百分比 + 已用进度条），配色按已用阈值反转（越高越警示）
- 指标条第三卡（`account-metrics.tsx`）：改名为「全局消费积分」，主值改为「已消费积分合计」（`Σ(usageLimit − remaining)`），脚注改为 `{{queried}}/{{total}} 已查询`，移除环形图与 lowestAccount 脚注
- 指标条第二卡「全局剩余积分」：新增右侧环形图（`MetricAside`），环形图中心叠加剩余平均百分比文案
- i18n（`zh.json` / `en.json`）：新增/调整对应文案键

**不在范围内：**
- 后端接口、数据模型、协议层（`balance.remaining` / `usageLimit` / `usagePercentage` 字段语义不变）
- 账号行其它列（调用 / RPM / 最后调用 / 操作）
- 排序逻辑（`remainingSnapshot` 仍按 `remaining` 排序，不改口径——排序是「剩余额度」语义，与展示口径解耦）
- 余额弹窗（`BalanceDialog`）内的展示
- user-ui 的任何改动

## 技术方案

### 数据来源（全部复用现有接口，无新增请求）

| 指标 | 来源 | 计算 |
|------|------|------|
| 账号行已用值 | `balance.usageLimit − balance.remaining` | 直接相减，`usageLimit <= 0` 时降级 |
| 账号行已用百分比 | `balance.usagePercentage` | 后端已返回，直接用（省去前端 `100 − remainingPct` 心算，且与后端口径一致） |
| 第三卡已消费积分合计 | `balanceMap` 遍历 | `Σ(usageLimit − remaining)`，仅对 `usageLimit > 0` 的账号累加 |
| 第二卡环形图百分比 | `avgRemainingPercent`（现有派生） | 复用 dashboard 已计算的启用账号平均剩余百分比 |
| 第三卡 `queried/total` | `liveCreditsQueried` / `data.total` | 复用现有 state |

### 配色阈值反转（账号行）

现有 `quotaTone(remainingPct)` 按 60/30 分界（剩余越高越绿）。消费口径下「已用越高越警示」，需新增 `usageTone(usedPct)`：

- `usedPct < 40`（对应剩余 > 60）→ `text-ok` / `bg-ok`
- `usedPct < 70`（对应剩余 > 30）→ `text-warn` / `bg-warn`
- `usedPct >= 70` → `text-danger` / `bg-danger`

阈值与现有 60/30 剩余阈值在数学上等价（`usedPct = 100 − remainingPct`），保证视觉语义连续。

### 环形图中心文案

`metrics.tsx` 的 `Ring` 组件当前是纯装饰 SVG（`aria-hidden`），中心无文本。第二卡需要在环形图中心叠加百分比文案。方案：**不修改 `Ring` 原语**（它被多处复用，加文本会污染），在 `account-metrics.tsx` 第二卡内用一个 `relative` 容器包裹 `Ring`，叠加一个绝对定位的 `<span>` 显示百分比。这样 `Ring` 保持纯装饰语义，文本由调用方注入。

### 已消费积分合计的实时性

`liveCreditsTotal` 现有逻辑在批量查询时累加 `remaining`。第三卡需要的是 `Σ(usageLimit − remaining)`。方案：**不新增第三个 live state**，改为在 `account-metrics.tsx` 内基于 `balanceMap` 派生——但 `account-metrics.tsx` 当前只接收聚合后的 `creditsTotal`（remaining 合计），不接收 `balanceMap`。

两个选项：
- **选项 A**：在 `dashboard.tsx` 新增派生 `consumedCreditsTotal`（`Σ(usageLimit − remaining)`），作为新 prop 传入 `AccountMetrics`，复用现有 `balanceMap` effect 的计算时机
- **选项 B**：把 `balanceMap` 直接传入 `AccountMetrics`，由组件内部派生

选 **选项 A**：保持 `AccountMetrics` 为纯展示组件（符合现有架构——它只接收聚合标量，不持有原始 map），派生逻辑留在 `dashboard.tsx` 与现有 `liveCreditsTotal` 派生并列。

## 预期影响

- **账号行**：额度列数字语义从「剩余/总额」变为「已用/总额」，百分比从「剩余%」变为「已用%」，进度条从「剩余占比」变为「已用占比」。用户视觉上会看到进度条更满（因为已用通常 > 剩余的对偶情况），需在 CR 确认无误导。
- **指标条**：第二卡多一个 42px 环形图（右侧 `MetricAside`），第三卡主值数字从「平均剩余%」变为「已消费积分合计」（绝对值，单位与 `creditsTotal` 一致），第三卡脚注从「启用中最低 X · Y%」变为「X/X 已查询」。
- **排序**：表头「剩余额度」排序键语义不变（仍按 `remaining` 排），但展示口径已改为已用——排序与展示口径解耦，需在 CR 确认是否需要同步改排序键名/语义。**本变更不改排序**，保持 surgical。
- **性能**：无新增请求、无新增 effect 依赖，`consumedCreditsTotal` 与 `liveCreditsTotal` 同源派生，零额外开销。

## 风险

| 风险 | 应对 |
|------|------|
| 账号行配色反转后，原本「绿色满进度条=健康」变为「绿色短进度条=健康」，用户短期可能误读 | 配色阈值数学等价，且已用进度条短=健康符合直觉（消费少=好）；CR 重点验证 |
| 第三卡移除 `lowestAccount` 后，「最低账号」信息丢失 | 用户明确要求移除（改为「已查询」脚注）；如需保留可后续单独提案，本变更不保留 |
| `Ring` 中心文案与环形图尺寸对齐（静态 + 响应式） | 42px 环形图中心用 `absolute inset-0 grid place-items-center`，字号 10px `tabular-nums`；`MetricCard` 宽度变化时中心文案随容器居中（grid place-items-center 自适应），CR 在多视口宽度下验证无溢出/错位 |
| i18n 历史键保留触发 lint 告警 | 任务 4 验证步骤显式运行 `npm run build`（含 lint）；若项目 i18n 未使用键检测报错，改为删除 `metricAvgLabel` / `metricAvgLowest`（确认无其他引用后）；本变更倾向保留但以构建通过为准 |
| 排序键名仍叫「剩余额度」但展示已用 | 本变更不改排序，surgical 原则；如需改排序键名单开提案 |
