# 任务清单：redesign-remaining-pages-ui

## 状态：ARCHIVED

## 任务

### T1 — 共享层：PageHead 组件 + index.css 白名单工具类
- [x] 新建 `components/page-head.tsx`：`{ crumb: string[]; title: string; note?: ReactNode; actions?: ReactNode; onBack?: () => void }`
- [x] 面包屑容器标 `aria-hidden`（CR 后改为条件式：仅末级 === title 时隐藏），末级 `font-medium text-ink-2`；`onBack` 存在时渲染 26px iconbtn 返回按钮（`aria-label` 取 `common.back`）
- [x] `index.css` 增补 design.md 决策 2 白名单三类：`.logstream` 等宽流（含 `.logrow` / `.lv` 5 档，TRACE 与 DEBUG 同档）、图表绘图层（`.plot-grid` / `.plot-y` / `.plot-band` / `.plot-x`）、`.tl-main` 时间线竖线（含 `.rel::before` / `.rel-li::before`）
- [x] `dashboard.tsx:1082-1105` 的内联页头替换为 `<PageHead>`，CR 已逐项比对 className 确认视觉输出与替换前一致
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过（CSS 47.10 kB / JS 690.06 kB）

**T1 实施记录**
- 增量 CR（SHARDS=1，code-reviewer）：**PASS**，0 Critical / 0 High，3 条发现（2 Medium + 1 Low）经用户勾选全部修复 —— ① `aria-hidden` 改条件式，避免末级与标题不一致的页面丢失导航层级；② `actions` 容器补 `flex-wrap justify-end` 防窄屏溢出；③ `crumb.map` 的 key 简化为索引
- **验证盲区（需在 T6 / T9 / T10 / T11 复核）**：Tailwind 3 对 `@layer components` 中未被源码引用的自定义类同样执行 purge，本次三类白名单 CSS 暂无消费者，未进入构建产物（仅 `.logrow .ts` 因 `ts` 意外命中候选类名扫描而残留）。CSS 正确性只能在下游引用任务中验证
- 历史未修复问题复核：`~/.claude/cr-known-issues/` 中 4 条 admin-ui 相关条目涉及 daily-detail-page / credential-detail-page / api-key-detail-page / daily-credits-trend-chart，均未出现在本轮 diff 中，本轮不参与复核，原样保留

### T2 — ui 原语令牌对齐（下游 14 个任务的共享前置，单独 CR）
- [x] `ui/input.tsx` 改为设计稿 `.field` 形态（`surface-2` 底 + `hairline-2` 边 + 7px 圆角），支持右侧单位 slot
- [x] `ui/button.tsx` 变体映射到 `.btn` / `.btn-primary` / `.btn-ghost` / `.btn-danger` / `.btn-sm` / `.btn-icon`
- [x] `ui/badge.tsx` 变体映射到 `.tag` 及 ok / warn / danger / off / new 语义档
- [x] `ui/card.tsx` 取色改为 `surface` + `hairline` + `shadow-panel`（CR 修正：`.panel` 角色对应 `--shadow-md`，`shadow-hair` 是指标卡那档）
- [x] `ui/progress.tsx` 改为 `.bar` 形态（`track` 底 + 语义色填充）
- [x] `ui/dialog.tsx` 取色与圆角对齐设计稿面板（`shadow-pop`）
- [x] 清点全部调用点（含账号管理页与已验收的编辑对话框），逐处确认无视觉回退，清单写入实施记录
- [x] 本任务不与任何页面任务合并提交，完成后立即单独跑一次增量 CR（proposal 风险表要求）
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过（CSS 44.80 kB / JS 689.72 kB）

**T2 实施记录**

- 改动 6 个原语文件（+69/-37）。取色全部改为设计稿原生令牌族（`surface` / `hairline` / `ink` / `brand` / `ok` / `warn` / `danger` / `track`），清掉旧 `neon-*` 霓虹配色与 `bg-primary` / `bg-background` shadcn 默认色
- 增量 CR（SHARDS=1，code-reviewer）：**PASS**，0 Critical / 2 High / 1 Medium / 2 Low。用户勾选修 4 条：① card 由 `shadow-hair` 改 `shadow-panel`（已 grep 复核 `account-table.tsx:101` = `shadow-panel`、`account-metrics.tsx:181` = `shadow-hair`，两档区分真实存在）；② input `unit` 分支外壳补 `has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-50`，使两分支禁用降权一致；③ button 的 svg 取色（`.btn svg` / `.btn:hover svg`）提到 cva 基类；④ input 无 `unit` 分支改 `focus:border-brand`（`focus-within` 从共用 `FIELD_BOX` 拆出，两分支各自表意）
- ③ 落地时**修正了 CR 的前提**：基类 `[&_svg]:text-ink-3` 会给 svg 显式着色，`text-brand-fg` 的继承链被切断；且 `hover:[&_svg]:text-ink-2:hover svg` 特异性 (0,2,1) 高于 (0,1,1)。故 `default` / `destructive` 各自补 `[&_svg]:*` + `hover:[&_svg]:*` 覆盖对（tailwind-merge 同修饰符组后者胜，无特异性冲突），与 `dashboard.tsx` 的 `ACTION_BTN_PRIMARY` / `ACTION_BTN_DANGER` 同口径
- 构建产物校验：`.has-\[\:disabled\]\:{cursor-not-allowed,opacity-50}:has(:disabled)` 两条规则均已生成（Tailwind 3.4 支持 `has-*` 变体）
- **调用点清单（逐处确认无视觉回退）**
  - `Button`：`dashboard.tsx:916/992/1022` 为 `variant="ghost" size="icon"` + `className="h-7 w-7 … hover:bg-surface-3 hover:text-ink-2"`，tailwind-merge 下调用点尺寸与配色胜出 → 仅 focus ring 由 `ring-2 offset` 变 `ring-1 ring-brand`，恰好对齐 `ACTION_BTN_BASE`；`account-row.tsx:440/447` 为对话框 footer 的 `outline` / `destructive`，改为 `.btn` / `.btn-danger` 口径即设计稿目标态；`size="sm"` 全库 58 处无一落在已验收文件
  - `Badge`：7 个调用文件全在下游未改版页（账号管理页用本地 `.tag` 实现，不经该原语）
  - `Card`：`dashboard.tsx` 2 处仅 `shadow-sm`→`shadow-panel`、`bg-card`→`bg-surface`（后者同值）
  - `Progress`：唯一调用点 `balance-dialog.tsx:82` 未传 className，直接受益于 `.bar` 形态
  - `Input`：`edit-credential-dialog.tsx` 12 处（2 处在 `grid grid-cols-2 gap-2` 内），故 `.field` 的 `min-width:150px` 有意不进基类
  - `Dialog`：仅移除 `tailwindcss-animate` 未安装时生成 0 条 CSS 的死动画类，无行为变化
- **有意保留的偏差（非遗漏）**：`.field` 的 `font-family:mono` 不进基类（等宽字体对中文 placeholder 不适用，留给 T12 设置页按需加 `font-mono`）；`[&_svg]:size-4` 不改 `size-[15px]`（后代选择器特异性本就高于调用点直挂 svg 的类，改动只是把既有 1px 偏差挪位，属 pre-existing）
- **登记技术债（本次不改）**：CR Low#2 —— `h-[31px]` / `rounded-[7px]` / `text-[12.5px]` 未抽进 `tailwind.config.js` 的 `theme.extend`，与 `dashboard.tsx`、`account-toolbar.tsx` 重复书写；抽取会连带修改已验收文件，超出本任务范围
- 历史未修复问题复核：`~/.claude/cr-known-issues/` 中 4 条 admin-ui 条目均指向 daily-detail-page / credential-detail-page / api-key-detail-page / daily-credits-trend-chart，未出现在本轮 diff，原样保留（T5 / T6 / T7 / T13 须附加进对应 CR prompt）

### T2.5 — 共享指标卡与工具条原语提取（T3 / T6 / T8 / T10 的共同前置，单独 CR；design.md 决策 8）
- [x] 新建 `components/metrics.tsx`：`MetricsBar`（`cols?: 4 | 5`）/ `Metric` / `MetricValue` / `MetricAside` / `MetricFoot` / `FootSep` / `Ring`（新增 `tone` 参数）/ `Sparkline` / `Delta`，纯展示、零领域字段
- [x] 新建 `components/toolbar.tsx`：`Toolbar` / `SearchBox`（含 ⌘K 劫持与平台判定）/ `Segmented<T extends string>` / `UpdatedAgo`（内置 30s tick + `formatAgo`）
- [x] `account-metrics.tsx` 改为消费共享原语，保留 `remainingTone()` 与 4 张卡的领域装配；`Ring` 传 `tone={remainingTone(...).stroke}`
- [x] `account-toolbar.tsx` 改为消费共享原语，保留 `AccountStatusFilter` 类型与 `SEGMENTS` 常量导出
- [x] 约束核对：className 串逐字保留，零令牌值调整，账号管理页视觉输出零变化（CR 逐项核对）
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过

**实施记录（T2.5）**

新增两个共享原语文件，共 321 行；两个消费方文件净减 264 行。

`components/metrics.tsx`（144 行，9 个导出）：
- `MetricsBar({ cols?: 4 | 5 })` — 动态拼接类名会被 purge，故 `cols` 走静态白名单三元（`grid-cols-5` / `grid-cols-4`），不做 `grid-cols-${cols}`
- `Ring({ percent, tone = 'stroke-brand' })` — 阈值判定移出原语（决策 8 的语义参数化）。账号管理页表达「剩余越高越好」，API Keys 页表达「占用越高越坏」，二者语义相反
- 其余 7 件（`Metric` / `MetricValue` / `MetricAside` / `MetricFoot` / `FootSep` / `Sparkline` / `Delta`）为 `account-metrics.tsx` 私有实现的逐字搬迁

`components/toolbar.tsx`（177 行，4 个导出 + 2 个类型）：
- `SearchBox` 新增 `shortcut?: boolean`（默认 true）与 `widthClass?: string`（默认 `w-[236px]`），供同页多搜索框场景关闭 ⌘K 劫持
- `Segmented<T extends string>` 泛化分段 key 类型，`count` 改为 `count !== undefined` 条件渲染（`0 !== undefined` 为 true，count=0 仍渲染）
- 平台判定 `PLATFORM` / `SHORTCUT_HINT` 与 `formatAgo()` 一并移入，`UpdatedAgo` 自带 30s tick

「只搬不改」约束的落地偏差（**有意保留，已在 CR 中核对**）：
- `account-metrics.tsx` 第 2、3 张卡的脚注原本**不含** `flex items-center gap-1.5`，而 `MetricFoot` 固定带这三个类 → 这两处保留为原生 `div`，未强行套用原语。第 1、4 张卡（原本含 flex）改用 `MetricFoot`，第 4 张传 `className="truncate pr-[92px]"`
- `Ring` 的取色输入由 clamp 后的值改为未 clamp 的原始 `avg`。因 60/30 断点均落在 `[0,100]` 内且 clamp 单调，全实数区间（含负数、>100）分类结果恒等，视觉零变化

验证：`npx tsc --noEmit` 通过；`npm run build` 通过（CSS 44.89 kB / gzip 8.97 kB，JS 690.38 kB；较 T2 收尾时 CSS +0.09 kB，源于新增 `grid-cols-5` 与 `Metric` 的 focus-visible ring 类）。

CR 结果（2 分片并行，`agentType: code-reviewer`，均 PASS，0 Critical / 0 High）：
- 分片 1（`metrics.tsx` + `account-metrics.tsx`）4 项专项核对全 ✅：原语 className/DOM/aria 逐字等价；`Ring` 取色全区间恒等；第 2/3 张卡正确保留原生 `div`；四项兜底（`Number.isFinite` / `length < 2` / `span === 0` / `percent === null`）无一丢失
- 分片 2（`toolbar.tsx` + `account-toolbar.tsx`）5 项专项核对全 ✅：默认值与原硬编码等价；⌘K 全链路保留，依赖数组 `[]`→`[shortcut]` 因布尔原始值引用相等而无重复注册；`count=0` 正常渲染；`UpdatedAgo` 三个 Hook 全在早退之前、顺序合法，`dataUpdatedAt <= 0` 时的定时器空转与原实现完全一致（原 `setInterval` 同样挂组件顶层无条件运行）
- 用户勾选修复 2 条：`toolbar.tsx` 的 `widthClass` 补「须为静态字符串否则被 Tailwind purge」约束注释；`Segmented` 上方块注释统一为 JSDoc。修复后构建产物哈希与体积完全未变（`index-DgbdhZp-.css` / `index-CdCNPWt0.js`），证实纯注释、零运行时影响
- Step 4 判定：中变更 + 无 critical/high 被选中 + 修复量 2 行 → 不触发第 2 轮 CR

登记为技术债（只提不改）：
- `MetricFoot` 的 `className` 为空时拼出尾随空格（无 CSS 影响）
- `Ring` 的 `tone` 默认值 `stroke-brand` 在当前唯一调用点不可达（`avg === null` 时进度弧本就不渲染），为后续 4 页预留
- `Segmented` 的 `count` 依赖 `Record<K, number>` 类型契约；若运行时缺 key 会静默隐藏计数（当前 `SEGMENTS.map` 保证全键覆盖，不可达）
- 后续 4 页复用 `credentials.*` 命名空间的 i18n 键（`searchPlaceholder` / `searchClear` / `updatedAgo` / `justNow` / `secondsAgo` / `minutesAgo` / `hoursAgo` / `daysAgo`），键名语义与页面不匹配，待 T17 一并核对

历史未修复问题复核：`~/.claude/cr-known-issues/` 中 4 条 admin-ui 条目均不涉及本轮 4 个文件，原样保留。

### T3 — API Keys 页 · 连接卡 + 指标卡 + 工具条（`api-keys-panel.tsx`）
- [x] 清点改版前该页全部交互能力，逐条写入本任务实施记录（供 T4 核对）
- [x] 回溯核对 T2 原语在本页真实渲染中的实际表现，偏差回到 T2 修正而非页面侧打补丁
- [x] `PageHead` 页头 + `.conn` 连接卡：`window.location.origin` + `/v1`、兼容协议徽章、复制地址、新窗口打开
- [x] 4 张指标卡：Key 数（启用 / 停用 / 即将到期）、今日请求（含 `.delta`）、累计请求、消费上限占用最高（含环形图）
- [x] `.actionbar` + `.toolbar`（搜索 ⌘K + 状态分段 + 更新时间）
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过

#### T3 实施记录

改版前该页交互能力清单（**T4 须逐条核对可达**）：

本次改动直接触及（原 327-437 行整段替换）的 6 项：
1. 服务信息卡展示 BASE URL + 复制地址按钮 → 迁入 `.conn` 卡，复制逻辑 `copyToClipboard(origin + '/v1', 'url')` 未变
2. 5 张统计卡（总数 / 启用 / 待激活 / 已停用 / 已过期）→ 合并为指标卡①的数值 + 脚注五段
3. 排序 3 按钮（最新 / 消费降 / 消费升）→ 改用 `Segmented`，`sortBy` 取值与排序逻辑未变
4. 搜索框（原生 `Search` 图标 + `Input`）→ 改用 `SearchBox`，新增 ⌘K 聚焦与清除按钮
5. 创建 Key 按钮（`generateUniqueSerial()` 预填 + 开对话框）→ 保留，仅换 `Button` 原语
6. 清理无效 Key 按钮 → 保留，新增「`invalidKeys.length > 0` 才渲染（含前置分隔线一并显隐）」条件

