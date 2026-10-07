# 任务清单：balance-dialog-bento-redesign

## 状态：ARCHIVED

## 任务

### 前端：i18n 文案

- [x] **T1** `web-ui/admin-ui/src/i18n/locales/zh.json` 与 `en.json` 各新增以下 `credentials` key：
  - `copyEmail` — 复制按钮 `title`（zh: 复制邮箱 / en: Copy email）
  - `copied` — 复制成功态 `title`（zh: 已复制 / en: Copied）
  - `proxyLabel` — 代理徽章前缀（zh: 代理 / en: Proxy）
  - `adaptiveLabel` — Adaptive 徽章前缀（zh: 自适应注入 / en: Adaptive）
  - `regionCombinedLabel` — 卡片 B 的 Region 合并行标签（zh: Auth / API Region / en: Auth / API Region）

  验证：`node -e` 解析两文件，确认新 key 均存在且 `credentials` 分组 key 集合一致。

### 前端：弹窗重构

- [x] **T2** `web-ui/admin-ui/src/components/balance-dialog.tsx` 重写 Header 区块：
  - `DialogHeader` 加 `p-5 pb-4 border-b border-hairline flex flex-col gap-1.5 text-left`
  - 标题行容器加 `pr-6`（避让右上角关闭按钮，防止标题与 X 重叠）
  - 标题行 = `DialogTitle` + 健康状态药丸（复用 `ACCOUNT_STATE_VISUAL[deriveAccountState(...)]` 的 `tagClass` / `pipClass`）
  - **不渲染** `#ID` 徽章（标题已含账号 ID，用户已确认移除）
  - 副标题 = 邮箱（`truncate`）+ `·` + `优先级 N`，`credential` 为 null 时不渲染

  验证：`pnpm build` 通过；打开弹窗标题行显示状态药丸、无重复 ID、标题不与关闭按钮重叠、Header 底边线满宽。

- [x] **T3** 重写 Hero 额度卡片：
  - 容器 `rounded-xl border border-hairline bg-surface-2 p-4 space-y-3.5`
  - 左上 = 订阅标题（保留 `getSubscriptionColor`）+ 图标，其下方补小标签 `t('credentials.remainingQuotaLabel')`
  - 右上 = 剩余额度，`font-mono text-[22px] font-bold`，颜色取 `quotaTone(remainingPct).badgeText`（**复用 `quotaTone`，不自算阈值**；因该值为 CSS var 需用 `style` 承载）
  - 进度条 = 既有 `<Progress value={clampedProgressValue} className="h-2 rounded-full" />`
  - 用量行**左右分布**：左 = `t('credentials.usedLabel', { amount: formatNumber(balance.currentUsage) })`（**amount 传纯数字，不拼 `$`**）+ 单处百分比徽章（复用既有 `QuotaPercentBadge`，显示为整数百分比）；右 = `t('credentials.limitLabel', { amount: formatNumber(balance.usageLimit) })`（同样传纯数字）
  - 重置周期行 = `border-t border-dashed border-hairline`（**不使用 `/60` 透明度修饰符**）

  验证：`pnpm build` 通过；目视确认金额不出现 `$$` 前缀、百分比仅一处、进度条沿用四档渐变、用量行左「已使用+百分比」右「限额」。

- [x] **T4** 重写双栏 Bento 卡片网格（`grid grid-cols-1 sm:grid-cols-2 gap-3`）：
  - 卡片 A「核心标识」：邮箱（带复制按钮，`title` 为完整邮箱值）、昵称、优先级，三行结构
  - 卡片 B「认证与区域」：登录来源（`title` 为来源未知提示 `loginSourceUnknownHint`）、认证方式、`Auth / API Region` 合并行，**保持三行结构**（用户已确认不压缩）
  - Region 合并行空值兜底：`credential.authRegion || t('credentials.regionFallbackGlobal')` / `credential.apiRegion || t('credentials.regionFallbackGlobal')`，中间以 `/` 分隔
  - 两卡片头部统一 `text-[11px] font-semibold uppercase tracking-wider text-ink-3` + 装饰图标

  验证：`pnpm build` 通过；目视确认双栏布局与三行结构、两处 title 悬浮提示、Region 为空时显示「跟随全局」。

- [x] **T5** 重写全宽运行健康卡片：
  - 状态徽章群：启禁用状态（含 `disabledReason === 'quota_exceeded'` 分支）、代理配置**三态**（有 `proxyUrl` → `代理: {url}`；`hasProxy` 但无 url → `proxyConfigured`；否则 → `proxyNone`）、Adaptive 开关（`adaptiveLabel` + `switchOn`/`switchOff`）
  - 三列指标矩阵：成功调用 / 连续失败 / 7 天限流（`grid-cols-3 divide-x divide-hairline`），失败数 > 0 着 danger 色、限流数 > 0 着 warn 色
  - 时间戳页脚：Token 过期时间 + 最后使用时间，`grid-cols-1 sm:grid-cols-2`
  - 页脚分隔线用 `border-hairline`（**不用 `/60`**）

  验证：`pnpm build` 通过；目视确认三列矩阵与页脚对齐、代理徽章三态文案正确。

