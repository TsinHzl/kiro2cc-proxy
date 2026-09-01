## 上下文

`user-ui/` 与 `admin-ui/` 是本项目两个独立前端，构建后各自通过 rust-embed 嵌入 Rust 二进制（`src/user_ui/`、`src/admin_ui/`）。两者技术栈相同（React 18 + TS + Vite + Tailwind 3.4 + shadcn/ui 原语 + cva + lucide-react + sonner + react-query），但 user-ui 的视觉系统停留在 shadcn 默认模板 + Google Fonts CDN，admin-ui 已演进为完整设计令牌系统 + 自托管 IBM Plex 字体 + 自定义 panel/PANEL/PageHead/table-kit 结构。

用户要求 user-ui 样式与 admin-ui **一模一样**（暗黑 + 白天），且页面结构也尽量对齐 admin-ui dashboard 风格。

## 目标 / 非目标

**目标：**
- user-ui 令牌系统、字体、Tailwind 扩展、UI 原语与 admin-ui 完全一致
- user-ui 页面结构（login/dashboard/usage-log）对齐 admin-ui 的 panel/PANEL/PageHead/table-kit 风格
- 暗黑 + 白天模式视觉一致
- user-ui 独立可构建，业务逻辑零改动

**非目标：**
- 不改动 admin-ui（source of truth，只读）
- 不改动后端 Rust
- 不引入 i18next / @radix-ui/react-checkbox 新依赖
- 不迁移 favicon（非样式范畴）
- 不做多语言扩展

## 决策

### D1：字体方案 — 复制 admin-ui 自托管 woff2 子集

**决策**：直接 `cp` admin-ui 的 8 个 woff2 + `fonts.css` 到 `user-ui/public/`，`index.html` 移除 Google Fonts CDN，加 preload + fonts.css 引用。

**理由**：用户明确要求字体方案复制 admin-ui 自托管字体。自托管 woff2 子集（unicode-range 分段，font-display: swap）相比 Google Fonts CDN 的优势：离线可用、字形与 admin-ui 完全一致、无第三方域名依赖。体积约 200-400KB，与 admin-ui 持平，可接受。

**代价**：git 仓库体积略增（woff2 二进制），但已是子集压缩格式，与 admin-ui 持平。

### D2：令牌系统 — 整体替换，双轨制令牌全量迁移

**决策**：`user-ui/src/index.css` 整体替换为 admin-ui 版本（含 `:root`/`.dark` 双套 shadcn HSL 三元组 + 下半区设计稿原生 hex/rgba 令牌 + 滚动条 + @layer utilities/components）。

**理由**：admin-ui 的双轨制令牌（shadcn 三元组供 `bg-primary/80` 透明度修饰，原生 hex 供 -soft/-line 变体）是达成"一模一样"的基础。整体替换避免令牌遗漏，保证两份 index.css 完全一致。

**代价**：admin-ui index.css 含 admin 专属 `@layer components` 类（logstream/logrow/lv/plot-grid/tl-main/rel），复制到 user-ui 后为惰性 CSS（未被引用）。为达"一模一样"接受此冗余——这些类无副作用，仅轻微增加 CSS 体积。

### D3：Tailwind 扩展 — 整体替换，保留 content 路径差异

**决策**：`user-ui/tailwind.config.js` 整体替换为 admin-ui 版本，仅 `content` 数组保留 user-ui 的 index.html/main.tsx 引用路径（两者目录结构相同，content 路径实际一致，但显式保留以避免硬编码 admin-ui 路径）。

**理由**：colors/boxShadow/fontFamily 扩展必须与 admin-ui 完全一致才能让令牌类（`bg-surface`/`border-hairline`/`text-ink`/`shadow-panel` 等）在 user-ui 生效。admin-ui 注释明确"整值 var() 引用无法应用透明度修饰符，需半透明时用 -soft/-line 变体"——一并保留此注释防止误用。

### D4：UI 原语 — 直接覆盖，5 个原语全量迁移；sonner 保持现状

**决策**：button/card/input/badge/progress 直接用 admin-ui 版本覆盖 user-ui 对应文件，内容完全一致。`sonner.tsx` 不改动。

**理由**：UI 原语是视觉一致性的最小单元，admin-ui 已统一为 31px 高 / 7-11px 圆角 / 12.5px 字号 + 新令牌。直接覆盖保证原语层完全一致，无需逐行 cherry-pick。input 的 unit prop 功能 user-ui 当前未用但保留（惰性 prop，无副作用）。

**sonner 不改动的理由**：已核实 user-ui 与 admin-ui 的 `sonner.tsx` 当前逐字节完全相同，均用 shadcn 三元组令牌（`bg-background`/`text-muted-foreground`/`bg-primary`/`bg-muted`）。这些令牌在双轨制 index.css 上半区（`:root`/`.dark` 的 `--background`/`--foreground`/`--primary`/`--muted`）已定义，暗黑/白天模式均生效。既然 admin-ui 的 sonner 本就用这套令牌且未改为下半区原生令牌，user-ui 保持一致即可达成"一模一样"，无需改动。

### D5：i18n 处理 — 硬编码中文，不引入 i18next

**决策**：user-ui 保持无 i18n 现状，admin-ui 中所有 `t('...')` 调用在迁移到 user-ui 时改为硬编码中文文案。

