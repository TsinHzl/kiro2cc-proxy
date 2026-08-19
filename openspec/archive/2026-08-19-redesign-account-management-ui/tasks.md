# 任务清单：redesign-account-management-ui

## 状态：ARCHIVED

## 范围声明

纯前端变更（`admin-ui/`）。无 Rust 侧任务，无 API 契约变更，`cargo` 相关校验不涉及。
每个任务的验证手段统一为：`cd admin-ui && npx tsc --noEmit`（类型）+ `npm run build`（构建）+ 浏览器双主题目视核对（`npm run dev`）。

---

## 任务

### T1 — 设计令牌落地（index.css）
- [x] 重写 `admin-ui/src/index.css` 的 `:root` / `.dark`：shadcn HSL 三元组按 design.md 决策 1 映射表重映射；新增设计稿原生令牌（`--surface` / `--surface-2` / `--surface-3` / `--sidebar` / `--hairline` / `--hairline-2` / `--ink` / `--ink-2` / `--ink-3` / `--brand{,-hover,-soft,-line,-fg}` / `--ok{,-soft,-line}` / `--warn{,-soft,-line}` / `--danger{,-soft,-line}` / `--track` / `--code-bg` / `--shadow-sm` / `--shadow-md` / `--shadow-pop` / `--grid-dot`），并以注释标注 HSL↔hex 配对关系
- [x] `body` 字族改为 IBM Plex Sans 回退链；新增 `.bg-grid-dot` 网格背景工具类（16px 网格，设计稿仅用于侧栏）。数字等宽改用 Tailwind 内置 `tabular-nums` 工具类，不写全局 CSS
- [x] 滚动条颜色跟随新令牌
- 验收：`npm run build` 通过；浅色页面底 `#F6F7F8`、深色 `#0C0D0F`
- spec.md 引用：需求「设计令牌体系」场景 1–2

### T2 — Tailwind 令牌注册
- [x] `admin-ui/tailwind.config.js` 的 `theme.extend.colors` 注册全部设计稿令牌为 `var(--x)` 形式；`boxShadow` 注册 `hair/panel/pop`（不覆盖内置 `shadow-sm`/`shadow-md`，其他 6 页仍依赖其默认值）；`fontFamily.sans` → IBM Plex Sans 链、`fontFamily.mono` → IBM Plex Mono 链
- [x] 保留既有 `space.*` / `neon.*` 调色板（其他页面仍在使用，本次不动）
- 验收：新建临时元素使用 `bg-surface-2 text-ink-2 border-hairline shadow-panel` 能正确出色
- spec.md 引用：需求「设计令牌体系」场景 1–2

### T3 — 字体自托管（public/fonts.css + index.html）
- [x] 从设计稿 `fonts.css` 中仅提取 IBM Plex Sans（400/500/600/700）与 IBM Plex Mono（400/500/600）的 **14** 个 `@font-face`（7 字重 × 2 个 `unicode-range` 子集），生成 `admin-ui/public/fonts.css`；`font-display` 由设计稿的 `block` 改为 `swap`（避免首屏中文被 FOIT 遮挡）；已机器核对其 `url(fonts/...)` 引用的 8 个文件与 `admin-ui/public/fonts/` 磁盘文件逐一对应、无缺无余
- [x] `index.html`：移除 Google Fonts 的两个 `preconnect` 与 CSS `<link>`，改为 `<link rel="stylesheet" href="/fonts.css" />`，并对首屏两个 Latin 子集加 `<link rel="preload" as="font" type="font/woff2" crossorigin>`
- [x] 确认内联主题引导脚本（`.dark` 类 + `localStorage['admin-ui-theme']`）未被改动
- 验收：断网状态下正文仍渲染为 IBM Plex Sans；DevTools Network 无任何 fonts.googleapis.com / fonts.gstatic.com 请求；`npm run build` 后 `dist/fonts/` 含 8 个 woff2
- spec.md 引用：需求「设计令牌体系」场景 3

### T4 — 主题继承回归核对
- [x] 因禁止常驻后台进程（`agents.md`）无法在前台跑 dev server 目视核对，改用**等价的机械验证**：计算全部令牌配对的 WCAG 对比度，并与旧令牌逐对做新旧对照；同时枚举六页中的硬编码调色板颜色，核算其在新底色上的对比度
- [x] 结论：六页依赖的每一对令牌驱动配对对比度**全部提升**（浅色 `fg/bg` 16.01→17.64、`muted-fg/card` 5.67→5.96；深色 `fg/bg` 11.59→16.72、`muted-fg/card` 6.10→6.88），无可读性回归。六页 13 处硬编码色所在卡片底色未被改动，对比度一字未变（其中 `text-green-600` 3.30 / `text-orange-600` 3.56 / `text-cyan-600` 3.68 为既存问题，非本次引入，按规则不动）
- [x] 依据核算结果调整令牌数值：`--ink-3` 浅色 `#909699`→`#71777A`（3.00→4.54）、深色 `#6C737C`→`#79818B`（3.75→4.55），达 WCAG AA；未修改任何页面组件代码
- known issue（Low，不阻塞）：`api-keys-panel.tsx:1065` 的 `bg-violet-50` 选中态在新冷白底上对比 1.02（旧暖米底约 1.05 但色相差更明显），略难辨识。它是硬编码色而非令牌，无法通过调令牌修复，改动需动该页组件代码 —— 超出本次范围
- 验收：新令牌 22 对配对中承载文字的 20 对全部 ≥ 4.5 达 AA（浅色 4.24–18.92 / 深色 4.55–16.72）
- spec.md 引用：需求「设计令牌体系」场景 4

### T5 — 状态派生纯函数
- [x] 新建 `admin-ui/src/lib/account-state.ts`：导出 `deriveAccountState(item, hasBalance)` 返回 `'disabled' | 'error' | 'warning' | 'pending' | 'healthy'`，以及各状态对应的标签 i18n key、语义色 token、头像底色映射表
- 验收：`npx tsc --noEmit` 通过；5 条判定分支与 spec.md 状态映射表逐条对应
- 实施记录：
  - `AccountState` 类型 + `deriveAccountState()` 5 分支（顺序即优先级）+ `ACCOUNT_STATE_VISUAL`（`labelKey` / `tagClass` / `pipClass` / `avatarClass`）+ `ACCOUNT_STATE_ORDER`（工具栏分段顺序）
  - 标签 i18n key 定为 `credentials.state{Healthy,Warning,Error,Pending,Disabled}`（新键，不复用既有 `healthXxx`，因文案不同：「额度告警」≠「警告」、「异常」≠「不健康/降级」），实际写入 zh/en 留给 T20
  - `pipClass` 用 `ring-[2.5px] ring-*-soft` 表达设计稿 `box-shadow: 0 0 0 2.5px`，未使用透明度修饰符 —— 符合 T2 记录的 Tailwind 3.4 整值 `var()` 约束
  - CR（SHARDS=1）唯一 High「`healthStatus: 'disabled'` 静默归为 healthy」经核实**驳回**：`token_manager.rs:1745-1748` 中 `compute_health()` 仅在 `entry.disabled` 为真时返回 `Disabled`，且 `snapshot()`（同文件 2263-2287）用同一个 `e` 同时产出 `disabled` 与 `health_status`，故 `healthStatus === 'disabled'` ⟹ `disabled === true`，已被第一分支覆盖
  - CR Low「`-line` 令牌未注册」亦驳回：`tailwind.config.js:75-84` warn/danger/brand 三族均含 `soft` + `line`
  - 采纳 CR 派生的可维护性建议：末行 `return 'healthy'` 拆为显式 `healthStatus === 'healthy'` 分支 + 兜底 `return 'error'`，后端日后新增 `HealthStatus` 变体不会被静默显示为「健康」（今日行为零变化）
- spec.md 引用：需求「账号表格」场景「状态标签映射」