本次改动未触及、T4 须确认仍可达的 5 组：
7. 表格行操作 6 项：查看日志详情、复制「URL+Key」文本、启停 `Switch`、编辑、删除、重置用量
8. 创建对话框：名称输入 + 序号冲突校验（`nameConflict`）、date/quota 模式切换、时长预设 / 自定义 / 永久 + 时·天单位、限额单位 usd/credits、预设额度 / 自定义、绑定账号多选（含下拉内搜索 + Enter 键选中）
9. 编辑对话框：与创建对话框同构的全部字段
10. 清理对话框：`handlePurge` 执行 + `purging` 期间禁用取消/确认与 `onOpenChange`
11. 分页（本任务未涉及，T4 负责）

本任务新增的 2 项能力：
- 状态筛选 `statusFilter`（全部 / 启用 / 待激活 / 已停用 / 已过期），与既有 `searchQuery` 串联过滤
- 明文展示开关 `revealAll`（纯前端 state，切换表格内 `maskKey(key)` ↔ `key`）—— `spec.md`「现有能力保留」曾列此项，但改版前代码中并不存在，本任务补齐

T2 原语回溯核对结果：发现 `ui/button.tsx` cva 基类的 `[&_svg]:size-4`（16px）偏离设计稿 `.i { 15px }`，按「偏差回到 T2 修正」改为 `[&_svg]:size-[15px]`。**影响面为全站所有 `Button` 内的 lucide 图标**；已 grep 确认产物 CSS 中 `size-4` 残留为 0，`sm`(26px 高) 与 `icon`(31×31) 档位下均无溢出/裁切。

两处数据来源缺口的降级（已在代码注释中记录）：
- 指标卡②「今日请求」只能取全局 `useDailyUsage()` 的 `DailySummary`（后端无按 Key 拆分的按天数据），故该卡为**全局口径**而非按 Key 口径
- 设计稿该卡脚注的「失败 · 失败率」因 `DailySummary` 无失败数字段，改为「消费 / 积分」

两处设计稿元素按 proposal「不在范围内」不渲染：
- `.actionbar` 的「导出配置」（后端无该能力）
- `.conn` 卡右侧的「在新窗口打开」图标按钮（`/v1/*` 均需认证、`origin` 根无路由，渲染即死交互；**经用户确认**）

一处按真实能力偏离设计稿：兼容协议徽章由设计稿的「Anthropic Messages + OpenAI Chat」改为「Anthropic Messages + Claude Code /cc/v1」（后端无 OpenAI `chat/completions` 端点）。

验证：`npx tsc --noEmit` 通过；`npm run build` 通过（CSS 44.98 kB / gzip 8.99 kB，JS 698.77 kB）。

CR 结果（2 分片并行，`agentType: code-reviewer`，0 Critical / 2 High / 3 Medium / 4 Low）：
- 分片 1（`api-keys-panel.tsx`，+335/-107）两项专项核对：① `copiedUrl` 时同时给出 `[&_svg]:text-ok` 与 `hover:[&_svg]:text-ok` 的写法**已达成预期** —— 两个变体同色，无论 `:hover` 实际挂在按钮父元素还是 svg 自身，三种悬停情形颜色恒为 ok，从根本上消除了对挂载点的依赖；② 派生逻辑边界除 `expiringSoonCount` 外全部正确（`todayRequests` 的 `null`/`0` 区分、`requestsDeltaPercent` 除零保护、`requestTrend` 先 `.slice()` 拷贝再排序、`cumulative` 的 `if (u)` 跳过、`topQuota` 的 `spendingLimit <= 0` 跳过与 `>=` 并列取先者、`formatLocalDate` 本地时区口径均无 NaN / Infinity / 越界风险）
- 分片 2（`zh.json` + `en.json` + `ui/button.tsx`）四项专项核对全 ✅：新增 23 键在两侧键名/层级/顺序完全一致；4 个插值键变量名一致；`apiKeys` 命名空间内无重名覆盖；JSON 语法合法
- 用户勾选修复 2 条 High：① `handleRefresh` 改 async + `Promise.all` + 检查 `isError`（`refetch()` 默认 `throwOnError: false` 不抛异常，故不用 try/catch），失败时弹 `toast.error(extractErrorMessage(...))` 而非无条件成功 toast；② `PageHead` 的 `crumb` 末级与 `title` 由硬编码 `'API Keys'` 改回 `t('apiKeys.pageTitle')`，与账号管理页 `t('dashboard.navCredentials')` 的做法对齐，同时消除 `pageTitle` 的孤儿键嫌疑
- Step 4 判定：中变更（382 行）+ 有 high 被选中 + 修复量 6 行（< 50）→ 不触发第 2 轮 CR
- 本轮无未修复的 Critical/High，`~/.claude/cr-known-issues/` 无新增条目

登记为技术债（只提不改）：
- `expiringSoonCount` 未排除 `disabled` 状态 —— 已停用且 7 天内到期的 Key 会同时计入指标卡①脚注的「已停用」与「即将到期」两个互斥语义段
- 约 70 行派生指标块未 `useMemo` —— 搜索框每敲一字符触发全量重算（当前数据量级下影响有限，memo 化需重排依赖、回归面较大）
- `revealAll` 无自动收起/超时，与 `copiedUrl`/`copiedId` 的 2s 收敛心智不一致（旁窥风险，非越权漏洞）
- `api-keys-panel.tsx` 已 1323 行（超 800 行上限），派生块与指标条 JSX 可抽 `useApiKeyMetrics()` + `<ApiKeyMetricsBar />`
- `statusOf` / `statusCounts` / `cumulative` 三轮独立遍历可合并为一次 `reduce`
- `formatLocalDate` 与 `dashboard.tsx:54-56` 重复实现，未提取到 `lib/`
- `metricExpiringSoon` 用 `{{count}}` 但无 i18next 复数变体（当前英文文案不含可数名词，暂不影响）
- 疑似孤儿 i18n 键 6 个（`serviceInfo` / `statTotal` / `statActiveLabel` / `statPendingLabel` / `statDisabledLabel` / `statExpiredLabel`）—— `pageTitle` 已因 High#2 的修复重新被消费；清理由 T17 统一核对

历史未修复问题复核：`~/.claude/cr-known-issues/` 中 4 条 admin-ui 条目均不涉及本轮 4 个文件，原样保留。

### T4 — API Keys 页 · 表格 + 分页 + 能力核对（`api-keys-panel.tsx`）
- [x] 10 列表格（列头文案按 spec 降级表改名，不删列）+ 停用行降权 + 到期警示徽章
- [x] `.pager` 分页器 + 跨页勾选行为与账号管理页一致
- [x] 逐条核对 T3 记录的能力清单，全部可达，核对结果写入实施记录
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过

**T4 实施记录**

4 个新文件：`table-kit.tsx`(63) 共享表格原语、`api-key-table.tsx`(114) 表格外壳、`api-key-row.tsx`(293) 单行、`api-key-panel-foot.tsx`(119) 面板脚。5 个文件修改：`api-keys-panel.tsx`（卡片列表 → 表格 + 分页 + 批量，1323 → 约 1210 行）、`account-table.tsx` / `account-row.tsx` / `account-panel-foot.tsx`（改为导入共享原语）、`zh.json` / `en.json`（各 +26 键）。

共享原语提取（用户选定「方案 B / 提取到 table-kit.tsx」）：`TH_BASE`、`CELL`、`ICON_BTN`、`FOOT_BTN`、`PAGER_BTN`、`pageWindow()`、`DataCheckbox`（原 `AccountCheckbox` 重命名）从账号页三文件私有提升为共享导出，类串逐字一致，为纯机械替换。

表格化后 4 处信息补位（原卡片有位置、表格无独立列）：① 有效期/待激活时长 → 状态徽章 `title`（`statusTitleOf()`，在 panel 侧复用既有 `formatDuration`）；② 创建时间 → 序号副行 `title`（`createdTitle`）；③ in/out token → 请求数列副行；④ 无上限 Key 的累计消费 → 消费上限列副行。

批量操作范围按用户选定「停用 + 重置用量」，严格对齐设计稿 `panel-foot` 的两个按钮，不加批量删除。`runBatch()` 逐个串行调用、单个失败不中断、结束统一失效缓存并清空勾选；部分失败时弹 `toastBatchPartial`（比原计划多加 1 个 i18n 键）。

分页页码用 `const page = Math.min(currentPage, totalPages)` 钳制而非 setState，避免删除末页最后一行后多一轮渲染。排序由 `Segmented` 三档承担，不引入第二套表头排序机制（设计稿名称/日配额两列的 sortable 图标故意不实现，已在 `api-key-table.tsx` 注释记录）。

删除的死代码：`Card`/`CardContent`/`Switch`/`Pencil`/`Trash2`/`BarChart3`/`RotateCcw`/`Globe` 导入与 `formatCost`/`maskKey`/`formatSerial` 三个函数（仅旧卡片使用）；`formatDate`/`formatDuration` 保留（编辑对话框仍用）。

T3 记录的 11 组能力清单逐条核对结果 —— **全部可达**：
1. 服务信息卡 BASE URL + 复制 → `.conn` 卡（T3 已迁，本任务未触碰）✅
2. 5 张统计卡 → 指标卡①数值 + 脚注五段 ✅
3. 排序 3 按钮 → `Segmented`，`sortBy` 取值与排序逻辑未变（`sorted` 的 `costOf` 分支逐字保留）✅
4. 搜索框 → `SearchBox`（含 ⌘K 聚焦与清除）✅
5. 创建 Key 按钮 ✅
6. 清理无效 Key 按钮（`invalidKeys.length > 0` 条件渲染）✅
7. 行操作 6 项 → 全部迁入 `api-key-row.tsx` 并在 panel L839-849 接线：`onCopy`（URL+Key 文本）、`onViewDetail`、`onToggleEnabled`（`SwitchPrimitive`）、`onEdit`、`onResetUsage`（菜单内，`requests === 0` 时禁用）、`onDelete`（菜单内 `danger`）✅
8. 创建对话框：`newName` + `nameConflict`(L338) 校验、`newMode` date/quota 切换、`newDuration` + `newDurationUnit` 时·天、`newLimitUnit` usd/credits、`newSpendingLimit`、`newBoundCredentialIds` 多选（`credSearchQuery` 下拉内搜索 + L1262 `onKeyDown` Enter 选中）✅
9. 编辑对话框：`editName`/`editMode`/`editDuration`/`editDurationUnit`/`editSpendingLimit`/`editLimitUnit`/`editBoundCredentialIds` 同构 7 项齐备 ✅
10. 清理对话框：`handlePurge`(L172) + `purging` 期间禁用取消/确认与 `onOpenChange`(L1167) ✅
11. 分页：本任务新建，`ITEMS_PER_PAGE = 50` + `ApiKeyPanelFoot` 的 `.pager`，跨页勾选与账号管理页一致 ✅

验证：`npx tsc --noEmit` 无输出；`npm run build` 成功（1844 modules，CSS 44.34 kB / gzip 8.89 kB，JS 706.89 kB）。Tailwind purge 抽查全部命中：`opacity-[.55]`、`min-w-[132px]`、`max-h-[70vh]`、`bg-track`、`border-brand-line`、`bg-brand-soft`、`text-warn`、`bg-warn-soft`、`py-14`、`shadow-panel`、`not(:last-child)`、`td]:border-b-0`、`sr-only`。

CR 结果（第 1 轮 3 分片并行，`agentType: code-reviewer`，约 1060 行 / 8 文件 → `SHARDS = 3`；0 Critical / 1 High / 3 Medium / 5 Low）。三片专项核对：账号页三文件原语提取经逐字符比对确认无行为漂移、删除符号无残留引用、zh/en 新增 26 键的键名与插值参数完全对齐、`colgroup` 10 列与表头列数一致、空状态 `colSpan` 覆盖全列、`apiKey.key` 为纯文本节点由 React 转义无 XSS。

用户勾选修复 High + Medium 共 4 条：
- High#1 `selectedIds` 残留已删除 id（页脚计数虚高 + 批量重置对幽灵 id 发请求 + `toastBatchPartial` total 污染）→ 新增 `selectedExistingIds`（按 `allKeys` 过滤），`selectedCount` 与 `handleBatchResetUsage` 统一改用它，与既有 `selectedEnabledCount` 的做法对齐
- Medium#2 禁用按钮的禁用原因只挂祖先 `title`（屏幕阅读器不播报）→ 加 `useId()` + `aria-describedby` + `sr-only` 文本；**账号管理页 `account-panel-foot.tsx` 的对等实现同步修改**，避免两页 a11y 行为不一致
- Medium#3 `activatedAt`/`createdAt` 畸形时静默渲染 `NaN-NaN-NaN` → `Number.isNaN(getTime())` 判空后回退 `—`（与同文件 RPM 列空值表示一致）
- Medium#4 `sorted` 的 `useMemo` 因依赖未 memo 的 `filteredKeys` 而失效 → 首次修复为 `filteredKeys` 包 `useMemo`

Step 4 判定：大变更（> 500 行）→ 第 2 轮 CR 必触发。第 2 轮（`SHARDS = 1`，范围收窄为 4 处修复的增量）判定 **FAIL**，1 High：`filteredKeys` 的 `useMemo` 依赖故意省略 `statusOf`，而 `getKeyStatus` 内部读当前时间、`statusOf` 每渲染重算 —— 缓存导致行内状态徽章（新 `statusOf`）与筛选结果（旧缓存）脱节，属该次修复引入的新回归。另核实 High#1 / Medium#2 / Medium#3 均已修复（`useId()` 在顶层无条件调用，id 全程稳定；`aria-describedby` 与 `sr-only` 严格同条件渲染无悬空引用）。

用户选定「撤销 `filteredKeys` 的 `useMemo`」：回退为每渲染重算，回归消除；`sorted` 的 `useMemo` 保留但补注说明其缓存实质不生效（当前几十到几百条 Key 的排序开销可忽略，不为此引入按分钟节流的时间源）。**Medium#4 转技术债**。第 2 轮为硬上限，不升第 3 轮。撤销后 `npx tsc --noEmit` + `npm run build` 复验通过。

本轮无未修复的 Critical/High，`~/.claude/cr-known-issues/` 无新增条目。历史 4 条 admin-ui 条目均不涉及本轮 8 个文件，原样保留。

技术债（只提不改）：Medium#4 `sorted` 的 memo 名义失效；Low — `ApiKeyRow` 单函数 150+ 行、短 Key（≤11 字符）掩码头尾切片重叠、`pad2` 日期拼接可提 `formatDateParts`、`api-key-table.tsx` 的 `num` 字段语义实为「右对齐」应改名 `align`、空状态可提 `TableEmptyState` 与账号页共用、`selectedExistingIds` 未 memo、`handleBatchDisable` 重复 `filter(allKeys)`；`api-keys-panel.tsx` 仍超 800 行（约 1210 行）；`COLUMN_WIDTHS`(10) 与 `COLUMNS`(9) 未合并；批量 handler 为 fire-and-forget（`void runBatch`）。