**理由**：user-ui 当前无 i18n，引入 i18next 需新增依赖（react-i18next + i18next）+ 初始化代码 + 语言资源文件，违反"不新增外部依赖"约束且超出"样式迁移"范围。user-ui 文案量少（登录页 + 页头 + 分页），硬编码中文符合现状。

**映射表**：
- `t('login.description')` → 「请输入您的 API Key 或粘贴发货信息查看用量数据」
- `t('settings.adminPassword')` → 「API Key」（语义调整：user-ui 是 API Key 登录非管理密码）
- `t('login.loginButton')` → 「查看用量」
- `t('dashboard.consoleSubtitle')` → 「额度用量监控控制台」
- `t('common.back')` → 「返回」
- `t('common.prevPage')` → 「上一页」
- `t('common.nextPage')` → 「下一页」

### D6：table-kit 的 DataCheckbox — 舍弃，不引入 Radix checkbox 依赖

**决策**：table-kit 仅迁移 PANEL/PANEL_TITLE/PANEL_FOOT/TH_BASE/CELL/ICON_BTN/FOOT_BTN/PAGER_BTN 常量 + pageWindow 函数 + Pager 组件，**舍弃 DataCheckbox**（因依赖 `@radix-ui/react-checkbox`，user-ui 未引入）。

**理由**：usage-log-page 当前无复选框需求，引入 `@radix-ui/react-checkbox` 仅为迁移一个未使用的组件违反"不新增外部依赖"约束。舍弃 DataCheckbox 不影响 usage-log-page 的表格/分页功能。未来如 user-ui 需复选框再评估引入依赖。

### D7：page-head / table-kit 依赖核查 — 仅 lucide-react + React + cn，无隐藏依赖

**决策**：引入 page-head.tsx 与 table-kit.tsx 前已核查依赖。

**核查结论**：
- `page-head.tsx`：admin-ui 版仅依赖 `lucide-react`（ChevronLeft 图标）+ React + `cn`（`@/lib/utils`），无 @radix-ui、无 react-i18next（i18n 调用改硬编码后）、无 admin 专属 context。user-ui 已有 lucide-react + cn，可直接引入。
- `table-kit.tsx`：admin-ui 版的 Pager 仅依赖 React + cn + lucide-react（ChevronLeft/ChevronRight），无 context 依赖；DataCheckbox 依赖 `@radix-ui/react-checkbox`（user-ui 未引入，故舍弃）。常量（PANEL/TH_BASE/CELL 等）为纯字符串，无依赖。舍弃 DataCheckbox 后，table-kit 在 user-ui 内可独立渲染。

### D8：use-theme.ts — 不改动主题逻辑，仅令牌对齐

**决策**：`use-theme.ts` 的 `STORAGE_KEY = 'user-ui-theme'` 保持不变，主题切换逻辑（toggle .dark class）不改。

**理由**：已核实 user-ui 与 admin-ui 的 `use-theme.ts` 主题切换机制完全相同（均为 `localStorage.getItem(STORAGE_KEY)` + `document.documentElement.classList.toggle('dark', theme === 'dark')`，默认 dark），仅 STORAGE_KEY 不同（user-ui-theme vs admin-ui-theme）。令牌对齐后，同一套 .dark class 切换会应用一致的令牌，视觉自然一致。改 key 名会导致已登录用户主题偏好丢失，无收益。

### D9：sonner.tsx — 不改动，保持与 admin-ui 一致

**决策**：`user-ui/src/components/ui/sonner.tsx` 不改动。

**理由**：已核实 user-ui 与 admin-ui 的 sonner.tsx 逐字节完全相同，均用 shadcn 三元组令牌类（`bg-background`/`text-muted-foreground`/`bg-primary`/`bg-muted`）。这些令牌在双轨制 index.css 上半区已定义，暗黑/白天均生效。admin-ui 的 sonner 本就用这套令牌未改下半区原生令牌，user-ui 保持一致即达"一模一样"。若改动反而会偏离 admin-ui。

### D10：文件路径 — user-ui 无 pages/ 目录，组件均位于 components/

**决策**：所有页面组件（login-page/dashboard/usage-log-page）位于 `user-ui/src/components/`，非 `pages/`。

**理由**：已核实 `user-ui/src/pages/` 目录不存在，user-ui 与 admin-ui 的页面组件均放在 `src/components/` 下。proposal/tasks 中所有路径已修正为 `src/components/`。

## 风险 / 权衡

| 风险 | 权衡 |
|------|------|
| 令牌整体替换后旧令牌类失效 | 所有页面重写时统一改用新令牌类，逐文件审查（任务 7/8/9 覆盖） |
| DataCheckbox 舍弃限制未来扩展 | 当前无需求，未来按需引入依赖 |
| admin 专属 @layer 类惰性冗余 | 为"一模一样"接受，无副作用 |
| woff2 仓库体积增加 | 与 admin-ui 持平，可接受 |
| i18n 硬编码限制多语言扩展 | user-ui 当前无 i18n 需求，符合现状 |

## 迁移方案

纯前端迁移，无数据迁移、无协议迁移、无 Rust 改动。迁移顺序按 tasks.md 任务 1-10 串行执行，每个任务独立可验证。任务 1-6 为基础设施层（字体/令牌/Tailwind/原语/公共组件），任务 7-9 为页面层（login/dashboard/usage-log），任务 10 为构建验证。

## 待决问题（Open Questions）

无。所有技术决策已在 D1-D8 明确，无影响 spec/方案/任务拆分的未决项。
