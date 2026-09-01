// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import * as SwitchPrimitive from '@radix-ui/react-switch'
import {
  BarChart3,
  Check,
  Copy,
  Globe,
  KeyRound,
  Link2,
  MoreHorizontal,
  Pencil,
  RotateCcw,
  Trash2,
} from 'lucide-react'
import { ChatGptIcon, ClaudeIcon } from '@/components/brand-icons'
import { CELL, DataCheckbox, ICON_BTN } from '@/components/table-kit'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { formatTokenCount, localeTag } from '@/lib/locale'
import type { ApiKeyItem, UsageSummary } from '@/types/api'

export type KeyStatus = 'active' | 'disabled' | 'expired' | 'pending'

/** 配额占用按 60/85 阈值切色（越高越坏，与账号页「剩余越高越好」方向相反） */
export function quotaTone(percent: number) {
  if (percent >= 85) return { stroke: 'stroke-danger', text: 'text-danger', bar: 'bg-danger' }
  if (percent >= 60) return { stroke: 'stroke-warn', text: 'text-warn', bar: 'bg-warn' }
  return { stroke: 'stroke-ok', text: 'text-ok', bar: 'bg-ok' }
}

/** 状态徽章配色（设计稿 .tag ok/off/warn + .pip）；详情页页头复用同一份，避免两处配色漂移 */
export const STATUS_VISUAL: Readonly<Record<KeyStatus, { cls: string; pip: string; labelKey: string }>> = {
  active: { cls: 'border-ok-line bg-ok-soft text-ok', pip: 'bg-ok', labelKey: 'apiKeys.statusActive' },
  pending: { cls: 'border-brand-line bg-brand-soft text-brand', pip: 'bg-brand', labelKey: 'apiKeys.statusPending' },
  expired: { cls: 'border-warn-line bg-warn-soft text-warn', pip: 'bg-warn', labelKey: 'apiKeys.statusExpired' },
  disabled: { cls: 'border-hairline bg-surface-3 text-ink-3', pip: 'bg-ink-3', labelKey: 'apiKeys.statusDisabled' },
}

export const TAG_BASE =
  'inline-flex h-5 items-center gap-[5px] whitespace-nowrap rounded-[6px] border px-[7px] text-[11px] font-semibold'

const pad2 = (n: number) => String(n).padStart(2, '0')

export interface ApiKeyRowProps {
  apiKey: ApiKeyItem
  status: KeyStatus
  /** 有效期 / 待激活口径的补充说明，挂在状态徽章 title 上（表格无独立到期列） */
  statusTitle?: string
  /** >0 且 ≤7 天时给出剩余天数，触发到期警示徽章 */
  expiringInDays: number | null
  usage?: UsageSummary
  rpm: number
  bound: { label: string; balance: string | null }[]
  selected: boolean
  onToggleSelect: () => void
  revealed: boolean
  copied: boolean
  createdTitle: string
  onCopy: () => void
  /** 生成 ccswitch:// 深链接并跳转，把本行 Key 导入 cc-switch 的 Claude Code 供应商 */
  onImportClaude: () => void
  /** 同上，导入 Codex 供应商 */
  onImportCodex: () => void
  onViewDetail: () => void
  onEdit: () => void
  onDelete: () => void
  onToggleEnabled: () => void
  onResetUsage: () => void
}