### T5 — API Key 详情页（`api-key-detail-page.tsx`）
- [x] 清点改版前交互能力
- [x] 带 `onBack` 的三级面包屑页头 + 汇总指标卡 + `.panel` + `.tscroll` 逐模型用量表
- [x] 仅使用 A 组已实现组件（design.md 决策 3）
- [x] 逐条核对能力清单
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过

#### T5 实施记录

**设计稿状态**：`/Users/MacBook/Downloads/kiro2ccproxy-redesign` 无本页稿件 → 按阶段 0 决策①「按已建立模式推导重建」，组件池限定为 design.md 决策 3 白名单。

**改动文件（4 个）**
| 文件 | 变化 |
|---|---|
| `api-key-detail-page.tsx` | 282 → 366 行，整体重写 |
| `api-key-row.tsx` | `STATUS_VISUAL`（加 `export` + `Readonly`）、`TAG_BASE`（加 `export`）共 2 处 |
| `i18n/locales/zh.json` / `en.json` | 各新增 4 键 |

**新增 i18n 键**：`apiKeys.colInputTokens`、`apiKeys.colOutputTokens`、`apiKeys.footRecords`、`apiKeys.toastLogRefreshed`。复用既有键 20+ 个（`common.col*` / `common.*TokensLabel` / `credentials.footPerPage` / `dashboard.navMain` 等）。

**六项改造**
1. 页头：`ArrowLeft` + `Badge` 自绘头部 → `PageHead`（三级面包屑 `主要 / API Key 管理 / <名称>` + `onBack` + `note` 放 `#NNN` 序号 + `actions` 放状态徽章与刷新钮）
2. 指标条：5 张 shadcn `Card` → `MetricsBar cols={5}` + `Metric` / `MetricValue`；节省额走 `MetricValue` 的 `trailing`（不用 `MetricFoot`，避免 5 段高度参差）
3. 逐模型用量：卡片网格 → `.panel` + 6 列表格（模型 / 请求数 / 输入 / 输出 / 费用 / 积分），`max-h-[42vh]` 滚动
4. 请求日志：7 列与 token 四行副行结构原样保留，外壳换 `.panel` + `max-h-[70vh]`，空/加载态对齐 `api-key-table.tsx` 的 `Loader2` / `SearchX` 写法
5. 分页：居中「上一页 / 下一页」→ 面板脚 `.pager`（`PAGER_BTN` + `pageWindow`），服务端分页语义不变；`totalPages` 经 `Math.max(1, ...)` 兜底
6. 令牌化：`text-muted-foreground` / `bg-muted/50` / `text-orange-600` / `text-purple-600` 等 shadcn 语义类与硬编码色全部换为 `--ink-*` / `--surface-*` / `--hairline` / `--warn` / `--brand` / `--ok`

**取舍**
- 指标条主值统一 ink 色（设计稿 `.metric` 主值无着色），费用/积分的橙/蓝强调下移到表格列 —— `MetricValue` 不接受 `className`，为此改原语签名超出 T5 范围
- 模型着色：无紫色令牌，`opus`→`text-brand`、`sonnet`→`text-ok`、`haiku`→`text-ink-2`，兜底 `text-ink-3`
- 标题行原有的 `common.totalCountSuffix` 计数移入面板脚 `footRecords`，避免同一数字渲染两处
- 刷新补齐错误处理（`refetch()` 默认不抛异常，须查 `result.isError`），与列表页 `handleRefresh` 同口径
- 顺带修历史 known-issue：IP 列 `{geo.country}·{geo.city}` 在 `city` 为空时的悬空分隔符（用户在计划确认时勾选）
- `/0.72` 积分换算硬编码按用户决定不动（仍为技术债）

**能力清单核对（改版前 8 项，逐项可达）**
| # | 改版前能力 | 改版后落点 |
|---|---|---|
| 1 | `ArrowLeft` 返回 | `PageHead onBack` |
| 2 | 序号 + 名称 + 4 态状态徽章 | `note` 序号 / `title` 名称 / `actions` 内 `STATUS_VISUAL[status]` |
| 3 | 5 张汇总卡（含节省额副行） | `MetricsBar cols={5}` 五段 + `trailing` 节省额 |
| 4 | 逐模型分组（请求/入/出/费用/积分/节省） | 逐模型表 6 列 + 积分列 `creditsSaved` 副行 |
| 5 | 日志标题 + 总数 + 刷新（`animate-spin` + disabled） | `h3` 标题 + 面板脚 `footRecords` + `Button` 刷新（`isLoading` 时 spin + disabled） |
| 6 | 7 列日志（时间/IP+geo/账号/模型/token 四行/费用/积分含 ✓ 与节省） | 逐列保留，字段 `displayIp`/`credentialLabel`/`cacheReadInputTokens`/`creditsUsed`/`estimatedCost`/`creditsSaved` 全部可达 |
| 7 | 三态 loading / 空 / 表格 | 空状态行内 `Loader2` / `SearchX` 二分 + 表体 |
| 8 | 分页（上/下 + 页码信息） | 面板脚 pager（含页码窗口，信息量增加） |

**CR（1 轮，SHARDS=2，均 PASS）**
- 分片 1（`api-key-detail-page.tsx` 全文，366 行）：0 Critical / 0 High / 0 Medium / 2 Low；历史悬空分隔符条目判定**已修复**
- 分片 2（`api-key-row.tsx` export + i18n）：1 Medium（`STATUS_VISUAL` 未 `Readonly`）/ 2 Low / 1 Note
- 用户处置：修 Medium#1（加 `Readonly`）；其余 4 条记为技术债
- 中变更 + 未选 critical/high → 按门控 Step 4 不触发第 2 轮

**验证**：`npx tsc --noEmit` 无输出；`npm run build` 成功（1844 modules，CSS 44.48 kB / gzip 8.92 kB，JS 706.80 kB / gzip 209.98 kB）；dist CSS 已含 `grid-cols-5` / `max-h-[42vh]` / `max-w-[132px]` / `text-warn` / `text-brand`。

**技术债**：① 图标 `aria-hidden` 标注不统一（`RotateCw` / `ChevronLeft` / `ChevronRight` 未标，同文件 `Loader2` / `SearchX` 已标）；② `STATUS_VISUAL` / `TAG_BASE` 由页面级组件反向 import 行组件常量，建议后续下沉到中性文件；③ `/0.72` 换算在本页出现两处且无集中定义；④ `apiKeys.toastLogRefreshed` 与 `apiKeys.toastRefreshed` 文案相近。

### T6 — 每日统计页 + 趋势图（`daily-stats-page.tsx` + `daily-credits-trend-chart.tsx`）
- [x] 清点改版前交互能力（区间切换、导出、跳转明细）
- [x] 4 张指标卡（含环比 `.delta` 三态与上一区间为 0 时不渲染）
- [x] `.chart-card` 重写趋势图：面积 + 折线（`vector-effect="non-scaling-stroke"`）、Y/X 轴、峰值标注、零消耗区间带
- [x] 三个边界守卫：数据点 < 2、全部为 0、峰值为 0（spec 对应场景）
- [x] `.toolbar` + `.panel.is-static` 4 列表格 + 行点击跳转明细
- [x] 逐条核对能力清单
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过（CSS 46.58 kB / JS 717.23 kB）

**T6 实施记录**

- 两个组件整体重写：`daily-credits-trend-chart.tsx` 142 → 190 行、`daily-stats-page.tsx` 90 → 约 325 行；`types/api.ts:239` 注释修正；zh/en 各新增 38 键 + 改 1 键（`trendTitle` 去掉硬编码的「最近 14 天」）
- **修复真实缺陷（CST/UTC 口径不一致）**：旧趋势图用 `Date.UTC(...)` 推「今天」，后端 `get_daily_summaries()`（`src/model/usage.rs:738`）按 `FixedOffset::east_opt(8*3600)` 聚合 → CST 00:00–08:00 期间当日数据完全落在窗口外。新实现把日期序列构造整体移到统计页的 `cstDateString()` + `buildWindow()`，趋势图降为纯受控组件（props 仅 `points: TrendPoint[]`）。同时闭环了 known-issues 中「跨午夜滞后」的 MEDIUM 条目（`buildWindow` 有意不做 `useMemo`，由 `refetchInterval: 60000` 驱动，滞后 ≤ 1 分钟），第 2 轮 CR 已确认该条已修复并从持久化文件移除
- **T1 遗留验证盲区已闭环**：T6 是首个引用 `.plot-*` 白名单 CSS 的任务，已在 dist CSS 中 grep 确认 `.plot-grid` / `.plot-y` / `.plot-band` / `.plot-x` / `.plot-x span.on` / `.plot-grid i.base` 六个选择器全部命中；`shadow-[0_0_0_4px_var(--brand-soft)]`、`[transform:translate(-50%,-9px)]`、`bg-track`、`stroke-warn`、`tabular-nums` 亦均已生成
- **未新增任何 CSS 组件类**（design.md 决策 2 硬约束）：`.chart-card` / `.chart-head` / `.plot-dot` / `.plot-val` / `.plot-band-lab` 全部用 Tailwind 工具类表达，仅消费 T1 已写入的白名单绘图层
- 三个边界守卫：① 点数 < 2 → `plottable=false` 短路进空态（避免 `i/lastIndex` 得 NaN）；② 全零 → `niceAxisMax(0)` 返回 1（避免 `credits/axisMax` 除零），折线贴底、`annotated` 为空故不渲染峰值圈与数值标注；③ 峰值为 0 → 表格 `.bar` 宽度取 0。第 2 轮 CR 给出 `axisMax ≥ max(credits)` 的数学证明（`nice*magnitude ≥ rawStep = value/3`），`bottom` 不会超 100%
- **能力清单核对（改版前 → 改版后）**：区间切换 7/14/30（保留，改为 `Segmented`）；跳转明细（保留，行点击 + `tabIndex`/Enter/Space 键盘可达，旧实现仅 onClick）；刷新（保留，并按 T3 口径补 `result.isError` 检查 —— `refetch()` 默认 `throwOnError: false` 不抛异常）；表格 8 列 → 4 列（后端 `DailySummary` 仅 5 字段，无失败数/成功率/主要模型/Token；`$cost` 与省额降为积分单元格内 10px 副行，费用信息不丢）；设计稿日历按钮**降级为非交互区间徽标**（无日期选择器需求支撑，避免「点了没反应」）；导出 CSV 与零消耗日折叠行按用户决策不纳入
- 分片 CR（718 insertions > 300 行且文件数 > 1 → `SHARDS = 2`，贪心装箱后 453 / 446 行，单条消息内并行）：两片均 **PASS**，0 Critical / 0 High，合计 1 Medium + 6 Low。用户勾选修 2 条 —— ① `useId()` 返回形如 `:r1:` 的含冒号 id 直接用作 SVG `<linearGradient id>` 并以 `url(#…)` 引用，改为 `` `credits-trend-${useId().replace(/:/g,'')}` ``，并同步修 `metrics.tsx` 的 `Sparkline`（同一模式，否则两处不一致）；② `shareTone` 的 `50`/`1` 提为 `SHARE_TONE_STRONG`/`SHARE_TONE_WEAK`
- 因属大变更（> 500 行）按门控 Step 4 必触发第 2 轮 CR，范围收窄为修复的 10 行增量 diff（`SHARDS = 1`）：**PASS，0 发现**
- **未修的 5 条 Low（技术债）**：① 可点击 `<tr>` 缺无障碍语义 —— sub-agent 建议的 `role="button"` **会破坏表格行列关系**，且 `account-row` / `api-key-row` 同样是可点击行，应全站统一处理而非单页改；② `<section>` 缺 `aria-labelledby`（`aria-label` 会与 `.chart-head` 内已有的可读标题重复播报）；③ 空数据态不一致（`peak` 显示 `—`、`avgActive`/`median` 显示 `0.00`；区间无消耗时「日均 0.00」本身语义正确）；④ 页面组件约 290 行（全站同类，`api-keys-panel.tsx` 1213 行）；⑤ Props 用内联匿名类型 —— **该条不成立**，`metrics.tsx` 全部原语（`Ring`/`Delta`/`MetricValue`）均为内联匿名类型，是本项目既有风格
- **遗留给 T17 的孤儿键**：`dailyStats.colCostUsd` / `dailyStats.colCredits` / `dailyStats.creditsUsageLegend`（改版后无消费者。T7 已判定：`dailyStats.colCredits` 确为孤儿键 —— 共用日志表改用新增的 `common.colCredits`；`apiKeys.colCredits` 仍被 `api-key-detail-page.tsx` 逐模型表使用，非孤儿）

### T7 — 每日明细页（`daily-detail-page.tsx`）
- [x] 清点改版前交互能力
- [x] 带 `onBack` 页头 + 指标卡 + `.panel` + `.tscroll` 表格 + `.panel-foot`
- [x] 仅使用 A 组已实现组件
- [x] 逐条核对能力清单
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过（CSS 46.52 kB / JS 714.06 kB，均较 T6 下降）

**T7 实施记录**

