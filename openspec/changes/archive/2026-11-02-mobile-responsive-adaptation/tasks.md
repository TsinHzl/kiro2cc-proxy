# 任务清单：mobile-responsive-adaptation

## 状态：COMPLETED

## 任务
- [x] 任务 1：适配 `admin-ui` 主框架与移动端抽屉式侧边栏（修改 `web-ui/admin-ui/src/components/dashboard.tsx` 与 `web-ui/admin-ui/src/components/dashboard/sidebar.tsx`，增加 `md:hidden` 移动端顶栏、抽屉开关状态与遮罩层，主内容区在 `max-md:` 下取消左边距并收敛内边距；通过 `pnpm --dir web-ui/admin-ui build` 验证编译通过）
- [x] 任务 2：适配 `admin-ui` 共享工具栏原语与全局移动端响应式样式（修改 `web-ui/admin-ui/src/components/toolbar.tsx`、`web-ui/admin-ui/src/components/docs-page.tsx`、`web-ui/admin-ui/src/components/changelog-page.tsx` 与 `web-ui/admin-ui/src/index.css`，在 `@media (max-width: 767px)` 下统一处理 `MetricsBar` 双列折叠、表格横向滚动、设置行自适应换行与实时日志列宽收缩；通过 `pnpm --dir web-ui/admin-ui build` 验证编译通过）
- [x] 任务 3：适配 `user-ui` 仪表盘与使用日志页手机端布局（修改 `web-ui/user-ui/src/components/dashboard.tsx`、`web-ui/user-ui/src/components/usage-log-page.tsx` 与 `web-ui/user-ui/src/index.css`，优化窄屏容器边距、时间信息单列堆叠、指标条双列折叠及日志表横向滚动；通过 `npm --prefix web-ui/user-ui run build` 验证编译通过）

## 验收标准
- [x] `admin-ui` 在手机尺寸（如 375×812）下主内容区全宽显示，可通过顶部菜单按钮展开/收起侧边栏抽屉并切换所有页面
- [x] `admin-ui` 与 `user-ui` 的指标卡在手机尺寸下自动折行展示、无文字重叠，所有数据表格可左右平滑滚动查看完整列
- [x] `admin-ui` 系统设置页、实时日志页、使用文档页及弹窗在手机尺寸下无横向撑破屏幕现象
- [x] 网页端（`>=1024px`）所有页面的布局、样式与交互保持零变化
- [x] `web-ui/admin-ui` 与 `web-ui/user-ui` 生产构建（`pnpm build` / `npm run build`）零错误通过