### T6 — 侧栏重构（品牌区 + 导航）
- [x] `dashboard.tsx` 内 `<aside>`：品牌区（30px 渐变方块 + `Kiro2CC` / `Proxy Console` 双行 + 收起按钮）
- [x] 导航项改为 33px 高、7px 圆角、12.5px 字号；激活态 `bg-brand-soft` + `text-brand` + 左侧 2.5px 竖条；分组标题「主要」/「系统」
- [x] 导航项右侧计数：账号数取已加载列表长度；数据未加载时不渲染数字
- 验收：激活项配色与设计稿一致；未加载时无 `0` 出现
- 实施记录：
  - `<aside>` 宽度 `w-[232px]`、`bg-sidebar bg-grid-dot`；品牌区改为渐变方块（`linear-gradient(150deg, var(--brand), var(--brand-deep))` + 盾形 SVG），删除 `kiro-icon.png` import，原 PNG（`admin-ui/src/assets/`）经 CR 确认成孤立资产后 `git rm`（favicon 走 `public/kiro-icon.png`，不受影响）
  - `index.css` 双主题新增 `--brand-deep`：设计稿 `color-mix(in srgb, brand 62%, #063B36)` 手算预算为 `#0A6259` / `#1D897B`，避免依赖 `color-mix()` 浏览器支持；该令牌仅内联 `style` 消费，不注册进 tailwind
  - 3 个重复的系统导航 `<button>` + 主导航数组合并为单一 `navGroups`（两级数据驱动，7 项）；CR 逐项核实 `active` 判定与 `onClick` 副作用序列与原代码**逐字等价**（含 `setDailyFromSidebar(true)` 与复合条件 `activeTab === 'credentials' && dailyView === null`）
  - 计数以 `count === undefined` 为哨兵：仅账号数传 `data?.credentials.length`，其余传 `undefined` → 数据未加载时不渲染数字（无误导性 `0`）
  - CR（SHARDS=1，0 Critical / 0 High / 3 Medium / 3 Low）中 1 条 Medium 经证据**驳回**：`font-[450]` 不会降级 —— fontTools 验证两个 `plexsans-*.woff2` 均含 `fvar` 且 `wght 100.0–700.0`，Tailwind 探针实测生成 `.font-\[450\]{font-weight:450}`
  - 已采纳 4 条：① 恢复收起态激活竖条（旧 `boxShadow` 无 `sidebarCollapsed` 判断，设计稿 `.shell.is-collapsed .nav-item{margin:0 8px}` + `::before{left:-8px}` 同样显示）② 删孤立 PNG ③ `leading-tight`(1.25) → `leading-[1.2]` 对齐设计稿 ④ 为 `-left-2` ↔ `<nav> px-2` 的数值耦合加注释
- spec.md 引用：需求「侧栏外壳」场景 1–3

### T7 — 侧栏身份区与页脚
- [x] 新增身份区：28px 头像 + `admin` + 角色文案 + 退出按钮（hover 转 danger 色），退出复用现有登出逻辑
- [x] 页脚：6px 运行状态点（3px 光环）+ 版本号 + 主题切换按钮；`useServerInfo` 失败或加载中时状态点转灰且隐藏版本号
- 验收：点击退出行为与改动前一致；断开后端后状态点转灰
- 实施记录：
  - 身份区（设计稿 `.side-user`）：28px `rounded-[8px]` 头像（`bg-surface-3` + `border-hairline-2` + 11.5px/700）+ `admin` / `dashboard.adminRole` 双行 + 退出按钮 `hover:bg-danger-soft hover:text-danger`；退出按钮由品牌区迁入，`handleLogout` 未改动
  - 名称以模块级 `ADMIN_NAME = 'admin'` 承载（后端仅单一管理员口令，无用户名概念），头像首字母由该常量派生以防漂移，并标 `aria-hidden`（与紧邻的 `admin` 文本重复）
  - 页脚（设计稿 `.side-foot`）：`ring-[3px]` 光环状态点 + 版本链接 + 主题切换；按设计稿去掉原 `<Github>` 图标（品牌区已链向同一仓库），同步删除失效 import
  - 新增 i18n 键 `dashboard.{adminRole,serviceRunning,serviceUnknown}`（zh/en 同步）—— 身份区与状态点即时渲染，不推迟到 T20，避免中间态暴露 key 原文
  - CR（SHARDS=1，0 Critical / 0 High / 3 Medium）三条全修：① 版本号叠加 `serverHealthy` 条件，加载/失败时不再显示 `v...`；② `useServerInfo` 加 `refetchInterval: 30_000` —— 经核实 `main.tsx:12-13` 全局为 `staleTime: 5000, refetchOnWindowFocus: false`，原先页面停留期间后端断开**无任何重取触发点**，验收条件实际不成立（该 hook 仅 `dashboard.tsx:148` 一处消费，影响面可控）；③ 头像补 `aria-hidden`
- spec.md 引用：需求「侧栏外壳」场景 1、7、8

### T8 — 侧栏收起态
- [x] 64px 收起态：隐藏品牌文字 / 导航文案 / 计数 / 身份文字 / 版本号，图标居中；分组标题降级为水平分隔线
- [x] 收起态导航图标 hover 显示右侧 tooltip（用已在依赖中的 `@radix-ui/react-tooltip`，需新建 `src/components/ui/tooltip.tsx` 包装）
- [x] `localStorage['sidebar-collapsed']` 行为保持不变
- 验收：收起/展开切换无布局跳动；刷新后状态保持；tooltip 正常浮出
- 实施记录：
  - 分组标题按设计稿 `.shell.is-collapsed .nav-group{margin:9px 14px;height:1px;background:var(--hairline-2)}` + `:first-child{display:none}` 降级为 `role="separator"` 分隔线，`groupIndex > 0 &&` 抑制首组（布尔短路，`false` 被 React 安全跳过渲染）
  - `fullLabel = count === undefined ? label : \`${label} · ${count}\`` 同时供 `aria-label` 与 tooltip 消费（对齐设计稿注释「aria-label 与 data-tip 同源」），收起态隐藏文案后可访问名称仍完整
  - 新建 `src/components/ui/tooltip.tsx`：Radix 包装，`bg-ink text-sidebar text-[11px] font-medium px-2 py-1 rounded-[5px] shadow-panel`、`sideOffset = 10`；**不用 `animate-*`** —— `tailwindcss-animate` 未安装（`plugins: []`），`dialog.tsx` 现有两处 `animate-in` 属既有死类名，超出本次范围不动
  - 移除按钮上的原生 `title`：Radix Tooltip 响应 `focus`/`blur`，键盘可达性优于原生 `title`；`localStorage['sidebar-collapsed']` 与 `w-16`/`ml-16` 几何未触碰
  - CR（SHARDS=1，0 Critical / 0 High / 2 Medium / 2 Low）：重点核查 7 项全通过（`asChild` 走 Slot 合并不覆盖 `aria-label`/`aria-current`/`onClick`；`Portal` 挂 body 不受 `<nav> overflow-y-auto` 裁剪，`z-50` > `aside z-10`）。1 条 Medium 经证据**驳回**：称暗色下 tooltip「浅底深字」违背设计意图 —— 设计稿 `option-a-console.html:170` 正是 `background:var(--text);color:var(--sidebar)`，其 `[data-theme="dark"]` 中 `--text:#ECEEF0`、`--sidebar:#111316`，设计稿自身同样反转，实现逐字等价。另 1 条 Medium（补 `aria-orientation`）为 `role="separator"` 默认值、1 条 Low（拆 `key`）需复制整块 button JSX，均按最小改动原则不采纳
  - 已采纳 1 条 Low：`TooltipProvider` 子树缩进对齐（纯空白，`tsc --noEmit` 通过；规模为「极小/纯格式」，按 00-change-gate Step 4 不升第 2 轮 CR）
- spec.md 引用：需求「侧栏外壳」场景 4–6

### T9 — 页头
- [x] 账号管理页头：面包屑「主要 / 账号管理」+ 19px 标题 + 11.5px 副标题；右侧「自动刷新 30s」ok 色标签（带脉冲点）+「文档」次级按钮
- 验收：标签文案中的 30s 与 `useCredentials` 的 `refetchInterval` 实际值一致
- 实施记录：
  - 页头对齐设计稿 `.head`（`items-end gap-4`）/`.crumb`（11px + 末级 `text-ink-2 font-medium`）/`h1`（19px/600/`leading-[1.2]`/`tracking-[-.02em]`）/`.head-note`（11.5px 同基线）
  - 验收条件以常量落地：`use-credentials.ts` 新增 export `CREDENTIALS_REFETCH_INTERVAL_MS = 30_000`，`useCredentials` 与页头文案同源消费，杜绝文案漂移
  - 「自动刷新」标签用设计稿 `.tag.ok`（`bg-ok-soft`/`border-ok-line`/`ring-[2.5px]` pip）+ `animate-pulse`（属 Tailwind core，不依赖未安装的 `tailwindcss-animate`）
  - 「文档」按 `.btn-ghost` 实现为外链（GitHub README），`group-hover` 联动图标色；新增 i18n 键 `dashboard.{autoRefreshTag,docs}`（zh/en 同步）
  - CR（SHARDS=1，0 Critical / 0 High / 4 Medium+Low / 1 重构）修 3 条：面包屑标 `aria-hidden`（末级与 h1 同文案，避免读屏连读）、容器补 `flex-wrap` + 副标题 `min-w-0 truncate`、`seconds` 包 `Math.round`；未采纳 2 条：单独给文档外链补 `focus-visible` 会与全站既有惯例分裂（CR 亦确认非本次回归）、抽 `<ExternalLink>` 仅 2 处消费且类名完全不同
- spec.md 引用：需求「页头与指标条」场景 1

