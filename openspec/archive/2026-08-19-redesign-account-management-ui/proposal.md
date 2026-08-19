# 变更提案：redesign-account-management-ui

## 背景

`admin-ui` 账号管理页当前为「卡片列表 + 暖橙(light)/紫(dark) 主题」，每张卡片把 8 个读数塞进 3 行文本流，多账号无法纵向对比；统计区 4 张卡片中「今日用量」常为空态 `—`，「总数 / 可用」信息重复。

设计稿目录 `~/Downloads/kiro2ccproxy-redesign/` 提供了方案 A（紧凑表格型控制台）的完整实现参考：
- `option-a-console.html` — 7 页全量原型，含完整设计令牌（CSS 自定义属性）与组件 CSS，账号管理页位于 `#p-pool`（621–1078 行）
- `kiro2ccproxy-方案a-账号管理-{浅色,深色}.png` — 双主题视觉基准

设计语言：IBM Plex Sans / IBM Plex Mono，墨绿松石（teal）单一强调色 + ok/warn/danger 三态语义色，hairline 分层，深色态靠真实表面层次（`surface` / `surface-2` / `surface-3`）而非 1px 描边撑结构。

## 目标范围

**在范围内：**

1. **设计令牌体系落地（共享外壳级）**
   - `admin-ui/src/index.css`：`:root` / `.dark` 重写。现有 shadcn HSL 令牌（`--background` / `--card` / `--border` / `--primary` / `--muted-foreground` …）重映射到设计稿调色板；新增设计稿原生令牌（`--surface-2/3`、`--sidebar`、`--hairline`、`--hairline-2`、`--ink/-2/-3`、`--brand{,-hover,-soft,-line,-fg}`、`--ok/warn/danger{,-soft,-line}`、`--track`、`--code-bg`、`--shadow-sm/md/pop`、`--grid-dot`）
   - `admin-ui/tailwind.config.js`：暴露上述令牌为 Tailwind 颜色 / 阴影 / 字族
   - `admin-ui/index.html`：Google Fonts 字族由 `Inter + JetBrains Mono` 切换为 `IBM Plex Sans + IBM Plex Mono`
2. **共享外壳（侧栏）重构** — `dashboard.tsx` 内的 `<aside>`：品牌区（渐变 brand-mark + 收起按钮）、导航项 teal 激活态（`accent-soft` 背景 + 左侧 2.5px 条）+ 右侧计数、分组标题（主要 / 系统）、**新增当前登录身份区**（头像 `A` + `admin` + `超级管理员` + 退出登录）、页脚（运行状态点 + 版本号 + 主题切换）、64px 收起态（文案降级为 hover tooltip、分组标题降级为分隔线）
3. **账号管理页正文重构** — 页头（面包屑 + 标题 + 副标题 + `自动刷新 30s` 标签 + 文档入口）、4 段指标条（竖分隔 + 环形图 + sparkline）、操作条（6 项常驻，危险操作竖线隔离并染红）、工具栏（搜索 + 状态分段筛选 + 更新时间）、10 列紧凑表格（表头吸顶、列排序、行 hover、禁用行整行降权）、面板脚（已选计数 + 批量操作 + 总数 + 分页 + 每页）
4. **新增前端交互**（全部本地实现，零后端改动）
   - 搜索：按 昵称 / 邮箱 / 账号 ID 过滤，`⌘K` / `Ctrl+K` 聚焦（**不含 Profile ARN**——见下方数据缺口表，后端只返回 `hasProfileArn: boolean`，无 ARN 字符串可匹配）
   - 状态分段筛选：全部 / 健康 / 异常 / 禁用 / 待验证，每段带实时计数
   - 列排序：账号（昵称）、剩余额度（已查询余额优先，未查询排末位）
   - `自动刷新 30s` 标签绑定 `useCredentials` 既有 `refetchInterval: 30000`
5. **分页容量**：每页 12 → 50（表格行高约为卡片的 1/2，与设计稿脚注 `每页 50` 对齐）
6. **删除 `credential-card.tsx`** — 被表格行替代后成为死代码；其内联的删除确认对话框与编辑对话框调用迁移至新表格行组件
7. **i18n** — `zh.json` / `en.json` 同步新增键（面包屑、指标条标签、列名、状态标签、筛选段、面板脚、行操作 tooltip）

**不在范围内：**

