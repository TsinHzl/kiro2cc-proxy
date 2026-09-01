# 任务清单：align-user-ui-styles-with-admin-ui

## 状态：ARCHIVED

## spec 级行为变化

无。本次变更为纯前端样式迁移（令牌、字体、UI 原语、页面视觉结构），不涉及公开 API 签名、状态机、跨系统数据协议或可验证的"当 X 发生时应产生 Y 结果"行为契约。等价于官方 `skip_specs: true`，跳过 spec.md 创建。

## 文件路径说明

user-ui 无 `src/pages/` 目录，所有页面组件位于 `user-ui/src/components/`：`login-page.tsx`、`dashboard.tsx`、`usage-log-page.tsx`。UI 原语位于 `user-ui/src/components/ui/`（button/card/input/badge/progress/sonner）。

## 任务

### 基础设施层

- [x] 1. 复制字体文件（8 个 woff2）+ `fonts.css` 到 `user-ui/public/`，改 `user-ui/index.html` 移除 Google Fonts CDN、加 preload 与 `fonts.css` 引用（保留 `user-ui-theme` key） — 验证：`ls user-ui/public/fonts/` 含 8 个 woff2 + `fonts.css`；`grep -c googleapis user-ui/index.html` = 0；`index.html` 含 preload + fonts.css link
- [x] 2. 用 admin-ui 版本替换 `user-ui/src/index.css` — 验证：`diff admin-ui/src/index.css user-ui/src/index.css` 输出为空（两文件逐字节一致，含 admin 专属 @layer 类一并复制）
- [x] 3. 用 admin-ui 版本替换 `user-ui/tailwind.config.js`，仅 `content` 数组保留 user-ui 路径 — 验证：colors/boxShadow/fontFamily 扩展与 admin-ui 一致（`diff <(grep -E 'colors|boxShadow|fontFamily|space|neon' admin-ui/tailwind.config.js) <(grep -E 'colors|boxShadow|fontFamily|space|neon' user-ui/tailwind.config.js)` 为空）；content 数组指向 user-ui 的 index.html/main.tsx
- [x] 4. 用 admin-ui 版本替换 5 个 UI 原语 — 验证：`for f in button card input badge progress; do diff admin-ui/src/components/ui/$f.tsx user-ui/src/components/ui/$f.tsx; done` 全部输出为空

### 公共组件层

- [x] 5. 引入 `page-head.tsx` 到 `user-ui/src/components/`（i18n `t('common.back')` 改硬编码「返回」） — 验证：`grep react-i18next user-ui/src/components/page-head.tsx` = 0；`grep -E '@radix-ui|@tanstack' user-ui/src/components/page-head.tsx` = 0（仅依赖 lucide-react + React + cn）；无类型错误
- [x] 6. 引入 `table-kit.tsx` 到 `user-ui/src/components/`（仅迁移 PANEL/PANEL_TITLE/PANEL_FOOT/TH_BASE/CELL/ICON_BTN/FOOT_BTN/PAGER_BTN 常量 + pageWindow + Pager，舍弃 DataCheckbox；i18n 改硬编码「上一页」/「下一页」） — 验证：`grep react-i18next user-ui/src/components/table-kit.tsx` = 0；`grep @radix-ui/react-checkbox user-ui/src/components/table-kit.tsx` = 0；`grep DataCheckbox user-ui/src/components/table-kit.tsx` = 0；无类型错误

### 页面层

- [x] 7. 重写 `user-ui/src/components/login-page.tsx` 对齐 admin-ui panel 布局 — 子校验：
  - 7a. 布局骨架：自定义 panel（352px/11px 圆角/surface/hairline/shadow-pop）+ 品牌渐变徽标 + LABEL + Input h-[34px] + Button h-[34px] w-full
  - 7b. i18n 硬编码：描述/按钮/副标题改中文（无 `react-i18next` import）
  - 7c. 业务逻辑保留：textarea 粘贴发货信息 + extractApiKey + login API + storage.setApiKey + toast error + loading 态不变
  - 验证：`grep -E 'bg-background|text-muted-foreground|bg-card|KeyRound' user-ui/src/components/login-page.tsx` = 0；`grep react-i18next user-ui/src/components/login-page.tsx` = 0；保留 extractApiKey/login/storage/toast 调用
