# 设计：redesign-account-management-ui

## 上下文

`admin-ui` 是 React 18 + TypeScript + Vite + Tailwind 3.4 + shadcn 风格原语的单页应用，构建产物由 `rust-embed` 嵌入 Rust 二进制。现有主题系统是标准 shadcn 布局：`index.css` 里 `:root` / `.dark` 两组 HSL 三元组令牌，`tailwind.config.js` 通过 `hsl(var(--x))` 消费，`darkMode: 'class'`，主题由 `index.html` 内联脚本从 `localStorage['admin-ui-theme']` 引导。

设计稿 `option-a-console.html` 是独立 HTML 原型：令牌为完整颜色值（hex / rgba），主题切换靠 `data-theme="dark"` 属性，组件样式是手写 CSS 类（`.metric` / `.tag` / `.quota` / `.rowops` …）。

两套系统的差异必须在本次变更中显式桥接，否则会出现「令牌两套并行、组件半新半旧」的长期债务。

## 目标 / 非目标

**目标**
1. 一套令牌，两种消费方式：既能被既有 shadcn 工具类（`bg-background` / `border-border`）消费，也能被新组件按设计稿语义（`bg-surface-2` / `text-ink-2` / `border-hairline`）消费。
2. 新组件全部走 Tailwind 工具类，不引入手写 CSS 类层——避免与既有 40+ 组件的样式表达方式分裂。
3. `dashboard.tsx` 只做状态编排，展现层下沉到 4 个新组件，单文件不再增长。
4. 零新增依赖、零后端改动。

**非目标**
- 不把设计稿的 CSS 类体系移植过来（`.btn` / `.tag` / `.panel` 等）。
- 不改造主题切换机制（保持 `.dark` 类 + `localStorage`，不迁移到 `data-theme`）。
- 不追求与设计稿 1:1 像素级一致，行高 / 字号 / 间距按设计稿数值落地，但功能差异处以真实能力为准。

## 决策

### 决策 1：令牌双轨制 — HSL 三元组保命名，原始色值新增语义

**方案 A（否决）**：把设计稿颜色全部转成 HSL 三元组，只用 shadcn 命名。
否决原因：深色态 `--brand-soft: rgba(43,184,166,.13)`、`--grid-dot: rgba(255,255,255,.035)`、三组阴影都是半透明多层值，无法塞进 `H S% L%` 三元组格式；强行转换会丢失透明度语义（在深色表面上叠加实色 soft 背景会破坏层次感）。

**方案 B（否决）**：完全丢弃 shadcn 令牌，全量改用设计稿命名。
否决原因：`src/components/ui/*` 的 10 个原语（button / dialog / input / select / switch / tabs …）与其余 6 个页面共约 40+ 处依赖 `bg-background` / `text-foreground` / `border-border` / `bg-primary`，全量重写超出「页面 + 共享外壳令牌」的既定范围。

**方案 C（采纳）**：双轨并存，语义对齐。
- shadcn HSL 三元组保留原名，数值重映射到设计稿调色板 → 既有组件与其他 6 页自动继承新配色，零改动
- 新增设计稿原生令牌为完整色值（hex / rgba），在 `tailwind.config.js` 中以 `var(--x)` 直接注册 → 新组件按设计稿语义命名消费

映射关系（省略中间灰阶，完整数值在实现任务中落地）：

| shadcn 令牌 | 对齐到设计稿 | 说明 |
|---|---|---|
| `--background` | `--bg` | 页面底 |
| `--card` / `--popover` | `--surface` | 卡片 / 弹层表面 |
| `--secondary` / `--muted` | `--surface-3` | 次级表面 |
| `--accent` | `--surface-3` | shadcn 语义是 hover 表面，非强调色 |
| `--foreground` | `--ink` | 主文字 |
| `--muted-foreground` | `--ink-2` | 次文字 |
| `--border` / `--input` | `--hairline` | 分隔线 |
| `--primary` / `--ring` | `--brand` | 品牌色 |
| `--destructive` | `--danger` | 危险色 |

**命名避让**：设计稿的 `--accent`（teal 品牌色）与 shadcn 的 `--accent`（hover 表面）语义直接冲突。品牌色一律命名为 `--brand{,-hover,-soft,-line,-fg}`，绝不复用 `--accent`。

**取舍**：`:root` / `.dark` 块会从 21 行增长到约 55 行，且同一颜色以两种格式各存一份（如 `--background: 210 13% 97%` 与 `--bg: #F6F7F8`）。接受这份重复，换取「既有代码零改动 + 新组件语义准确」。重复值的一致性由实现时的换算表保证，并在 CSS 中以注释标注配对关系。

### 决策 2：Tailwind 而非手写 CSS 类