### T10 — 指标条组件（账号池 + 全局积分）
- [x] 新建 `admin-ui/src/components/account-metrics.tsx`：单卡 4 列栅格 + 列间竖 hairline；实现前两段
- [x] ① 账号池：总数主值 + 「N 启用 · N 禁用 · N 项异常」脚注，异常数 > 0 时染 warn（原文误写 danger，已按设计稿 markup `style="color:var(--warn)"` 更正，用户裁定）
- [x] ② 全局剩余积分：已查询余额求和（1 位小数）+ 「已覆盖 N / M 个账号」；全未查询时主值 `—` 并提示点击查询信息；不渲染环比徽章
- 验收：未查询状态与已查询状态两种视图均正确
- 实施记录：
  - `.metrics` → `<section className="grid shrink-0 grid-cols-4 ... rounded-[11px] border-hairline bg-surface shadow-hair">`；`.metric + .metric::before` 用 `before:` 伪元素 + `first:before:hidden` 复刻，无需调用方传首段标记
  - 抽出私有 `Metric`（段外壳 + `.m-label`）/ `MetricValue`（`.m-row` + `.m-val.mono` + `.m-unit`，含 `tabular-nums`）/ `FootSep`（`.m-foot .sep`），T11 的③④直接复用
  - 判空口径统一为 `Number.isFinite(creditsTotal)`，一次覆盖 `null`/`NaN`/`Infinity`，满足「不出现 NaN / 0.0」
  - 异常数口径（`error` + `warning`，禁用与待查询不计入）由 T11 的 dashboard 派生逻辑计算后经 `abnormalCount` 传入，组件不做判定
  - 新增 i18n 键 `credentials.{metricPoolLabel,metricPoolUnit,metricEnabled,metricDisabled,metricAbnormal,metricCreditsLabel,metricCreditsCovered,metricCreditsHint}`（zh/en 同步）
  - **本任务只交付组件，未挂载**（用户裁定：T11 补齐③④后一次性替换旧的 4 个 `<Card>`）；故验收标准的两种视图**视觉**验证顺延至 T11，本轮仅 `tsc --noEmit` 类型验证通过
  - CR（SHARDS=1，0 Critical / 2 High / 2 Medium / 4 Low）修 4 条：`Number.isFinite` 兜底、补 `shrink-0`（设计稿 `.metrics{flex:none}`）、`first` prop 改 `first:before:hidden`、补 `tabular-nums`（CR 疑 `font-mono` 越界，实为设计稿 `.m-val mono` 且漏译了 `.mono` 的 `font-variant-numeric:tabular-nums`）
  - 未采纳 1 条 High：补单元测试 —— `admin-ui/package.json` scripts 仅 `dev`/`build`/`preview`，无 vitest/jest/jsdom/testing-library，零测试基建；引入测试框架属独立项目级变更，不在本提案 In Scope
  - 未采纳 2 条 Low：`<b>` 改 `<span>`（设计稿 markup 用 `<b>`，T9 面包屑已沿用，一致性成本 > 语义纯度收益）、props 按段分组（当前 6 个扁平在合理规模，T11 加参数时再评估，避免过度设计）
- spec.md 引用：需求「页头与指标条」场景 2–4

### T11 — 指标条（平均剩余环形图 + 今日调用 sparkline）
- [x] ③ 平均剩余：已查询启用账号百分比均值 + 42px 环形图（r=17 / stroke-width 4.5 / dasharray 106.8），按 60/30 阈值切 ok/warn/danger 色；脚注显示最低账号；无数据时 `—` + 空轨 + 隐藏脚注
- [x] ④ 今日调用：`useDailyUsage` 今日 `totalRequests`（千分位）+ 74×26 sparkline（最近 7 天，渐变填充）；有昨日数据时渲染环比徽章，无则不渲染
- [x] ④ 脚注降级：显示账号池**累计**失败数与累计失败率，文案明确标注「累计」；为 0 时用次要色
- 验收：单日数据时无环比徽章且不报错；7 天数据时折线单调性与数值一致
- 实施记录：
  - 新增私有 `Ring`（`.ring`，`stroke-track` 空轨 + 进度圆，`percent === null` 时只画空轨）/ `Sparkline`（`.spark`，`useId()` 隔离渐变 id，`span === 0` 走中线避免除零，`values.length < 2` 自保返回 null）/ `Delta`（`.delta.up/.flat/.dn`）/ `MetricAside`（`.m-aside`）
  - `Metric` 增加可选 `onClick`：传入时渲染为 `<button type="button">`（共享同一 shell class 串 + `hover:bg-surface-3` + `focus-visible:ring-brand`）
  - **用户裁定 1**：④ 段保留点击进入日用量（设计稿 `.metric` 无 hover 态）—— 否则 `dailyFromSidebar` 退化为恒 true，会废掉日用量页的返回按钮
  - **用户裁定 2**：③「启用中最低」百分数按 60/30 阈值着色（设计稿硬编码 `var(--warn)`），与环形图同源
  - dashboard 派生（零后端改动）：`abnormalCount` = `deriveAccountState ∈ {error, warning}`；`enabledRemaining` 仅取未禁用 + 已查余额 + `usageLimit > 0`，percent 在计算侧 clamp 到 0–100；`todayRequests` 用 `dailyUsageData ? … ?? 0 : null` 区分「未加载」与「今日 0 次」；环比仅在昨日 `totalRequests > 0` 时计算；趋势 `.slice().sort()` 复制后排序，不污染 Query 缓存；失败脚注降级为账号池累计值（后端无按天失败数）
  - 挂载：删除旧的 4 个统计 `<Card>`，随之删除 `useCountUp`、`CreditsProgressRing`、`liveCreditsCapacity` 状态与其 `capacity` 累加、`useId` / `CardHeader` / `CardTitle` 导入；`todayLocal` 内联 IIFE 提为模块级 `formatLocalDate()` 供今日/昨日共用
  - 新增 i18n 键 `credentials.{metricAvgLabel,metricAvgLowest,metricCallsLabel,metricFailPrefix,metricFailRatePrefix}`（zh/en 同步）
  - 孤儿键 `dashboard.{statTotal,statAvailable,statGlobalCredits,statQueried,statTodayUsage,creditsRingLabel}` 有意留待 T20「i18n 收口」统一清理
  - CR（SHARDS=1，0 Critical / 0 High / 2 Medium / 5 Low，Verdict PASS）修 5 条：④ 脚注补 `truncate` 且留白 `pr-[86px]` → `pr-[92px]`（sparkline 实占 88px）、`enabledCount` 口径澄清（i18n「启用」→「未禁用」，前端 `!disabled` 而非后端 `available`）、percent 计算侧 clamp、`Sparkline` 自保 `length < 2`、`Ring` 收紧为只接 `percent` 内部自算 tone（消除「两 prop 必须协同为 null」的隐式契约）
  - 未采纳 2 条 Low：`useMemo` 包裹派生计算（n < 50 的 O(n)，CR 自身也判为可选）、避免「均值绿环 + 最低值红字」（`avg ≥ lowest` 恒成立，绿环红字正是「整体尚可但有个例外」的预期表达）
  - 环比在上午必然显示大幅下跌 —— CR 与本轮均判定为「仅用现有数据」的固有局限，非缺陷，记为已知局限
- spec.md 引用：需求「页头与指标条」场景 5–8

### T12 — 操作条重排
- [x] 操作条改为：刷新账号列表 / 查询信息 / 竖分隔线 / 清除已禁用（danger 描边）/ 竖分隔线 / KAM 导入 / 批量导入 / `ml-auto` 主按钮「添加账号」
- [x] 「查询信息」进行中时禁用 + 动效：复用既有 `queryingInfo` 状态（**不是** `loadingBalanceIds`，后者是行级余额单元格的加载态），并保留现有的 `queryInfoProgress` 进度文案
- [x] 保留条件性显隐：「查询信息」与「清除已禁用」仅在 `credentials.length > 0` 时渲染；「清除已禁用」在 `disabledCredentialCount === 0` 时禁用并 title 提示
- [x] 保留验活进度浮动入口：`verifying && !verifyDialogOpen` 时在操作条显示带旋转图标的 `N / M` 进度按钮，点击重开进度对话框
- 验收：对照 proposal.md 页面级能力表第 15、16 项逐条核对显隐与禁用条件；6 个常驻按钮的 onClick 与改动前一一对应；空列表与「无禁用账号」两种场景各核对一次
- 实施记录：
  - 新增模块级类串常量 `ACTION_BTN_BASE`（31px 高 / 7px 圆角 / `[&_svg]:size-[15px]`）+ `ACTION_BTN`（`.btn`）/ `ACTION_BTN_DANGER`（`.btn-danger`）/ `ACTION_BTN_PRIMARY`（`.btn-primary`）/ `ACTION_VDIV`（`.vdiv`，19px 高）；全为静态模板拼接，Tailwind JIT 可扫到
  - 操作条由 shadcn `<Button>` 换为原生 `<button type="button">`：图标尺寸交给容器 `[&_svg]:size-[15px]` 统一，删掉逐个图标上的 `h-4 w-4 sm:mr-2`，间距由 `gap-1.5` 接管
  - 两处竖分隔线与「清除已禁用」共用同一个 `allCredentials.length > 0` 条件（原本是两个独立 `data?.credentials && …` 判断），空列表时整块消失，不留孤立/紧贴分隔线
  - 验活进度浮动入口设计稿无对应位，为保留既有能力挂在 `ml-auto` 主按钮左侧
  - 批量选择操作（已选 N 项 / 取消选择 / 批量验活 / 批量恢复 / 批量删除）拆为独立一行、样式暂留 shadcn —— 设计稿把它归入面板页脚 `.panel-foot`，由 T18 迁移
  - 指标条容器 `mb-6` → `mb-[15px]`，对齐设计稿 `.actionbar{padding:15px 0 0}`
  - CR（SHARDS=1，0 Critical / 0 High / 2 Medium / 1 Low，Verdict PASS）修 1 条：6 个按钮补 `aria-label`（窄屏下文字被 `hidden`，图标按钮此前无可访问名称 —— 改动前的 shadcn 版本同样缺失，本次顺带修掉）；「查询信息」的 `aria-label` 固定用 `queryInfo` 不跟进度文案，避免读屏被进度递增反复打断
  - 已知问题（Medium，未修）：主操作条 31px 原生按钮与批量行 shadcn `size="sm"` 按钮规格不一致，勾选账号时两种风格并排 —— 已定为 T18 迁移页脚时统一
