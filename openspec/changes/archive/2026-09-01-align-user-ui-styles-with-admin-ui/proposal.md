# 变更提案：align-user-ui-styles-with-admin-ui

## 背景

当前 `user-ui/` 与 `admin-ui/` 虽同属本项目前端，但视觉系统完全独立、风格不一致：

- **令牌系统**：`user-ui/src/index.css` 仅有 shadcn HSL 三元组令牌（浅色橙色 `--primary: 21 73% 42%` / 暗黑紫色 `--primary: 271 91% 65%`），缺少 admin-ui 的下半区设计稿原生令牌（surface/surface-2/surface-3/sidebar/hairline/hairline-2/ink/ink-2/ink-3/brand/brand-hover/brand-deep/brand-fg/brand-soft/brand-line/ok/warn/danger/track/code-bg/shadow-* 等）。`--radius` 也不同（0.75rem vs 0.6875rem）。
- **字体**：`user-ui/index.html` 依赖 Google Fonts CDN（Inter + JetBrains Mono），admin-ui 自托管 IBM Plex Sans + IBM Plex Mono 的 woff2 子集。CDN 方案需联网且与 admin-ui 字形不一致。
- **Tailwind 扩展**：`user-ui/tailwind.config.js` 缺 admin-ui 的 colors（surface/sidebar/hairline/ink/brand/ok/warn/danger/track）、boxShadow（hair/panel/pop）、fontFamily（IBM Plex）扩展。
- **UI 原语**：`user-ui/src/components/ui/` 的 button/card/input/badge/progress 仍是 shadcn 默认模板（h-10/rounded-md/text-sm + `bg-primary`/`text-muted-foreground`/`bg-secondary` 等旧令牌），admin-ui 已统一为 31px 高 / 7-11px 圆角 / 12.5px 字号 + 新令牌（`bg-surface`/`border-hairline`/`text-ink`/`bg-brand`/`bg-track` 等）。
- **页面结构**：`user-ui/src/components/` 下的 login-page.tsx（Card + KeyRound + textarea）、dashboard.tsx（朴素 `border-b` header + Card 网格 + `text-muted-foreground`）、usage-log-page.tsx（朴素表格 + 硬编码 `text-orange-600`/`text-blue-600`/`text-green-600`）与 admin-ui 的 panel/PANEL/PageHead/table-kit 结构差异显著。
- **sonner toast**：`user-ui/src/components/ui/sonner.tsx` 与 admin-ui 版本当前完全相同，但均用旧令牌类（`bg-background`/`text-muted-foreground`/`bg-primary`/`bg-muted`），与新令牌系统不一致。

用户要求将 user-ui 的页面样式改成与 admin-ui **一模一样**，暗黑和白天模式都要一致，且页面结构也尽量对齐 admin-ui dashboard 风格。

## 目标范围

**在范围内：**

- 复制 admin-ui 自托管字体（8 个 woff2 + `fonts.css`）到 `user-ui/public/`，并改 `user-ui/index.html` 移除 Google Fonts CDN、加 preload 与 `fonts.css` 引用（保留 `user-ui-theme` localStorage key）
- 用 admin-ui 完整令牌系统替换 `user-ui/src/index.css`（双套令牌 `:root`/`.dark` + 下半区令牌 + 滚动条 + `@layer utilities/components`）
- 用 admin-ui 版本替换 `user-ui/tailwind.config.js`（colors/boxShadow/fontFamily 扩展）
- 用 admin-ui 版本替换 5 个 UI 原语：button/card/input/badge/progress
- `user-ui/src/components/ui/sonner.tsx` 保持现状（当前与 admin-ui 版本完全一致，均用 shadcn 三元组令牌类 `bg-background`/`text-muted-foreground`/`bg-primary`/`bg-muted`，这些令牌在双轨制 index.css 上半区存在，暗黑/白天模式均生效，无需改动以达到一致）
- 重写 `user-ui/src/components/login-page.tsx` 对齐 admin-ui panel 布局（自定义 `.panel` + 品牌渐变徽标 + LABEL 小型大写字距 + Input h-[34px] + Button h-[34px] w-full），i18n 调用改为硬编码中文
- 引入 `page-head.tsx` 与 `table-kit.tsx` 到 user-ui（i18n 调用改硬编码中文；table-kit 的 DataCheckbox 因依赖 `@radix-ui/react-checkbox` 而 user-ui 未引入，改为不依赖 Radix 的简化实现或仅迁移 PANEL/TH_BASE/CELL/Pager 等常量与组件）
- 重写 `user-ui/src/components/dashboard.tsx` 对齐 admin-ui 页头风格（PageHead 或等价结构）+ 卡片结构（PANEL/Card 新令牌）
- 重写 `user-ui/src/components/usage-log-page.tsx` 对齐 table-kit 表格结构（PANEL/TH_BASE/CELL/Pager + 用 brand/ok/warn/danger 令牌色替代 orange/blue/green 硬编码）
- `use-theme.ts` 的 `STORAGE_KEY` 保持 `user-ui-theme`（与 index.html 一致，不改动主题切换逻辑本身，仅令牌与组件视觉对齐）
- 前端独立可构建验证（`cd user-ui && npm run build`）

