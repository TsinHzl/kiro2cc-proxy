// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import type { TFunction } from 'i18next'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import * as SwitchPrimitive from '@radix-ui/react-switch'
import { Boxes, FileText, MoreHorizontal, Pencil, RefreshCw, Trash2, Wallet } from 'lucide-react'
import { EditCredentialDialog } from '@/components/edit-credential-dialog'
import { CELL, DataCheckbox, ICON_BTN } from '@/components/table-kit'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { useDeleteCredential, useResetFailure, useSetDisabled } from '@/hooks/use-credentials'
import { ACCOUNT_STATE_VISUAL, accountLabel, deriveAccountState, maskEmail } from '@/lib/account-state'
import { localeTag } from '@/lib/locale'
import { getSubscriptionColor } from '@/lib/utils'
import type { BalanceResponse, CredentialStatusItem } from '@/types/api'

/** 剩余额度阈值配色（design.md 决策：60/30 分界，比旧卡片的 50/20 更早预警） */
function quotaTone(remainingPct: number): { text: string; bar: string } {
  if (remainingPct >= 60) return { text: 'text-ok', bar: 'bg-ok' }
  if (remainingPct >= 30) return { text: 'text-warn', bar: 'bg-warn' }
  return { text: 'text-danger', bar: 'bg-danger' }
}

/** RPM 高压阈值：设计稿把 12 染成 danger、1–3 用 accent，取 10 作分界 */
const RPM_DANGER = 10

const pad2 = (n: number) => String(n).padStart(2, '0')

/** 相对时间：全项目唯一实现，口径沿用改版前的账号卡片 */
function formatLastUsed(lastUsedAt: string, t: TFunction): string {
  const diff = Date.now() - new Date(lastUsedAt).getTime()
  if (diff < 0) return t('credentials.justNow')
  const seconds = Math.floor(diff / 1000)
  if (seconds < 60) return t('credentials.secondsAgo', { count: seconds })
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return t('credentials.minutesAgo', { count: minutes })
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return t('credentials.hoursAgo', { count: hours })
  return t('credentials.daysAgo', { count: Math.floor(hours / 24) })
}