- 其他 6 个页面（API Keys / 每日统计 / 支持模型 / 查看日志 / 更新日志 / 设置）的布局重构。它们通过共享令牌自动继承新配色，但结构、间距、组件不动
- `user-ui`（用户端前端）
- 任何 Rust 侧改动：无新增 / 修改 API 端点、无数据模型变更
- 账号详情页、限流日志页、失败日志页、各类对话框（Add / BatchImport / KAM / BatchVerify / Edit）的内部布局 —— 仅保留 / 新增指向它们的入口，页面本身不动
- **当前账号标识**：`CredentialStatusItem.isCurrent` 字段虽存在，但现有 UI 从未渲染它（全仓库仅 `types/api.ts:16` 一处声明）。设计稿也无对应元素。本次不新增该标识，避免范围蔓延
- **Profile ARN 值的展示与复制**：后端不返回 ARN 字符串，仅保留现有的「已配置 ARN」布尔徽章

## 技术方案

### 令牌双轨制

保留 shadcn HSL 三元组命名（`--background: 210 13% 97%`）以免破坏 40+ 处既有 `bg-background` / `border-border` 用法；同时新增设计稿原生令牌为**完整颜色值**（hex / rgba），因为深色态 `*-soft` / `*-line` 是半透明 rgba，无法用 `hsl(var(--x))` 表达。

命名避让：设计稿的 `--accent`（teal 品牌色）与 shadcn 的 `--accent`（hover 表面色）语义冲突，故品牌色统一命名为 `--brand*`，shadcn `--accent` 降级为 `surface-3` 同值。

### 组件拆分

`dashboard.tsx` 已 1181 行，本次不再向其堆叠。正文拆为 4 个新文件（均放 `admin-ui/src/components/`）：

| 文件 | 职责 |
|------|------|
| `account-metrics.tsx` | 4 段指标条 + 环形图 + sparkline + delta 徽章 |
| `account-toolbar.tsx` | 搜索框（⌘K）+ 状态分段筛选器 + 更新时间 |
| `account-table.tsx` | 表格外壳：colgroup、吸顶表头、排序状态、面板脚 |
| `account-row.tsx` | 单行渲染 + 行操作（toggle / 额度 / 日志 / 编辑 / ⋯ 下拉）+ 删除确认与编辑对话框 |

`dashboard.tsx` 保留全部既有业务逻辑（余额查询、批量验活、批量删除、清除已禁用等），仅替换渲染层并新增 `searchQuery` / `statusFilter` / `sortKey` 三个 UI 状态。

### 依赖

零新增依赖。⋯ 下拉菜单与 tooltip 使用 `package.json` 中已声明但尚未使用的 `@radix-ui/react-dropdown-menu` / `@radix-ui/react-tooltip`。

### 数据缺口与降级（用户已确认「用现有数据 + 优雅降级」）

| 设计稿读数 | 数据来源 | 降级策略 |
|---|---|---|
| 账号池 8 / 6 启用 / 2 禁用 / 2 项异常 | `CredentialsStatusResponse` | 全部可算，无降级 |
| 全局剩余积分 3426.2 | 现有 `liveCreditsTotal`（逐个查余额累加） | 未查询时显示 `—` + 「点击查询信息」提示；**环比 delta 无历史基线 → 整个徽章不渲染** |
| 启用账号平均剩余 68% + 环形图 | 已查询的 `balanceMap` 中启用账号均值 | 未查询时环形图为空轨 + 值显示 `—` |
| 启用中最低 备用池 A · 18% | `balanceMap` 最小值账号 | 无数据时该行隐藏 |
| 今日调用 2,461 + ↑8.4% + sparkline | `useDailyUsage()` → `DailySummary.totalRequests` | delta = 今日 vs 昨日；sparkline = 最近 7 天。历史不足 2 天时不渲染 delta |
| 失败 14 · 失败率 0.57% | **每日失败数后端不存在** | 改为账号池累计：`Σ failureCount` 与 `Σfail/(Σfail+Σsuccess)`，文案明确标注「累计」 |
| 行内 套餐 KIRO PRO | `BalanceResponse.subscriptionTitle`（需逐个查询） | 未查询显示 `—` |
| 行内 剩余额度 933.8 / 1000 + 93% + 进度条 | 同上 | 未查询显示设计稿自带的「尚未查询 / —」占位 + 空进度条 |
| Profile ARN 字符串（用于搜索匹配与复制） | **后端不返回**：`CredentialStatusItem` 只有 `hasProfileArn: boolean`，ARN 字符串仅存在于写侧 `AddCredentialRequest` / `UpdateCredentialRequest` | 搜索字段改为 昵称 / 邮箱 / 账号 ID；行操作**不提供**「复制 ARN」；沿用现有的 `ARN` 布尔徽章 |
| 当前使用中的账号 | `isCurrent` 字段存在但现有 UI 从未消费 | 不在本次范围（见「不在范围内」） |