- `daily-detail-page.tsx` 整体重写 215 → 151 行；**新建 `usage-log-table.tsx`（171 行）** 承载 API Key 详情页与每日明细页共用的 7 列请求日志表；`api-key-detail-page.tsx` 369 → 205 行（净 -164）；`table-kit.tsx` 63 → 119 行；`daily-stats-page.tsx` 换用 `PANEL_FOOT`（2 行）；zh/en 各 +8 键
- **去重是本任务的主要成果**：两页的日志表列结构与单元格渲染此前逐字重复（约 104 行 × 2）。抽取时只提表体，`.panel` 外壳 / `.tscroll` 滚动容器 / `.panel-foot` 仍由调用方给出（两页的 foot 内容不同）。另把散落 4 处的 `/0.72` 硬编码收敛为 `USD_TO_CREDITS_RATE` + `fallbackCredits()` + `recordCredits()` 三个导出，`recordCredits` 统一了「优先真实 `creditsUsed`、缺失时按汇率估算」的口径。构建产物 JS 717.23 → 714.06 kB 可证去重生效
- `table-kit.tsx` 追加 4 个共享项：`PANEL` / `PANEL_TITLE` / `PANEL_FOOT` 三个静态字符串常量 + `Pager` 组件（前后翻页钮 + 最多 5 个数字钮，复用既有 `pageWindow()`）。抽取 `Pager` 时把原先的函数式更新 `setPage(p => …)` 改为值式 `onPage(…)`，clamp 收进组件内部，调用方直接传 `setPage`
- **未新增任何 CSS 类**（design.md 决策 2 硬约束），全部用 Tailwind 工具类 + T1 已写入的白名单
- **能力清单核对（改版前 → 改版后）**：分页（保留，文本「上一页/下一页」→ 设计稿 `Pager` 数字页码）；请求日志 7 列（全部保留，含 IP 归属地、账号标签、Token 四层明细、费用、积分 ✓ 角标与省额副行）；刷新（保留，**并补齐旧实现缺失的错误处理** —— 旧代码为 `onClick={() => refetch()}`，`refetch()` 默认 `throwOnError: false` 不抛异常，失败静默无提示，现按 T3 口径检查 `result.isError`）；**新增第 4 张指标卡**「本页 Token + 缓存读占比 Ring」；**有意不加搜索/筛选** —— 后端为服务端分页、前端只持有当页 50 条，`SearchBox` 只能筛当页会误导用户
- **指标口径已在 UI 上显式声明**：除请求数取后端 `total` 外，费用 / 积分 / Token 三卡均为「当页」汇总，页头 `note` 与卡内 foot 均写明（`detailNote` / `metricPageOf`）。旧实现同样只能算当页，但未告知用户
- **旧实现的 palette 色残留已清零**：`text-purple-600 dark:text-purple-400` 等模型着色改为设计令牌（opus → `text-brand`、sonnet → `text-ok`、haiku → `text-ink-2`、其余 `text-ink-3`；设计稿无紫色令牌）；3 张 shadcn `Card` → `MetricsBar`；`bg-muted/50` 表头 → `TH_BASE`
- 分片 CR（约 410 行 / 7 文件 → `SHARDS = 2`，按「共享层 227 行 / 消费方组装 183 行」切分而非纯行数均衡，以保住调用契约的一致性视野；分片 2 的 prompt 内联了共享组件的完整导出签名，使其无需读取另一分片文件）：两片均 **PASS**，0 Critical / 0 High，合计 1 Medium + 3 Low，用户选择全部不修
- **未修的 4 条（技术债）**：① `UsageLogTable` 函数体约 100 行，建议拆 `LogRow` 子组件；② `Pager` 值式 `onPage` 依赖闭包 `page` 快照，极端连点理论上有过期值风险（每次点击通常跨渲染，实际风险低）；③ IP 单元格嵌套三元可抽 `renderIpCell()`；④ 页面组件函数体 119 / 164 行超「函数 < 50 行」阈值 —— 审查方明确指出属本库页面级既定约定且本次为净减少，不建议改
- **闭环 known-issues 一条**：`server-side-ip-geolocation` 段的 MEDIUM「city 缺失悬空分隔符」中 `daily-detail-page.tsx` 已随表体委托给 `UsageLogTable` 而修复（共享组件内 `geo.city ? \`${geo.country}·${geo.city}\` : geo.country`），第 1 轮 CR 复核确认，已从持久化条目移出，剩 `credential-detail-page.tsx`（T13）与 `user-ui/src/components/usage-log-page.tsx`（本变更范围外）
- **T17 孤儿键清单更新**：`UsageLogTable` 被两个不同命名空间的页面共用，故新增 `common.colCredits`；连带 `dailyStats.colCredits` 转为孤儿键，而 `apiKeys.colCredits` 仍被本页逐模型表使用（非孤儿）。`apiKeys.footRecords` / `credentials.footPerPage` 在 `daily-detail-page.tsx` 中跨命名空间引用，**有意沿用 T5 同口径**不搬 `common`（避免新增 2 个孤儿键），是否搬迁统一交 T17 决定

### T8 — 支持模型页（`model-list-page.tsx`）
- [x] 清点改版前交互能力
- [x] 4 张指标卡（倍率区间全 null 时显示「—」）
- [x] `.actionbar` + `.toolbar`（搜索 ⌘K + 类型分段）
- [x] 家族分组表格：`.grp` 分组行 + 7 列（按 spec 降级表）+ `.mult-bar` 倍率条 + 高倍率 warn 色
- [x] 家族推导逻辑抽为纯函数，无法归类归入「其他」
- [x] 逐条核对能力清单
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过（CSS 45.74 kB / JS 724.00 kB；CSS 较 T7 的 46.52 kB 再降 0.78 kB）

**T8 实施记录**

- `model-list-page.tsx` 整体重写 111 → 508 行；zh/en 各 9 → 51 键（净 +42 键，`colMaxTokens` 文案由「最大 Tokens」改为「最大输出」）。**未新增任何 CSS 类**，全部用 Tailwind 工具类 + T2.5 共享原语 + `table-kit.tsx` 既有导出（`PANEL` / `TH_BASE` / `CELL` / `ICON_BTN` / `PANEL_FOOT`），本任务未再改动共享层
- **列结构按 spec 降级表落地为 7 列**：`# / 模型 ID / 显示名称 / 提供方 / 最大输出 / 费率倍率 / 操作`。设计稿的「类型」列**改为「提供方」**（后端 `available_model_to_model()` 把 `model_type` 恒定写死为 `"chat"`，该列零信息量；`guess_owned_by()` 有 8 个真实取值），「状态」列**改为「倍率来源」**（后端无 enabled/disabled 概念，唯一可判别的状态是 `rate_multiplier` 是否为 `None`）。两处降级均已在阶段 0 经用户确认
- **家族推导为模块级纯函数** `modelFamily()`：双正则分层匹配 —— `MODERN_CLAUDE = /^claude-(?:opus|sonnet|haiku)-(\d+)/` 命中新代际（`claude-opus-4-6-…` → `Claude 4.x`），未命中再走 `LEGACY_CLAUDE = /^claude-(\d+)(?:-(\d+))?-/`（`claude-3-opus-…` → `Claude 3`、`claude-3-5-sonnet-…` → `Claude 3.5`）；`id === 'auto'` 与无法归类分别返回哨兵 `'@@auto'` / `'@@other'`，由组件内 `familyLabel()` 映射为 i18n 文案。用哨兵而非直接存中文，是为了让 `groupByFamily()` 保持纯函数、切换语言无需重算分组
- **`formatK()` 而非 `formatTokenCount()`**：设计稿的 Token 上限以 K 为单位（`8.2K` / `32K`），而 `lib/locale` 的 `formatTokenCount()` 只做千分位（`32,000`），故本页单独实现；`< 1000` 直出原值，整数 K 不带小数
- **倍率区间卡三态**：全部为 `null` → 显示「—」；min === max → 单值；否则 `min － max` 并在 foot 标注最低 / 最高倍率对应的模型名。`rateModelName()` 用 `===` 比对浮点安全，因为 min/max 直接取自同一批原始值、无算术运算
- **能力清单核对（改版前 → 改版后）**：模型清单展示（保留，平铺表 → 家族分组表）；刷新（保留，**并补齐旧实现缺失的错误处理**，按 T3 口径检查 `result.isError`）；**新增** 搜索（id / display_name 模糊）、三档倍率状态筛选、复制单个 ID、复制全部 ID、导出 JSON、4 张指标卡、`UpdatedAgo` 时间戳。零能力丢失
- **旧实现的 palette 色残留已清零**（该文件原有 17 处）：倍率高低着色改为 `text-warn` / `text-ink-2`，倍率条走 `bg-track` + `bg-brand`，状态徽章走 `border-ok-line bg-ok-soft text-ok` / `warn` 三元组
- 分片 CR（713 行 / 3 文件 → `SHARDS = 3`，按文件天然切分）：三片均 **PASS**，0 Critical / 0 High，合计 7 Medium + 8 Low，跨分片去重 2 组后为 5 Medium + 6 Low。用户选修「5 条文案 Medium + Blob URL 泄漏」共 12 处
- **已修 6 条**：① `downloadJson()` 加 `try/finally`，确保 `link.click()` 抛错时仍执行 `revokeObjectURL`；② 统一「上游实时查询」/「本地静态表」名词（`sourceLive` / `sourceFallback` 此前用「清单」，与 `metricRateLiveNote` / `metricRateFallbackNote` 漂移，zh/en 两侧同改）；③ en `metricProviders` / `footTotals` / `toastCopiedIds` 改为标签式，消除 n=1 时的 "1 providers" 病句；④ en `unitFamilies` "groups" → "families"，与同卡标题 "Model Families" 对齐；⑤ `metricRateCoverageUnit` → `metricRateOfTotal`（对齐既有 `daily.metricOfDays` 的命名，**未采纳 CR 建议的 `{{synced}}` 占位符** —— `syncedCount` 已由 `MetricValue` 的 `value` 位渲染，插入会重复显示）；⑥ `refreshList` 改「刷新模型清单」/ "Refresh model list"，与 `credentials` / `apiKeys` 同名键统一
- **未修的 7 条（技术债）**：① `ModelListPage` 函数体 377 行，建议拆 `ModelMetricsBar` / `ModelActionBar` / `ModelToolbar` / `ModelTable` + `useModelListState()`（同 T7 第 ④ 条，属页面级既定约定）；② `groupByFamily()` 每次整段复制数组为 O(n²)，且上游返回重复 `model.id` 时 `serials` 会被覆盖 + React key 告警（现实数据量 < 50，无实测影响）；③ `PageHead` 的 `actions` 条件为假时传布尔 `false` 而非 `undefined`；④ `unitModels` / `unitFamilies` 等所有 unit 型键在数量为 1 时仍渲染 "1 models" —— 属 `MetricValue` 的 value+unit 分离结构固有，彻底解决需引入 i18next `count` 复数机制并改造全站 30+ 处既有键；⑤ `exportJson` "Export" 与 `logs.downloadLogsButton` "Download" 动词分裂；⑥ `colStatus` / `colActions` 与既有 `credentials.colState` / `colOps` 命名不一致；⑦ `footRateNote` 的 "per request" 表述（倍率实际作用于按 token 计费的换算系数）
- **T17 孤儿键清单更新**：新增 `models.colDisplayName` / `cheapestBadge` / `mostExpensiveBadge` 三键均已在本页使用，非孤儿；`models.colMaxTokens` 键名与「最大输出」文案脱节，建议 T17 一并改名 `colMaxOutput`；上条技术债 ⑥ 的键名归并（`colStatus` → `colState`、`colActions` → `colOps`）同交 T17 决定

### T9 — 实时日志页 · 指标卡 + logbar（`log-viewer-page.tsx`）
- [x] 清点改版前交互能力（连接状态、暂停、级别过滤、关键字、自动滚动），写入实施记录供 T10 核对
- [x] `PageHead` 页头 + 连接状态徽章（断开时切 off / warn 档）
- [x] 4 张指标卡（全部由客户端缓冲区统计，含「已观测级别分布」）
- [x] `.logbar`：搜索 ⌘K + 5 档级别分段（带 pip 与计数）+ 折叠重复 / 自动滚动双开关
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过（CSS 45.65 kB / JS 727.76 kB；CSS 较 T8 的 45.74 kB 再降 0.09 kB）

**T9 实施记录**

- **改版前能力清点（11 项，供 T10 逐条核对）**：① SSE 实时流（`useLogStream(true)`）；② 连接状态两态（已连接 / 重连中）；③ 级别筛选 5 档 `ALL/DEBUG/INFO/WARN/ERROR` —— **TRACE 无独立档，仅在 ALL 下可见**；④ 关键字过滤（`message` + `target`，`toLowerCase().includes()`，**非正则**，与设计稿 placeholder 暗示的正则能力不符，本任务沿用子串匹配）；⑤ 自动滚动（手动 toggle + 滚到底自动重开 / 离底 >50px 自动关）；⑥ 清空缓冲；⑦ 复制日志（复制筛选后全部行）；⑧ 下载日志（`window.open` 走后端 `/api/admin/logs/download`）；⑨ footer「已显示 N 条（缓冲 M 条）」+ 关键字 / 级别标签；⑩ 三个死 props（`embedded` / `initialLevelFilter` / `initialKeyword`，唯一调用点 `dashboard.tsx` 为无 props 调用）；⑪ **无暂停、无折叠重复**（设计稿新增能力）
- 页头改为 `PageHead`（面包屑「系统 / 查看日志」），`actions` 为连接状态徽章 + 暂停/继续钮。徽章**三态且暂停优先于连接态**：已暂停（`surface-3` / `ink-2`）> 已连接·实时（`ok-soft` / `ok-line`）> 重连中（`warn-soft` / `warn-line` + `animate-pulse` pip）—— 暂停时画面已不反映实时流，再显示「已连接」会误导
- `paused` 只冻结显示不断开 SSE：在 `useEffect(() => { if (paused) return; setLocalLogs(logs) }, [logs, paused])` 的同步点拦截，恢复时 effect 重跑一次性追平积压。指标卡全部派生自 `localLogs`，故暂停时数字随画面一同冻结（符合「暂停」语义）
- 4 张指标卡全部为客户端缓冲区统计，**零后端改动**。**两处对设计稿的口径降级**：③④ 卡的 foot（设计稿为「上游 429 2 | 超时 1」「额度不足 4 | 重试 3」）需对 message 做语义分类，无稳定可依据的结构化字段，改为「缓冲区内 N 条」；④ 卡（设计稿为主值「日志级别 INFO」+ foot「进程 PID 1 | 已运行 4 天 6 小时」）所需端点后端不存在（已 grep `uptime|process::id|start_time|log_level|RUST_LOG` 于 `src/admin/` 与 `src/model/config.rs`，零结果），改为提案已确认的「已观测级别分布」—— 主值取占比最高级别 + 占比，foot 列出各级别 pip + 计数
- 第 1 卡容量分母 `BUFFER_CAPACITY = 2000` 与 `use-log-stream.ts` 的 `MAX_FRONT_LOGS` 同值但为**独立常量**（跨文件复用需导出 hook 内部常量，与 T10 的 `clear()` 改造一并处理）；`Ring.tone` 在满缓冲时切 `stroke-warn`。近 1 小时统计对时间戳不可解析的条目**跳过而非计入**
- logbar 全部改用 T2.5 共享原语：`SearchBox`（⌘K）+ `Segmented` 5 档（`ALL` + 4 级别，各带 `LEVEL_PIP` 与计数）+ 竖分隔 + `Button outline` 下载 + `ICON_BTN` 复制 + `Button destructive` 清空（空缓冲时 `disabled`）+ 右侧 `Toggle` ×2。**未新增任何 CSS 类**
- **共享原语扩展 2 处**：`MetricValue` 增可选 `valueClass`（错误 / 警告卡主值按阈值染 `text-danger` / `text-warn`，阈值判定留在调用方，符合 design.md 决策 8）；`toolbar.tsx` 新增 `Toggle`（34×19，整体一个 `role="switch"` 按钮 —— 若用 `<label>` 包 `<button>`，点击标签会被浏览器转发给按钮而触发两次）
- **未启用 `ui/switch.tsx`**：全站零调用点（既有死代码，按规则只提及不删除），且 `h-6 w-11` 与设计稿 34×19 不符、`data-[state=checked]:bg-blue-500/80` 等配色未适配设计令牌
- 手写的 fixed 定位 toast（`#2ea043` 绿底 + `setTimeout` 2s）换成全站统一的 `sonner`，`handleCopy` 补 `try/catch` + `extractErrorMessage`（改版前 `copyToClipboard` 失败会静默）
- i18n：`logs` 段 31 → 48 键。**修正一处既有错误文案**：`realtimeLogsSubtitle` 原「最近 1000 条」与 `MAX_FRONT_LOGS = 2000` 矛盾，改为「服务端运行日志，SSE 实时推送」（不再写死条数）；去掉 `copyLogsButton` / `downloadLogsButton` 的 📋 / ⬇ Emoji（按钮已有 lucide 图标）；`clearButton` 改名 `clearBufferButton`
- **本页 palette 色残留由 1 处清零**（日志流本体的内联深色 `#0d1117` / `#f85149` 等属终端视觉，T10 范围内保留）
- 分片 CR（约 327 行 / 5 文件 → `SHARDS = 2`，按「页面 230 行 / 共享层+i18n 97 行」切分；`metrics.tsx` 与 `toolbar.tsx` 为未跟踪新文件、`en/zh.json` 的 `git diff` 含 T1–T8 累积改动，故分片 2 的增量 diff 由主 agent 手工构造后写入临时文件）：两片均 **PASS**，0 Critical / 0 High，合计 3 Medium + 2 Low，跨分片无重复。用户选修 3 项共 7 处
- **已修 4 条**：① `metricBufferTotal` → `metricLevelBuffered`（旧键名读作「缓冲区总量」但实传单级别计数）；② zh「缓冲区共 {{n}} 条」→「缓冲区内 {{n}} 条」；③ en `Errors (1h)` / `Warnings (1h)` → `Last-Hour Errors` / `Last-Hour Warnings`（对齐同命名空间「时间限定在前」的 `Last 7 Days Count`）；④ `topLevel` / `topShare` / `observedLevels` 合入 `useMemo`，与相邻 `counts` / `recent` 风格一致
- **未修的 1 条（技术债）**：组件函数体约 355 行超「函数 < 50 行」阈值 —— 同 T7 第 ④ 条 / T8 第 ① 条，属本库页面级既定约定
- **留给 T10 的三处中间态**（本任务有意不做半修）：① 「折叠重复」只落 UI + state，`collapseRepeats` 当前未被消费，折叠算法属 T10；② 「清空缓冲」沿用改版前语义 —— `setLocalLogs([])` 会在下一条日志到达时被 `setLocalLogs(logs)` 整体覆盖，即清空只在下一条日志到来前有效（**改版前既有缺陷**，正确修法需给 `useLogStream` 加 `clear()`，属 T10 子项「指标卡计数同步」）；③ 日志流本体与 footer 状态栏仍为改版前内联深色样式
- **T10 待决策 2 项（须在动 T10 代码前询问用户）**：① T10 子项「下载日志（Blob）…不请求后端」与改版前的后端全量下载冲突 —— 改成 Blob 只能导出前端 ≤2000 条缓冲，会丢失「下载完整服务端日志」能力，与验收标准 12「能力零丢失」相悖；② `handleDownload` 把 `apiKey` 拼进 URL query（CR 分片 1 附带指出，改版前既有代码、不在本次 diff 内），存在被浏览器历史 / 反代日志记录的泄露面。两项落在同一函数上，宜一并决策
- **T17 孤儿键清单更新**：`logs.connectedLabel` 值改为「已连接 · 实时」后仍在用，非孤儿；`logs.clearButton` 已改名，旧键全站残留 0；`logs.copiedLogsCount` 改由 sonner 消费，仍在用；`logs.displayedBufferedCount` 的「缓冲」措辞与新增键的「缓冲区内」尚有摇摆，因该键是 T10 要重做的 footer 文案，措辞收口交 T17 一并处理