设计稿的 `.metric` / `.tag.ok` / `.quota-pct` 等约 90 个 CSS 类不移植。理由：
- 项目现有全部组件（含 `credential-card.tsx`、6 个页面组件）都是纯 Tailwind 工具类，引入并行的 CSS 类体系会让后续维护者面对两种样式来源
- 设计稿的类是为静态原型服务的，状态变体（hover / active / disabled / sortable）在 React 里用条件 class 表达更直接
- Tailwind JIT 只产出实际用到的类，体积优于全量 CSS

例外：三处确实需要写进 `index.css` 的全局样式 —
1. 滚动条样式（已存在，仅更新颜色令牌）
2. `--grid-dot` 网格点背景（`background-image: radial-gradient(...)`，Tailwind 表达冗长且复用于多处容器）
3. `font-feature-settings` 与表格数字等宽（`font-variant-numeric: tabular-nums`）

### 决策 3：组件边界 — 按「状态归属」而非「视觉区块」切分

正文拆 4 个新组件，切分依据是各自需要的状态最小集：

```
dashboard.tsx（状态编排层，保留全部既有业务逻辑）
├─ 既有状态：balanceMap / loadingBalanceIds / selectedIds / currentPage / dialogs…
├─ 新增状态：searchQuery / statusFilter / sortKey + sortDir
├─ 派生（useMemo）：filtered → sorted → paged
│
├─ <AccountMetrics>      ← 只读：credentials + balanceMap + dailyUsage
├─ <AccountToolbar>      ← 受控：searchQuery / statusFilter + 各状态计数 + dataUpdatedAt
└─ <AccountTable>        ← 受控：paged 行 + 排序状态 + 选中集 + 全部行级回调
   └─ <AccountRow>       ← 单行：item + balance + rpm + 回调；自持删除确认与编辑对话框开关
```

**为什么派生计算留在 `dashboard.tsx`**：状态计数（工具栏分段徽章）需要「搜索后但筛选前」的集合，分页需要「筛选+排序后」的集合，两者都跨组件。把 `filtered` / `sorted` 提升到父级用 `useMemo` 一次算出，避免子组件各自重算或通过回调向上报数。

**为什么删除/编辑对话框下沉到 `AccountRow`**：这两个对话框的开关状态天然按行隔离，提升到父级需要额外维护 `deletingId` / `editingId`，等于把行内私有状态泄漏到页面级。`credential-card.tsx` 现有实现已是此模式，直接沿用。

### 决策 4：排序中的「未查询」语义 — 恒定末位

剩余额度排序时，未查询余额的账号既不能当作 0（会在升序时抢占「额度最低」的语义位置，误导运维判断），也不能当作无穷大。采用**分区排序**：已查询组按数值排序（受 `sortDir` 控制），未查询组恒定拼接在末尾且内部按 ID 升序保持稳定。

### 决策 5：状态派生集中在一个纯函数

状态标签（表格状态列）与筛选分段（工具栏）必须用同一套判定，否则会出现「筛选『健康』却包含显示为『待验证』的行」。抽出单一纯函数：

```
deriveAccountState(item, hasBalance) → 'disabled' | 'error' | 'warning' | 'pending' | 'healthy'
```

表格状态列与工具栏计数、筛选过滤全部消费它。`hasBalance` 由 `balanceMap` 决定，因此「待验证」会在首次查询余额后自动转为「健康」——这符合语义（待验证 = 未经任何调用且未查过额度）。

放置位置：`admin-ui/src/lib/account-state.ts`（与既有 `lib/utils.ts` 同级），供 3 个组件共享导入。

### 决策 6：sparkline 与环形图手写 SVG

不引入图表库（recharts 已在依赖中，但用于每日统计页的完整图表）。74×26 的 sparkline 与 42px 环形图用内联 `<svg>` + `<path>`：
- sparkline：把最近 7 天 `totalRequests` 归一化到 26px 高度生成 `polyline` points，另生成一份闭合 `path` 填充品牌色渐变
- 环形图：`<circle r=17 stroke-width=4.5>` + `stroke-dasharray: 106.8` + `stroke-dashoffset` 按百分比计算（复用现有 `CreditsProgressRing` 的算法，仅换半径与配色）

recharts 用于两个 42px 级别的装饰性图形属于杀鸡用牛刀，且其容器测量逻辑在窄尺寸下需要额外配置。

### 决策 7：字体自托管（实测数据推翻初稿的 CDN 方案）

初稿基于「自托管约 400KB + 需改 `vite.config.ts` 与 `rust-embed` 路径」的估计选择了 CDN。实测后前提不成立，改为**自托管**：

- 实际体积为 **8 个 woff2 共 130KB**（Latin 子集；中文本就由系统字体承担，IBM Plex Sans 无中文字形）
- 无需任何构建配置改动：文件放 `admin-ui/public/fonts/`，Vite 原样拷入 `dist/`，`rust-embed` 自动嵌入
- 设计稿自带 `fonts.css`，已按 `unicode-range` 切分 Latin / Latin-Ext 两段，浏览器只下载实际用到的子集
- 消除 Google Fonts 在国内的可达性风险（本项目 zh-CN 优先，界面文案默认中文）

