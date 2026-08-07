# 变更提案：admin-global-i18n

## 背景

更新日志页面（`add-changelog-page`）目前只在该页面内部实现了一个局部的中/英文切换（页面级 `useState`），此前的设计明确决定"不引入全局 i18n 框架，不改造其他 Admin 页面的语言"。现在需求扩大：将语言切换入口迁移到"设置"页面，切换后需要覆盖全部 Admin 页面的中英文渲染，这直接推翻了此前的范围限制，属于新的架构级功能，需要引入真正的全局 i18n 能力。同时，"设置"页面自身的卡片网格布局需要重做为 iOS 风格的分组列表（Tableview）样式。

## 目标范围

**在范围内：**
- 引入 `react-i18next` + `i18next`，建立全局 i18n 基础设施（初始化模块 + 中/英资源文件）
- "设置"页面新增"语言"分组，提供中文/English 切换控件
- 语言偏好持久化到 `localStorage`，刷新页面或重新登录后保持
- 首次访问（无持久化记录）默认中文，不做浏览器语言自动检测
- 全部 Admin 页面文案迁移到 i18n key，支持中英渲染：`dashboard.tsx` 导航与统计卡片、账号管理相关组件（`credential-card`/`credential-detail-page`/`add-credential-dialog`/`edit-credential-dialog`/`balance-dialog`/`batch-import-dialog`/`kam-import-dialog`/`batch-verify-dialog`）、API Keys 相关（`api-keys-panel`/`api-key-detail-page`）、每日统计相关（`daily-stats-page`/`daily-detail-page`/`daily-credits-trend-chart`）、日志相关（`log-viewer-page`/`failure-log-page`/`throttle-log-page`）、支持模型页面（`model-list-page`）、更新日志页面（`changelog-page`）、登录页（`login-page`）、设置页面自身（`settings-panel`）
- 更新日志页面移除页面内独立的中/英切换按钮，改为跟随全局语言状态渲染其 `zh`/`en` 双语字段
- "设置"页面视觉重构为 iOS Tableview 风格分组列表（分组标题 + 圆角容器 + 组内行以分隔线分隔，替换现状的并排卡片网格）
- ~~侧边栏底部（主题切换按钮旁）新增一个语言切换图标按钮，点击在中/英之间切换，与设置页面的语言切换功能并存、共享同一套 i18n 状态与持久化逻辑~~（已实现后按后续布局调整决策移除，语言切换入口仅保留设置页面一处）

**不在范围内：**
- 不翻译后端返回的动态运行时数据内容（账号名称、错误提示原文、日志原始内容等），仅翻译前端硬编码的界面文案
- 不新增中/英以外的语言，不做 RTL 支持
- 不做浏览器 `Accept-Language` 自动检测
- 不改动 `user-ui`（面向最终用户的独立前端）
- 不改造设置页面以外其他页面的卡片视觉风格
- 更新日志内容本身（版本说明文案）仍由后端 `{ zh, en }` 双语字段提供，不迁移进前端 i18n 资源文件

## 技术方案

- **依赖**：新增 `react-i18next` + `i18next`
- **初始化**：新增 `admin-ui/src/i18n/index.ts`，配置 `resources`（引用 `locales/zh.json`/`locales/en.json`）、`fallbackLng: 'zh'`；启动时读取 `localStorage.getItem('admin-lang')` 决定初始语言，缺省为 `'zh'`
- **资源文件**：`admin-ui/src/i18n/locales/zh.json`、`en.json`，按页面命名空间分组 key（如 `common.*` 存放"加载中""保存""取消"等复用词，`credentials.*`/`apiKeys.*`/`dailyStats.*`/`models.*`/`logs.*`/`settings.*`/`changelog.*`/`login.*` 按页面隔离）
- **组件改造**：各组件改用 `useTranslation()` 的 `t('namespace.key')` 替换硬编码中文字符串
- **key 复用规则**：跨 3 个及以上页面/组件复用的通用短语（如"保存""取消""加载中""确认""删除"等）一律归入 `common.*`；仅在单一页面/组件内出现的文案归入该页面自身命名空间，禁止同一短语在多个页面命名空间下重复定义不同 key
- **切换与持久化**：设置页面切换控件调用 `i18n.changeLanguage(lang)` 并 `localStorage.setItem('admin-lang', lang)`；不引入 `i18next-browser-languagedetector`（只有两种语言，手动读写足够，避免额外依赖）
- **更新日志页面改造**：移除内部 `useState<'zh'|'en'>` 与切换按钮 JSX，改用 `const { i18n } = useTranslation()`，依据 `i18n.language` 选择渲染后端返回的 `zh`/`en` 字段
- **设置页面重构**：分组标题（灰色小字）+ `Card` 作为分组容器 + 组内 `divide-y divide-border` 行分隔，每行左侧标签/说明、右侧控件（按钮/开关/文本），不新增通用 List/Row UI 组件到 `ui/` 目录，直接在 `settings-panel.tsx` 内实现

## 预期影响

- 前端新增一个依赖包（`react-i18next`/`i18next`），构建产物体积小幅增加
- 全部 Admin 页面文案来源从硬编码字符串改为 i18n key 查找，功能行为不变，仅渲染语言可切换
- 设置页面视觉从卡片网格改为列表分组，需人工目测确认无回归
- 不影响后端任何接口和数据结构

## 风险

- 涉及文件多（约 20 个组件，累计约 4000+ 个中文字符的文案），存在个别文案遗漏未迁移、切换后仍显示中文的风险；通过阶段 3 末尾的逐页人工核对任务降低该风险
- 英文文案为人工整理翻译，非专业翻译团队产出，措辞可能不够地道，可接受
- 设置页面同时进行"新增语言切换"与"视觉重构"两项改动，耦合在同一文件内，需要人工目测确认无非预期的样式回归
- 若现有前端测试用例中存在依赖硬编码中文文案做断言或快照比对的写法，文案迁移为 i18n key 后可能导致这些用例失败；任务 12 中显式回归执行现有前端测试套件以排查此风险