**不在范围内：**

- 不改动 `admin-ui/` 任何文件（admin-ui 作为样式 source of truth，只读不改）
- 不改动后端 Rust 代码、路由、嵌入逻辑（`src/user_ui/` rust-embed 仅在构建后嵌入 dist，本次不动 Rust）
- 不引入 i18next 到 user-ui（保持 user-ui 无 i18n 现状，所有文案硬编码中文）
- 不新增 `@radix-ui/react-checkbox` 依赖（table-kit 的 DataCheckbox 简化为不依赖 Radix 的实现，或仅迁移需要的常量/组件）
- 不改动 user-ui 业务逻辑（API 调用、状态管理、cc-switch 导入、用量计算逻辑保持不变）
- 不改动 `brand-icons.tsx`（user-ui 与 admin-ui 的 ClaudeIcon/ChatGptIcon 已完全相同）
- 不改动 `user-ui/src/components/ui/sonner.tsx`（当前与 admin-ui 版本完全一致，用 shadcn 三元组令牌，双轨制令牌下暗黑/白天均生效）

## 技术方案

**字体复制**：直接 `cp admin-ui/public/fonts/* user-ui/public/fonts/` 与 `cp admin-ui/public/fonts.css user-ui/public/fonts.css`，`user-ui/index.html` 移除三行 Google Fonts 引用，加 `<link rel="preload">`（plexsans + plexmono woff2）与 `<link rel="stylesheet" href="/fonts.css">`。主题脚本保持 `user-ui-theme` key。

**令牌迁移**：`user-ui/src/index.css` 整体替换为 admin-ui 版本（含 `:root`/`.dark` 双套令牌、body font-family 改 IBM Plex Sans、滚动条用 `var(--hairline-2)`/`var(--ink-3)`、`@layer utilities .bg-grid-dot`、`@layer components` 白名单类）。注意 admin-ui index.css 中如有 admin 专属类（logstream/logrow/lv/plot-grid/tl-main/rel）一并复制——这些是惰性 CSS，user-ui 不用也不会有副作用，但保证两份 index.css 完全一致以达"一模一样"。

**Tailwind 扩展迁移**：`user-ui/tailwind.config.js` 整体替换为 admin-ui 版本（colors surface/sidebar/hairline/ink/brand/ok/warn/danger/track/code-bg + space/neon + boxShadow hair/panel/pop + fontFamily IBM Plex）。注意 admin-ui 的注释明确"整值 var() 引用无法应用透明度修饰符"，需半透明时用 -soft/-line 变体——迁移时一并保留此注释。

**UI 原语迁移**：直接用 admin-ui 版本覆盖 user-ui 对应文件：
- `button.tsx`：31px 高 / 7px 圆角 / 12.5px 字号，变体 default(brand)/destructive(danger)/outline/secondary/ghost/link，svg 基类 `[&_svg]:size-[15px] [&_svg]:text-ink-3`
- `card.tsx`：`rounded-lg border border-hairline bg-surface text-ink shadow-panel`
- `input.tsx`：FIELD_BOX + FIELD_CONTROL，支持 unit prop
- `badge.tsx`：20px 高 / 6px 圆角 / 11px 字号，变体 default(brand-soft)/secondary/destructive/outline/success(ok-soft)/warning(warn-soft)
- `progress.tsx`：`h-1 bg-track`，填充色三档 >80 danger / >60 warn / 否则 ok

`sonner.tsx` 不改动：当前 user-ui 与 admin-ui 版本逐字节一致，均用 shadcn 三元组令牌（`bg-background`/`text-muted-foreground`/`bg-primary`/`bg-muted`），这些令牌在双轨制 index.css 上半区（`:root`/`.dark` 的 `--background`/`--foreground`/`--primary`/`--muted` 等）已定义，暗黑/白天模式均生效，无需改动即一致。