### 状态映射（5 段筛选 ↔ 后端 `healthStatus`）

| 表格状态标签 | 视觉 | 判定条件 |
|---|---|---|
| 已禁用 | `off` 灰 | `disabled === true` |
| 待验证 | `new` teal | `healthy` 且 `successCount === 0` 且无余额数据 |
| 健康 | `ok` 绿 | `healthy`（其余情况） |
| 额度告警 | `warn` 黄 | `warning` |
| 异常 | `rate` 红 | `degraded` / `unhealthy` |

筛选段「异常」= `warning + degraded + unhealthy` 合集，与设计稿 5 段一致。

### 与设计稿的有意偏离

1. **面板脚批量操作**：设计稿示意为「批量启用 / 批量查询额度」，实际替换为现有真实能力「批量验活 / 恢复异常 / 批量删除 / 取消选择」——保功能优先于像素级复刻。
2. **导航计数**：账号数取已加载列表；API Keys 与支持模型计数复用既有 query（React Query 缓存共享，不新增轮询），缓存未命中时不显示数字而非显示 0。
3. **字体加载**：沿用现有 Google Fonts CDN 链接方式（仅换字族），不把设计稿目录的 woff2 打进二进制——保持与当前部署形态一致，离线可用性不变（现状本就依赖 CDN，回退到 PingFang SC / Menlo）。
4. **限流次数与两个日志入口**：设计稿的「调用/失败」列只有两个数字、且行操作只有 4 个图标 + 3 项菜单，容纳不下现有的「限流次数」读数与「限流日志 / 失败日志」两个跳转入口。落地方案：
   - 「调用/失败」单元格扩展为三段：成功数 / 失败数（可点击 → 失败日志）/ 限流数（> 0 时显示，可点击 → 限流日志），沿用现有「点击计数跳转日志」的交互习惯
   - ⋯ 菜单固定提供「失败日志」「限流日志」两项作为稳定入口（不依赖计数是否 > 0）
   - `doc` 图标按钮对应现有 `onViewDetail`（账号详情 / 调用记录页），tooltip 沿用现有 `credentials.viewLog` 文案
5. **代理与 ARN 标识**：设计稿无对应列。现有卡片展示的代理地址与 `ARN` 徽章下沉为账号单元格内邮箱右侧的两个 10px 微徽章（`PROXY` / `ARN`），代理地址完整值通过 title 悬停查看。

## 预期影响

