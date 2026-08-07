# 任务清单：admin-global-i18n

## 状态：ARCHIVED

## 任务

- [x] 任务 1：安装 `react-i18next` + `i18next` 依赖；创建 `admin-ui/src/i18n/index.ts` 初始化模块（读取 `localStorage.getItem('admin-lang')`，缺省 `zh`，`fallbackLng: 'zh'`）；创建空壳 `locales/zh.json`/`locales/en.json`（含 `common.*` 基础词条：加载中/保存/取消/确认等）；在应用入口引入 i18n 模块
  - spec.md 引用：全局语言切换
- [x] 任务 2a：`settings-panel.tsx` 新增"语言"分组（zh/en 切换控件，调用 `i18n.changeLanguage` + `localStorage.setItem('admin-lang', lang)`），暂不改动页面其余部分的布局
  - spec.md 引用：全局语言切换
- [x] 任务 2b：将 `settings-panel.tsx` 整体从 `grid md:grid-cols-2` 卡片布局重构为 iOS Tableview 风格分组列表（认证密钥/负载均衡/语言三组，`Card` 容器 + `divide-y` 行分隔），保留原有认证密钥编辑保存与负载均衡切换交互逻辑不变
  - spec.md 引用：设置页面 iOS 风格列表布局
- [x] 任务 3：迁移 `dashboard.tsx` 导航栏与顶层统计卡片文案至 `common.*`/`dashboard.*` i18n key（复用判定：跨 3 个以上页面出现的短语归入 `common.*`）
  - spec.md 引用：全局语言切换
- [x] 任务 4：迁移账号管理相关组件文案（`credential-card.tsx`、`credential-detail-page.tsx`、`add-credential-dialog.tsx`、`edit-credential-dialog.tsx`、`balance-dialog.tsx`）至 `credentials.*` i18n key（复用判定同上）
  - spec.md 引用：全局语言切换
- [x] 任务 5：迁移批量导入相关组件文案（`batch-import-dialog.tsx`、`kam-import-dialog.tsx`、`batch-verify-dialog.tsx`）至 `credentials.*` i18n key（复用判定同上）
  - spec.md 引用：全局语言切换
- [x] 任务 6：迁移 API Keys 相关组件文案（`api-keys-panel.tsx`、`api-key-detail-page.tsx`）至 `apiKeys.*` i18n key（复用判定同上）
  - spec.md 引用：全局语言切换
- [x] 任务 7：迁移每日统计相关组件文案（`daily-stats-page.tsx`、`daily-detail-page.tsx`、`daily-credits-trend-chart.tsx`）至 `dailyStats.*` i18n key（复用判定同上）
  - spec.md 引用：全局语言切换
- [x] 任务 8：迁移日志相关组件文案（`log-viewer-page.tsx`、`failure-log-page.tsx`、`throttle-log-page.tsx`）至 `logs.*` i18n key（复用判定同上）
  - spec.md 引用：全局语言切换
- [x] 任务 9：迁移 `model-list-page.tsx`、`login-page.tsx` 文案至 `models.*`/`login.*` i18n key（复用判定同上）
  - spec.md 引用：全局语言切换
- [x] 任务 10：改造 `changelog-page.tsx` —— 移除内部 `useState<'zh'|'en'>` 与页面内语言切换按钮 JSX，改用 `useTranslation()` 的 `i18n.language` 决定渲染后端 `zh`/`en` 哪个字段；页面自身少量界面文案（如无数据提示）迁移至 `changelog.*` i18n key
  - spec.md 引用：更新日志页面语言跟随全局设置
- [x] 任务 11：逐页人工核对 + 量化核验 —— 切换语言分别浏览全部已迁移页面确认无遗漏硬编码中文残留；对 `admin-ui/src/components/**/*.tsx` 做 CJK 字符残留扫描（排除注释与直接渲染后端 `zh`/`en` 字段的代码路径），确认扫描结果为空或均为已知排除项；Playwright 切换语言测试中捕获浏览器控制台 `i18next: missingKey` 警告并断言为空
  - spec.md 引用：全局语言切换
- [x] 任务 12：`cargo fmt` / `cargo clippy`（若涉及后端）+ 前端 `tsc` 类型检查 + `npm run build`（admin-ui）全量验证；运行现有前端测试套件确认文案迁移未导致依赖硬编码中文断言/快照的用例回归失败；Playwright 浏览器测试切换语言与设置页面新布局的关键路径
- [x] 任务 13：`dashboard.tsx` 侧边栏底部（主题切换按钮旁）新增语言切换图标按钮，点击在 `zh`/`en` 之间切换并写入 `localStorage`（复用 `settings-panel.tsx` 已有的 `i18n.changeLanguage` + `LANG_STORAGE_KEY` 逻辑），与设置页面语言切换功能并存 —— **已实现后按后续布局调整决策移除**，`toggleLanguage`/`Languages` 图标/`LANG_STORAGE_KEY` 引用等相关代码已一并清理，语言切换入口仅保留设置页面一处
  - spec.md 引用：全局语言切换（复用同一需求场景，新增侧边栏触发入口，现已撤销）

## 验收标准

- [ ] 设置页面新增语言切换控件，切换后 `localStorage` 写入 `admin-lang`，刷新页面后语言保持
- [ ] 无 `localStorage` 记录时默认中文渲染
- [ ] 切换语言后，任务 3-9 覆盖的全部页面文案实时切换为对应语言，无遗漏硬编码中文
- [ ] 更新日志页面不再显示页面内独立语言切换按钮，且跟随全局语言渲染对应字段
- [ ] 设置页面视觉呈现为分组列表（灰色分组标题 + 圆角容器 + 行分隔线），认证密钥与负载均衡原有交互功能不变
- [ ] `cargo fmt`/`cargo clippy`/`tsc`/`npm run build` 全部通过
- [x] ~~侧边栏底部新增语言切换图标按钮，点击可在中/英之间切换，且与设置页面语言切换功能互相同步（共享同一状态与持久化）~~ —— 已撤销，语言切换入口仅保留设置页面一处
