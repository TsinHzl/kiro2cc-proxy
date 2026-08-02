# 任务清单:add-light-theme

## 状态:ARCHIVED

## 任务

- [x] 1. admin-ui:新增 `src/hooks/use-theme.ts`,实现 `theme`/`toggleTheme`,读写 `localStorage['admin-ui-theme']`(缺省 `'dark'`),同步 `document.documentElement` 的 `dark` class(spec.md 引用:主题状态管理)
- [x] 2. admin-ui:在 `index.html` `<head>` 内联防闪烁脚本,挂载前依据 `localStorage['admin-ui-theme']` 提前设置 `<html>` class(spec.md 引用:防闪烁加载)
- [x] 3. admin-ui:重写 `src/index.css` —— `:root` 替换为参考页面暖色系 HSL 变量(含已下调的 `--muted-foreground: 37 17% 38%`),`.dark` 保留现有暗色数值不变(spec.md 引用:CSS 变量重构)
- [x] 4. admin-ui:在 `dashboard.tsx` 侧边栏图标按钮区新增 Sun/Moon 主题切换按钮,接入 `use-theme`(spec.md 引用:切换入口)
- [x] 5. admin-ui:审计并按需调整 `credential-card.tsx`、`ui/progress.tsx`、`ui/badge.tsx`、`lib/utils.ts` 中的 `neon-*`/`space-*` 硬编码类,确保白天模式下可读协调(spec.md 引用:硬编码色板适配)
- [x] 6. user-ui:新增 `src/hooks/use-theme.ts`,实现 `theme`/`toggleTheme`,读写 `localStorage['user-ui-theme']`(缺省 `'dark'`),同步 `document.documentElement` 的 `dark` class(spec.md 引用:主题状态管理)
- [x] 7. user-ui:在 `index.html` `<head>` 内联防闪烁脚本,挂载前依据 `localStorage['user-ui-theme']` 提前设置 `<html>` class(spec.md 引用:防闪烁加载)
- [x] 8. user-ui:重写 `src/index.css` —— `:root` 替换为参考页面暖色系 HSL 变量(与 admin-ui 同一套映射表),`.dark` 保留现有暗色数值不变(spec.md 引用:CSS 变量重构)
- [x] 9. user-ui:在 `dashboard.tsx` 顶部 header 按钮区新增 Sun/Moon 主题切换按钮,接入 `use-theme`(spec.md 引用:切换入口)
- [x] 10. user-ui:审计并按需调整 `ui/progress.tsx`、`ui/badge.tsx` 中的 `neon-*`/`space-*` 硬编码类,确保白天模式下可读协调(spec.md 引用:硬编码色板适配)
- [x] 11. 浏览器验证 admin-ui:切换按钮、刷新后 `admin-ui-theme` 持久化、首次访问默认暗色、无 FOUC 闪烁
- [x] 12. 浏览器验证 user-ui:切换按钮、刷新后 `user-ui-theme` 持久化、首次访问默认暗色、无 FOUC 闪烁;并验证与 admin-ui 分别设置不同主题时互不覆盖(spec.md 引用:两 App 主题互不干扰)
- [x] 13. 对比度复核:使用浏览器开发者工具实测白天模式下 `--muted-foreground`、`--foreground` 等关键文字/背景组合的对比度,确认达到 WCAG AA 正文标准(4.5:1),若仍不达标需进一步下调对应变量 L 值并复测

## 验收标准

- [x] admin-ui、user-ui 均可通过按钮在亮/暗两种主题间手动切换
- [x] 刷新页面后主题选择保持不变(各自 localStorage key 持久化生效)
- [x] admin-ui 与 user-ui 分别设置不同主题时互不覆盖(验证 `admin-ui-theme`/`user-ui-theme` 两个 key 相互独立)
- [x] 首次访问(无 localStorage 记录)默认仍为暗色,与现状一致
- [x] 页面加载/刷新过程中无可见的主题闪烁(FOUC)
- [x] 白天模式 `:root` 各 CSS 变量取值与 proposal.md 色值映射表一致(±5% 微调范围内;`--primary`/`--ring` 因对比度复核由 45% 下调至 42%)
- [x] 白天模式下 `--foreground`/`--muted-foreground` 与其所在背景色的对比度均 ≥ 4.5:1(WCAG AA 正文标准;实测 `--primary-foreground`/`--primary` 4.83:1,侧边栏/明细页 `muted-foreground` 文字 5.32:1)
- [x] 暗色模式视觉效果与变更前完全一致,无回归(所有修复均只新增/调整 light 基础类,`dark:` 变体数值未改动)
- [x] 涉及硬编码 `neon-*`/`space-*` 色板的 6 个文件在白天模式下逐一浏览器实测,人工确认无刺眼/不可读的配色冲突(含本轮 CR 修复的 `credential-card.tsx` degraded/disabled 状态)