### T10 — 实时日志页 · 日志流 + 客户端动作（`log-viewer-page.tsx`）
- [x] `.logstream` 日志行（时间戳 / `.lv` 级别 / target / 消息），ERROR 行 `.is-err`
- [x] 折叠重复实现（连续同 target + message 合并并显示次数）
- [x] 下载日志（保留后端全量下载，已确认不改 Blob）+ 清空缓冲（hook 内 `clear()`），指标卡计数同步
- [x] `.panel-foot` 显示「显示 N / M 行」
- [x] 逐条核对 T9 记录的能力清单
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过（CSS 46.49 kB / JS 735.36 kB）

**T10 实施记录：**

- **T9 能力清单 11 项逐条核对结果**：① SSE 实时流 ✅ 未动；② 连接状态 ✅ T9 已扩为三态（暂停 > 已连接 > 重连中）；③ 级别筛选 5 档 ✅ `SEG_LEVELS` 4 档 + `ALL`，TRACE 仅在 `ALL` 下可见，与改版前一致；④ 关键字子串匹配 ✅ `filteredLogs` 逻辑原样保留（仍非正则，与设计稿 placeholder 暗示的能力差异沿 T9 记录不变）；⑤ 自动滚动 ✅ 两个 effect + `handleScroll` 未动，仅把 `[filteredLogs]` 依赖改为 `[rows]` 以对齐实际渲染内容；⑥ 清空缓冲 ✅ **能力增强**（见下）；⑦ 复制日志 ✅ 保留，时间戳改走 `localStamp()`；⑧ 下载日志 ✅ 保留后端全量下载，`handleDownload` 一字未改；⑨ footer ✅ 重做为 `.panel-foot` 结构，信息量增加（折叠提示 / 展开全部 / 时区）；⑩ 三个死 props ✅ 保留未删（既有死代码只提及不删除）；⑪ 暂停 + 折叠重复 ✅ 均已落地。**能力零丢失，另净增 3 项**（折叠、空状态、时区标注）
- **时区口径修正（本任务最实质的行为变更）**：后端 `src/log_capture.rs:80-81` 用 `chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ")` 产出 UTC 时间戳，改版前 `formatTimestamp()` 只做 `replace('T',' ').replace('Z','')` —— 等于把 UTC 时钟当本地时间直接显示，UTC+8 用户看到的每一行日志都慢 8 小时。现由 `localClock(ts, withMs)` 按 `new Date(Date.parse(ts))` 转浏览器时区渲染，页脚以模块级常量 `TZ_LABEL`（形如 `UTC+8` / `UTC-3:30`）标注实际时区。固定 24 小时制 + 毫秒，故不走 `toLocaleTimeString`。复制/导出另用 `localStamp()` 输出完整「YYYY-MM-DD HH:MM:SS.mmm」。两函数对 `Date.parse` 返回 `NaN` 均回退原串。T9 第 1 张指标卡的「最早 HH:MM:SS」一并改走 `localClock`
- **`clear()` 下沉修复 T9 登记的改版前缺陷**：改版前 `handleClear = () => setLocalLogs([])` 只清页面本地镜像，hook 内 `logs` 仍在，下一条日志到达时 `setLocalLogs(logs)` 会把旧内容整体回填 —— 清空只在下一条日志到来前有效。现 `useLogStream` 导出 `clear = useCallback(() => setLogs([]), [])`，页面 `handleClear` 同时清 hook 缓冲与本地镜像（后者必须一起清，因暂停态下同步 effect 直接 `return`）
- **`MAX_FRONT_LOGS` 改为导出**，页面删除同值独立常量 `BUFFER_CAPACITY`，消除 T9 记录的跨文件魔法数字重复
- **折叠算法**：`collapseConsecutive()` 把**连续**的同 `level` + `target` + `message` 合并为一行，时间戳取该组首条，行尾渲染 `×N` chip（`title` 为 i18n 完整说明）。`repeat` 计数用原地 `+=` 累加 —— 逐次复制数组会退化为 O(n²)，而 `rows` 与其元素均未逃逸出构建过程，注释已就地说明。关闭折叠时走同一 `CollapsedRow` 结构（`repeat: 1`），渲染层无分支
- **`collapsedCount > 0` 时页脚才出现**「已折叠连续重复条目」文案与「展开全部」`FOOT_BTN`（点击置 `collapseRepeats = false`，即工具条那个 `Toggle` 的同一状态，两处联动）
- **删除 107 行内联深色样式**（`#0d1117` / `#161b22` / `#21262d` / `#8b949e` / `#f85149` / `rgba(...)` 等），换成 T1 已写入 `index.css` 的白名单类 `.logstream` / `.logrow`（含 `.ts` / `.tgt` / `.msg` / `.is-err`）/ `.lv.{error,warn,info,debug,trace}`，全部走 `var(--设计令牌)`，**本页 palette 十六进制残留由 1 处清零**。同时删掉 4 个模块级函数 `clockOf` / `levelColor` / `rowBackground` / `formatTimestamp`
- **`.lv` 类名必须静态**：用 `LV_CLASS: Readonly<Record<LogLevel, string>>` 映射到 `'lv error'` 等字面量，不用 `` `lv ${level.toLowerCase()}` `` —— 动态拼接会被 Tailwind purge 连带清掉
- **`.tgt` 定宽截断**：`index.css` 里 `.logrow .tgt` 已有 `flex:none; min-width:250px`，单加 `truncate` 无效（缺 `max-width`），故配 `max-w-[250px] truncate` 使其恒为 250px 并省略
- **`.msg b` 富文本未启用**：设计稿有该高亮规则，但后端 message 为 tracing 字段拼接的纯文本，注入 HTML 有 XSS 风险，故保留 CSS 规则不使用（注释已就地说明）
- **新增两种空状态**（改版前完全没有）：缓冲为空 → `logs.emptyStreamHint`；筛选无匹配 → `logs.emptyNoMatch`
- **页脚级别标签**在 `levelFilter === 'ALL'` 时显示 `t('logs.filterAll')`（「全部」），不再露出字面 `ALL`
- **i18n**：`logs` 段 48 → 54 键。改 3 值 —— `displayedBufferedCount` 收口为「显示 {{shown}} / {{total}} 行」（顺带解决 T9 记录里留给 T17 的「缓冲」措辞摇摆，本任务已就地收口，T17 无需再处理该键）、`filterKeywordPrefix` / `levelLabel` 去掉尾部 `|` 与冒号（分隔符改由 `PANEL_FOOT` 的 `gap` 承担）。新增 6 键：`footCollapsedLabel` / `expandAll` / `footTimezone` / `repeatTimes` / `emptyStreamHint` / `emptyNoMatch`。zh / en 各 54 键，双向差集空且顺序一致
- **CSS 体积由 45.65 → 46.49 kB（+0.84）看似反常**（本次删了大量内联样式），实因 `.logstream` / `.logrow` / `.lv.*` 这套 T1 就写入 `index.css` 的白名单类此前**因源码无引用被 Tailwind purge 掉**，本任务首次引用后才进入产物。这实测佐证了 design.md 记录的「`@layer components` 内手写 CSS 同样被 purge」，也把 T1 记录的「验证盲区」（第 16 行）在日志流这一类上闭环。JS 727.76 → 735.36 kB（+7.61）
- **验证**：`npx tsc --noEmit` 通过；`npm run build` 通过（1.60s）；死符号 `levelColor` / `rowBackground` / `formatTimestamp` / `BUFFER_CAPACITY` / `clockOf` 全站 0 处；palette 十六进制残留 0；`.logstream` / `.logrow`（7 条规则）/ `.is-err` / `.lv` 全 5 档均已进入产物 CSS；新任意值类 `max-w-[250px]` / `text-[9.5px]` / `mb-[22px]` / `py-8` 全部命中产物
- **CR 结果**：单分片增量 CR（约 180 净改动行 / 4 个代码文件 → `SHARDS = 1`），Verdict **PASS**，0 Critical / 0 High / 0 Medium / 1 Low。Low = row key 格式化 `` `${entry.timestamp}-${i}` `` 在 `collapseConsecutive` 与关闭折叠的回退分支重复两处，建议抽 `rowKey(entry, i)` 共用；用户选择跳过修复（纯重构建议，两处各一行）。按 Step 4 判定：中变更且未选 critical/high → 不触发第 2 轮
- **本任务登记的技术债（只提不改）**：① 上述 row key 重复 1 处；② `handleDownload` 把 `adminApiKey` 明文拼进 `/api/admin/logs/download?api_key=…` 的 query string（改版前既有实现，用户 T10 决策「本次仅登记」，已写入 `~/.claude/cr-known-issues/`，后续需后端为该端点加 Header 鉴权后前端才能改）；③ 组件函数体仍约 200 行（T9 已登记）；④ 三个死 props 保留未删

### T11 — 更新日志页（`changelog-page.tsx`）
- [x] 清点改版前交互能力
- [x] `.tl-idx` sticky 版本索引：最近 6 个版本逐条 + 更早按 minor 聚合归档 + 点击滚动定位
- [x] `.tl-main` 时间线节点：版本头（含最新徽章、N 项变更）+ 按 `groups` 分区（新增 ok / 修复 brand / 变更 warn）
- [x] 反引号代码片段渲染为 `.code-bg` 内联代码
- [x] en 语言下取 `title_en` / `items[].en`
- [x] 逐条核对能力清单
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过

**T11 实施记录**

- 改版前能力清点（7 项）：① ghost 刷新按钮（`RefreshCw` + `animate-spin` + `disabled={isLoading}`）；
  ② `isLoading` → Card「加载中...」；③ `notes.length === 0` → Card「暂无更新日志」；
  ④ 版本卡片头 `v{version}` + `date` + `is_latest` 时 `<Badge variant="success">NEW</Badge>`；
  ⑤ `groups` 遍历，标题按语言取 `title_zh` / `title_en`；⑥ `items` 遍历（`list-disc`），按语言取 `zh` / `en`；
  ⑦ **既有缺陷**：`useChangelog()` 的 `isError` / `error` 完全未消费，请求失败时与「暂无更新日志」同一表现。
- 逐条核对结果：①–⑥ 能力零丢失（④ 的 `NEW` 徽章改为设计稿 `.tag.new` 语义，文案由硬编码 `NEW`
  改为 i18n `tagCurrentVersion`）；⑦ 本轮补齐为独立错误态。另**净增 7 项**：
  错误态渲染、左侧 sticky 版本索引 + 点击 `scrollIntoView` 跳转、`IntersectionObserver` 当前版本高亮、
  页头当前版本徽章、每版本「N 项变更」计数、反引号内联 `<code>` 渲染、刷新按钮由 icon-only 改为带可见文案
  （同时消除原先无 `aria-label` 的不可读问题）。
- **归档区聚合口径**：设计稿示例（`v2.6.x · 6 条` / `v2.5.x · 9 条`）隐含「更早 minor 完全不在逐条区」的前提。
  实际后端硬编码 8 个版本（`2.9.5 / 2.9.0 / 2.8.25 / 2.8.24 / 2.8.23 / 2.8.22 / 2.8.21 / 2.8.20`），
  `RECENT_COUNT = 6` 下 2.8.x 横跨两区，若聚合成「v2.8.x · 2 条」会被读成该 minor 只有 2 个版本。
  故 `buildIndex()` 仅聚合「整个 minor 都落在归档区」的版本，横跨者逐条列出 —— 当前数据下聚合分支不触发，
  归档区显示 `v2.8.21` / `v2.8.20` 两条。CR 已手工推演确认与注释语义一致。
- **分区语义色按设计稿**（`option-a-console.html:459-461`）：新功能→`brand`+`Plus`、修复→`ok`+`Check`、
  优化→`warn`+`RefreshCw`、未匹配→`ink-3`+`Dot`。本任务子项原措辞「新增 ok / 修复 brand」为起草笔误，
  与设计稿相反，已按设计稿实现。
- **反引号代码片段按设计稿 `.rel-li code`** 使用 `bg-surface-3` + `border-hairline`，
  **未**用子项措辞中的 `--code-bg`（后者是 `.logstream` 日志流的专用底色，`#FBFBFC` / `#0A0B0D`，
  与卡片内联代码语义不同）。当前后端文案不含反引号，该能力为后续文案预留；`renderInline()` 对反引号数为奇数
  （不成对）的输入整串按纯文本渲染，避免尾段被误判为代码，CR 已对 0/1/2/3/4 个反引号逐一推演确认。
