## Purpose

展示账号池的额度消费态势：账号行直接呈现「已用多少」，指标条用环形图与消费积分合计让管理员一眼判断全局消耗进度。

## Requirements

### Requirement: 账号行额度单元格按消费口径展示

The system SHALL render each account row's quota cell in consumption terms: the used value (`usageLimit − remaining`), the used percentage (`usagePercentage`), and a progress bar whose width and color reflect the used ratio.

#### Scenario: 已查询余额的账号
- **GIVEN** 某账号已查到余额且 `usageLimit > 0`
- **WHEN** 渲染该账号行额度单元格
- **THEN** 左侧展示 `<已用值>/<usageLimit>`（已用值加粗）
- **AND** 右侧展示 `usagePercentage%`（已用百分比）
- **AND** 下方进度条宽度为 `usagePercentage%`，配色按已用阈值（`≤40` ok / `≤70` warn / `>70` danger）

#### Scenario: 未查询或加载中
- **GIVEN** 某账号余额未查询或正在查询
- **WHEN** 渲染该账号行额度单元格
- **THEN** 展示「尚未查询」或「查询中…」文案 + `—` + 空轨道（加载中轨道脉冲），与改版前一致

### Requirement: 全局剩余积分卡片含剩余比例环形图

The system SHALL display a ring chart in the "全局剩余积分" (total credits left) metric card, with the enabled-accounts average remaining percentage rendered at the ring's center.

#### Scenario: 有启用账号已查询余额
- **GIVEN** `avgRemainingPercent` 非 null
- **WHEN** 渲染全局剩余积分卡片
- **THEN** 卡片右侧 `MetricAside` 出现 42px 环形图，`percent = avgRemainingPercent`，`tone` 按 60/30 剩余阈值
- **AND** 环形图中心叠加 `Math.round(avg)%` 文案

#### Scenario: 无启用账号已查询余额
- **GIVEN** `avgRemainingPercent` 为 null
- **WHEN** 渲染全局剩余积分卡片
- **THEN** 不渲染环形图与中心文案（`MetricAside` 省略环形图，主值仍展示积分合计）

### Requirement: 全局消费积分卡片展示已消费积分合计

The system SHALL render the third metric card (formerly "启用账号平均剩余") as "全局消费积分", whose main value is the sum of consumed credits (`Σ(usageLimit − remaining)` over accounts with `usageLimit > 0`) and whose footnote is the `queried/total 已查询` coverage.

#### Scenario: 有已查询账号
- **GIVEN** `consumedCreditsTotal` 非 null
- **WHEN** 渲染全局消费积分卡片
- **THEN** 主值展示 `consumedCreditsTotal.toFixed(1)`
- **AND** 脚注展示 `{{queried}}/{{total}} 已查询`
- **AND** 不展示环形图与「启用中最低」脚注

#### Scenario: 无已查询账号
- **GIVEN** `consumedCreditsTotal` 为 null
- **WHEN** 渲染全局消费积分卡片
- **THEN** 主值展示 `—`，脚注展示「尚未查询 —」

### Requirement: 配色阈值数学等价

The system SHALL keep the consumption color thresholds mathematically equivalent to the existing remaining thresholds: used `≤40` corresponds to remaining `≥60` (ok), used `≤70` corresponds to remaining `≥30` (warn), used `>70` corresponds to remaining `<30` (danger).

#### Scenario: 剩余 60% 的账号
- **GIVEN** 某账号剩余百分比 60%（已用 40%）
- **WHEN** 渲染账号行额度单元格
- **THEN** 已用百分比 40% 染 ok 色（因 40 ≤ 40），与改版前剩余 60% 染 ok 色视觉一致

### Requirement: i18n 文案键策略

The system SHALL add a `metricConsumedLabel` key to both `zh.json` ("全局消费积分") and `en.json` ("Global credits consumed"), and SHALL NOT remove the existing `metricAvgLabel` / `metricAvgLowest` keys (kept as historical keys, no longer referenced by the third card).

#### Scenario: 新增消费积分标签键
- **GIVEN** i18n locale 文件（zh / en）
- **WHEN** 本次变更落地
- **THEN** `metricConsumedLabel` 键存在且值为「全局消费积分」(zh) / "Global credits consumed" (en)
- **AND** `metricAvgLabel` / `metricAvgLowest` 键仍存在（不被删除）
