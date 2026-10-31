## ADDED Requirements

### Requirement: 主题文字对比度达标
The system SHALL 保证管理后台每一个色彩主题的浅色与深色变体中，文字类令牌相对其实际所在背景的对比度不低于 WCAG AA 的 4.5:1。

覆盖的“文字 × 背景”组合：

- 正文与次要文字（`foreground`、`ink`、`ink-2`、`ink-3`、`muted-foreground`）对页面背景、卡片、`surface-2`、`surface-3`、侧边栏、`muted` 表面。
- 品牌文字色（`brand-ink`）对 `surface`、侧边栏、`brand-soft` 叠加后的表面。
- 状态文字色（`ok`、`warn`、`danger`）对 `surface` 及各自的 `-soft` 叠加后的表面。
- 填充上的文字：`brand-fg` 对 `brand` 与 `brand-hover`，`primary-foreground` 对 `primary`，`destructive-foreground` 对 `destructive`。
- 用量进度四档徽章文字对各自徽章底色；NEW 徽章文字 `badge-new-fg` 对其渐变两端。

半透明背景（如 `-soft`、徽章底色）分别叠加在 `surface` 与 `surface-2` 上，取对比度较低的合成结果计算。

#### Scenario: 全部主题变体通过对比度审计
- **GIVEN** `index.css` 基准令牌与 `themes.css` 中全部 36 个主题的浅色、深色变体
- **WHEN** 运行 `web-ui/admin-ui/scripts/theme-contrast.py --check`
- **THEN** 上述每组文字 × 背景的对比度均不低于 4.5
- **AND** 脚本退出码为 0，输出失败项数量为 0

#### Scenario: 切换到任意主题后次要文字清晰可读
- **GIVEN** 用户在「主题设置」页选择了任意一个色彩主题与任意明暗模式
- **WHEN** 打开概览、凭据、使用日志、模型、文档、主题设置等页面
- **THEN** 使用 `text-ink-2`、`text-ink-3`、`text-muted-foreground` 的说明文字、表头和备注，其对比度均不低于 4.5:1
- **AND** 状态徽标（`ok` / `warn` / `danger`）与用量进度徽章的文字对其底色不低于 4.5:1

#### Scenario: 品牌文字与品牌填充分离
- **GIVEN** 某主题的 `--brand` 取值较亮（例如琥珀、樱花、薄荷类主题的浅色变体）
- **WHEN** 页面渲染 `text-brand`（如选中态标签、链接式文字）与 `bg-brand`（如主按钮）
- **THEN** `text-brand` 使用 `--brand-ink`，对其背景不低于 4.5:1
- **AND** `bg-brand`、`border-brand`、`outline-brand` 仍使用 `--brand`，主题的品牌填充色与变更前一致

#### Scenario: 填充按钮上的文字可读
- **GIVEN** 任意主题的主按钮（`bg-brand` + `text-brand-fg`）
- **WHEN** 按钮处于默认或悬停状态
- **THEN** `brand-fg` 对 `brand` 与 `brand-hover` 的对比度均不低于 4.5:1

#### Scenario: 主题视觉身份保持不变
- **GIVEN** 修正前后的同一主题
- **WHEN** 比较 `--background`、`--card`、`--surface*`、`--sidebar`、`--brand` 的取值
- **THEN** 这些背景与品牌填充令牌的取值与变更前逐值一致
- **AND** 仅文字侧令牌（`ink-*`、`muted-foreground`、`ok/warn/danger`、`foreground`、`*-foreground`、`brand-fg`、`quota-*-badge-text`、`badge-new-fg`）与新增的 `--brand-ink` 发生变化；例外有两类：(1) `brand-fg` 无法同时满足 `brand` 与 `brand-hover` 的浅色变体，其 `--brand-hover` 允许沿明度微调（`--brand` 本身不变）；(2) 近黑或纯黑文字都压不住 NEW 徽章渐变较暗一端的深色变体（catppuccin、dracula、stripe），其 `--badge-new-to` 允许沿明度向更亮方向微调（`--badge-new-from` 不变）

### Requirement: 主题预览区文字可读
The system SHALL 保证「主题设置」页实时预览（工作台与基础组件两个场景）中，每一处文字相对其所在底色的对比度不低于 4.5:1，且不修改各主题预览色板（`theme-definitions.ts`）的取值。

#### Scenario: 任意主题与明暗模式下预览文字清晰
- **GIVEN** 用户在「主题设置」页选中 36 个主题中的任意一个，并处于浅色或深色模式
- **WHEN** 查看实时预览的工作台场景与基础组件场景
- **THEN** 品牌名、指标数值、侧栏项、按钮标签、徽标和次要说明文字对其所在底色（预览背景、卡片面、侧栏、主色填充）均不低于 4.5:1

#### Scenario: 预览仍体现所选主题的视觉身份
- **GIVEN** 修正前后的同一主题预览
- **WHEN** 比较预览的背景、卡片面、侧栏与主色填充
- **THEN** 这些底色与变更前一致
- **AND** 色板色本身已达标的文字保持原色，不被替换为近黑或近白