- spec.md 引用：需求「操作条」场景「操作按钮常驻」「查询信息进行中」「空列表时的操作条」「无可清除账号」「验活进度浮动入口」

### T13 — 工具栏组件（搜索 + 分段筛选）
- [x] 新建 `admin-ui/src/components/account-toolbar.tsx`：236px 搜索框（占位文案写明可搜 昵称 / 邮箱 / ID，含 `⌘K` kbd 提示 + 清除按钮）、5 段状态筛选器（每段带彩色圆点 + 计数）、右侧「更新于 X 前」
- [x] `⌘K` / `Ctrl+K` 全局监听：焦点不在输入框时 `preventDefault` 并聚焦搜索框，卸载时移除监听
- [x] 分段计数基于「搜索过滤后」集合，通过 `deriveAccountState` 计算
- 验收：⌘K 在输入框内不劫持；计数随搜索词实时变化
- 实施记录：
  - 组件本轮**只建不挂载**（同 T10/T11 的节奏），接入 dashboard 的 `searchQuery` / `statusFilter` / `counts` / `dataUpdatedAt` 属 T14
  - 新增导出类型 `AccountStatusFilter = 'all' | 'healthy' | 'abnormal' | 'disabled' | 'pending'`：设计稿把 `AccountState` 的 `error + warning` 合并为「异常」一段（与 T11 `abnormalCount` 同源），故未直接复用 `AccountState`；未动 `account-state.ts`
  - 分段计数走 `counts: Record<AccountStatusFilter, number>` 契约传入（编译期保证 5 段齐全），实际按 `deriveAccountState` 的计算落在 T14
  - 「更新于 X 前」接 TanStack Query 的 `dataUpdatedAt`，`=== 0` 时整段不渲染；30s `setInterval` 心跳强制重渲染，避免文案冻结在首次渲染时刻
  - `⌘K` 监听挂在 `window`，`INPUT` / `TEXTAREA` / `contentEditable` 获焦时不劫持，卸载时 `removeEventListener`
  - 搜索框有值时 kbd 提示位换成清除按钮（设计稿只有 kbd，清除按钮是任务要求的增量）
  - 复用既有相对时间 i18n 键 `credentials.{justNow,secondsAgo,minutesAgo,hoursAgo,daysAgo}`；新增 9 键 `credentials.{searchPlaceholder,searchClear,filterGroupLabel,filterAll,filterHealthy,filterAbnormal,filterDisabled,filterPending,updatedAgo}`（zh/en 同步）
  - `formatAgo` 与 `credential-card.tsx:44` 的 `formatLastUsed` 逻辑重复，未提取到 `lib/` —— 该文件 T19 整体删除后此处即唯一实现
  - CR（SHARDS=1，0 Critical / 1 High / 3 Medium / 6 Low）修 4 条：input 补 `aria-label`（`<label>` 内只有装饰图标 + `aria-hidden` 的 kbd，可访问名称为空字符串，AT 不会回退读 `placeholder`，属 WCAG 4.1.2 违规）；`⌘K` 判定改 `e.key.toLowerCase()`（CapsLock / Shift+⌘K 时 `e.key` 为 `'K'` 会静默失效）；分段容器 `role="group"` + `aria-pressed` → `role="radiogroup"` + `role="radio"` + `aria-checked`（互斥单选语义，未做 roving tabindex，5 按钮均可 Tab）；平台判定优先 `navigator.userAgentData?.platform` 并改为大小写不敏感（UA-CH 返回 `'macOS'`，旧 API 返回 `'MacIntel'`）
  - 未采纳 6 条 Low：心跳在 `dataUpdatedAt === 0` 时空转（组件未挂载，留待 T14 视实际接入决定）、清除按钮 focus-visible 样式、`flex:none` 只写 `shrink-0`（渲染等价）、input 用 `text-ink` 而设计稿字面继承 `text-3`（有意增强，占位符浅、输入深更易读）、`"Updated Just now"` 大小写生硬（继承既有 `lastCallLabel` 措辞模式）、`formatAgo` 重复（同上，T19 后消失）
- spec.md 引用：需求「搜索与筛选」场景 1–4、7

### T14 — dashboard 派生管道
- [x] `dashboard.tsx` 新增 `searchQuery` / `statusFilter` 状态；`useMemo` 依次派生 `filtered`（搜索：昵称 / 邮箱 / 账号 ID，不区分大小写）→ `stateCounts` → `visible`（状态筛选），再由 `visible` 切片得 `paged`（切片与页码钳制是普通 `const`，未记忆化；`sortKey` / `sortDir` 顺延至 T15，见实施记录）
- [x] `itemsPerPage` 12 → 50
- [x] 搜索 / 筛选任一变化时 `setCurrentPage(1)`（排序变化的重置随 T15 接入）
- [x] 剩余额度排序按 design.md 决策 4 分区：已查询组按数值排序，未查询组恒定末位且内部按 ID 升序（本轮只落到 `sortCredentials()` 纯函数，尚无 UI 排序入口，端到端验收随 T15）
- 验收：在第 3 页输入搜索词后自动回到第 1 页 ✅；额度升序时未查询行仍在最后 —— 纯函数逻辑已落地，**UI 验收顺延至 T15**（本轮无排序入口，不可手动复现）
- spec.md 引用：需求「搜索与筛选」场景 1、4；「账号表格」场景「剩余额度列排序」「排序重置分页」；「面板脚与分页」场景 1
- 实施记录：
  - 新增 `searchQuery` / `statusFilter` 状态与 `useMemo` 管道 `filtered` → `stateCounts` → `visible` → `paged`；`itemsPerPage` 12 → 50；`useCredentials()` 解构出 `dataUpdatedAt`；在操作条下方挂载 T13 的 `<AccountToolbar>`
  - **`sortKey` / `sortDir` 状态顺延至 T15**：`tsconfig.json` 开启 `noUnusedLocals`，本轮声明 `setSortKey` / `setSortDir` 却无消费方（可排序表头属 T15）会直接编译失败（已实测 TS6133）。改为本轮先在 `lib/account-state.ts` 落地纯函数 `sortCredentials()`（导出即不触发 TS6133），T15 声明状态并接表头时调用 —— 与 T10/T11/T13「只建不挂载」同一节奏
  - 决策 4 排序器签名 `sortCredentials(list, sortKey, dir, remainingOf)`：`remainingOf: (id) => number | null` 而非直接传 `balanceMap`，避免 lib 层耦合 `BalanceResponse`；返回新数组不修改入参；`account` 键按 `accountLabel()` 本地化比较（中文按拼音）+ 同名回退 ID 升序，`remaining` 键按已查询 / 未查询分区，未查询组恒定末位并按 ID 升序
  - 新增 `accountLabel()`（昵称 → 邮箱 → #ID）统一显示名口径，指标条 `enabledRemaining` 的内联表达式一并收敛到该函数
  - 分段计数基于**搜索后**集合（`filtered`），与指标条 `abnormalCount` 的**全量**口径有意不同：指标条描述账号池全局健康度，工具栏计数描述当前搜索结果的分布
  - 空态判断 `data?.credentials.length === 0` → `allCredentials.length === 0`；页码文案的 `count` 由全量总数改为 `visible.length`（筛选后总数）
  - CR（SHARDS=1，0 Critical / 1 High / 0 Medium / 0 Low，Verdict FAIL）修 1 条 High：`visible`/`totalPages` 会随余额查询实时收缩（`pending` → `healthy`），而重置 effect 不监听 `balanceMap`，导致停在第 2 页时 `paged` 为空且 `totalPages > 1` 转假使分页控件整体隐藏、用户困在无按钮的空白页。改为渲染期钳制 `const page = Math.min(currentPage, totalPages)`，`startIndex` 与分页控件全部改用 `page`，同时消除 effect 异步生效的滞后窗口
  - CR 另 4 条非计分观察未采纳：`localeCompare(...) * factor || fallback` 的 `-0` 不构成短路陷阱（`Boolean(-0) === false` 仍会落入兜底）、`stateCounts` 与 `visible` 各调一次 `deriveAccountState` 的 O(2n) 在百级规模可接受、搜索不匹配 `#12` 形式（`accountLabel` 的展示形态，非字段值）、筛选后 0 结果的空状态与工具栏间距归 T15

