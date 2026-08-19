// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import { Search, X } from 'lucide-react'

/**
 * 设计稿 `.toolbar` 系视觉原语（design.md 决策 8）。
 *
 * 只承载展示与通用交互（⌘K 聚焦、相对时间刷新），分段口径与筛选语义留在调用方。
 */

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

/** 工具栏容器（设计稿 .toolbar） */
export function Toolbar({ children }: { children: ReactNode }) {
  return <div className="flex shrink-0 flex-wrap items-center gap-2 pb-[11px] pt-[14px]">{children}</div>
}

export interface SearchBoxProps {
  value: string
  onChange: (value: string) => void
  placeholder: string
  /** 清除按钮的 aria-label */
  clearLabel: string
  /** false 时不注册 ⌘K / Ctrl+K 全局聚焦，也不渲染快捷键提示（同页多个搜索框时用） */
  shortcut?: boolean
  /** 容器宽度类名，默认 w-[236px]（设计稿 .search）；须为源码中的静态字符串，否则被 Tailwind purge */
  widthClass?: string
}

/** 搜索框（设计稿 .search）：⌘K 聚焦 + 有值时切换为清除按钮 */
export function SearchBox({
  value,
  onChange,
  placeholder,
  clearLabel,
  shortcut = true,
  widthClass = 'w-[236px]',
}: SearchBoxProps) {
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!shortcut) return
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
  }, [shortcut])

  return (
    <label
      className={`flex h-[31px] ${widthClass} shrink-0 items-center gap-[7px] rounded-[7px] border border-hairline-2 bg-surface px-2.5 text-ink-3 shadow-hair focus-within:border-brand`}
    >
      <Search className="size-[15px] shrink-0" />
      <input
        ref={inputRef}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        aria-label={placeholder}
        className="w-full border-0 bg-transparent text-[12.5px] text-ink outline-none placeholder:text-ink-3"
      />
      {value ? (
        <button
          type="button"
          onClick={() => onChange('')}
          aria-label={clearLabel}
          className="shrink-0 text-ink-3 transition-colors hover:text-ink"
        >
          <X className="size-[13px]" />
        </button>
      ) : (
        shortcut && (
          <span
            aria-hidden="true"
            className="shrink-0 rounded-[4px] border border-hairline px-1 py-px font-mono text-[10px] text-ink-3"
          >
            {SHORTCUT_HINT}
          </span>
        )
      )}
    </label>
  )
}

export interface SegmentedOption<T extends string> {
  key: T
  label: string
  /** 圆点色类名（如 bg-ok）；须为源码中的静态字符串，否则被 Tailwind purge */
  pipClass?: string
  /** 计数徽标；undefined 时不渲染 */
  count?: number
}

export interface SegmentedProps<T extends string> {
  value: T
  onChange: (value: T) => void
  /** radiogroup 的 aria-label */
  groupLabel: string
  options: SegmentedOption<T>[]
}

/**
 * 分段筛选（设计稿 .seg）：互斥单选用 radiogroup 语义。
 * 未做 roving tabindex，各按钮均可 Tab 到达，键盘可达性不受影响。
 */
export function Segmented<T extends string>({ value, onChange, groupLabel, options }: SegmentedProps<T>) {
  return (
    <div role="radiogroup" aria-label={groupLabel} className="flex shrink-0 gap-0.5 rounded-[8px] bg-surface-3 p-0.5">
      {options.map((opt) => {
        const on = value === opt.key
        return (
          <button
            key={opt.key}
            type="button"
            role="radio"
            aria-checked={on}
            onClick={() => onChange(opt.key)}
            className={`flex h-[27px] items-center gap-[5px] whitespace-nowrap rounded-[6px] px-2.5 text-[12px] transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand ${
              on ? 'bg-surface font-semibold text-ink shadow-hair' : 'font-medium text-ink-2 hover:text-ink'
            }`}
          >
            {opt.pipClass && <span aria-hidden="true" className={`size-[5px] shrink-0 rounded-full ${opt.pipClass}`} />}
            {opt.label}
            {opt.count !== undefined && (
              <span className={`font-mono text-[10.5px] tabular-nums ${on ? 'text-ink-2' : 'text-ink-3'}`}>
                {opt.count}
              </span>
            )}
          </button>
        )
      })}
    </div>
  )
}

export interface ToggleProps {
  checked: boolean
  onChange: (checked: boolean) => void
  /** 同时作为按钮可访问名，故不另设 aria-label */
  label: string
}

/**
 * 开关（设计稿 logbar 右侧 34×19 switch）：整体为一个 role="switch" 按钮，标签文字含在按钮内 ——
 * 若用 <label> 包 <button>，点击标签会被浏览器转发给按钮而触发两次。
 * 未复用 ui/switch.tsx：其 h-6 w-11 与设计稿尺寸不符且配色未适配设计令牌，且全站无调用点。
 */
export function Toggle({ checked, onChange, label }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className="flex shrink-0 items-center gap-[7px] text-[11.5px] text-ink-2 transition-colors hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand"
    >
      {label}
      <span
        aria-hidden="true"
        className={`relative h-[19px] w-[34px] shrink-0 rounded-full transition-colors ${
          checked ? 'bg-brand' : 'bg-hairline-2'
        }`}
      >
        <span
          className={`absolute top-[2.5px] size-[14px] rounded-full bg-surface shadow-hair transition-[left] ${
            checked ? 'left-[17px]' : 'left-[2.5px]'
          }`}
        />
      </span>
    </button>
  )
}

/** 「更新于 X 前」（设计稿 .refresh-note）；dataUpdatedAt 为 0（尚未取到数据）时不渲染 */
export function UpdatedAgo({ dataUpdatedAt }: { dataUpdatedAt: number }) {
  const { t } = useTranslation()
  // 纯时间推移的展示，靠 30s 心跳重渲染，避免文案冻结在首次渲染时刻
  const [, tick] = useState(0)

  useEffect(() => {
    const timer = setInterval(() => tick((n) => n + 1), 30_000)
    return () => clearInterval(timer)
  }, [])

  if (dataUpdatedAt <= 0) return null
  return (
    <span className="ml-auto shrink-0 text-[11px] text-ink-3">
      {t('credentials.updatedAgo', { time: formatAgo(dataUpdatedAt, t) })}
    </span>
  )
}