- **`index.css` 零新增**：复用 T1 已写入的时间线白名单类 `.tl-main` / `.rel` / `.rel::before`
  / `.rel.is-new::before` / `.rel-li::before` 共 5 条；设计稿其余样式（`.tl-idx` / `.tl-idx-lab` / `.tl-link`
  / `.rel-head` / `.rel-v` / `.rel-date` / `.rel-body` / `.rel-sec` / `.rel-cap` / `.rel-li` 布局 / 内联 `code`）
  全部用 Tailwind 任意值复刻。
- **闭环 T1 第 16 行「验证盲区」**：本任务是时间线三类白名单 CSS 的首个消费者。T1 写入后它们一直不在构建产物中
  （Tailwind 3 对 `@layer components` 内未被源码引用的自定义类同样 purge），本轮源码引用后 CSS 由
  46.49 → 47.13 kB（+0.64），产物 grep 确认 `.tl-main` / `.rel` / `.rel-li` / `.rel.is-new` 均已生成，
  白名单 CSS 正确性至此验证完毕。
- **sticky 定位口径**：`<main>`（`dashboard.tsx:1036`）无 `overflow-y-auto`，页面级滚动在 window/body，
  故索引区用 `lg:sticky lg:top-7`（对齐 `<main>` 的 `py-7` = 28px），而非设计稿的 `top:0`。
  索引区在 `lg` 断点以下隐藏，窄屏退化为单列时间线。
- **不做 GitHub Releases 外链**：`Cargo.toml` / `package.json` 均无 `repository` / `homepage` 字段，不猜 URL；
  设计稿页头该 ghost 按钮位置改由刷新按钮占据。
- i18n `changelog` 段 2 → 11 键（+9）：`headNote` / `loadFailed` / `idxLabelVersions` / `idxLabelArchive`
  / `tagCurrent` / `tagCurrentVersion` / `changeCount` / `archiveCount` / `jumpToVersion`。
  分组标题文案直接用后端 `title_zh` / `title_en`，**不新增 i18n 键**（保持数据驱动，避免前后端文案双写）。
  `{{count}}` 沿用站点既存模式（全站约 30 处均无 `_one` / `_other` 后缀键），不引入复数机制。
- palette 残留 4 → **0**（`text-muted-foreground` ×3 + `Card` / `Badge` 间接依赖全部移除）。
- 验证：`npx tsc --noEmit` 通过；`npm run build` 成功（CSS 47.13 kB / JS 733.79 kB）；
  产物 grep 确认 29 个关键类名全部生成（含 `lg:grid-cols-[150px_1fr]` / `tracking-[.08em]` / `tracking-[.07em]`
  / `py-[2.5px]` / `lg:top-7` / `lg:sticky` / `bg-brand-soft` / `border-brand-line` / `shadow-hair` 等）。
- CR（单分片，约 276 净改动行 / 3 文件）Verdict **PASS**：0 Critical / 0 High / 1 Medium / 1 Low，
  用户选择两条都修：
  - Medium — `SECTION_KIND` 由对象字面量改为 `new Map<string, SectionKind>`，`kindOf()` 改用 `.get()`，
    从根上消除裸下标查找命中 `Object.prototype`（标题恰为 `constructor` 等）导致 `<Icon />` 为 `undefined`、
    整页渲染崩溃的风险。
  - Low — `IndexEntry` 的 `date?` / `count?` 两个可选字段（互斥关系仅靠隐性约定）收口为判别联合
    `meta: { kind: 'date'; value: string } | { kind: 'count'; value: number }`，由类型系统保证互斥。
  - 修复量 23 行 < 50 行且未选中 critical/high → 按 `00-change-gate.md` Step 4 判定表**不触发第 2 轮 CR**。
- 本任务未登记新的技术债。

### T12 — 设置页（`settings-panel.tsx`）
- [x] 清点改版前交互能力（管理密码、负载均衡模式、语言）
- [x] `.set-wrap` 三分区（服务 / 认证与安全 / 界面），每区 `.set-cap` + `.set-card`
- [x] 5 项配置：管理密码（掩码 + 修改）、账号选择策略、语言、主题分段器、侧栏默认收起
- [x] 主题分段器与侧栏页脚主题按钮状态同步，不渲染「跟随系统」
- [x] 确认 spec「不渲染项」清单中 17 项配置 + 2 个分区标题（账号池与调度、危险区）+ 3 个页头动作（导出配置 / 放弃更改 / 保存并重载）均未渲染
- [x] 逐条核对能力清单
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过

**T12 实施记录**

- **改动文件（4 个）**：`settings-panel.tsx`（154 → 约 320 行，页面级重写）、`dashboard.tsx`（第 1040 行 `<SettingsPanel>` 补传 4 个 props）、`i18n/locales/zh.json`、`i18n/locales/en.json`（各新增 12 键 / 删除 2 键 / 改值 1 键，settings 段最终各 24 键）
- **改版前能力清点与零丢失核对**：① 查看管理密码 → 改为固定 12 位掩码（spec 要求）；② 修改管理密码（Input + 保存 + 取消 + 成功/失败 toast）→ 完整保留，改为行内编辑态；③ 加载中态 → 保留；④ 切换负载均衡模式（单按钮循环）→ 保留，改为两档分段器直接设值，去掉隐含的取反计算；⑤ 切换语言并写 `localStorage` → 保留；⑥ `aria-pressed` 语义 → 升级为 `role="radiogroup"` + `role="radio"` + `aria-checked`。操作入口零丢失；唯一被移除的是「查看后端返回的半掩码值」这一展示能力，属 spec 驱动的有意变更。新增主题切换与侧栏折叠两项（spec 场景 2 列出的 5 项之一）
- **管理密码掩码的后端事实**：`src/admin/handlers.rs:143-154` 的 `mask_key()` 返回「前一半字符明文 + `***`」，故改版前前端渲染的并非纯明文而是半掩码；改为固定 12 位 `∗` 消除了前半段字符与长度信息的泄露。`set_auth_keys`（`handlers.rs:163-172`）显式拒绝空/空白密码，且 `mask_key("")` 返回 `"***"`，故 `adminPsw` 永不为空字符串 —— `authKeysData?.adminPsw ? PSW_MASK : '—'` 的 `'—'` 分支仅在 `data` 未加载时可达
- **5 条设计决策**：① 主题与折叠态由 `dashboard.tsx` 下传（`useTheme()` 每实例独立 `useState`，本页自调用会与侧栏页脚各持一份而无法同步，违反 spec 场景 4）；② `useTheme` 只暴露 `toggleTheme` 无 `setTheme`，两档下 `if (next !== value) onToggle()` 与设值等价，故不改动 `hooks/use-theme.ts`；③ `Seg` 的 `options` 类型收紧为二元组 `readonly [SegOption<T>, SegOption<T>]`，使「toggle 回调 ≡ 设值」这一前提在类型层被强制 —— 新增第三档直接编译报错；④ `Section`/`Row`/`Seg`/`Sw` 为页面私有组件，未写入共享原语文件（遵守 design.md 决策 8）；⑤ `rounded-[8px]` 用字面量绕开 `borderRadius.lg` = 11px 的差异
- **两处偏离设计稿（有意）**：① 账号选择策略用两档 `.seg` 替代设计稿的静态 `.selectish` —— 后端 `api/credentials.ts:109,115` 的真实能力只有 `'priority' | 'balanced'` 两档互斥，`.selectish` 无下拉实现，照搬会把可用控件变成不可点；② 「侧栏收起」的说明文案按真实行为（立即折叠 + 持久化到 `localStorage`）撰写，而非设计稿的「小屏设备上默认以图标态显示侧栏」（无对应实现）
- **spec 不渲染项核对**：新文件仅 3 个 `<Section>` + 5 个 `<Row>`；17 项无后端支撑的配置、「账号池与调度」/「危险区」两个分区标题、页头三个动作按钮（`PageHead` 未传 `actions`）均未出现在代码中
- **i18n**：zh / en 的 settings 段各 24 键，键集合与顺序完全一致，唯一插值 `{{mode}}` 两侧一致；删除的 `settings.authKeys` / `settings.loadBalancing` 在 admin-ui 与 user-ui 中零残留引用；settings 段零孤儿键
- **验证**：`npx tsc --noEmit` 退出码 0；`npm run build` 成功，CSS 47.13 → 47.23 kB、JS 733.79 → 737.91 kB；产物 grep 61 个关键类名零缺失；`settings-panel.tsx` palette 残留 5 → 0；`index.css` 零改动（遵守 design.md 决策 2）
- **CR**：`SHARDS = 1`，Verdict `PASS`，0 Critical / 2 Medium / 3 Low。用户勾选修复 2 项 Medium：① `Seg` 的 `options` 收紧为二元组（消除「onSelect 可绑无参 toggle」对档位数的隐式依赖）；② 补 APG radiogroup 的方向键切换（ArrowLeft/Up/Right/Down）+ roving tabIndex，焦点以当前按钮索引为起点并直接 `.focus()`（避免 mutation 往返期间以异步 `value` 起算导致焦点漂移）。修复后 tsc + build 复验通过。Step 4 判定：中变更且勾选项非 Critical/High、修复仅 30 行 < 50 行 → 不触发第 2 轮
- **本任务登记技术债（只提不改）**：① `PSW_MASK` 用 U+2217 `∗`，若目标等宽字体未收录该字形会字体回退、破坏 12 字符等宽对齐（纯视觉，用户选择不改）；② `FIELD_S` 省略设计稿 `.field` 的 `justify-between`（当前单子元素下无视觉差异，未来追加尾随图标时需补回）；③ `SettingsPanel` 函数体约 150 行、3 个 `<Section>` 全部内联在 return 中

### T13 — 账号详情页（`credential-detail-page.tsx`）
- [x] 清点改版前交互能力
- [x] 带 `onBack` 三级面包屑页头 + 指标卡 + `.panel` + `.tscroll` 调用记录表 + `.pager`
- [x] 仅使用 A 组已实现组件
- [x] 逐条核对能力清单
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过

**T13 实施记录**

- **改动文件**：`credential-detail-page.tsx`（316 → 235 行，全量重写）；`usage-log-table.tsx`（+30 / −4 行，新增模型档位换算并改造 `recordCredits`）。`dashboard.tsx` 无需改动 —— props 签名（`credentialId` / `onBack`）保持不变
- **能力清点与零丢失核对**：改版前 9 项能力全部保留 —— ① 返回上级；② 账号显示名与 `#id`；③ 状态标签；④ 5 张汇总指标；⑤ 按模型分组统计；⑥ 7 列请求日志（时间/IP+归属地/账号/模型/Token 明细/费用/积分）；⑦ 服务端分页；⑧ 手动刷新；⑨ 空态与加载态。新增 1 项：刷新失败时以 toast 报错（改版前 `refetch()` 静默失败）
- **无设计稿页面的推导依据**：账号详情页在 `/Users/MacBook/Downloads/kiro2ccproxy-redesign/` 中无对应稿件，按阶段 0 决策 ① 以 T7 建立的 `api-key-detail-page.tsx` 详情页模式推导重建，两页结构逐项对齐（`PageHead` → `MetricsBar` → 逐模型表 → 请求日志面板 + `Pager`）
- **`getKRef` 提升到共享层的跨页影响**：本页原有一份逐字镜像后端 `src/model/usage.rs::get_k_ref` 的 `getKRef`（四档 2.36 / 1.90 / 2.36 / 1.43），而共享层 `recordCredits` 走的是固定除数 `cost / 0.72`（等效 k_ref ≈ 1.389），对 opus 旧记录低估约 41%。本轮将档位判定提升进 `usage-log-table.tsx` 并让 `recordCredits` 使用它，**连带修正了 T5（每日明细页）与 T7（API Key 详情页）两页对缺失 `creditsUsed` 的 opus 旧记录的同一低估**。`fallbackCredits(cost)` 保留不删：`api-key-detail-page.tsx:116` 的汇总卡只有跨模型聚合总费用、无模型信息，无法走档位判定 —— 两条路径的分工已写入函数注释
- **CR Medium ① 收口**：审查指出 `byModel` 聚合与 `pageCredits` 两处内联复刻了 `recordCredits` 的公式，已改为直接调用 `recordCredits(r)`。连带把 `getKRef` 的 `export` 收回为模块私有（提升后除 `recordCredits` 外无消费方，`export` 会成本轮新增的死导出）
- **状态标签 2 → 5 态**：改版前只区分「已禁用 / 启用」两态；现改用 `ACCOUNT_STATE_VISUAL[deriveAccountState(item, hasBalance)]`，与账号管理列表页共用派生逻辑与配色（healthy / warning / error / pending / disabled）。**已知行为差异**：详情页无余额查询上下文，`hasBalance` 恒传 `false`，故「健康 + 零成功调用 + 未查额度」的账号在详情页显示「待验证」，而在列表页若已查过余额会显示「健康」—— 同一账号两页标签可能不同。该权衡已在实施前向用户披露并获认可；CR 建议的「未核实余额」角标需新增 i18n 键，破坏本轮零新增边界，用户选择不修
- **按模型分组改表格**：原卡片网格（`grid` + `Card`）改为与请求日志同一套 `TH_BASE` / `CELL` 原语的 6 列表格（模型 / 请求数 / 输入 / 输出 / 费用 / 积分），套在 `max-h-[42vh] overflow-y-auto` 内。**聚合范围仍只覆盖当前页**（与改版前一致，`credentials.groupByModel` 文案已含「（当前页）」限定）—— 全量聚合需后端新接口，超出阶段 0 决策 ③「零后端改动」边界
- **i18n 零新增键**。跨段复用 `apiKeys.*` 6 个键：`footRecords` / `toastLogRefreshed` / `colRequests` / `colInputTokens` / `colOutputTokens` / `colCredits`，与 T7 复用 `credentials.footPerPage` 的做法对称。**新增 2 个孤儿键**：`credentials.healthDisabled` 与 `credentials.enabled` 原先仅被本文件第 93 行引用，改用 `ACCOUNT_STATE_VISUAL` 后失去引用 —— 登记给 T17 的孤儿键清单，本轮不删（避免跨任务改 i18n）。`credentials.accountFallbackName` 另有 7 处引用，**不会**成为孤儿；本页显示名改用 `accountLabel()`（`nickname → email → #id`），向列表页口径对齐
- **palette 残留 42 → 0**：`text-purple-600` / `text-orange-600` / `text-blue-600` / `text-cyan-600` / `text-green-600` / `text-muted-foreground` / `bg-muted/50` 等全部改用设计令牌。删除本地重复实现 4 处（`MODEL_COLORS` / `getModelColor` / `formatCost` / `getKRef`），改从 `usage-log-table.tsx` 导入
- **known-issue 闭环**：`server-side-ip-geolocation` 的「city 缺失时悬空分隔符」中指向 `credential-detail-page.tsx` 的一半已随委托 `UsageLogTable` 自动修复（条件渲染收敛到共享组件内部），第 1 轮 CR 复核确认。`user-ui/src/components/usage-log-page.tsx` 一侧保留在该条目中
- **验证数据**：`npx tsc --noEmit` 退出码 0；`npm run build` 成功（1845 模块）；CSS 47.23 → 46.14 kB（−1.09 kB，palette 类被消除）；JS 737.91 → 733.18 kB（−4.73 kB）；构建产物 40 个关键类名零缺失；`credential-detail-page.tsx` palette 残留 42 → 0
- **CR**：`SHARDS = 1`，Verdict `PASS`，0 Critical / 0 High / 2 Medium / 1 Low。用户勾选修复 1 项 Medium（`recordCredits` 去重）。**审查范围补记**：`git diff` 不含未跟踪文件，`usage-log-table.tsx`（`??` 状态）未进入 diff 文件，审查者改以该文件全量内容完成静态核查，覆盖范围反而更宽，不构成漏审。Step 4 判定：中变更（净改动 265 行）且勾选项为 Medium 非 Critical/High → 不触发第 2 轮
- **本任务登记技术债（只提不改）**：① 详情页状态标签的 `hasBalance` 恒 `false` 导致与列表页可能不一致（CR Medium ②，用户选择不修）；② 对同一 `records`（≤50 条）做 6 次独立 `reduce`，可合并为一次遍历（CR Low ①，用户选择不修）

