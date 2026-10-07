# 变更提案：balance-dialog-bento-redesign

## 背景

余额弹窗（`web-ui/admin-ui/src/components/balance-dialog.tsx`）在上一变更 `balance-dialog-account-details` 中新增了 17 项账号信息，但实现是「灰标签 + 平铺文字」的单一网格，存在四类体验缺陷：

1. **信息层级扁平**：额度进度条上下分散，已用金额/限额与百分比徽章割裂，且同时展示 `48.2% 已使用` 与 `48%` 药丸徽章，信息冗余；用户最关注的「剩余额度」被弱化在下方的双列文字网格中，缺乏视觉焦点。
2. **二级滚动条割裂浏览**：账号详情区设 `max-h-[40vh] overflow-y-auto`，弹窗内部产生二级纵向滚动，空间利用率低。
3. **元数据罗列松散**：10+ 项配置字段未做模块化封装，无法快速定位同类信息。
4. **状态传达欠缺**：额度接近耗尽时缺乏显著的色阶梯度提示。

用户提供外部设计稿 `/Users/MacBook/Downloads/balance-dialog-redesign/`（含 `index.html` 交互预览 + 生产级 `balance-dialog.tsx`），要求按此重构弹窗 UI。

## 目标范围

**在范围内：**

- 按设计稿重写 `balance-dialog.tsx` 的信息架构为三级阶梯 + Bento 卡片网格：
  - 阶梯一 Header：标题 + 健康状态药丸 + 副标题（邮箱 · 优先级）
  - 阶梯二 Hero 额度卡片：剩余额度大号等宽字体（视觉焦点）、复合进度条（单处百分比）、虚线分隔的重置周期行
  - 阶梯三 双栏卡片网格：卡片 A 身份标识（邮箱/昵称/优先级）、卡片 B 认证与路由（登录来源/认证方式/Auth·API Region）
  - 阶梯四 全宽运行健康卡片：状态徽章群（启禁用/代理/Adaptive）+ 三列指标矩阵（成功/失败/限流）+ 时间戳页脚
- 消除弹窗内部二级滚动条（移除 `max-h-[40vh] overflow-y-auto`）
- 移除重复的 `#ID` 徽章（标题已含 `账号 #4 余额信息`）
- 保留卡片 B 的三行结构（不压缩为一行双列）
- 新增邮箱一键复制按钮，复用项目既有 `@/lib/clipboard`（含 `execCommand` 降级）
- 补充 i18n 文案（复制按钮提示、代理/Adaptive 标签、Region 合并行标签）

**不在范围内：**

- 不改动 `dashboard.tsx` / `dashboard/dialogs.tsx` 的 props 传递（`credentialId` / `credential` 契约不变）
- 不改动 `account-state.ts` 的 `loginSource()` / `deriveAccountState()` / `ACCOUNT_STATE_VISUAL`
- 不改动 `Progress` / `QuotaPercentBadge` 组件与 `quotaTone()` 的阈值常量
- 不改动 `account-row.tsx` 表格行
- 不新增后端接口或字段
- 不改动 `package.json` 依赖

## 技术方案

### 关键约束

