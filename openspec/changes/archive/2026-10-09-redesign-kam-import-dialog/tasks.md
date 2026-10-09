# 任务清单：redesign-kam-import-dialog

## 状态：ARCHIVED

## 任务
- [x] 任务 1：改造 batch-import-dialog.tsx — 头部标题+胶囊徽章、快捷工具栏（填入示例/格式化/清空），验证：pnpm build 通过、工具栏三个按钮行为正确
- [x] 任务 2：实现 Dropzone 拖拽与本地文件选择（.json、2MB 校验、悬停高亮覆盖层），验证：pnpm build 通过、拖拽/点击选择均能填入 textarea
- [x] 任务 3：实现实时 JSON 健康度状态条（300ms 防抖，🟢 已识别 X 个账号 / 🔴 语法错误），验证：pnpm build 通过、输入合法/非法 JSON 状态条正确切换
- [x] 任务 4：琥珀色卡片式验活规则提示替换现有 hint 文案，验证：pnpm build 通过、样式与原型一致
- [x] 任务 5：导入按钮加载态改 SVG spinner（Loader2），导入中禁用工具栏/拖拽/文件选择，验证：pnpm build 通过
- [x] 任务 6：i18n 同步 — zh.json / en.json 新增全部新 key，验证：pnpm build 通过、两种语言文案完整
- [ ] 任务 7：整体验证 — `cd web-ui/admin-ui && pnpm install && pnpm build` 成功，导入验活业务流（查重/验活/结果列表/Toast）行为不变

## 验收标准
- [ ] 弹窗 UI 与原型 index.html 布局一致：徽章、工具栏、拖拽区、状态条、琥珀提示卡、底部按钮组
- [ ] 拖拽 .json 文件与点击选择文件均能正确填入并触发实时校验
- [ ] 非法 JSON 提交前即有红色错误提示
- [ ] 现有导入验活流程（8 种账号状态、进度条、Toast）行为无任何变化
- [ ] zh / en 文案完整，pnpm build 零报错
