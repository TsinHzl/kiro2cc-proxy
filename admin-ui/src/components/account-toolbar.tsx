// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import { Search, X } from 'lucide-react'

/** 分段筛选口径：设计稿把 warning + error 合并为「异常」一段（与指标条 abnormalCount 同源） */
export type AccountStatusFilter = 'all' | 'healthy' | 'abnormal' | 'disabled' | 'pending'

const SEGMENTS: { key: AccountStatusFilter; labelKey: string; pipClass?: string }[] = [
  { key: 'all', labelKey: 'credentials.filterAll' },
  { key: 'healthy', labelKey: 'credentials.filterHealthy', pipClass: 'bg-ok' },
  { key: 'abnormal', labelKey: 'credentials.filterAbnormal', pipClass: 'bg-warn' },
  { key: 'disabled', labelKey: 'credentials.filterDisabled', pipClass: 'bg-ink-3' },
  { key: 'pending', labelKey: 'credentials.filterPending', pipClass: 'bg-brand' },
]

/** mac 显示 ⌘K，其余平台显示 Ctrl K；navigator.platform 已废弃，优先取 UA-CH 的 userAgentData */
const PLATFORM =
  (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ?? navigator.platform
// 大小写不敏感：UA-CH 返回 'macOS'，旧 API 返回 'MacIntel'
const SHORTCUT_HINT = /mac|iphone|ipad/i.test(PLATFORM) ? '⌘K' : 'Ctrl K'

function formatAgo(timestamp: number, t: TFunction): string {
  const diff = Date.now() - timestamp
  if (diff < 0) return t('credentials.justNow')
  const seconds = Math.floor(diff / 1000)
  if (seconds < 60) return t('credentials.secondsAgo', { count: seconds })
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return t('credentials.minutesAgo', { count: minutes })
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return t('credentials.hoursAgo', { count: hours })
  return t('credentials.daysAgo', { count: Math.floor(hours / 24) })
}

export interface AccountToolbarProps {
  searchQuery: string
  onSearchChange: (value: string) => void
  statusFilter: AccountStatusFilter
  onStatusFilterChange: (value: AccountStatusFilter) => void
  /** 各分段计数（基于搜索过滤后的集合，由调用方按 deriveAccountState 计算） */
  counts: Record<AccountStatusFilter, number>
  /** TanStack Query 的 dataUpdatedAt；0 = 尚未取到数据，不渲染「更新于」 */
  dataUpdatedAt: number
}

/** 工具栏（设计稿 .toolbar）：搜索 + 状态分段筛选 + 更新时间 */
export function AccountToolbar({
  searchQuery,
  onSearchChange,
  statusFilter,
  onStatusFilterChange,
  counts,
  dataUpdatedAt,
}: AccountToolbarProps) {
  const { t } = useTranslation()
  const inputRef = useRef<HTMLInputElement>(null)
  // 「更新于 X 前」是纯时间推移的展示，靠 30s 心跳重渲染，避免文案冻结在首次渲染时刻
  const [, tick] = useState(0)

  useEffect(() => {
    const timer = setInterval(() => tick((n) => n + 1), 30_000)
    return () => clearInterval(timer)
  }, [])

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      // CapsLock 开启或同时按下 Shift 时 e.key 为大写 'K'
      if (e.key.toLowerCase() !== 'k' || !(e.metaKey || e.ctrlKey)) return
      const el = document.activeElement
      // 焦点已在输入类元素时不劫持
      if (el instanceof HTMLElement && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)) return
      e.preventDefault()
      inputRef.current?.focus()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2 pb-[11px] pt-[14px]">
      {/* 搜索框（设计稿 .search） */}
      <label className="flex h-[31px] w-[236px] shrink-0 items-center gap-[7px] rounded-[7px] border border-hairline-2 bg-surface px-2.5 text-ink-3 shadow-hair focus-within:border-brand">
        <Search className="size-[15px] shrink-0" />
        <input
          ref={inputRef}
          value={searchQuery}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder={t('credentials.searchPlaceholder')}
          aria-label={t('credentials.searchPlaceholder')}
          className="w-full border-0 bg-transparent text-[12.5px] text-ink outline-none placeholder:text-ink-3"
        />
        {searchQuery ? (
          <button
            type="button"
            onClick={() => onSearchChange('')}
            aria-label={t('credentials.searchClear')}
            className="shrink-0 text-ink-3 transition-colors hover:text-ink"
          >
            <X className="size-[13px]" />
          </button>
        ) : (
          <span
            aria-hidden="true"
            className="shrink-0 rounded-[4px] border border-hairline px-1 py-px font-mono text-[10px] text-ink-3"
          >
            {SHORTCUT_HINT}
          </span>
        )}
      </label>

      {/* 状态分段筛选（设计稿 .seg） */}
      {/* 互斥单选用 radiogroup 语义；未做 roving tabindex，5 个按钮均可 Tab 到达，键盘可达性不受影响 */}
      <div
        role="radiogroup"
        aria-label={t('credentials.filterGroupLabel')}
        className="flex shrink-0 gap-0.5 rounded-[8px] bg-surface-3 p-0.5"
      >
        {SEGMENTS.map((seg) => {
          const on = statusFilter === seg.key
          return (
            <button
              key={seg.key}
              type="button"
              role="radio"
              aria-checked={on}
              onClick={() => onStatusFilterChange(seg.key)}
              className={`flex h-[27px] items-center gap-[5px] whitespace-nowrap rounded-[6px] px-2.5 text-[12px] transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand ${
                on ? 'bg-surface font-semibold text-ink shadow-hair' : 'font-medium text-ink-2 hover:text-ink'
              }`}
            >
              {seg.pipClass && <span aria-hidden="true" className={`size-[5px] shrink-0 rounded-full ${seg.pipClass}`} />}
              {t(seg.labelKey)}
              <span className={`font-mono text-[10.5px] tabular-nums ${on ? 'text-ink-2' : 'text-ink-3'}`}>
                {counts[seg.key]}
              </span>
            </button>
          )
        })}
      </div>

      {dataUpdatedAt > 0 && (
        <span className="ml-auto shrink-0 text-[11px] text-ink-3">
          {t('credentials.updatedAgo', { time: formatAgo(dataUpdatedAt, t) })}
        </span>
      )}
    </div>
  )
}