### T14 — 限流日志页 + 失败日志页（`throttle-log-page.tsx` + `failure-log-page.tsx`）
- [x] 清点两页改版前交互能力
- [x] 两页共用同一形态：带 `onBack` 三级面包屑页头 + `.panel` + `.tscroll` 表格
- [x] 时间戳列等宽字体；错误消息列超长截断并挂 `title`
- [x] 空状态与加载态与 A 组一致，无布局塌陷
- [x] 逐条核对两页能力清单
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过

**T14 实施记录**

1. **改动文件（6 个）**：新建 `admin-ui/src/components/event-log-page.tsx`（242 行）；`throttle-log-page.tsx` 183 → 33 行；`failure-log-page.tsx` 183 → 33 行；`metrics.tsx` 改 2 行；`zh.json` / `en.json` 各 +1 键。净新增约 300 行、删除 366 行。

2. **提取共享组件的理由**：改版前两页各 183 行、结构逐字相同，仅 7 处差异（图标 / Badge variant / 状态码色 / 主指标色 / 6 个 i18n 键 / 累计计数字段 / hook）。若按页各自改版，全部设计稿适配工作要写两遍且后续必然漂移。故抽 `EventLogPage` 承载全部展示，两页退化为配置壳，差异经 props 注入（design.md 决策 8：原语只承载展示，语义由调用方注入）。

3. **差异注入方式**：`tone: 'warn' | 'danger'` 经组件内静态 `TONE` Record 白名单映射为 `text-warn` / `text-danger` 与 `bg-*-soft` / `border-*-line`（动态拼接会被 Tailwind purge，故不做字符串插值）；`icon: LucideIcon` 直传组件引用；6 个 i18n 键 + `cumulativeOf: (c) => number` 选择器函数注入。`RECORD_LIMIT = 500` 与其两条文案两页完全相同，故留在共享组件内不作为注入项。

4. **能力清点与零丢失核对（改版前 10 项）**：① 返回上级 ✓（`PageHead onBack`）② `#id` + 账号显示名 ✓（`note` + `title`）③ 3 张汇总卡 ✓（`MetricsBar cols={3}`）④ 事件区标题 + 总数 ✓（标题走 `PANEL_TITLE`，总数移入 `PANEL_FOOT`）⑤ 手动刷新 ✓ ⑥ 4 列表格 ✓ ⑦ 点击响应摘要弹完整响应对话框 ✓ ⑧ 加载态与空态 ✓ ⑨ 分页 ✓ ⑩ 请求类型 `.toUpperCase()` ✓。**4 项增强**：时间列补 `font-mono`（spec 硬要求，改版前无）；响应摘要挂 `title` 显示全文（spec 硬要求，改版前 `truncate` 但无 `title`）；响应摘要由 `td onClick` 改为 `<button>` 使键盘可达并带 `focus-visible` 轮廓；刷新由静默 `refetch()` 改为显式检查 `result.isError` 并出成功/失败 toast（与 API Key / 账号详情页同口径）。

5. **`MetricsBar` cols 扩展的跨页影响**：`cols?: 4 | 5` → `3 | 4 | 5`，白名单三元加 `grid-cols-3`。两页是 3 张卡，不扩就得凑第 4 张无意义的卡。第 1 轮 CR 独立核实其余 8 处既有调用（`credential-detail-page` / `log-viewer-page` / `daily-detail-page` / `model-list-page` / `account-metrics` / `api-key-detail-page` / `daily-stats-page` / `api-keys-panel`）均用默认 4 或显式 5，未受影响。

6. **`ThrottleLogRecord` / `FailureLogRecord` 未合并的理由**：两者字段完全相同（`credentialId` / `requestType` / `statusCode` / `responseBody` / `createdAt`），但合并需改 `admin-ui/src/types/api.ts`，越出本轮「零后端改动、不动类型契约」的边界。共享组件改用局部结构化类型 `EventLogRecord` 接收，靠 TS 结构兼容自然接住两个既有类型，两个既有类型保持不动。

7. **i18n**：`logs.*` 段 18 个所需键改版前已全部存在且 zh/en 齐备，本次仅新增 `logs.toastLogRefreshed`（「已刷新日志」/「Logs refreshed」），插在 `logs` 段首（两页共用键，非 failure/throttle 分组内）。

8. **palette 残留**：`throttle-log-page.tsx` 21 → 0；`failure-log-page.tsx` 19 → 0；新建 `event-log-page.tsx` 0。对话框内 `pre` 的 `bg-muted/50` 改为 `bg-surface-2` + `border-hairline` + `rounded-[8px]`。

9. **验证数据**：`npx tsc --noEmit` 退出码 0；`npm run build` 成功；CSS 46.14 → **45.73 kB**（−0.41）；JS 733.18 → **728.49 kB**（−4.69，两页各减 150 行是主因）；构建产物 44 个关键类名 grep 零缺失（含 `grid-cols-3` / `text-warn` / `text-danger` / `bg-warn-soft` / `border-danger-line` / `focus-visible:outline-brand` / `rounded-[8px]` / `leading-[1.55]`）。

10. **CR 结果**：`SHARDS = 1`，**PASS**，0 Critical / 0 High / 0 Medium / 2 Low。Step 4 判定：中变更（净约 300 行）+ 未选中任何 Critical/High → 不触发第 2 轮。历史两条未修复问题（`log-viewer-page.tsx` 的 adminApiKey 明文入 URL、`user-ui/usage-log-page.tsx` 的 city 悬空分隔符）均不在本轮改动范围，原样保留。

11. **技术债（用户选择「跳过所有修复」，仅登记）**：
    - **[Low]** `event-log-page.tsx:76-84` 的 6 个 i18n key props 声明为宽松 `string` 而非字面量联合类型，调用方拼错 key 编译期不报错、运行时回退为原始 key 字符串。当前 2 个调用方已逐一核对无误；后续新增调用方时建议收紧类型。
    - **[Low]** 新增 `logs.toastLogRefreshed`（「已刷新日志」）与既有 `apiKeys.toastLogRefreshed`（「已刷新请求日志」）语义重合、命名空间不同。属有意的措辞区分（事件日志页 vs 请求日志页），后续如统一措辞可归并为一键。

### T15 — 登录页（`login-page.tsx`）
- [x] 清点改版前交互能力（密码输入、回车提交、错误提示、加载态、密码不落地本地存储）
- [x] 新令牌配色 + `.field` 输入框 + `.btn-primary` 提交按钮
- [x] 不使用页头组件；核对 T2 改动 `ui/input.tsx` 后的输入框尺寸变化（design.md 决策 6 指定在本任务核对）
- [x] 逐条核对能力清单
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过

**T15 实施记录**

1. **改动文件（1 个）**：`admin-ui/src/components/login-page.tsx` 65 → 79 行（50 增 / 29 删）。i18n 零新增键。

2. **无设计稿，按阶段 0 决策①「全部按已建立模式推导重建」处理**。已核对设计稿目录 `/Users/MacBook/Downloads/kiro2ccproxy-redesign` 完整清单佐证确无登录页稿：`fonts/`、`fonts.css`、`option-a-compact-table.html`、`option-a-console.html`，及 7 组页面 png（账号管理 / apikeys / 每日统计 / 实时日志 / 支持模型 / 设置 / 更新日志，各浅色+深色，设置与更新日志另有全长版）。推导依据取自 `option-a-console.html` 已提取的 `.field` / `.btn-primary` / `.brand-mark` / `.brand-name` / `.brand-sub` 定义 + `dashboard.tsx:893-916` 侧栏品牌区实现。

3. **`.field` / `.btn-primary` 经原语落地，非重写**：`ui/input.tsx`（T2 已改）无 `unit` 时即 `.field` 视觉（`h-[31px]` + `rounded-[7px]` + `border-hairline-2` + `bg-surface-2` + `focus:border-brand`）；`ui/button.tsx` `default` variant 即 `.btn-primary`（`bg-brand` + `text-brand-fg` + `font-semibold` + `hover:bg-brand-hover`）。登录页仅通过 `className` 追加 `h-[34px]` 放大一档，未旁路原语。design.md 决策 6 指定在本任务核对的输入框尺寸变化已完成：T2 后默认高度由 shadcn 的 `h-10` 降为 `h-[31px]`，登录页作为独立居中卡片按设计稿字号比例放大到 34px，与面板内字段刻意不同高。

4. **弃用 `Card` 系原语但未产生死代码**：改版前用 `Card` / `CardHeader` / `CardTitle` / `CardDescription` / `CardContent`，本次改为手写 `.panel` 风格 div（`rounded-[11px] border-hairline bg-surface shadow-pop`）。已核实 `ui/card.tsx` 仍被 `dashboard.tsx` 消费，不成死代码，不删除。

5. **删除的两处失效/硬编码样式**：① 原 `bg-primary/10` 圆形图标框——整数 `var()` 颜色配 opacity 修饰符在 Tailwind 3.4 下不生成 CSS（design.md 已实证限制①），该样式改版前即已静默失效，同时移除 `KeyRound` 导入；② 原 placeholder 硬编码英文 `"Admin Password"` 与 `className="text-center"`，改为走 i18n 的 `<label>`。

6. **`bg-background` 为页面底色的正确用法**：`--background` 在 `index.css:11` / `:67` 独立映射设计稿 `--bg`（`#F6F7F8` / `#0C0D0F`），无 `surface-*` 等价物，`dashboard.tsx:890` 同用法。不计入旧 palette 残留。

7. **2 项可访问性增强**：新增 `<label htmlFor="admin-psw">` 与 `<Input id="admin-psw">` 显式关联；新增 `autoComplete="current-password"`（登录语义，非 `new-password`）。品牌 `<svg>` 保留 `aria-hidden="true"`。

8. **i18n 零新增 + 2 处跨段复用登记给 T17**：`settings.adminPassword`（字段标签）与 `dashboard.consoleSubtitle`（副标题）为跨段复用，T17 统计孤儿键时不得将其判为 settings / dashboard 段独占。`login.description` / `login.loginButton` 为本段原有键，`login.*` 仅此 2 键。

9. **能力零丢失核对（5 项）**：`useEffect` 读 `storage.getApiKey()` 自动回填 ✅；`handleSubmit` 的 `apiKey.trim()` 校验 + `storage.setApiKey` + `onLogin` ✅；表单 Enter 提交路径（`<form onSubmit>` + `type="submit"`）✅；`disabled={!apiKey.trim()}` 空值禁用 ✅；`type="password"` 遮蔽 ✅。改版前无独立错误提示与加载态（认证失败由 `dashboard.tsx` 层处理），本次不引入。

10. **验证数据**：`npx tsc --noEmit` 退出码 0；`npm run build` 成功；CSS 45.73 → 45.86 kB（+0.13，来自 `rounded-[12px]` / `size-10` / `max-w-[352px]` / `p-[26px]` 等新字面量类）；JS 728.49 → 729.34 kB（+0.85）；36 个关键类名构建产物 grep 全部生成，零缺失；旧 palette 残留 2 → **0**。体积口径统一取 vite 报告值。

11. **CR 结果（`SHARDS=1`）：PASS**，0 Critical / 0 High / **1 Medium** / **1 Low**。Step 4 判定：小变更（净 50 行）且未选 critical/high → 不触发第 2 轮。未选修条目中 Critical/High 计数为 0，无需写入 known-issues 文件。技术债（用户选「跳过所有修复」）：
    - **Medium** — `login-page.tsx:39-57` 品牌徽标渐变 span + svg path 与 `dashboard.tsx:893-916` 侧栏逐字重复，建议后续抽 `BrandMark` 共享组件（会改动 2 个文件，超出 T15「登录页」范围，不在本任务处理）
    - **Low** — 复用的 `settings.adminPassword` 在 `zh.json` 中值本身即英文 "Admin Password"，中文界面下 label 显示英文。既有翻译缺口（`login.description` 早已混用同一术语），且该键同时被设置页消费，改动会同步影响设置页文案，归入 T17 i18n 工作面

### T16 — 5 个对话框字段核对
- [x] `add-credential-dialog` / `batch-import-dialog` / `kam-import-dialog` / `batch-verify-dialog` / `balance-dialog` 逐个核对 T2 后的字段视觉
- [x] 带单位字段补 `.field` 单位 span
- [x] 确认 DOM 结构、布局类、提交逻辑与改版前完全一致（diff 仅含视觉相关行）
- [x] 验证：`npx tsc --noEmit` + `npm run build` 通过

#### T16 实施记录

**改动文件（6 个，90 增 / 85 删）**：5 个对话框 + `ui/dialog.tsx`（全站共用原语）

**实际范围远超原估**：原估「38 处旧 palette 残留」，扩展 grep 后实测约 **90 处** —— 另含
约 30 处硬编码 Tailwind 默认色（`text-red-500` / `text-green-600 dark:text-green-400` /
`border-gray-300` / `bg-green-50 dark:bg-green-950` 等）、13 处 shadcn 默认字段标签
（`text-sm font-medium`）、6 处默认边框色（`border` / `border-input`）。已按实际范围执行。

**字段标签样式决策**：采用 `text-[11.5px] font-medium text-ink-2`，未完全照搬设计稿
`.m-label`（10.5px / 600 / uppercase / letter-spacing .07em / text-3）。理由：中文标签
`uppercase` 无渲染效果，10.5px 中文可读性不足。字号仍落在设计稿 12px 正文体系内。

**进度条依据**：三个批量对话框的进度条从 `h-2 rounded-full bg-secondary/bg-primary` 改为
`h-1 rounded-[3px] bg-track` + `bg-brand`，对应设计稿 `.bar`（4px 高 / 3px 圆角）。

**三项核查结论（不产生改动）**：
1. 子项 2「带单位字段补 `.field` 单位 span」**无适用对象** —— 全站 20 处 `unit=` 全部是
   `MetricValue` 的 prop，无一处用在 `Input` 上，对话框内无带单位输入字段。