- [x] **T6** 新增 `CopyButton` 子组件：
  - 复用 `@/lib/clipboard` 的 `copyToClipboard()`，不直接调用 `navigator.clipboard`
  - `useEffect` 清理 `setTimeout`，避免 unmount 后 setState
  - 成功态文案走 `t('credentials.copied')`，失败静默降级
  - `e.stopPropagation()` 防止冒泡

  验证：`pnpm build` 通过；点击邮箱复制按钮后图标变勾、1.5s 后复原。

- [x] **T7** 移除弹窗内部二级滚动条：
  - 删除账号信息区的 `max-h-[40vh] overflow-y-auto`
  - `DialogContent` className 设为 `max-h-[90vh] overflow-y-auto sm:max-w-[520px] p-0 gap-0 border-hairline bg-surface`——其中 `p-0 gap-0` 为必需项：`DialogContent` 默认 `p-4 sm:p-6`，不置零会使 T2 的 Header `border-b` 无法满宽；`gap-0` 消除默认 `gap-4`

  验证：`pnpm build` 通过；目视确认弹窗内无二级滚动条、Header 底边线通栏。

- [x] **T8** 三态渲染分支正确（不回退上一变更的 CR 修复 B1）：
  - `isLoading` → 主体区渲染居中 spinner（`animate-spin rounded-full border-2 border-brand border-t-transparent`），Hero 与错误卡片均不渲染
  - `error` → 渲染 `parseError` 错误卡片（danger 配色），Hero 不渲染
  - `credential === null` → 三张卡片不渲染，显示 `t('credentials.accountInfoLoading')` 占位
  - `balance === null`（含错误与加载中）→ 三张账号信息卡片**仍然渲染**（数据源与余额接口独立）

  验证：临时断网或模拟余额接口失败，确认账号信息卡片仍可见；加载中可见 spinner。

### 构建与验收

- [x] **T9** `cd web-ui/admin-ui && npx tsc --noEmit` 0 error；`pnpm build` 成功。验证：两条命令均无错误输出。

## 验收标准

- [ ] 弹窗标题行显示账号 ID（来自标题文案）+ 健康状态药丸，**无重复的 `#ID` 徽章**
- [ ] 标题行右侧留出 `pr-6`，标题不与关闭按钮重叠；Header 底边线通栏满宽
- [ ] 副标题显示邮箱与优先级，`credential` 为 null 时不渲染
- [ ] Hero 卡片右上角以大号等宽字体突出显示剩余额度，订阅标题下方有「剩余额度」小标签
- [ ] 剩余额度金额前只有一个 `$`（不出现 `$$518.41`）
- [ ] 百分比仅在进度条下方出现**一处**（无 `48.2% 已使用` + `48%` 双重展示），显示为整数百分比（复用 `QuotaPercentBadge`，精度口径变化已在 proposal 中声明）
- [ ] 用量行左右分布：左侧「已使用 + 百分比徽章」，右侧「限额」
- [ ] 进度条颜色沿用项目既有 `quotaTone` 四档（与表格进度条一致）
- [ ] 重置周期行有日历图标与虚线分隔
- [ ] 双栏卡片 A/B 各为三行结构，卡片 B 未压缩为一行双列
- [ ] 邮箱与登录来源两处有 `title` 悬浮提示（完整邮箱值 / 来源未知提示）
- [ ] Region 为空时显示「跟随全局」（`regionFallbackGlobal`），不为空时 Auth / API 以 `/` 分隔
- [ ] 全宽运行健康卡片含状态徽章群 + 三列指标矩阵 + 时间戳页脚
- [ ] 代理徽章三态正确：有 url → `代理: {url}`；有代理无 url → 已配置；无代理 → 未配置
- [ ] 邮箱旁有复制按钮，点击后图标变勾并 1.5s 后复原；复制失败静默降级
- [ ] 弹窗内**无二级滚动条**（账号信息区无 `overflow-y-auto`）
- [ ] 分隔线全部使用 `border-hairline`，无 `/60` 透明度修饰符（不产生空规则）
- [ ] 中英文切换下无硬编码中文残留（复制按钮、代理、Adaptive 标签、Region 合并行标签）
- [ ] `isLoading` 时主体区显示居中 spinner
- [ ] `credential` 为 null 时显示占位态且不隐藏
- [ ] 余额接口失败时账号信息卡片**仍然可见**（不回退上一变更 CR 修复）
- [ ] `BalanceDialogProps` 契约不变，`dashboard.tsx` / `dialogs.tsx` 无需改动
- [ ] `npx tsc --noEmit` 0 error，`pnpm build` 成功