- **视觉**：全站配色由暖橙/紫切换为中性灰 + teal，其他 6 页自动继承。这些页面中硬编码的 Tailwind 调色板类（`text-green-600`、`neon-*`、紫色渐变环）不在本次范围内，会与新令牌存在局部色相不一致 —— 已记录为后续变更项。
- **功能**：账号管理页既有能力全部保留。以 `credential-card.tsx` 实测清点的 **11 项行级能力**为基准（本清单同时作为 tasks.md 的逐项验收依据）：

  | # | 现有能力 | 现有实现位置 | 新形态落点 |
  |---|---|---|---|
  | 1 | 勾选选择 | `onToggleSelect` | 行首复选框 |
  | 2 | 优先级修改 | 卡片内联数字输入（回车提交 / Esc 取消） | 账号列 `P{n}` 徽章展示 + 编辑对话框修改（**交互路径变更**） |
  | 3 | 启停开关 | `Switch` → `setDisabled` | 操作列开关 |
  | 4 | 余额查询 | `Wallet` → `onViewBalance` | 操作列钱包图标 |
  | 5 | 账号详情 / 调用记录 | `FileText` → `onViewDetail` | 操作列 doc 图标 |
  | 6 | 编辑 | `Pencil` → `EditCredentialDialog` | 操作列铅笔图标 |
  | 7 | 重置失败计数 | `RefreshCw`，`failureCount === 0` 时禁用 | ⋯ 菜单项，同样在计数为 0 时禁用 |
  | 8 | 删除 | `Trash2` + 二次确认，**`disabled === false` 时按钮禁用并提示需先禁用账号** | ⋯ 菜单项 + 二次确认，**保留同一前置约束** |
  | 9 | 失败日志跳转 | 点击「失败：N」文本 → `onViewFailureLog` | 调用列失败数可点击 + ⋯ 菜单项 |
  | 10 | 限流日志跳转 | 点击「限流(7天)：N」文本 → `onViewThrottleLog` | 调用列限流数可点击 + ⋯ 菜单项 |
  | 11 | 代理 / ARN 标识展示 | 卡片第 2 行文本与徽章 | 账号列邮箱右侧 `PROXY` / `ARN` 微徽章 |

  仅第 2 项与第 7、8 项的**入口位置**发生变化（内联 → 对话框 / 图标 → ⋯ 菜单），能力与约束条件均不变。

  另有 **5 项页面级能力**，其条件性显隐同样必须原样保留（`dashboard.tsx:1016–1075` 实测）：

  | # | 现有能力 | 现有条件约束 | 新形态落点 |
  |---|---|---|---|
  | 12 | 批量验活 | 仅 `selectedIds.size > 0` 时出现 | 面板脚选中区 |
  | 13 | 恢复异常（批量重置失败计数） | 同上 | 面板脚选中区 |
  | 14 | 批量删除 | 同上，且 **`selectedDisabledCount === 0` 时禁用并 title 提示「仅可删除已禁用账号」** | 面板脚选中区，保留同一禁用条件与提示 |
  | 15 | 验活进度浮动入口 | 仅 `verifying && !verifyDialogOpen` 时出现，显示 `N / M` 进度 | 操作条内，位置与显隐条件不变 |
  | 16 | 查询信息 / 清除已禁用 | **两者仅在 `credentials.length > 0` 时渲染**；「清除已禁用」额外在 `disabledCredentialCount === 0` 时禁用并 title 提示 | 操作条，保留同一显隐与禁用条件 |

  因此 proposal 中「操作条 6 项常驻」应理解为**布局位置固定**，而非无条件渲染 —— 空列表与无禁用账号两种场景下的隐藏/禁用语义原样保留。
- **性能**：分页 12→50 使单页 DOM 行数上升，但表格行 DOM 远轻于卡片；表体内滚 + 吸顶表头避免整页回流。
- **兼容性**：无 API 契约变更，后端零改动，`cargo` 侧不受影响。仅需 `admin-ui` 重新构建（`build-mac.sh` / `build-windows.ps1` 已包含前端构建步骤）。

## 风险

| 风险 | 影响 | 应对 |
|---|---|---|
| shadcn 令牌重映射导致其他 6 页对比度不足或不可读 | 中 | 令牌落地任务完成后逐页截图核查（双主题），发现问题就地调整令牌数值而非改页面 |
| 重构中丢失条件性显隐/禁用逻辑（行级：内联优先级编辑、删除需先禁用、失败/限流计数点击跳转、重置失败计数在计数为 0 时禁用；页面级：批量删除需含已禁用账号、查询信息/清除已禁用在空列表时隐藏、清除已禁用在无禁用账号时禁用、验活进度浮动入口） | 高 | 已在「预期影响」中把 11 项行级 + 5 项页面级能力**连同各自的条件约束**逐项列表化（共 16 项），作为 T19 删除前与 T12 / T18 各自的强制勾对清单；这类隐性约束不会被类型检查或构建捕获，只能靠清单逐条核对 |
| 表格 10 列在 1280px 窄屏挤压 | 中 | 沿用设计稿 `colgroup` 固定列宽 + 表体横向滚动；账号列 `min-width:0` + 邮箱 ellipsis |
| 新增排序/筛选与既有分页状态耦合出错（筛选后页码越界） | 中 | 筛选 / 搜索 / 排序变更时统一重置 `currentPage = 1`，作为独立验收条件 |
| `⌘K` 与浏览器/系统快捷键冲突 | 低 | 仅在无输入框聚焦时拦截，`preventDefault` 后聚焦搜索框 |
| IBM Plex 中文缺字回退到 PingFang SC 造成中英混排基线差 | 低 | 字族链显式声明 `'IBM Plex Sans','PingFang SC','Hiragino Sans GB'`，与设计稿一致 |