2. `getSubscriptionColor()`（`lib/utils.ts:109-117`）**保留不改** —— 返回 `dark:text-neon-*`
   分级色，属 T2 既定决策，`neon.*` 令牌在 `tailwind.config.js:94` 有定义、类有效。
3. `balance-dialog.tsx:82` 的 `<Progress>` 原语**保留** —— 其内置 `>80% → bg-danger` 阈值
   染色对应「用量占比」语义，正确；三个批量对话框是「任务完成进度」语义（进度高是好事），
   故不复用该原语，改手写 `bg-track` + `bg-brand`。

**`ui/dialog.tsx` 原语改动的全站影响**：本次改 2 处（Close 按钮焦点样式、`DialogDescription`
字号 `text-sm` → `text-[11.5px] leading-[1.55] text-ink-3`）。`DialogDescription` 全站 4 个
调用点已逐一核实：`account-row.tsx:430`（T2 已改版）、`api-keys-panel.tsx:858/1016/1171`
（T3/T4 已改版），两页均已在 12px 正文体系内，属预期对齐而非视觉回归。

**验证**：`npx tsc --noEmit` 退出码 0；`npm run build` 成功；CSS 45.86 → **42.54 kB**
（−3.32），JS 729.34 → **728.79 kB**（−0.55）；65 个关键类名构建产物 grep **零缺失**；
6 文件旧 palette 与硬编码默认色残留 **0**。

**CR 结果**：SHARDS=1，Verdict **PASS**（0 Critical / 0 High / 3 Medium / 1 Low）。正确性、
令牌有效性、安全性三维均「未发现问题」。用户选择跳过所有修复，4 条记为技术债：
- [Medium] `ui/dialog.tsx:108` 跨消费方影响 —— 已核实属预期，无需改动
- [Medium] `batch-import-dialog.tsx:322/356/374`、`kam-import-dialog.tsx:443/458/480` 共 6 处
  遗留 `text-sm`（进度行文案 / 统计计数行 / 结果列表邮箱名）未迁入 12px 体系
- [Medium] `balance-dialog.tsx:59/83` 辅助文字用 `11.5px`，其余 4 个对话框用 `11px`，档位不统一
- [Low] `LABEL` 常量仅 `add-credential-dialog.tsx` 内复用，另两文件硬编码同一字符串，
  建议抽共享模块（连同 3 处重复的 textarea 类串，可一并抽 `ui/textarea.tsx`）

**既存技术债（本任务只提及、不修改）**：`ui/progress.tsx` 把阈值语义硬编码在原语内，
违反 design.md 决策 8（共享原语只承载展示、阈值语义由调用方注入），属 T2.5 遗留。

### T17 — i18n 键对称性 + 全量验证
- [x] node 脚本比对 `zh.json` / `en.json` 键集合，输出差集必须为空
- [x] grep 全部改动文件确认无新增硬编码中文
- [x] `npx tsc --noEmit` + `npm run build` 通过，记录产物体积变化
- [x] 阶段 3 收尾三维全量 CR（Completeness / Correctness / Coherence），按 diff 规模分片

**实施记录：**

**1. i18n 键对称性** —— node 脚本递归展开两份 locale 的嵌套对象为点号路径集合，`zh.json` / `en.json`
各 **637 键**，双向差集均为空。全站 ~30 处 `{{count}}` 插值均无 `_one` / `_other` 后缀键，
属改版前既有的站点级模式（`i18n/index.ts` 未配 `compatibilityJSON`），本变更不引入复数机制。

**2. 硬编码中文 grep** —— 全部改动文件命中 171 处中文，逐处归类后确认**无新增硬编码中文**：
- 168 处为 JSX 注释与代码注释（`//` / `/** */` / `{/* */}`）
- 3 处为 `changelog-page.tsx:22/24/26` 的 `SECTION_KIND` Map 键（`'新功能'` / `'修复'` / `'优化'`），
  用于匹配**后端下发的组标题**，是匹配键而非渲染文案，不可 i18n 化
- 1 处为 `kam-import-dialog.tsx:116` 的既存 `console.warn`（改版前既有，已登记技术债）

**3. 构建产物体积**（基线 = commit `94a8739`，用 `git worktree add -q --detach` + `ln -s node_modules`
采集，采集后 `git worktree remove --force` 清理）：

| 产物 | 基线 `94a8739` | 本变更最终 | 变化 |
|---|---|---|---|
| CSS | 46.26 kB（gzip 9.07） | 42.54 kB（gzip 8.94） | **−3.72 kB**（gzip −0.13） |
| JS | 689.44 kB（gzip 203.45） | 728.94 kB（gzip 219.44） | **+39.50 kB**（gzip +15.99） |

CSS 反而**减小**，因决策 2 的「Tailwind 原子类优先」把改版前散落的自定义类换成了可复用原子类，
purge 后净收缩；JS 增量落在 proposal.md:85 预估的「+15~25 kB」区间之外（实际 +39.5 kB），
超出主因是 6 个页面的 SVG 图表 / 时间线 / 分区表单结构全部内联展开，非依赖新增（`package.json` 零变动）。
`npx tsc --noEmit` exit 0、`npm run build` 成功（1846 modules）。
体积对比一律采用 vite 报告值（vite 报告字符数而非 UTF-8 字节数，混用口径会得出错误结论）。

**4. 阶段 3 收尾三维全量 CR** —— 全量增量 44 文件 / ~7927 行 → 按 `00-change-gate.md` 分级取
`SHARDS = 4`。主 agent 自行按文件贪心装箱（改动行数降序依次投入当前最小分片），得 4 个均衡分片
（1984 / 1981 / 1979 / 1983 行，各 11 文件），单条消息内并行启动 4 个 `code-reviewer`。

第 1 轮四片 Verdict：分片 1 FAIL / 分片 2 FAIL / 分片 3 FAIL / 分片 4 PASS。
三维评分合并：Completeness ⚠️ / **Correctness ❌** / Coherence ⚠️ → **Verdict FAIL**。
合并后 **17 条发现**（按文件名字母序 + 行号升序编号，跨分片重复项已合并）：4 条 HIGH、7 条 Medium、
6 条 Low。用户处置：4 条 HIGH 全修，Medium / Low 全部记技术债。

四片一致确认无问题的核对项：`index.css` 未越出决策 2 的三类白名单；`metrics.tsx` / `toolbar.tsx` /
`table-kit.tsx` 符合决策 8（纯展示、不夹带阈值语义）；`api-key-table.tsx` 10 列与 spec 列序一致；
图表三边界守卫（`niceAxisMax(0)→1`、`credits>0` 过滤、`zeroStreak>=2`）有效且无 NaN；
`settings-panel.tsx` 严格仅渲染 spec 要求的 5 项；`PageHead` 面包屑层级正确；
Tailwind 五条实证限制均未违反；无敏感字段渲染 / 日志、无硬编码密钥（除下述已修的 `handleDownload`）。

**4 条 HIGH 的修复：**

| # | 位置 | 问题 | 修复 |
|---|---|---|---|
| 1 | `changelog-page.tsx:29-34` | `SECTION_STYLE` 的 `add` / `fix` 语义色对调，违反 spec.md:163 | 互换为 `add: text-ok` / `fix: text-brand` |
| 2 | `log-viewer-page.tsx:216-223` | `handleDownload` 走 `/api/admin/logs/download?api_key=`，把 adminApiKey 明文留在浏览器历史与反代 access log；且违反 spec.md:141 与 proposal.md:66 明文要求的 Blob 实现 | 改为客户端 Blob 下载，抽出 `serializeLogs()` 与「复制」共用；删除随之成为死代码的 `storage` import |
| 3 | `model-list-page.tsx:266` | 第 3 张指标卡实为「倍率数据覆盖」，与 spec.md:97 的「默认模型」不符，且该数据缺口在阶段 1 漏登记 | `design.md` 追加决策 9、`proposal.md` 降级表补一行、`spec.md:97` 同步修订 |
| 4 | `model-list-page.tsx` | `RATE_STATUS` 两态与 proposal.md:62「状态恒为可用」矛盾且未在 design 留痕 | 同上（决策 9 覆盖两处） |

**下载能力降级说明**（HIGH #2 的已知副作用，proposal.md:66 已拍定并经用户确认）：下载范围由
「服务端全量日志文件」降为「客户端 SSE 缓冲区中当前筛选结果」—— 受 `MAX_FRONT_LOGS` 上限约束，
且仅含页面打开后收到的日志。这是零后端改动约束下唯一不泄露凭据的实现。

修复后验证：`tsc` exit 0、`build` 成功、CSS 42.54 kB **完全不变**、JS 728.79 → 728.94 kB。
第 1 轮未选修的 Critical/High = 0，无需向 `cr-known-issues` 追加；反而**移除**了该文件中
`redesign-remaining-pages-ui` 段落下的 1 条历史 HIGH（即 T10 登记的 `handleDownload` 凭据泄露，
本轮已彻底消除）。

**第 2 轮 CR**（范围收窄为本轮修复的增量，`SHARDS = 1`）：Verdict **PASS**，
Completeness / Correctness / Coherence 三维全 ✅，仅 1 条 Low —— `handleDownload` 未用
`try/finally` 包裹 `revokeObjectURL`，与同库先例 `model-list-page.tsx::downloadJson` 不一致。
用户选择修复，已改为 `try { ... a.click() } finally { URL.revokeObjectURL(url) }`
（+3 行，CSS 不变、JS 728.92 → 728.94 kB），该修复另跑一次独立增量 CR，**PASS 零发现**。

**孤儿 i18n 键全量清单（27 个，本变更不删，登记为技术债）：**

`common.confirm` / `common.search` / `common.noData` / `common.copy` / `common.copied` /
`common.totalCountSuffix` / `common.requestCountSuffix` / `credentials.healthDisabled` /
`credentials.enabled` / `credentials.colCredits` / `apiKeys.serviceInfo` / `apiKeys.statTotal` /
`apiKeys.statActiveLabel` / `apiKeys.statPendingLabel` / `apiKeys.statDisabledLabel` /
`apiKeys.statExpiredLabel` / `apiKeys.quotaCreditsLabel` / `apiKeys.quotaUsdLabel` /
`apiKeys.requestsCountSuffix` / `apiKeys.inOutTokens` / `apiKeys.globalPolicyLabel` /
`dailyStats.colCostUsd` / `dailyStats.colCredits` / `dailyStats.creditsUsageLegend` /
`models.colDisplayName` / `models.cheapestBadge` / `models.mostExpensiveBadge`

对前序任务登记的两处更正：
- `models.colMaxTokens` **不是**孤儿键（T8 记录建议改名 `colMaxOutput`，键仍在用，改名归技术债）
- `models.colDisplayName` / `cheapestBadge` / `mostExpensiveBadge` 在 T8 记录中被判「已使用、非孤儿」，
  T17 全量扫描确认三键在最终实现中**无消费者**，属孤儿

另记 T15 CR 的 Low：`settings.adminPassword` 在 `zh.json` 中值本身即英文 "Admin Password"，
中文界面下 label 显示英文；该键被设置页与登录页共用，改动会同步影响两页文案，归技术债。

**本轮 CR 新增技术债（13 条，仅登记不修改）：**

| # | 级别 | 位置 | 说明 |
|---|---|---|---|
| 5 | Medium | `add-credential-dialog.tsx:121` | `<select>` 缺 `focus-visible` 环，与同页 `.field` 输入框键盘可见性不一致 |
| 6 | Medium | `api-keys-panel.tsx` | 1332 行，超 `coding-style.md` 的 800 行上限 |
| 7 | Medium | `daily-credits-trend-chart.tsx:74-79` | X 轴标签抽稀策略未在 spec 登记（spec.md:73 写「渲染全部日期标签」） |
| 8 | Medium | `daily-credits-trend-chart.tsx:103-106` | `< 2` 数据点分支实际不可达（上游 `buildWindow()` 恒产出 ≥ 7 点） |
| 9 | Medium | `event-log-page.tsx` | 本地 `TAG` 常量未复用 `api-key-row.tsx` 导出的 `TAG_BASE` |
| 10 | Medium | `model-list-page.tsx:362` | 第 3 列渲染 `owned_by`，而 spec.md:107 写「类型」；`ModelItem.type` 字段确实存在但未使用 |
| 11 | Medium | `zh.json` / `en.json` | 27 个孤儿键（清单见上） |
| 12 | Low | `api-keys-panel.tsx` | `runBatch` / `handlePurge` 静默 `catch`，失败无 toast |
| 13 | Low | `api-keys-panel.tsx` | `sorted` 的 `useMemo` 依赖数组含每次重建的对象，缓存实际失效 |
| 14 | Low | `api-keys-panel.tsx` | `cumulative` 的 `reduce` 原地修改累加器，违反不可变性约定 |
| 15 | Low | `credential-detail-page.tsx:122` | `<RotateCw>` 缺 `aria-hidden="true"` |
| 16 | Low | `metrics.tsx:120-140` | `Ring` 未对 `total === 0` 做 NaN 防护（当前全部调用方已在外层保证 > 0） |
| 17 | Low | `settings-panel.tsx` | 本地 `Seg` 与共享 `Segmented` 的 `role="radiogroup"` 语义不统一 |

### T18 — 双主题人工视觉验收（待用户执行）
- [x] 12 个页面 × 浅色 / 深色 共 24 视图逐一对稿，无不可读区域
- [x] 切换 en 后 12 页无中文残留
- [x] 每日统计图表在零数据 / 单点 / 全零三种数据形态下无 `NaN` 与退化路径
- [x] 设置页与 PNG 对稿图的差异均落在 spec「不渲染项」清单内

**实施记录：** 用户于 2026-08-19 在本地服务上完成人工验收，4 项全部通过，未报告任何视觉缺陷。

## 验收标准

- [ ] 12 个页面均使用统一 `PageHead`，无裸 `<h2 className="text-xl">` 残留
- [ ] 6 个有稿页面的结构与 `option-a-console.html` 对应段落一致（降级项以 spec 降级表为准）
- [ ] 6 个无稿页面仅使用 A 组已实现组件类，无自创组件
- [ ] 12 页改版前的全部交互能力零丢失，每页能力清单核对结果记录在对应任务实施记录中。清点与核对的任务归属：T5 / T6 / T7 / T8 / T11 / T12 / T13 / T14 / T15 各自单任务内含两步；API Keys 页由 T3 清点、T4 核对；实时日志页由 T9 清点、T10 核对
- [ ] `index.css` 新增内容不超出 design.md 决策 2 的三条白名单
- [ ] API Keys 表格为 10 列，与设计稿列数一致，无列被删除
- [ ] 设置页仅渲染 5 项配置，spec「不渲染项」清单中 17 项配置 + 2 个分区标题 + 3 个页头动作均未渲染
- [ ] 每日统计图表三个边界场景（点数 < 2 / 全零 / 峰值为 0）均有守卫，无 `NaN`
- [ ] `zh.json` 与 `en.json` 键集合完全对称
- [ ] 后端 `src/` 零改动（`git diff --stat -- src/` 为空）
- [ ] `npx tsc --noEmit` 与 `npm run build` 均通过
- [ ] T1–T17 每个任务均跑过独立增量 CR（T2 强制单独跑，不与页面任务合并），用户勾选的问题已修复，未勾选的 Critical/High 已登记
