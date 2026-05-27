> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# 任务清单：theme-antigravity

## 状态：ARCHIVED

## 任务

- [x] 任务 1：admin-ui/index.html — 添加 `class="dark"` 到 `<html>`，引入 Google Fonts（Inter + JetBrains Mono）
- [x] 任务 2：admin-ui/src/index.css — 全量替换 CSS 变量为深空/霓虹配色，新增自定义深色滚动条
- [x] 任务 3：admin-ui/tailwind.config.js — 新增 `space` / `neon` 调色盘 + `fontFamily` 配置
- [x] 任务 4：admin-ui/src/components/ui/badge.tsx — `success` / `warning` variant 改为霓虹半透明风格
- [x] 任务 5：admin-ui/src/components/ui/progress.tsx — 进度条颜色改为霓虹色系
- [x] 任务 6：admin-ui/src/components/credential-card.tsx — HEALTH_CONFIG 状态色改为霓虹风格
- [x] 任务 7：user-ui/index.html — 同任务 1
- [x] 任务 8：user-ui/src/index.css — 全量替换（不含 popover 变量）
- [x] 任务 9：user-ui/tailwind.config.js — 同任务 3
- [x] 任务 10：user-ui/src/components/ui/badge.tsx — 同任务 4
- [x] 任务 11：user-ui/src/components/ui/progress.tsx — 同任务 5
- [x] 任务 12：admin-ui 构建验证（`npm run build`）
- [x] 任务 13：user-ui 构建验证（`npm run build`）

## 验收标准

- [ ] `admin-ui` 和 `user-ui` 页面背景为深黑色（#09090b），主色为霓虹紫（#a855f7）
- [ ] Badge 的 success/warning variant 显示霓虹绿/黄半透明风格，无白色背景
- [ ] Progress 组件颜色随进度值变化（绿→黄→红）显示霓虹色
- [ ] credential-card 健康状态徽章显示霓虹色系
- [ ] `npm run build` 对两个子项目均无报错
- [ ] 字体加载后正文使用 Inter，代码/数字区域使用 JetBrains Mono