### T15 — 表格外壳组件
- [x] `dashboard.tsx` 补 `sortKey` / `sortDir` 状态（T14 因 `noUnusedLocals` 顺延）：接 `sortCredentials()` 派生 `sorted`，插在 `visible` 与 `paged` 之间；排序变化时 `setCurrentPage(1)`
- [x] 新建 `admin-ui/src/components/account-table.tsx`：面板容器（11px 圆角 + `shadow-panel`）+ `colgroup` 固定列宽（34/46/auto/92/104/172/112/60/96/150）+ 表体内滚 + `sticky` 表头
- [x] 表头：全选复选框 + 9 个列标题；「账号」「剩余额度」可点击排序并显示方向指示；数字列右对齐
- [x] 空状态：搜索/筛选无结果时表体显示提示 + 清除筛选入口
- 验收：滚动时表头固定 ✅；点击排序表头方向指示正确切换 ✅
- spec.md 引用：需求「账号表格」场景 1–4；「搜索与筛选」场景 5
- 实施记录：
  - `account-table.tsx`（约 165 行）：`.panel`（`rounded-[11px]` + `border-hairline` + `shadow-panel`）→ `.tscroll` 滚动容器 → `border-separate` 表格；`COLUMN_WIDTHS` 驱动 `colgroup`（账号列 `undefined` = auto），导出 `ACCOUNT_TABLE_COLUMNS`（=10）供空状态与过渡行 `colSpan` 复用
  - 表头 `sticky top-0 z-[2]` + `bg-surface-2`，10.5px / 600 / `tracking-[0.07em]` 大写 + `text-ink-3`，数字列 `text-right`；「账号」「剩余额度」渲染为按钮，激活列显示 `ArrowUp`/`ArrowDown`（`text-brand`）并置 `aria-sort`，非激活列显示中性 `ChevronsUpDown`
  - **默认 `sortKey = null`**（保持后端返回顺序）：设计稿默认排序图标为中性双向态，挂载即按昵称重排会打乱既有列表顺序，故引入 null 表示未排序；`handleSort` 同列切升降序、换列从升序开始
  - **行渲染取「单元格适配器过渡」路线**（用户确认）：本轮行为 `<tr><td colSpan={10}>` 包裹既有 `<CredentialCard>`，真实单元格自 T16 起逐列替换；`tbody` 上挂 `[&_tr:last-child>td]:border-b-0` 承接设计稿末行去边框，行组件无需自理
  - **内滚高度用 `max-h-[70vh]` 临时收口**：设计稿靠整页 flex 撑满（`.app` 网格 + `min-h-0`），页面外壳重构不在本次范围；组件已同时带 `flex-1 min-h-0`，后续外壳落地后删掉这一条即可
  - 全选作用域为当前页（`allPagedSelected` / `toggleSelectPage`），跨页已选项保留；批量操作汇总仍在表格上方，随 T18 迁入面板页脚
  - i18n：zh/en 各新增 12 键（`colId`…`colOps` 9 个列名 + `selectPage` / `emptyFiltered` / `clearFilters`）
  - CR（PASS，0 严重 / 3 改进）：修 1 条 High —— 表头全选补 `indeterminate` 半选态。shadcn `Checkbox` 封装把指示器图标固定为 `Check`，半选显示对勾会误导，故表头改为局部 `SelectAllCheckbox` 直接用 `@radix-ui/react-checkbox` Primitive（全选对勾 / 半选 `Minus` 横线），并由 dashboard 传入 `somePagedSelected`
  - CR 另 2 条 Medium 经用户决定记为已知限制：① `COLUMN_WIDTHS`(10) 与 `COLUMNS`(9) 靠下标隐式对应，改列错位编译期不报错 → T16 加列时合并为单一列定义数组；② 按「剩余额度」排序时批量查询余额，每个结果到达都触发整表重排、行位置跳动 → 随 T16B 额度单元格一并处理

### T16 — 表格行组件（身份类单元格）
- [x] 新建 `admin-ui/src/components/account-row.tsx` 骨架 + 行容器（hover 背景、禁用行整行降低不透明度但操作控件保持可点击）
- [x] ID 单元格（mono `#003` 补零）
- [x] 账号单元格：26px 语义色头像 + 昵称/`账号 #ID` + `P{priority}` 徽章 + 脱敏邮箱单行截断（title 悬停显示完整值）
- [x] 账号单元格邮箱右侧微徽章：`hasProxy && proxyUrl` → `PROXY`（title 显示完整地址）；`hasProfileArn` → `ARN`
- [x] 状态标签：消费 `deriveAccountState`，5 种配色
- 验收：⏭ 视觉验收顺延至 T17（用户确认的挂载节奏：T16/T16B 只建不挂载，T17 补齐操作列后一次性替换过渡行，集中核对四种状态行与徽章）
- spec.md 引用：需求「账号表格」场景「账号单元格」「代理与 ARN 标识」「状态标签映射」「禁用行降权」

**实施记录**
- 组件签名 `AccountRow({ credential, balance, selected, onToggleSelect })`：`balance !== null` 即 `hasBalance`，喂给 `deriveAccountState` 派生唯一状态，标签 / 头像配色统一取 `ACCOUNT_STATE_VISUAL[state]`
- 本轮只落 4 个 `<td>`（选择框 / ID / 账号 / 状态），共 10 列的余下 6 列属 T16B（套餐·额度·调用·RPM·最后调用）与 T17（操作）；**只建不挂载**，dashboard 仍走 `colSpan={10}` 过渡行
- 单元格基线常量 `CELL = 'border-b border-hairline px-3 py-[11px] align-middle'`，对齐设计稿 `tbody td{padding:11px 12px}`；hover 用 `hover:bg-surface-2`
- 禁用行降权用 `[&>td]:opacity-[.55]`（而非整 `<tr>` 降权），T17 时操作列 `<td>` 单独恢复不透明度即可满足「控件保持可点击且不显灰」
- 头像首字符按码点取（`[...accountLabel(item)][0]`），避免 emoji / 代理对昵称被截半
- 新增 `maskEmail()` 到 `lib/account-state.ts`：保留本地部分首字符 + 完整域名，中间打 `∗`，`＠` 用全角（对齐设计稿 `h∗∗∗∗∗∗∗∗＠gmail.com`）；仅为紧凑展示，`title` 仍给完整邮箱（与旧卡片明文展示等价，无泄露升级）
- `PROXY` / `ARN` 微徽章是设计稿没有的自定降级项，用于保住旧卡片的这两处信息；`PROXY` 用 brand 描边并把完整地址放进 `title`
- T15 的局部 `SelectAllCheckbox` 泛化为导出的 `AccountCheckbox(checked, indeterminate?, disabled?)`，表头传 `indeterminate={someSelected}`、行内不传，两处共用同一 `.cb` 规格（14px / 4px 圆角 / 半选 `Minus`）
- i18n：zh/en 各新增 7 键 —— `state{Healthy,Warning,Error,Pending,Disabled}`（`ACCOUNT_STATE_VISUAL.labelKey` 自 T5 起就引用，原计划 T20 收口，因本轮状态标签必须消费而提前落地，T20 只剩孤儿键清理）+ `selectRow` + `profileArnBadge`
- CR（PASS，0 严重 / 2 改进，两条均已修）：① `maskEmail` 首字符改按码点取，与头像口径一致；② 行复选框 `aria-label` 的插值由 `displayName` 改为 `accountLabel`，避免无昵称账号读成「选择账号 账号 #12」
- T15 遗留的 CR Medium ①（`COLUMN_WIDTHS` 与 `COLUMNS` 下标隐式对应）本轮未合并：本轮没有增删列，合并动作留到 T16B/T17 真正改列结构时一并处理

### T16B — 表格行组件（数值类单元格）
- [x] 套餐单元格：`subscriptionTitle` 大写 / 未查询 `—`（沿用既有 `getSubscriptionColor` 的分级配色思路）
- [x] 剩余额度单元格：已查询显示 `剩余 / 上限` + 百分比（60/30 阈值着色）+ 4px 进度条；未查询显示「尚未查询」+ `—` + 空轨；`loadingBalance` 时显示行内加载态
- [x] 调用列三段：成功数 / 失败数（> 0 染 danger，**可点击 → `onViewFailureLog`**）/ 限流数（> 0 时显示，warn 色，**可点击 → `onViewThrottleLog`**）
- [x] RPM（0 用次要色）、最后调用（相对时间 + 绝对时间小字，复用既有 `formatLastUsed` 逻辑）
- 验收：⏭ 视觉验收顺延至 T17（同 T16，只建不挂载；三种额度形态与两处计数点击在 T17 挂载后一次性核对）
- spec.md 引用：需求「账号表格」场景「额度单元格」×2、「调用与 RPM」「计数点击跳转日志」

