# 变更提案：admin-ui-sidebar-layout

## 背景
当前 admin-ui 使用顶部 sticky header + tab 按钮切换导航（凭据管理/API Keys/设置）。
windsurf 仓库的 admin UI 采用固定左侧 sidebar（232px）+ nav-group 分组导航的布局，
视觉层次更清晰，可扩展性更强。目标是将 kiro2cc-proxy 的 admin-ui 布局改为与 windsurf 一致的 sidebar 风格。

## 目标范围
**在范围内：**
- 将顶部 header + tab 按钮导航替换为固定左侧 sidebar
- Sidebar 包含：品牌区（Kiro 图标 + 标题）、分组导航链接、底部操作区（刷新 + 登出）
- Main 内容区改为 `ml-[232px]` 布局，padding 与 windsurf 对齐（`p-7 md:p-9`）
- 导航项与现有 tab 对应：凭据管理 / API Keys / 每日统计 / 设置
- 活动项左侧 accent 竖线指示（`box-shadow: inset 2px 0 0`）
- Sidebar hover/active 状态与 windsurf 一致
- Page header（标题 + 副标题）替代原来的内联 h2

**不在范围内：**
- 业务逻辑变更（状态管理、API 调用等不动）
- 移动端 sidebar collapse（windsurf 原版也无此功能）
- index.css CSS 变量调整（现有 dark theme 与 windsurf 已基本一致）
- login-page.tsx 样式变更

## 技术方案
- 仅修改 `admin-ui/src/components/dashboard.tsx`
- 使用 Tailwind CSS 类实现 windsurf sidebar CSS 的对应样式（无需新增 CSS 文件）
- 保留所有现有 Tailwind 变量（`bg-card`, `border-border`, `text-muted-foreground` 等）
- 新增 Lucide 图标：`BarChart2`（每日统计）；`Server`/`Key`/`Settings` 已在用

## 预期影响
- UI 布局重构，无任何功能或 API 变化
- 视觉上 sidebar 固定左侧，main 内容右移 232px
- 所有子页面（ApiKeyDetailPage、CredentialDetailPage、DailyStatsPage 等）布局不变，
  仅外层容器变化

## 风险
- 小屏幕（< 900px）sidebar 会压缩内容宽度：与 windsurf 原版行为一致，可接受
- 无其他风险