- [x] 8. 重写 `user-ui/src/components/dashboard.tsx` 对齐 admin-ui 页头风格 + 新令牌卡片 — 子校验：
  - 8a. 页头结构：PageHead 或等价（标题 + 状态 Badge + actions 区：查看日志/刷新/主题切换/退出）
  - 8b. 令牌类替换：`text-muted-foreground` → `text-ink-3`，`bg-background` → `bg-surface`/`bg-background`（页面底保留），`bg-card` → `bg-surface`，Card 内令牌对齐
  - 8c. 业务逻辑保留：useQuery/getUsage/refetch 30s/cc-switch 导入/用量计算/formatTokens/formatCost/主题切换/退出/storage 不变
  - 验证：`grep -E 'bg-card|text-muted-foreground' user-ui/src/components/dashboard.tsx` = 0；保留 useQuery/getUsage/importToCcSwitch/storage 调用
- [x] 9. 重写 `user-ui/src/components/usage-log-page.tsx` 对齐 table-kit 表格结构 — 子校验：
  - 9a. 表格结构：PANEL/TH_BASE/CELL + Pager 分页
  - 9b. 令牌色替换：MODEL_COLORS 的 orange/blue/green 硬编码改为 brand/ok/warn/danger 令牌色或 Badge variant；`text-orange-600`/`text-blue-600`/`text-green-600` 清零
  - 9c. 业务逻辑保留：useQuery 分页/getUsageLogs/汇总计算/按模型分组 不变
  - 验证：`grep -E 'text-orange-600|text-blue-600|text-green-600|bg-muted' user-ui/src/components/usage-log-page.tsx` = 0；保留 useQuery/getUsageLogs 调用

### 验证层

- [x] 10. 构建验证：`cd user-ui && npm run build` 全量通过 — 验证：build 产物生成，无 TypeScript error、无未解析 import、无 Vite 警告

## 验收标准

- [ ] `diff admin-ui/src/index.css user-ui/src/index.css` 为空
- [ ] tailwind.config.js 的 colors/boxShadow/fontFamily/space/neon 扩展与 admin-ui 一致（content 路径可差异）
- [ ] 5 个 UI 原语（button/card/input/badge/progress）与 admin-ui 对应文件 diff 为空
- [ ] sonner.tsx 与 admin-ui 版本 diff 为空（未改动，保持一致）
- [ ] 3 个页面（login-page/dashboard/usage-log-page）无旧令牌类残留：`grep -rE 'bg-card|text-muted-foreground|border-input|bg-secondary|text-orange-600|text-blue-600|text-green-600|bg-muted' user-ui/src/components/{login-page,dashboard,usage-log-page}.tsx` = 0
- [ ] 3 个页面无 `react-i18next` import：`grep -r react-i18next user-ui/src/components/{login-page,dashboard,usage-log-page,page-head,table-kit}.tsx` = 0
- [ ] user-ui 无 `@radix-ui/react-checkbox` import：`grep -r @radix-ui/react-checkbox user-ui/src/` = 0
- [ ] `cd user-ui && npm run build` 通过（exit 0）
- [ ] user-ui 业务逻辑未变（login API/getUsage/getUsageLogs/importToCcSwitch/storage/extractApiKey 均保留）
- [ ] admin-ui 未被改动：`git diff --stat admin-ui/` 为空
- [ ] 后端 Rust 未被改动：`git diff --stat src/` 为空
- [ ] use-theme.ts 未改动（STORAGE_KEY=user-ui-theme 保持，主题切换机制与 admin-ui 同为 .dark class toggle）
