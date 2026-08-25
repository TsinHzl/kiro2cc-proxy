import type { CredentialStatusItem } from '@/types/api'
import { localeTag } from '@/lib/locale'

/** 账号派生状态（design.md 决策 5：状态列 / 工具栏计数 / 筛选共用同一判定） */
export type AccountState =
  | 'quota_exceeded'
  | 'disabled'
  | 'error'
  | 'warning'
  | 'pending'
  | 'healthy'

/** 账号显示名（昵称 → 邮箱 → #ID）：搜索、排序、头像首字母共用同一口径 */
export function accountLabel(item: CredentialStatusItem): string {
  return item.nickname || item.email || `#${item.id}`
}

/**
 * 邮箱脱敏（设计稿 `h∗∗∗∗∗∗∗∗＠gmail.com`）：保留本地部分首字符与完整域名，其余打星。
 * 仅为表格单行紧凑展示，不是安全措施 —— 行内 `title` 仍给出完整值（管理端本就明文可见）。
 */
export function maskEmail(email: string): string {
  const at = email.indexOf('@')
  if (at <= 0) return email
  const local = email.slice(0, at)
  // 首字符按码点取，与头像首字母同口径，避免 emoji / 代理对开头的本地部分被截半
  const first = [...local][0] ?? ''
  return `${first}${'∗'.repeat(Math.max(1, local.length - first.length))}＠${email.slice(at + 1)}`
}

/**
 * 由凭据状态与余额查询情况派生唯一账号状态。
 * 判定顺序即优先级，逐条对应 spec.md 状态映射表：
 *   1. disabled 为真              → disabled
 *   2. healthStatus 不健康 / 降级 → error
 *   3. healthStatus 警告          → warning
 *   4. 健康 + 零成功调用 + 未查额度 → pending
 *   5. 其余                       → healthy
 * @param hasBalance 该账号在 balanceMap 中是否已有余额结果（会话内内存态）
 */
export function deriveAccountState(
  item: CredentialStatusItem,
  hasBalance: boolean,
): AccountState {
  // 额度耗尽优先于通用「已禁用」：仪表盘禁用计数 + 工具栏「已禁用」筛选项
  // 都不应吞掉这一类，让用户能在筛选里精确看到下月自动恢复的账号
  if (item.disabled && item.disabledReason === 'quota_exceeded') return 'quota_exceeded'
  if (item.disabled) return 'disabled'
  if (item.healthStatus === 'unhealthy' || item.healthStatus === 'degraded') return 'error'
  if (item.healthStatus === 'warning') return 'warning'
  if (item.healthStatus === 'healthy' && item.successCount === 0 && !hasBalance) return 'pending'
  if (item.healthStatus === 'healthy') return 'healthy'
  // 兜底：后端 HealthStatus 日后新增变体时宁可张扬为异常，不静默显示为「健康」
  return 'error'
}

/** 单状态展现配置：标签文案 key + 标签 / 圆点 / 头像的语义色类名 */
export interface AccountStateVisual {
  labelKey: string
  /** 状态标签容器（设计稿 .tag）：文字 + 底色 + 描边 */
  tagClass: string
  /** 标签内 5px 圆点（设计稿 .tag .pip）：底色 + 2.5px 外发光 */
  pipClass: string
  /** 账号头像（设计稿 .avatar）：底色 + 文字色 */
  avatarClass: string
}

export const ACCOUNT_STATE_VISUAL: Record<AccountState, AccountStateVisual> = {
  healthy: {
    labelKey: 'credentials.stateHealthy',
    tagClass: 'text-ok bg-ok-soft border-ok-line',
    pipClass: 'bg-ok ring-[2.5px] ring-ok-soft',
    avatarClass: 'bg-ok-soft text-ok',
  },
  warning: {
    labelKey: 'credentials.stateWarning',
    tagClass: 'text-warn bg-warn-soft border-warn-line',
    pipClass: 'bg-warn ring-[2.5px] ring-warn-soft',
    avatarClass: 'bg-warn-soft text-warn',
  },
  error: {
    labelKey: 'credentials.stateError',
    tagClass: 'text-danger bg-danger-soft border-danger-line',
    pipClass: 'bg-danger ring-[2.5px] ring-danger-soft',
    avatarClass: 'bg-danger-soft text-danger',
  },
  pending: {
    labelKey: 'credentials.statePending',
    tagClass: 'text-brand bg-brand-soft border-brand-line',
    pipClass: 'bg-brand ring-[2.5px] ring-brand-soft',
    avatarClass: 'bg-brand-soft text-brand',
  },
  disabled: {
    labelKey: 'credentials.stateDisabled',
    tagClass: 'text-ink-3 bg-surface-3 border-hairline-2',
    pipClass: 'bg-ink-3 ring-[2.5px] ring-surface-3',
    avatarClass: 'bg-surface-3 text-ink-3',
  },
  // 复用 danger 配色（与异常同色系）让用户一眼区分「额度耗尽」与「通用已禁用」；
  // 文案独立以便跨月自动恢复时给出更明确的提示
  quota_exceeded: {
    labelKey: 'credentials.stateQuotaExceeded',
    tagClass: 'text-danger bg-danger-soft border-danger-line',
    pipClass: 'bg-danger ring-[2.5px] ring-danger-soft',
    avatarClass: 'bg-danger-soft text-danger',
  },
}

/** 工具栏分段筛选顺序（「全部」之后依次排列） */
export const ACCOUNT_STATE_ORDER: readonly AccountState[] = [
  'healthy',
  'warning',
  'error',
  'pending',
  'disabled',
  'quota_exceeded',
]

/** 可排序列（设计稿仅「账号」「剩余额度」两列可点击排序） */
export type AccountSortKey = 'account' | 'remaining'
export type SortDirection = 'asc' | 'desc'

/**
 * 排序（design.md 决策 4）。返回新数组，不修改入参。
 * - `account`：按显示名本地化比较（中文按拼音），同名回退 ID 升序
 * - `remaining`：已查询组按剩余额度排序（受 dir 控制）；未查询组恒定末位且内部按 ID 升序，
 *   避免升序时未查询账号抢占「额度最低」的语义位置
 * @param remainingOf 取该账号已查询到的剩余额度；未查询返回 null
 */
export function sortCredentials(
  list: readonly CredentialStatusItem[],
  sortKey: AccountSortKey,
  dir: SortDirection,
  remainingOf: (id: number) => number | null,
): CredentialStatusItem[] {
  const factor = dir === 'asc' ? 1 : -1
  if (sortKey === 'account') {
    return [...list].sort(
      (a, b) => accountLabel(a).localeCompare(accountLabel(b), localeTag()) * factor || a.id - b.id,
    )
  }
  const queried: CredentialStatusItem[] = []
  const unqueried: CredentialStatusItem[] = []
  list.forEach((item) => (remainingOf(item.id) === null ? unqueried : queried).push(item))
  queried.sort(
    (a, b) => ((remainingOf(a.id) ?? 0) - (remainingOf(b.id) ?? 0)) * factor || a.id - b.id,
  )
  unqueried.sort((a, b) => a.id - b.id)
  return [...queried, ...unqueried]
}