**实施记录**
- 组件签名扩展为 `AccountRow({ credential, balance, loadingBalance, rpm, selected, onToggleSelect, onViewFailureLog, onViewThrottleLog })`；至此 9 个 `<td>` 齐备，第 10 列（操作）属 T17
- 额度语义：进度条宽度取**剩余**百分比（对齐设计稿 93% 剩余 → 93% 填充），值为 `clamp(0, 100 - usagePercentage, 100)` 防脏数据撑出轨道；阈值 60/30（design.md 决策，比旧卡片 50/20 更早预警）
- 三态降级：`balance` 有值 → 绝对值 + 百分比 + 彩条；`loadingBalance` → 「查询中…」+ `—` + 空轨 `animate-pulse`（`tailwindcss-animate` 未装，用核心工具类）；未查询 → 「尚未查询」+ `—` + 空轨
- 套餐徽章取设计稿中性 chip 几何（`rounded-[5px]` + `bg-surface-3` + `border-hairline`），文字色沿用既有 `getSubscriptionColor` 分级，避免丢掉 ENTERPRISE/PRO+ 的既有语义
- 调用列在设计稿两段之外加了**限流第三段**（warn 色，> 0 才渲染），保住旧卡片的限流日志入口；失败数与限流数 > 0 时渲染为 `<button>` 并带 `focus-visible` 描边，= 0 时降级为不可点击的 `<span>`
- `RPM_DANGER = 10`：设计稿把 12 染 danger、1–3 用 accent，取 10 作分界；0 用 `text-ink-3`
- 绝对时间小字 `formatAbsolute`：当天 `今天 HH:mm` / 同年 `MM-DD HH:mm` / 跨年退化为 `YYYY-MM-DD`（避免把去年同月日误读成近期）
- **顺带修掉 T15 遗留的 CR Medium ②**（排序期重排跳动）：dashboard 新增 `remainingSnapshot` state，仅在 `loadingBalanceIds.size === 0` 时由 effect 刷新，批量查余额期间行序冻结、全部返回后一次性重排；不用 `useMemo` 内写 `useRef` 是为避免渲染期副作用
- i18n：zh/en 各新增 3 键（`balanceNotQueried` / `balanceLoading` / `today`）
- CR（PASS，0 严重 / 0 改进 / 2 条 Low，用户选择全部跳过）：① 额度单元格三重判空可合并为单一派生对象 —— 直接照搬会因 TS 不反推 `balance` 收窄而编译失败，需连带改写 6 处引用，净收益低故不动；② `formatLastUsed` 与 `credential-card.tsx` 重复 —— 由 T19 删除旧卡片时自然消解

### T17 — 表格行操作
- [x] 32×18 启停开关（调用现有启停接口，成功后状态标签与降权同步）
- [x] 4 个图标按钮：额度详情（`onViewBalance`）/ 账号详情·调用记录（`onViewDetail`，tooltip 沿用 `credentials.viewLog`）/ 编辑 / 更多，各带 tooltip
- [x] 更多菜单（`@radix-ui/react-dropdown-menu`，需新建 `src/components/ui/dropdown-menu.tsx` 包装）：重新查询余额 / 失败日志 / 限流日志 / 重置失败计数（`failureCount === 0` 时禁用）/ 删除账号（danger，**`disabled === false` 时禁用并提示需先禁用账号**）
- [x] 删除二次确认对话框与 `EditCredentialDialog` 调用迁入本组件，行内自持开关状态；确认按钮同样保留 `!credential.disabled` 时禁用的约束
- 验收：✅ 11 项能力表逐项可达且禁用条件一致（逐项结论见实施记录末条）；⏳ 双主题 × 五种状态行 × 三种额度形态的视觉核对（含 T16 / T16B 顺延项）待用户确认
- spec.md 引用：需求「账号表格」场景「行内操作」「更多菜单」「重置失败计数的可用条件」「删除的前置约束」「启停开关」；「账号列表展现形态」场景 1–2

**实施记录**
- 拆两轮落地：T17-a（新建 `ui/dropdown-menu.tsx` 57 行 + 操作列 + 两处保真修正）→ CR → T17-b（dashboard 挂载 + `handleRefetchBalance` + 编辑对话框补优先级字段）→ CR
- `ui/dropdown-menu.tsx` 只包 Root / Trigger / Content / Item / Separator，`align='end'` 与 `sideOffset={6}` 为默认值，`Item` 加 `danger` 布尔属性区分破坏性操作；不写 `animate-*`（`tailwindcss-animate` 未安装，写了也是空操作）
- 开关不复用 `ui/switch.tsx`（`h-6 w-11` + `bg-blue-500/80`，尺寸与配色都不符设计稿）：行内用 `SwitchPrimitive`，容器 `flex items-center` 做竖向居中，Thumb `translate-x-[2px]` → `translate-x-[18px]`（32 − 2 − 12）复现设计稿 `left:2px` / `right:2px`
- 两处设计稿保真修正：① 禁用行降权由 `[&>td]:opacity-[.55]` 改为 `[&>td:not(:last-child)]:opacity-[.55]` —— 父级 `opacity` 无法被子元素撤销，否则操作控件会跟着变灰，与「整行降权但操作可用」冲突；② RPM 对「已禁用」与「从未调用」行显示 `—`（对齐设计稿原型行 968 / 1007 / 1046，数字对这两类行无意义）
- 删除菜单项的 `title` 挂在内层 `pointer-events-auto` 的 `<span>` 上：Radix 禁用项带 `pointer-events-none`，直接挂在 Item 上则 hover 提示永不触发（T17-a CR High）；`pointer-events` 与 `opacity` 不同，**可**被子元素撤销
- 两个对话框（删除确认 + `EditCredentialDialog`）放在操作列 `<td>` 内部而非 `<tr>` 的兄弟节点：二者均经 Portal 渲染，表格 DOM 结构仍合法且无 React 嵌套告警
- 优先级修改路径迁移（能力表第 2 项）：旧卡片的行内编辑取消，改由编辑对话框承载；因 `UpdateCredentialRequest` 无 `priority` 字段，走既有独立端点 `POST /credentials/:id/priority`，与字段更新**串行** `mutateAsync`（先优先级后字段），任一端点进行中即锁表单；失败时把已成功部分一并 toast，避免用户重复设置优先级
- `handleRefetchBalance` 复用 `loadingBalanceIds` / `balanceMap`，写回后由既有两个 effect 自动重算全局积分与排序快照；防重入用 `useRef<Set<number>>` 而非 state —— state 在同一事件循环内读到的是渲染期快照，挡不住连点
- dashboard 移除 `CredentialCard` 与 `ACCOUNT_TABLE_COLUMNS` 导入，`colSpan={10}` 过渡行退场；`credential-card.tsx` 至此零引用，连同 `account-table.tsx:64` 的过渡期注释一并留给 T19 清理
- i18n：zh/en 各新增 3 键（`toggleEnabled` / `moreActions` / `refetchBalance`）；优先级 4 键（`priorityFieldLabel` / `priorityPlaceholder` / `priorityHint` / `toastPriorityInvalid`）既有，无需新增
- 两轮 CR 均 PASS：T17-a（1 High 已修 —— 禁用项 `title` 不可达；1 Medium 跳过 —— `handleDelete` 内的 `!disabled` 兜底保留作破坏性操作双保险；Low① 已修 —— 去掉多余 `as Error` 强转；Low② 跳过 —— 1px 级留白偏差）；T17-b（2 Medium 已修 —— ref 防重入、部分成功提示；Low① 已修 —— 成功文案合并展示；余 3 条 Low 为既有模式或经判定无缺陷）
- 11 项能力表核对：1 勾选 ✅ / 2 优先级 → 编辑对话框 ✅ / 3 启停开关 ✅ / 4 余额（额度详情钮 + 菜单重查）✅ / 5 调用记录 ✅ / 6 编辑 ✅ / 7 重置失败计数（`failureCount === 0` 禁用）✅ / 8 删除（菜单项禁用 + 二次确认 + 确认钮禁用，三处条件一致）✅ / 9 失败日志（调用列可点 + 菜单项）✅ / 10 限流日志（同上）✅ / 11 PROXY·ARN 徽章 ✅
- 验证：`npx tsc --noEmit` 与 `npm run build` 均通过（1837 modules，CSS 48.14 kB / JS 688.16 kB）