/** 绝对时间小字（设计稿 `今天 17:56` / `08-10 14:22`）：跨年退化为整日期，避免误读 */
function formatAbsolute(date: Date, t: TFunction): string {
  const now = new Date()
  const hm = `${pad2(date.getHours())}:${pad2(date.getMinutes())}`
  if (date.toDateString() === now.toDateString()) return `${t('credentials.today')} ${hm}`
  if (date.getFullYear() === now.getFullYear()) return `${pad2(date.getMonth() + 1)}-${pad2(date.getDate())} ${hm}`
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`
}

export interface AccountRowProps {
  credential: CredentialStatusItem
  /** 已查询到的余额；null = 尚未查询（参与状态派生） */
  balance: BalanceResponse | null
  /** 该账号余额正在查询中：额度单元格显示行内加载态 */
  loadingBalance: boolean
  /** 当前分钟请求数（来自 useRpm 轮询） */
  rpm: number
  selected: boolean
  onToggleSelect: () => void
  onViewFailureLog: (id: number) => void
  onViewThrottleLog: (id: number) => void
  onViewModels: (id: number) => void
  onViewBalance: (id: number) => void
  onViewDetail: (id: number) => void
  /** 单账号重新查询余额（dashboard 持有 balanceMap，故由其执行） */
  onRefetchBalance: (id: number) => void
}

/** 账号表格行（设计稿 tbody tr）：10 列 = 选择框 / ID / 账号 / 状态 / 套餐 / 额度 / 调用 / RPM / 最后调用 / 操作 */
export function AccountRow({
  credential,
  balance,
  loadingBalance,
  rpm,
  selected,
  onToggleSelect,
  onViewFailureLog,
  onViewThrottleLog,
  onViewModels,
  onViewBalance,
  onViewDetail,
  onRefetchBalance,
}: AccountRowProps) {
  const { t } = useTranslation()
  const state = deriveAccountState(credential, balance !== null)
  const visual = ACCOUNT_STATE_VISUAL[state]
  const label = accountLabel(credential)
  const displayName = credential.nickname || t('credentials.accountFallbackName', { id: credential.id })
  // 首字符按码点取，避免 emoji / 代理对昵称被截半
  const initial = [...label][0] ?? '#'
  // 剩余百分比：后端给的是已用百分比，钳到 0–100 防脏数据把进度条撑出轨道
  const remainingPct = balance ? Math.max(0, Math.min(100, 100 - balance.usagePercentage)) : null
  const tone = remainingPct === null ? null : quotaTone(remainingPct)
  const numFmt = localeTag()
  // 设计稿对「已禁用」与「从未调用」行的 RPM 显示 —（原型行 968/1007/1046），数字对这两类行无意义
  const rpmIdle = credential.disabled || credential.lastUsedAt === null

  const [showDeleteDialog, setShowDeleteDialog] = useState(false)
  const [showEditDialog, setShowEditDialog] = useState(false)
  const setDisabled = useSetDisabled()
  const resetFailure = useResetFailure()
  const deleteCredential = useDeleteCredential()

  const opFailed = (err: Error) => toast.error(t('credentials.toastOpFailed', { message: err.message }))

  const handleToggleDisabled = () => {
    setDisabled.mutate(
      { id: credential.id, disabled: !credential.disabled },
      { onSuccess: res => toast.success(res.message), onError: opFailed },
    )
  }

  const handleReset = () => {
    resetFailure.mutate(credential.id, { onSuccess: res => toast.success(res.message), onError: opFailed })
  }

  const handleDelete = () => {
    // 与旧卡片同约束：仅已禁用账号可删；菜单项已禁用，此处为二次兜底
    if (!credential.disabled) {
      toast.error(t('credentials.toastDisableFirst'))
      setShowDeleteDialog(false)
      return
    }
    deleteCredential.mutate(credential.id, {
      onSuccess: res => {
        toast.success(res.message)
        setShowDeleteDialog(false)
      },
      onError: err => toast.error(t('credentials.toastDeleteFailed', { message: err.message })),
    })
  }

  return (
    // 禁用行降权排除末列：父级 opacity 无法被子元素撤销，故直接不降权操作列，
    // 满足「禁用行整行降权但操作控件保持可点击且不显灰」
    <tr
      className={`transition-colors hover:bg-surface-2 ${
        credential.disabled ? '[&>td:not(:last-child)]:opacity-[.55]' : ''
      }`}
    >
      <td className={CELL}>
        <DataCheckbox
          checked={selected}
          onToggle={onToggleSelect}
          // 用 accountLabel（昵称 → 邮箱 → #ID）而非 displayName：后者无昵称时已含
          // 「账号 #ID」兜底，拼进 selectRow 会读成「选择账号 账号 #12」
          label={t('credentials.selectRow', { name: label })}
        />
      </td>
      <td className={CELL}>
        <span className="font-mono text-[11.5px] font-medium text-ink-3">
          #{String(credential.id).padStart(3, '0')}
        </span>
      </td>
      <td className={CELL}>
        {/* .acct：26px 头像 + 昵称行 + 脱敏邮箱行 */}
        <div className="flex min-w-0 items-center gap-[9px]">
          <div
            aria-hidden="true"
            className={`grid size-[26px] flex-none place-items-center rounded-[7px] text-[11px] font-semibold tracking-[-0.02em] ${visual.avatarClass}`}
          >
            {initial}
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-1.5 text-[12.5px] font-semibold tracking-[-0.01em] text-ink">
              <span className="truncate">{displayName}</span>
              <span
                title={t('credentials.priorityLabel') + credential.priority}
                className="flex-none rounded-[4px] border border-hairline-2 px-[3px] text-[9.5px] font-semibold tracking-[0.02em] text-ink-3"
              >
                P{credential.priority}
              </span>
            </div>
            <div className="flex items-center gap-1.5">
              <span
                title={credential.email || undefined}
                className="max-w-[190px] truncate font-mono text-[10.5px] text-ink-3"
              >
                {credential.email ? maskEmail(credential.email) : '—'}
              </span>
              {credential.hasProxy && credential.proxyUrl && (
                <span
                  title={t('credentials.proxyLabel', { url: credential.proxyUrl })}
                  className="flex-none rounded-[4px] border border-brand-line bg-brand-soft px-[3px] text-[9px] font-semibold tracking-[0.04em] text-brand"
                >
                  PROXY
                </span>
              )}
              {credential.hasProfileArn && (
                <span
                  title={t('credentials.profileArnBadge')}
                  className="flex-none rounded-[4px] border border-hairline-2 px-[3px] text-[9px] font-semibold tracking-[0.04em] text-ink-3"
                >
                  ARN
                </span>
              )}
            </div>
          </div>
        </div>
      </td>
      <td className={CELL}>
        {/* .tag：20px 高 + 5px pip（pip 的 2.5px 外发光已在 ACCOUNT_STATE_VISUAL 里定义） */}
        <span
          className={`inline-flex h-5 items-center gap-[5px] whitespace-nowrap rounded-[6px] border px-[7px] text-[11px] font-semibold ${visual.tagClass}`}
        >
          <span aria-hidden="true" className={`size-[5px] flex-none rounded-full ${visual.pipClass}`} />
          {t(visual.labelKey)}
        </span>
      </td>
      {/* 套餐（设计稿 .plan）：外形取设计稿中性徽标，文字色沿用既有 getSubscriptionColor 分级 */}
      <td className={CELL}>
        {balance?.subscriptionTitle ? (
          <span
            className={`inline-block whitespace-nowrap rounded-[5px] border border-hairline bg-surface-3 px-1.5 py-0.5 text-[10.5px] font-semibold uppercase tracking-[0.03em] ${getSubscriptionColor(balance.subscriptionTitle)}`}
          >
            {balance.subscriptionTitle}
          </span>
        ) : (
          <span className="text-[11.5px] text-ink-3">—</span>
        )}
      </td>
      {/* 剩余额度（设计稿 .quota）：已查询 = 绝对值 + 百分比 + 4px 进度条；未查询 = 文案 + — + 空轨 */}
      <td className={CELL}>
        <div className="flex min-w-[132px] flex-col gap-[5px]">
          <div className="flex items-baseline justify-between gap-2">
            {balance && remainingPct !== null && tone ? (
              <>
                <span className="font-mono text-[11.5px] tabular-nums text-ink-2">
                  <b className="font-semibold text-ink">{balance.remaining.toFixed(1)}</b> /{' '}
                  {balance.usageLimit.toFixed(0)}
                </span>
                <span className={`font-mono text-[11px] font-semibold tabular-nums ${tone.text}`}>
                  {remainingPct.toFixed(0)}%
                </span>
              </>
            ) : (
              <>
                <span className="text-[11.5px] text-ink-3">
                  {loadingBalance ? t('credentials.balanceLoading') : t('credentials.balanceNotQueried')}
                </span>
                <span className="font-mono text-[11px] text-ink-3">—</span>
              </>
            )}
          </div>
          <div
            className={`h-1 overflow-hidden rounded-[3px] bg-track ${
              !balance && loadingBalance ? 'animate-pulse' : ''
            }`}
          >
            {remainingPct !== null && tone && (
              <span className={`block h-full rounded-[3px] ${tone.bar}`} style={{ width: `${remainingPct}%` }} />
            )}
          </div>
        </div>
      </td>
      {/* 调用（设计稿 .calls 两段）+ 限流第三段：失败与限流 > 0 时可点击进对应日志 */}
      <td className={`${CELL} text-right`}>
        <div className="flex items-center justify-end gap-[9px]">
          <span
            title={t('credentials.successLabel', { count: credential.successCount })}
            className={`font-mono text-[12.5px] font-semibold tabular-nums ${
              credential.successCount > 0 ? 'text-ink' : 'text-ink-3'
            }`}
          >
            {credential.successCount.toLocaleString(numFmt)}
          </span>
          {credential.failureCount > 0 ? (
            <button
              type="button"
              onClick={() => onViewFailureLog(credential.id)}
              title={t('credentials.failureLabel', { count: credential.failureCount })}
              className="rounded-[4px] font-mono text-[11px] font-semibold tabular-nums text-danger hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand"
            >
              {credential.failureCount.toLocaleString(numFmt)}
            </button>
          ) : (
            <span
              title={t('credentials.failureLabel', { count: 0 })}
              className="font-mono text-[11px] tabular-nums text-ink-3"
            >
              0
            </span>
          )}
          {credential.throttleCount > 0 ? (
            <button
              type="button"
              onClick={() => onViewThrottleLog(credential.id)}
              title={t('credentials.throttleLabel', { count: credential.throttleCount })}
              className="rounded-[4px] font-mono text-[11px] font-semibold tabular-nums text-warn hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand"
            >
              {credential.throttleCount.toLocaleString(numFmt)}
            </button>
          ) : (
            <span
              title={t('credentials.throttleLabel', { count: 0 })}
              className="font-mono text-[11px] tabular-nums text-ink-3"
            >
              0
            </span>
          )}
        </div>
      </td>
      {/* RPM（设计稿 .rpm）：0 用次要色，高压值染 danger */}
      <td className={`${CELL} text-right`}>
        <span
          className={`font-mono text-[12.5px] tabular-nums ${
            rpmIdle || rpm === 0
              ? 'font-medium text-ink-3'
              : rpm >= RPM_DANGER
                ? 'font-semibold text-danger'
                : 'font-medium text-brand'
          }`}
        >
          {rpmIdle ? '—' : rpm}
        </span>
      </td>
      {/* 最后调用（设计稿 .when + small）：相对时间主行 + 绝对时间小字 */}
      <td className={CELL}>
        {credential.lastUsedAt ? (
          <span className="whitespace-nowrap text-[11.5px] text-ink-2">
            {formatLastUsed(credential.lastUsedAt, t)}
            <small className="block text-[10px] tracking-[0.01em] text-ink-3">
              {formatAbsolute(new Date(credential.lastUsedAt), t)}
            </small>
          </span>
        ) : (
          <span className="whitespace-nowrap text-[11.5px] text-ink-3">{t('credentials.neverUsed')}</span>
        )}
      </td>
      {/* 操作（设计稿 .rowops）：开关 → 分隔 → 3 个常规图标 → ⋯ 菜单；破坏性操作只在菜单内 */}
      <td className={`${CELL} text-right`}>
        <div className="flex items-center justify-end gap-[2px]">
          <SwitchPrimitive.Root
            checked={!credential.disabled}
            onCheckedChange={handleToggleDisabled}
            disabled={setDisabled.isPending}
            aria-label={t('credentials.toggleEnabled', { name: label })}
            className="relative flex h-[18px] w-8 flex-none items-center rounded-[10px] bg-track transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-brand"
          >
            <SwitchPrimitive.Thumb className="block size-3 translate-x-[2px] rounded-full bg-white shadow-[0_1px_2px_rgba(0,0,0,0.28)] transition-transform data-[state=checked]:translate-x-[18px]" />
          </SwitchPrimitive.Root>
          <span aria-hidden="true" className="mx-[5px] h-4 w-px flex-none bg-hairline" />
          <button
            type="button"
            className={ICON_BTN}
            title={t('credentials.viewBalance')}
            aria-label={t('credentials.viewBalance')}
            onClick={() => onViewBalance(credential.id)}
          >
            <Wallet className="size-[15px]" strokeWidth={1.7} />
          </button>
          <button
            type="button"
            className={ICON_BTN}
            title={t('credentials.viewLog')}
            aria-label={t('credentials.viewLog')}
            onClick={() => onViewDetail(credential.id)}
          >
            <FileText className="size-[15px]" strokeWidth={1.7} />
          </button>
          <button
            type="button"
            className={ICON_BTN}
            title={t('common.edit')}
            aria-label={t('common.edit')}
            onClick={() => setShowEditDialog(true)}
          >
            <Pencil className="size-[15px]" strokeWidth={1.7} />
          </button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                className={ICON_BTN}
                title={t('credentials.moreActions')}
                aria-label={t('credentials.moreActions')}
              >
                <MoreHorizontal className="size-[15px]" strokeWidth={1.7} />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent>
              <DropdownMenuItem onSelect={() => onRefetchBalance(credential.id)}>
                <RefreshCw className="size-[13px]" strokeWidth={1.7} />
                {t('credentials.refetchBalance')}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => onViewFailureLog(credential.id)}>
                <FileText className="size-[13px]" strokeWidth={1.7} />
                {t('credentials.viewFailureLog')}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => onViewThrottleLog(credential.id)}>
                <FileText className="size-[13px]" strokeWidth={1.7} />
                {t('credentials.viewThrottleLog')}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => onViewModels(credential.id)}>
                <Boxes className="size-[13px]" strokeWidth={1.7} />
                {t('credentials.viewModels')}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              {/* 与旧卡片同条件：失败计数为 0 时不可重置 */}
              <DropdownMenuItem
                onSelect={handleReset}
                disabled={resetFailure.isPending || credential.failureCount === 0}
              >
                <RefreshCw className="size-[13px]" strokeWidth={1.7} />
                {t('credentials.resetFailureCount')}
              </DropdownMenuItem>
              <DropdownMenuItem danger onSelect={() => setShowDeleteDialog(true)} disabled={!credential.disabled}>
                {/* title 挂内层：禁用项带 pointer-events-none，而它可被子元素 auto 撤销
                    （与 opacity 不同），否则「需先禁用账号」的原因无法 hover 获知 */}
                <span
                  className="pointer-events-auto flex items-center gap-2"
                  title={credential.disabled ? undefined : t('credentials.deleteNeedsDisableTitle')}
                >
                  <Trash2 className="size-[13px]" strokeWidth={1.7} />
                  {t('common.delete')}
                </span>
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>

        {/* 两个对话框均走 Portal，放在单元格内不影响表格 DOM 结构 */}
        <Dialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>{t('credentials.confirmDeleteTitle')}</DialogTitle>
              <DialogDescription>{t('credentials.confirmDeleteDesc', { id: credential.id })}</DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button
                variant="outline"
                onClick={() => setShowDeleteDialog(false)}
                disabled={deleteCredential.isPending}
              >
                {t('common.cancel')}
              </Button>
              <Button
                variant="destructive"
                onClick={handleDelete}
                disabled={deleteCredential.isPending || !credential.disabled}
              >
                {t('credentials.confirmDeleteButton')}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
        <EditCredentialDialog open={showEditDialog} onOpenChange={setShowEditDialog} credential={credential} />
      </td>
    </tr>
  )
}