落地方式：从设计稿 `fonts.css` 中**仅提取 IBM Plex Sans / IBM Plex Mono 的 14 个 `@font-face`**（Sans 400/500/600/700 + Mono 400/500/600，共 7 字重 × 2 个 `unicode-range` 子集；原文件含 7 个字族、48 个块，其余 5 族属于设计稿的其他方案），写入 `admin-ui/public/fonts.css`，由 `index.html` 以 `<link href="/fonts.css">` 引入，同时移除 Google Fonts 的 `preconnect` 与 CSS 链接。`font-display` 由设计稿的 `block` 改为 `swap` —— 界面主体是中文（由 PingFang 承担），FOIT 会让首屏中文被无谓遮挡。

回退链仍显式声明 `'IBM Plex Sans','PingFang SC','Hiragino Sans GB',system-ui`，中文与字体加载失败两种情况均有兜底。

### 决策 8：设计稿容纳不下的既有读数与入口 — 三处安置

设计稿是从零画的原型，不知道现有实现里有 4 项额外读数/入口。它们必须有明确落点，否则删除 `credential-card.tsx` 时会静默丢功能：

| 既有元素 | 设计稿是否有位置 | 安置决策 |
|---|---|---|
| 限流次数 `throttleCount` | 无 | 并入「调用/失败」单元格作为第三段，仅 > 0 时显示（避免 112px 列宽被三个恒显数字撑爆） |
| 失败日志入口 | 无 | 双入口：失败数可点击（沿用现有交互习惯）+ ⋯ 菜单固定项（计数为 0 时也能进） |
| 限流日志入口 | 无 | 同上：限流数可点击 + ⋯ 菜单固定项 |
| 代理地址 / `hasProfileArn` 徽章 | 无 | 降级为账号单元格内邮箱右侧的 10px 微徽章 `PROXY` / `ARN`，完整代理地址走 title |
| 验活进度浮动入口（`verifying && !verifyDialogOpen`） | 无 | 留在操作条内，显隐条件与进度文案不变 —— 它是长任务的唯一回归路径，丢掉会让用户在关掉对话框后失去进度可见性 |

**条件性显隐是重构中最容易静默丢失的一类逻辑**：它不产生类型错误、不影响构建，只有在特定数据态下才暴露。因此把行级 11 项 + 页面级 5 项能力**连同各自的禁用/显隐条件**写进 proposal.md 的表格，并让 T12 / T18 / T19 三个任务各自引用对应区段作为勾对清单，而不是笼统写「保留既有功能」。

「计数可点击」与「⋯ 菜单固定项」双入口不是冗余：计数为 0 时数字不显示（限流）或语义上无内容可看（失败），但运维仍需要能主动打开日志页确认历史；反之日常巡检时从计数直接点进去比展开菜单快。

**ARN 的处理**：后端 `CredentialStatusItem` 只有 `hasProfileArn: boolean`，真实 ARN 字符串仅存在于写侧请求体。因此不提供「复制 ARN」操作，搜索字段也不含 ARN —— 否则会实现出一个「匹配一段恒定静态文案」的假搜索。若后续需要，应作为独立变更先补后端字段。

**`isCurrent` 的处理**：字段存在但现有 UI 从未消费，设计稿亦无对应元素。不在本次新增，明确记入 proposal.md 的「不在范围内」，避免把「新功能」伪装成「保持现状」。

## 风险 / 权衡

| 项 | 权衡 |
|---|---|
| 令牌重复定义（HSL + hex 各一份） | 换取既有组件与其他 6 页零改动。风险是后续改色需同步两处 —— 以 CSS 注释标注配对关系缓解 |
| 其他 6 页局部色相不一致 | 这些页面存在硬编码的 `text-green-600` / `neon-*` / 紫色渐变类，不在本次范围。它们会与新 teal 体系并存一段时间 —— 明确记录为后续变更，不在本次静默扩大范围 |
| 分页 50 行的渲染成本 | 表格行 DOM 显著轻于卡片；若实测卡顿，虚拟滚动可作为独立优化，本次不预先引入 |
| 表格在 <1280px 挤压 | 固定 `colgroup` 列宽 + 表体横向滚动。移动端不作为目标形态（admin 控制台场景） |
| 优先级编辑路径变长（内联 → 对话框） | 内联数字输入在紧凑表格行里没有容身空间；编辑对话框已含该字段，功能不丢失，仅多一次点击 |
| `deriveAccountState` 中「待验证」依赖 `balanceMap` | 该 Map 是会话内内存状态，刷新页面后未查询的账号会重新显示为「待验证」。这与「待验证」的语义一致（本会话未验证过），接受 |