### T18 — 面板脚
- [x] 面板脚左区：无选中时不渲染；有选中时显示「已选 N 项」+ 批量验活 / 恢复异常 / 批量删除 / 取消选择（复用既有批量逻辑）
- [x] 「批量删除」保留 `selectedDisabledCount === 0` 时禁用 + title 提示「仅可删除已禁用账号」的现有约束
- [x] 右区：「共 N 个账号」+ 页码控件 + 「每页 50」
- [x] 全选复选框仅作用于当前页可见行；翻页保留已选集合
- 验收：✅ 第 12–14 项能力落地（左区条件渲染 / 批量删除禁用 + title / 右区三段，`totalCount === 0` 时收敛为仅计数一段）；✅ 全选与跨页选择沿用 T15 的 `toggleSelectPage` + `selectedIds`，未改动；⏳ 双主题下页脚配色与窄屏换行的视觉核对待用户确认
- 实施记录：
  - 新建 `account-panel-foot.tsx`（设计稿 `.panel-foot`：`surface-2` 底 + 上 hairline + 9/13px 内距 + 11.5px `ink-3`），两套按钮常量 `FOOT_BTN`（24px 高 ghost）与 `PAGER_BTN`（24px 方钮，当前页 `surface-3` + 加粗），一并解决 T12 遗留的 31px 原生按钮与 shadcn `size="sm"` 混用 Medium
  - `account-table.tsx` 增 `footer?: ReactNode` 插槽，渲染在 `.tscroll` 滚动区**之外**、`<section>` 之内 —— 页脚固定在面板底部不随表体滚动；表格外壳保持与业务状态解耦（CR 确认该方式优于让表格直收分页/批量 props）
  - `pageWindow()` 页码窗口最多 5 个数字钮，`start` 双向钳制；CR 逐一验证 totalPages=1/2/3/4/5/6/100 与首尾页时值域恒在 `[1, totalPages]`、长度恒为 `min(5, totalPages)`
  - 批量删除的 `title` 挂在**外层** span：原生 `disabled` button 不派发指针事件，title 必须由未禁用的父元素承接（与 T17 下拉菜单项往内层挂 title 以撤销 `pointer-events-none` 方向相反，CR 确认两处各自成立）
  - dashboard 删除表格上方批量行（27 行）与表格下方独立分页块（25 行），`<>…</>` 片段收敛为单一 `<AccountTable>`；连带清理失效导入 `Badge` 与 `RotateCcw`（`Trash2` / `CheckCircle2` 仍被主操作条与查询进行中图标使用，保留）
  - i18n zh/en 各新增 3 键：`credentials.footTotal` / `footFiltered` / `footPerPage`；复用既有 `dashboard.{selectedCount,batchVerify,batchRestore,batchDelete,deselectAll,deleteDisabledOnly}` 与 `common.{prevPage,nextPage}`
  - CR（PASS，0 严重 / 2 改进 / 4 记录）：两条 Medium 均已修 —— ① 计数文案按 `isFiltered` 切「筛选出 N 个账号」，避免筛选态被读成账号总量；② `totalCount === 0` 时不渲染页码区与「每页 50」，与表体空状态不打架
  - 未修 4 条 Low：③ 批量 handler 以 `() => void` fire-and-forget 传入、无进行中禁用，连点会重复请求（旧批量行等价行为，非本轮引入，登记技术债）；④ `dashboard.pageInfo` 成孤儿键（T20 清理）；⑤ 禁用按钮外层 span 无 `tabIndex`/`aria-describedby`，键盘用户读不到禁用原因（与 T17 同一既有权衡）；⑥ 窄屏 `flex-wrap` 后右区独占一行（判定可接受）
- 验证：`npx tsc --noEmit` 与 `npm run build` 均通过（1838 modules，CSS 48.32 kB / JS 690.29 kB）
- spec.md 引用：需求「面板脚与分页」场景「默认分页」「选中态批量操作」「批量删除的前置约束」「全选范围」「翻页保留选择」

### T19 — 删除 credential-card.tsx
- [x] 逐项勾对 proposal.md「预期影响」的 11 项能力表全部落地后，才删除 `admin-ui/src/components/credential-card.tsx`
- [x] 全局搜索确认无残留 import（`dashboard.tsx` 为唯一消费方）
- 验收：✅ `npx tsc --noEmit` 零错误、`npm run build` 成功，无未解析引用；11 项能力表已在 T17 实施记录中逐项核对为 ✅，本轮为其兜底前置条件
- 实施记录：
  - 删除 `credential-card.tsx`（339 行）：删除前 `grep -rn "credential-card\|CredentialCard" src` 仅命中该文件自身的 3 处定义与两处注释文字，无任何 import；唯一历史消费方 `dashboard.tsx` 已在 T18 迁移完毕
  - 连带删除 `src/components/ui/checkbox.tsx`：全项目 11 个 shadcn 原语中唯一因本次删除而归零引用者（表格复选框由 `account-table.tsx` 的 `AccountCheckbox` 直接封装 `@radix-ui/react-checkbox`，依赖与 `package.json` 均不变）
  - 保留判定：`getSubscriptionColor`（`account-row.tsx` + `balance-dialog.tsx` 仍用）、`ui/badge`（7 个消费方）、`EditCredentialDialog` 与 `use-credentials` 各 hook（`account-row.tsx` / `edit-credential-dialog.tsx` 仍用）
  - 重复实现消解：`formatLastUsed` 随文件删除，`account-row.tsx:52` 成全项目唯一实现（T15 / T16 CR 两次登记的重复项至此关闭）
  - 注释清理 3 处：`account-table.tsx:10`「T16 前的过渡行」→ 仅留空状态行；`account-table.tsx:64`「过渡期为包裹 CredentialCard 的单元格行」→「表体行：AccountRow 列表」；`account-row.tsx:51` 去掉对已删文件的引用
  - i18n 影响仅登记不动：新增 3 个孤儿键 `credentials.lastCallLabel` / `remainingLabel` / `remainingPercent`，连同 T18 的 `dashboard.pageInfo` 一并由 T20 收口
  - CR（PASS，0 严重 / 0 改进 / 0 记录）：额外排除了 4 类文本搜索盲区 —— 动态 `import()`（3 处均为无关的类型内联导入）、barrel 汇总（全库仅 `src/i18n/index.ts`）、`vite.config.ts` / `tsconfig*.json` 硬编码路径（零命中）、`@radix-ui/react-checkbox@^1.3.3` 依赖仍在；3 处注释措辞逐条与代码原文对齐（`ACCOUNT_TABLE_COLUMNS` 唯一消费方为 `account-table.tsx:162` 的 `colSpan`、`children` 实参确为 `paged.map(<AccountRow>)`、`formatLastUsed` 仅 `:52` 定义 + `:335` 调用）
- 验证：`npx tsc --noEmit` 与 `npm run build` 均通过（1838 modules，CSS 46.26 kB / JS 690.29 kB）；JS 体积与模块数较 T18 完全未变，印证两个被删文件本就不在模块图内，CSS 减少 2.06 kB 系 Tailwind 不再扫描已删文件的类名
- spec.md 引用：需求「账号列表展现形态」场景 1

### T20 — i18n 文案
- [x] `zh.json` / `en.json` 同步新增键：面包屑、自动刷新标签、文档、4 段指标条标签与脚注、10 个列名、5 个状态标签、5 个筛选分段、搜索占位、更新时间、空状态、面板脚、行操作 tooltip、更多菜单项
- [x] 确认无新增硬编码中文字符串遗留在 tsx 中
- 验收：✅ 脚本化核验 —— 代码引用的键在两个 locale 中零缺失、zh/en 扁平键集合差集为空、en.json 无漏译中文（`languageZh: "中文"` 为语言名母语写法，正确）；⏳ 浏览器切 en 的实地核对随 T21
- 实施记录：
  - 新增键无需补：T4–T18 已逐步落地，脚本比对结果为「代码用到但 locale 无 = 0」，故本项为核验而非新增
  - 自建 `/tmp/i18n_scan.mjs` 做扁平键 × src 字面量比对（正则同时覆盖 `t('x.y')` 与 `labelKey: 'x.y'` 两类，宽松匹配 → 倾向判为在用 → 对删除是保守方向）：扫出 20 个孤儿键
  - 用基线提交 `git grep HEAD` 归因：**14 个本次改版造成**（基线各有 1 个消费方）+ 6 个改版前既有（`common.{confirm,search,noData,copy,copied,refresh}`，基线消费方即 0）
  - 按「删除死代码；既有死代码只提不删」原则只删 14 个：`dashboard.{statTotal,statAvailable,statGlobalCredits,statQueried,statTodayUsage,creditsRingLabel}`（T5 指标条取代）、`dashboard.pageInfo`（T18 面板脚三键取代）、`credentials.health{Healthy,Warning,Degraded,Unhealthy}`（T10 五状态标签取代，`healthDisabled` 保留 —— `credential-detail-page.tsx:93` 仍用）、`credentials.lastCallLabel`（T16 双行时间取代）、`credentials.{remainingLabel,remainingPercent}`（T16 进度条取代）
  - 规避同名陷阱：`zh.json` 另有 `apiKeys.statTotal`（`api-keys-panel.tsx:365` 在用），Edit 的 `old_string` 带相邻锚点行以确保只删 `dashboard` 块内那一处
  - 硬编码中文核查：账号页 6 个文件的中文命中全为多行注释续行，无文案；`lib/utils.ts` 的 `'未知错误'`/`'服务错误'`、`lib/clipboard.ts` 的 `'复制失败：…'`、`kam-import-dialog.tsx` 的 `console.warn` 为改版前既有，且账号页组件均不调用 `parseError`，不在本页暴露 —— 只提不改
  - CR（PASS，0 严重 / 0 改进 / 0 记录）：独立复核补上关键一环 —— 全仓 27 处 `useTranslation()` 均无命名空间参数、`keyPrefix` 与 `<Trans i18nKey>` 零命中、`i18n/index.ts` 无 `ns`/`keySeparator`/`nsSeparator` 自定义，三者共同证明代码键字面量与 locale 扁平键格式恒等，扫描无系统性漏判；并修正「零拼接式键构造」为「4 处拼接均在插值参数对象内，`t()` 第一参数仍为完整字面量」
