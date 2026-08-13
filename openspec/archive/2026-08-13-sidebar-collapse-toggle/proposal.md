# 变更提案：sidebar-collapse-toggle

## 背景
当前 Admin 后台侧边栏（`admin-ui/src/components/dashboard.tsx` 中的 `<aside>`）固定宽度 232px，无法收起。用户希望在需要更大内容区域时可以折叠侧边栏，收起后仅保留图标按钮。

## 目标范围

**在范围内：**
- 侧边栏新增展开/收起状态切换（图标按钮触发）
- 收起状态下：仅展示各导航项对应的图标按钮（隐藏文字 label、分组标题、Logo 文字、footer 文字），点击图标按钮功能不变（切换页面）
- 展开/收起状态在浏览器本地持久化（刷新后保持）
- 主内容区随侧边栏宽度联动调整左边距

**不在范围内：**
- 不新增新的导航项/页面
- 不新增移动端专属响应式断点行为（当前无此布局，本次不新增）
- 不改变除侧边栏外的其它布局
- 不拆分为独立 `Sidebar` 组件（保持在 `dashboard.tsx` 内的最小改动）

## 技术方案
- 新增 `sidebarCollapsed` state（`useState<boolean>`），初始值通过 `readStoredSidebarCollapsed()` 函数从 `localStorage`（key: `sidebar-collapsed`）读取，切换时同步 `localStorage.setItem`——沿用 `admin-ui/src/hooks/use-theme.ts` 现有的 `readStoredTheme()` + 显式写回持久化模式，保持风格一致
- 侧边栏宽度：展开 `232px`（不变）/ 收起 `64px`（仅保留图标居中）
- 收起态下 nav 按钮的文字 label 通过条件渲染隐藏，图标居中；同时设置 `title` **与** `aria-label` 属性（说明用途一致），前者提供原生 tooltip，后者保证屏幕阅读器可读性
- header 区域：收起态隐藏 Logo 文案/副标题、Logout 按钮仅保留图标；新增折叠切换按钮，需从 `lucide-react` **新增导入** `PanelLeftClose`/`PanelLeftOpen`（当前 `dashboard.tsx` import 列表中尚未包含这两个图标）
- footer 区域：收起态隐藏 GitHub 文字与版本号、隐藏版本号文本，**保留 GitHub 图标（点击跳转行为不变）与主题切换图标**，与目标范围"仅隐藏文字 label"的原则保持一致
- 主内容 `<main>` 的 `ml-[232px]` 随折叠状态切换为 `ml-16`
- 使用 Tailwind `transition-all` 保证宽度切换动画流畅

## 预期影响
- 仅影响 `admin-ui/src/components/dashboard.tsx`，纯前端展示层变更，无后端/接口影响
- 不影响现有导航逻辑（各 `onClick` handler 不变），仅影响视觉展示

## 风险
- Tooltip 遮挡/延迟：收起态依赖浏览器原生 `title` tooltip 辅助显示名称，同时补充 `aria-label` 覆盖可访问性缺口，不引入额外组件库
- 布局抖动：宽度切换需保证 `<main>` margin 与 `<aside>` 宽度同步变化，否则出现内容错位