**页面结构对齐**：
- `login-page.tsx`：复用 admin-ui 的 panel 布局（352px/11px 圆角/surface/hairline/shadow-pop）+ 品牌渐变徽标 + LABEL + Input h-[34px] + Button h-[34px] w-full。admin-ui 用 `t('login.description')`/`t('settings.adminPassword')`/`t('login.loginButton')`/`t('dashboard.consoleSubtitle')`，user-ui 改硬编码中文。登录标题改为「额度用量监控」（保留 user-ui 原标题语义），副标题硬编码。
- 引入 `page-head.tsx`：PageHead 组件（面包屑 + 标题 + note + actions + onBack），admin-ui 用 `t('common.back')`，user-ui 改硬编码「返回」。
- 引入 `table-kit.tsx`：PANEL/PANEL_TITLE/PANEL_FOOT/TH_BASE/CELL/ICON_BTN/FOOT_BTN/PAGER_BTN 常量 + pageWindow + Pager 组件。admin-ui 的 DataCheckbox 依赖 `@radix-ui/react-checkbox`，user-ui 未引入该依赖——**决策：仅迁移常量与 Pager（不含 DataCheckbox）**，因 usage-log-page 当前无复选框需求，避免新增 Radix 依赖违反"不新增外部依赖"约束。Pager 的 `t('common.prevPage')`/`t('common.nextPage')` 改硬编码「上一页」/「下一页」。
- `dashboard.tsx`：header 改用 PageHead 风格（或等价结构，含标题 + 状态 Badge + actions 区），卡片用 Card/PANEL 新令牌，用量概览 4 卡、按模型分组卡、cc-switch 导入卡、Key 信息卡全部用新令牌（`text-ink-3` 替代 `text-muted-foreground`、`bg-surface` 替代 `bg-background`/`bg-card`）。
- `usage-log-page.tsx`：表格用 table-kit 的 PANEL/TH_BASE/CELL 结构，分页用 Pager，MODEL_COLORS 的 orange/blue/green 硬编码改为 brand/ok/warn/danger 令牌色或 Badge variant，汇总卡用 PANEL 结构。

**i18n 处理**：user-ui 无 i18n，所有 admin-ui 中的 `t('...')` 调用在迁移到 user-ui 时改为硬编码中文文案。需映射的 key：
- `t('login.description')` → 「请输入您的 API Key 或粘贴发货信息查看用量数据」
- `t('settings.adminPassword')` → 「API Key」（user-ui 是 API Key 登录非管理密码，语义调整）
- `t('login.loginButton')` → 「查看用量」
- `t('dashboard.consoleSubtitle')` → 「额度用量监控控制台」
- `t('common.back')` → 「返回」
- `t('common.prevPage')` → 「上一页」
- `t('common.nextPage')` → 「下一页」

## 预期影响

- **视觉一致性**：user-ui 暗黑/白天模式配色、字体、圆角、组件原语与 admin-ui 完全一致，用户在两端点间切换无视觉割裂。
- **离线可用性**：user-ui 移除 Google Fonts CDN 依赖后，离线/内网环境字体渲染一致（IBM Plex 自托管）。
- **无功能影响**：API 调用、状态管理、用量计算、cc-switch 导入、登录逻辑均不变，仅视觉层与组件结构层调整。
- **构建影响**：user-ui 独立可构建，`npm run build` 产物体积因嵌入字体 woff2 略增（约 200-400KB，与 admin-ui 持平），rust-embed 嵌入逻辑无需改动。
- **兼容性**：`user-ui-theme` localStorage key 不变，已登录用户的主题偏好保留。

## 风险

| 风险 | 影响 | 应对 |
|------|------|------|
| 令牌整体替换后，user-ui 现有页面中引用旧令牌类（`bg-background`/`text-muted-foreground`/`bg-card`/`bg-secondary`/`border-input` 等）的代码失效 | 页面样式错乱 | 所有页面重写时统一改用新令牌类，逐文件审查；dashboard/usage-log-page/login-page 均在范围内重写 |
| table-kit 的 DataCheckbox 简化或舍弃后，若未来 user-ui 需复选框需补 Radix 依赖 | 未来扩展受限 | 当前无复选框需求，舍弃 DataCheckbox 不影响；未来如需再评估引入依赖 |
| admin-ui index.css 含 admin 专属 `@layer components` 类（logstream 等）复制到 user-ui 后产生惰性 CSS | 轻微 CSS 冗余 | 为达"一模一样"接受此冗余；这些类未被 user-ui 引用，无副作用 |
| 字体 woff2 文件复制后 git 仓库体积增加 | 仓库体积 | 与 admin-ui 持平，可接受；woff2 已是子集压缩格式 |
| i18n 文案硬编码后，未来若 user-ui 需多语言需重做 | 国际化扩展受限 | 当前 user-ui 无 i18n 需求，硬编码符合现状；未来如需多语言再统一引入 i18next |