- 验证：两个 locale JSON 解析通过（各 10 个顶层命名空间，键数 469 → 455）；`npx tsc --noEmit` 与 `npm run build` 均通过（1838 modules，CSS 46.26 kB / JS 689.41 kB，JS -0.88 kB 即被删文案）
- spec.md 引用：全部需求（文案载体）

### T21 — 构建与双主题终检
- [x] `cd admin-ui && npx tsc --noEmit && npm run build`
- [ ] 浏览器核对账号管理页 × 双主题 × 三种数据态（空列表 / 未查询余额 / 已查询余额），与设计稿两张 PNG 逐区块比对 —— **待用户执行**
- [x] `./build-mac.sh` 完整构建通过
- 验收：✅ 无 TS 错误；✅ 三段构建全通；⏳ console 报错与布局溢出需浏览器核对
- 实施记录：
  - `build-mac.sh` 三段全通：admin-ui（1838 modules）→ user-ui（1693 modules）→ `cargo build --release` 增量 20.13s，产物 `target/release/kiro2cc-proxy` 22.13 MB
  - cargo 增量触发原因为 `user-ui/dist/kiro-icon.png` 时间戳变化，非 Rust 源码改动（本次改版无 Rust 侧任务）
  - 自托管字体已入产物：`dist/fonts/` 8 个 woff2（140K，IBM Plex Sans 2 档 + IBM Plex Mono 6 档）+ `dist/fonts.css`，随 rust-embed 嵌入二进制
  - 既有问题只提不改：`admin-ui/dist/.DS_Store` 会被 rust-embed 一并嵌入；`admin-ui/public/fonts/` 与 `public/fonts.css` 仍未纳入版本控制（全新 clone 将缺字体，构建仍会成功但字体回退系统栈）—— **已在阶段 3 收尾 CR 后 `git add` 纳入版本控制**

---

## 阶段 3 收尾 · 三维全量 CR（OpenSpec 第十一节）

按文件分 3 片并行审查（前两次因 sub-agent 请求体过大触发 API 流超时失败，将规范文档必读量由 4 份缩减为 spec.md + design.md、proposal 能力清单与 tasks 状态改为内联后串行跑通）。

- 分片 1（index.css / tailwind.config.js / index.html / public/fonts.css / account-metrics.tsx / account-state.ts）：Verdict FAIL（3 high + 1 medium + 1 low）
- 分片 2（account-table.tsx / account-row.tsx / ui/dropdown-menu.tsx / ui/tooltip.tsx / edit-credential-dialog.tsx）：Verdict FAIL（1 high + 1 medium）
- 分片 3（dashboard.tsx / account-toolbar.tsx / account-panel-foot.tsx / use-credentials.ts / zh.json / en.json）：Verdict PASS（2 medium + 2 low）

**主 agent 独立复核后的合并判定：Completeness ✅ / Correctness ⚠️ / Coherence ⚠️ / Verdict PASS**（无成立的 critical/high）

### 已修复（2 条 Medium）
- `zh.json:64` / `en.json:64` — `dashboard.pageSubtitle` 由「管理 Kiro 账号」改为「管理账号池、额度与限流策略」，对齐设计稿 `option-a-compact-table.html:449` 与 spec.md「页头结构」
- `edit-credential-dialog.tsx:59` — 表单重置 `useEffect` 依赖由 `credential` 改为 `credential.id`。该对话框在 `account-row.tsx:457` 常驻挂载，列表每 30s 自动刷新，依赖整个对象会在编辑中途因新引用触发重置而静默清空已输入内容。经 `git show HEAD` 核实为改版前既有缺陷（基线已有 30s `refetchInterval`，旧卡片同为常驻挂载），本次因优先级修改入口迁入该对话框而放大影响面

### 已推翻的误报（5 条，实现正确，规范文档表述陈旧）
- **异常数用 `text-warn`（分片 1 判 high）** → 设计稿 `:463` 为 `color:var(--warn)`，且阶段 3 已明确决策异常色按设计稿取 warn。`account-toolbar.tsx:13` 分段 pip 同源正确（设计稿 `:533`）
- **环比持平仍渲染灰色「—」徽标（分片 1 判 high）** → 设计稿定义 `.delta.flat{color:var(--text-3);background:var(--surface-3)}`，实现照稿
- **Plex Sans 500/600/700 指向 400 文件（分片 1 判 high）** → 解析 woff2 表目录证实两个 `plexsans` 文件均含 `fvar` 表（可变字体，40 KB / 26 KB，对比 Mono 静态字重 ~9 KB），单文件覆盖整个 wght 轴，四字重共用同一 URL 为正确用法
- **`index.html:16-18` 根绝对路径可能 404（分片 1 判 medium）** → `dist/index.html:16-18` 实测已被 Vite 按 `base: '/admin/'` 改写为 `/admin/fonts/*` 与 `/admin/fonts.css`；`fonts.css` 内 `url(fonts/…)` 相对该文件解析亦为 `/admin/fonts/*`，链路完整
- **页脚「运行中」仅在 `aria-label`（分片 3 判 medium）** → 设计稿 `:429-430` 同样只放在 `aria-label`，可视区仅版本号

### 待回写的规范文档漂移（用户选择不修，登记为 known issue）
- `spec.md:69` 与 `:193` 的「异常以 danger 色显示」应为 warn（与设计稿及既定决策一致）
- `spec.md:90` 的「持平不显示」应为「持平显示 flat 灰徽标」
- `spec.md:55` 的「页脚显示绿色状态点 + 『运行中』+ 版本号」应注明「运行中」仅存于 `aria-label`
- `design.md` 决策 2 例外条款 #3 称 `tabular-nums` 写入 `index.css`，实际为逐元素 Tailwind 类（与既有页面一致）

### 其他
- `dashboard.tsx:159` 昨日 `totalRequests === 0` 时 delta 返回 `null`，spec 未定义该边界（合理除零守卫，Low）
- 跨分片悬置项已闭合：`deriveAccountState` 判定顺序符合 spec.md:189-199；`sortCredentials` 的 `factor` 仅作用于 `queried` 分支，未查询组恒定末位符合 design.md 决策 4
- 修复后验证：两个 locale JSON 解析通过；`npx tsc --noEmit` 与 `npm run build` 均通过（JS 689.44 kB，+0.03 kB 即文案变长）
- 后端零改动已核验：`git diff HEAD -- 'src/*' Cargo.toml Cargo.lock` 输出为空
- `admin-ui/public/fonts/`（8 个 woff2，140 KB）与 `public/fonts.css` 已 `git add` 纳入版本控制

---

## 验收标准

- [ ] `npx tsc --noEmit` 零错误；`npm run build` 成功；`./build-mac.sh` 成功
- [ ] 账号管理页在浅色 / 深色下与设计稿两张 PNG 的区块结构一致（页头 / 4 段指标条 / 操作条 / 工具栏 / 10 列表格 / 面板脚）
- [ ] proposal.md「预期影响」的 **11 项行级能力**在新表格中逐项可达（勾选、优先级、启停、余额查询、账号详情、编辑、重置失败计数、删除、失败日志、限流日志、代理/ARN 标识），两处禁用条件不变：`failureCount === 0` 时不可重置失败计数、未禁用账号不可删除
- [ ] proposal.md 的 **5 项页面级能力**及其条件约束不变：批量验活 / 恢复异常 / 批量删除仅在有选中时出现且批量删除在 `selectedDisabledCount === 0` 时禁用并提示；空列表时隐藏「查询信息」「清除已禁用」；无禁用账号时「清除已禁用」禁用并提示；`verifying && !verifyDialogOpen` 时显示验活进度浮动入口
- [ ] 搜索按 昵称 / 邮箱 / 账号 ID 生效；`⌘K` 聚焦生效且不劫持输入框内按键
- [ ] 5 段筛选计数与表格实际行数一致；「异常」段等于 `warning + degraded + unhealthy` 之和
- [ ] 「账号」「剩余额度」两列排序正确，未查询余额行在额度排序中恒定末位
- [ ] 搜索 / 筛选 / 排序变化后页码重置为 1，不出现空白页
- [ ] 每页 50 条；跨页勾选保留；全选仅作用于当前页
- [ ] 未查询余额时套餐与额度显示「尚未查询 / —」占位，环形图为空轨，无环比徽章
- [ ] 侧栏展开 / 收起（64px）双态正常，收起态 tooltip 可见，刷新后状态保持
- [ ] 主题切换仍走 `.dark` 类 + `localStorage['admin-ui-theme']`，刷新后保持
- [ ] 其他 6 个页面 × 双主题共 12 个视图无不可读区域
- [ ] 切换 en 后账号管理页无中文残留与 i18n key 暴露
- [ ] 后端零改动：`git diff --stat` 中无 `src/**/*.rs` 变更