| 约束 | 说明 |
|---|---|
| 色阶口径 | **复用项目既有 `quotaTone()`**（按剩余额度 60/30/15 分四档，源自 `quota_progress_bar.html` 设计稿），不采用设计稿的「已用 ≥80% 两档」方案，以保证同一页面表格与弹窗色阶一致（用户已确认） |
| 进度条渲染 | 直接使用既有 `<Progress value={clampedPct} />`，其内部已按 `quotaTone(remainingPct)` 渲染四档渐变；弹窗不自算颜色 |
| 百分比展示 | 只保留**一处**（进度条下方），复用既有 `QuotaPercentBadge`，删除设计稿中新增的 `quotaBadgeBgClass` 自绘徽章，避免双重百分比与自绘配色 |
| 百分比精度 | 复用 `QuotaPercentBadge` 意味着**精度由设计稿的 1 位小数（`48.2%`）变为整数（`48%`）**——该组件内部固定 `toFixed(0)`。此处主动接受口径变化，以换取与表格行 `account-metrics.tsx` 的百分比展示完全一致（避免同页两套精度） |
| `$` 符号 | 既有 `usedLabel` / `limitLabel` 文案**已含 `$`**（`"已使用: ${{amount}}"`），调用时 amount 传纯数字，**禁止**再拼 `$`，否则渲染为 `$$518.41` |
| 透明度修饰符 | tailwind 配置明确注释：`surface`/`hairline`/`brand`/`ok`/`warn`/`danger`/`track` 这批 token 为整值 `var()` 引用，**`border-hairline/60` 会静默生成空规则**（无边框）。需半透明一律用 `-soft` / `-line` 变体。设计稿 L373 的 `border-t border-hairline/60` 必须改为 `border-hairline` |
| 硬编码文案 | 设计稿 4 处硬编码中文需改为 i18n：`'已复制'`、`'复制邮箱'`、`代理: `、`Adaptive: `；`Auth / API Region` 合并行标签同样纳入 i18n（`regionCombinedLabel`，用户已确认） |
| 复制实现 | 复用 `@/lib/clipboard` 的 `copyToClipboard()`（含 `execCommand` 降级），不直接调用 `navigator.clipboard` |
| 组件契约 | `BalanceDialogProps`（`credentialId` / `credential` / `open` / `onOpenChange`）保持不变；`credential === null` 时渲染占位态，`balance` 为 null 时 Hero 卡片不渲染但账号信息仍可见 |

### 文件改动

| 文件 | 改动 |
|---|---|
| `web-ui/admin-ui/src/components/balance-dialog.tsx` | 整体重写信息架构（Header / Hero / Bento 网格 / 运行健康卡片），新增 `CopyButton` 子组件，移除内部滚动条 |
| `web-ui/admin-ui/src/i18n/locales/zh.json` | 新增 `copyEmail`、`copied`、`proxyLabel`、`adaptiveLabel`（+ 视裁决结果决定 `regionCombinedLabel`） |
| `web-ui/admin-ui/src/i18n/locales/en.json` | 同上，英文译文 |

### 降级行为

| 场景 | 行为 |
|---|---|
| `credential === null` | Header 副标题与三张卡片不渲染，仅显示 `accountInfoLoading` 占位（与改动前一致） |
| `isLoading`（余额查询中） | 主体区渲染居中 spinner（沿用既有实现），Hero 卡片与错误卡片均不渲染；账号信息卡片仍渲染（数据源独立） |
| `balance === null`（余额查询失败） | 错误卡片渲染，Hero 额度卡片不渲染；账号信息卡片**仍然可见**（上一变更 CR 修复项 B1，不得回退） |
| `usagePercentage` 非有限值 | `clampedProgressValue` 归零，不抛异常 |
| 复制失败 | 静默降级（不弹 toast），与 `lib/clipboard.ts` 既有约定一致 |

## 预期影响

- 弹窗内不再有二级滚动条，整体高度收拢；信息层级由「平铺」变为「焦点 + 分组」
- 账号信息数据源与余额接口相互独立，保持上一变更的 B1 修复（余额失败时账号信息仍可见）
- 无接口、无数据模型、无后端改动，`cargo test` 不受影响
- 表格行 `account-row.tsx`、`Progress` 组件、`quotaTone()` 常量均不改动，无回归面

## 风险

| 风险 | 应对 |
|---|---|
| 设计稿中 `$` 双重拼接导致显示 `$$518.41` | 已在技术方案中列为硬约束；实现时 amount 传纯数字 |
| 设计稿自绘色阶与项目 `quotaTone` 四档冲突 | 已裁决复用 `quotaTone`，删除自绘 `quotaBadgeBgClass` |
| `border-hairline/60` 静默生成空规则（无边框） | 已列为硬约束，改为 `border-hairline` |
| 硬编码中文在英文界面露出 | 已列为硬约束，4 处全部纳入 i18n |
| 新增 `CopyButton` 的 `setTimeout` 未清理导致 unmount 后 setState | 实现时用 `useEffect` 清理定时器 |
| 弹窗高度增加导致小屏溢出 | `DialogContent` 保留 `max-h-[90vh] overflow-y-auto`（外层整体滚动），仅移除内部二级滚动条 |