export function ApiKeyRow({
  apiKey,
  status,
  statusTitle,
  expiringInDays,
  usage,
  rpm,
  bound,
  selected,
  onToggleSelect,
  revealed,
  copied,
  createdTitle,
  onCopy,
  onImportClaude,
  onImportCodex,
  onViewDetail,
  onEdit,
  onDelete,
  onToggleEnabled,
  onResetUsage,
}: ApiKeyRowProps) {
  const { t } = useTranslation()
  const visual = STATUS_VISUAL[status]
  const dimmed = status === 'disabled' || status === 'expired'
  const requests = usage?.totalRequests ?? 0
  const used = apiKey.limitUnit === 'credits' ? usage?.totalCredits ?? 0 : usage?.totalCost ?? 0
  const percent = apiKey.spendingLimit && apiKey.spendingLimit > 0 ? (used / apiKey.spendingLimit) * 100 : 0
  const tone = quotaTone(percent)
  const activated = new Date(apiKey.activatedAt ?? apiKey.createdAt)
  // 后端返回畸形日期时 getFullYear 等会给出 NaN，静默渲染成 NaN-NaN-NaN
  const activatedValid = !Number.isNaN(activated.getTime())
  const boundTitle = bound.map((b) => (b.balance ? `${b.label} ${b.balance}` : b.label)).join('\n')

  return (
    <tr className={`transition-colors hover:bg-surface-2 ${dimmed ? '[&>td:not(:last-child)]:opacity-[.55]' : ''}`}>
      <td className={CELL}>
        <DataCheckbox
          checked={selected}
          onToggle={onToggleSelect}
          label={t('apiKeys.selectRow', { name: apiKey.name })}
        />
      </td>

      {/* 名称（设计稿 .acct）：26px 图钥头像 + 名称 + 副行序号；.pri 优先级徽章无对应字段，不渲染 */}
      <td className={CELL}>
        <div className="flex min-w-0 items-center gap-[9px]">
          <span className="grid size-[26px] flex-none place-items-center rounded-[7px] border border-hairline bg-surface-3 text-ink-3">
            <KeyRound className="size-[13px]" strokeWidth={1.7} />
          </span>
          <div className="min-w-0">
            <div className="truncate text-[12.5px] font-medium text-ink">{apiKey.name}</div>
            <div className="truncate font-mono text-[10.5px] text-ink-3" title={createdTitle}>
              #{String(apiKey.id).padStart(3, '0')}
            </div>
          </div>
        </div>
      </td>

      {/* KEY（设计稿 .mid）：明文 / 掩码受操作条的「显示完整 Key」控制 */}
      <td className={CELL}>
        <div className="flex items-center gap-1.5">
          <code className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-ink-2">
            {revealed ? apiKey.key : `${apiKey.key.slice(0, 7)}...${apiKey.key.slice(-4)}`}
          </code>
          <button
            type="button"
            className={ICON_BTN}
            title={t('apiKeys.copyUrlKeyTitle')}
            aria-label={t('apiKeys.copyUrlKeyTitle')}
            onClick={onCopy}
          >
            {copied ? (
              <Check className="size-[15px] text-ok" strokeWidth={1.7} />
            ) : (
              <Copy className="size-[15px]" strokeWidth={1.7} />
            )}
          </button>
        </div>
      </td>

      {/* 状态：到期 ≤7 天时在启用徽章右侧追加警示徽章 */}
      <td className={CELL}>
        <div className="flex flex-wrap items-center gap-1">
          <span className={`${TAG_BASE} ${visual.cls}`} title={statusTitle}>
            <span className={`size-[5px] flex-none rounded-full ${visual.pip}`} aria-hidden="true" />
            {t(visual.labelKey)}
          </span>
          {expiringInDays !== null && (
            <span className={`${TAG_BASE} border-warn-line bg-warn-soft text-warn`}>
              {t('apiKeys.expiringSoonTag', { count: expiringInDays })}
            </span>
          )}
        </div>
      </td>

      {/* 绑定账号（原「权限范围」降级）：未绑定即全局策略，余额明细挂 title */}
      <td className={CELL}>
        {bound.length === 0 ? (
          <span className="inline-flex items-center gap-[5px] whitespace-nowrap text-[11.5px] text-ink-3">
            <Globe className="size-[13px] flex-none" strokeWidth={1.7} />
            {t('apiKeys.boundAll')}
          </span>
        ) : (
          <span
            className="inline-flex min-w-0 max-w-full items-center gap-[5px] rounded-[6px] border border-brand-line bg-brand-soft px-[7px] py-px text-[11px] font-medium text-brand"
            title={boundTitle}
          >
            <Link2 className="size-[12px] flex-none" strokeWidth={1.7} />
            <span className="truncate">{bound[0].label}</span>
            {bound.length > 1 && <span className="flex-none opacity-70">+{bound.length - 1}</span>}
          </span>
        )}
      </td>

      {/* 消费上限（原「日配额」降级）：无上限时副行给出累计消费，保证费用信息不丢 */}
      <td className={CELL}>
        {apiKey.spendingLimit == null ? (
          <div className="flex flex-col gap-[3px]">
            <span className="text-[11.5px] text-ink-3">{t('apiKeys.limitNone')}</span>
            <span className="font-mono text-[10.5px] tabular-nums text-ink-3">
              {apiKey.limitUnit === 'credits'
                ? `${(usage?.totalCredits ?? 0).toFixed(2)} cr`
                : `$${(usage?.totalCost ?? 0).toFixed(4)}`}
            </span>
          </div>
        ) : (
          <div className="flex min-w-[132px] flex-col gap-[5px]">
            <div className="flex items-baseline justify-between gap-2 font-mono text-[11.5px] tabular-nums text-ink-2">
              <span>
                <b className="font-semibold text-ink">{used.toFixed(2)}</b> / {apiKey.spendingLimit.toFixed(2)}
                <span className="text-ink-3">{apiKey.limitUnit === 'credits' ? ' cr' : ' $'}</span>
              </span>
              <span className={`font-semibold ${tone.text}`}>{Math.round(percent)}%</span>
            </div>
            <span className="block h-1 overflow-hidden rounded-[3px] bg-track">
              <span
                className={`block h-full rounded-[3px] ${tone.bar}`}
                style={{ width: `${Math.min(100, Math.max(0, percent))}%` }}
              />
            </span>
          </div>
        )}
      </td>

      {/* 请求数（原「请求 / 失败」降级：后端无失败计数）；副行补 in / out token */}
      <td className={`${CELL} text-right`}>
        <div className="font-mono text-[12px] tabular-nums text-ink-2">{requests.toLocaleString(localeTag())}</div>
        <div className="font-mono text-[10px] tabular-nums text-ink-3">
          {formatTokenCount(usage?.totalInputTokens ?? 0)} / {formatTokenCount(usage?.totalOutputTokens ?? 0)}
        </div>
      </td>

      {/* 实时 RPM（原「RPM 上限」降级）：0 时设计稿显示破折号 */}
      <td className={`${CELL} text-right`}>
        {rpm > 0 ? (
          <span className="font-mono text-[12px] font-semibold tabular-nums text-brand">{rpm}</span>
        ) : (
          <span className="text-ink-3">—</span>
        )}
      </td>

      {/* 启用时间（原「最后使用」降级：后端无 lastUsedAt） */}
      <td className={CELL}>
        <span className="whitespace-nowrap text-[11.5px] text-ink-2">
          {activatedValid ? (
            <>
              {`${activated.getFullYear()}-${pad2(activated.getMonth() + 1)}-${pad2(activated.getDate())}`}
              <small className="block text-[10px] tracking-[0.01em] text-ink-3">
                {`${pad2(activated.getHours())}:${pad2(activated.getMinutes())}`}
              </small>
            </>
          ) : (
            <span className="text-ink-3">—</span>
          )}
        </span>
      </td>

      {/* 操作（设计稿 .rowops）：开关 → 分隔 → 图标钮 → ⋯ 菜单；破坏性操作只在菜单内 */}
      <td className={`${CELL} text-right`}>
        <div className="flex items-center justify-end gap-[2px]">
          <SwitchPrimitive.Root
            checked={apiKey.enabled}
            onCheckedChange={onToggleEnabled}
            aria-label={t('apiKeys.toggleEnabledLabel', { name: apiKey.name })}
            className="relative flex h-[18px] w-8 flex-none items-center rounded-[10px] bg-track transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand data-[state=checked]:bg-brand"
          >
            <SwitchPrimitive.Thumb className="block size-3 translate-x-[2px] rounded-full bg-white shadow-[0_1px_2px_rgba(0,0,0,0.28)] transition-transform data-[state=checked]:translate-x-[18px]" />
          </SwitchPrimitive.Root>
          <span aria-hidden="true" className="mx-[5px] h-4 w-px flex-none bg-hairline" />
          {/* cc-switch 一键导入：Claude Code 与 Codex 是两个独立供应商，各占一条深链接 */}
          <button
            type="button"
            className={ICON_BTN}
            title={t('apiKeys.importCcSwitchClaude')}
            aria-label={t('apiKeys.importCcSwitchClaude')}
            onClick={onImportClaude}
          >
            <ClaudeIcon className="size-[15px]" />
          </button>
          <button
            type="button"
            className={ICON_BTN}
            title={t('apiKeys.importCcSwitchCodex')}
            aria-label={t('apiKeys.importCcSwitchCodex')}
            onClick={onImportCodex}
          >
            <ChatGptIcon className="size-[15px]" />
          </button>
          <span aria-hidden="true" className="mx-[5px] h-4 w-px flex-none bg-hairline" />
          <button
            type="button"
            className={ICON_BTN}
            title={t('apiKeys.viewLogsTitle')}
            aria-label={t('apiKeys.viewLogsTitle')}
            onClick={onViewDetail}
          >
            <BarChart3 className="size-[15px]" strokeWidth={1.7} />
          </button>
          <button
            type="button"
            className={ICON_BTN}
            title={t('apiKeys.editTitle')}
            aria-label={t('apiKeys.editTitle')}
            onClick={onEdit}
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
              {/* 与旧卡片同条件：无请求记录时不可重置 */}
              <DropdownMenuItem onSelect={onResetUsage} disabled={requests === 0}>
                <RotateCcw className="size-[13px]" strokeWidth={1.7} />
                {t('apiKeys.resetUsageTitle')}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem danger onSelect={onDelete}>
                <Trash2 className="size-[13px]" strokeWidth={1.7} />
                {t('common.delete')}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </td>
    </tr>
  )
}
