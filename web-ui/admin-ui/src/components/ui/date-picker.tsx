// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Calendar, ChevronDown, ChevronLeft, ChevronRight } from 'lucide-react'
import {
  buildCalendarCells,
  formatIsoDate,
  isDateDisabled,
  isMonthDisabled,
  isYearDisabled,
  isYearRangeDisabled,
  parseIsoDate,
  shiftIsoDate,
  shiftMonth,
  yearGridStart,
} from '@/lib/calendar'
import { localeTag } from '@/lib/locale'
import { cn } from '@/lib/utils'

type PickerView = 'day' | 'month' | 'year'

export interface DatePickerProps {
  /** 当前选中日期（YYYY-MM-DD） */
  value: string
  /** 选中回调（返回 YYYY-MM-DD） */
  onChange: (value: string) => void
  /** 最小可选日期（YYYY-MM-DD，闭区间） */
  min?: string
  /** 最大可选日期（YYYY-MM-DD，闭区间） */
  max?: string
  /** 业务口径的"今天"（如 UTC+8 的 CST 今日）；未传时退回浏览器本地日历日 */
  today?: string
  /** 可选区间高亮起点（YYYY-MM-DD） */
  rangeStart?: string
  /** 可选区间高亮终点（YYYY-MM-DD） */
  rangeEnd?: string
  placeholder?: string
  disabled?: boolean
  /** 桌面端相对触发器的水平对齐方式 */
  align?: 'start' | 'end'
  id?: string
  'aria-label'?: string
  className?: string
}

const POPOVER_ESTIMATED_HEIGHT = 336

const NAV_BTN_CLASS =
  'grid size-7 shrink-0 place-items-center rounded-[7px] text-ink-3 transition-all hover:bg-surface-3 hover:text-ink active:scale-95 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand disabled:pointer-events-none disabled:opacity-35'

function localTodayString(): string {
  const now = new Date()
  return formatIsoDate(now.getFullYear(), now.getMonth() + 1, now.getDate())
}

/**
 * 自定义日期选择器：沿用全站弹窗与浮层视觉（bg-glass-pop + shadow-pop + 14px 圆角 +
 * 品牌渐变选中态 + 磨砂内嵌触发框），替代外观不受主题/暗黑/语言控制的原生 date input。
 */
export function DatePicker({
  value,
  onChange,
  min,
  max,
  today,
  rangeStart,
  rangeEnd,
  placeholder,
  disabled = false,
  align = 'start',
  id,
  'aria-label': ariaLabel,
  className,
}: DatePickerProps) {
  const { t, i18n } = useTranslation()
  const popoverId = useId()
  const rootRef = useRef<HTMLDivElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const popoverRef = useRef<HTMLDivElement>(null)

  const effectiveToday = today ?? localTodayString()
  // i18n.language 变更时重新取 localeTag，驱动星期/月份标题重算
  const locale = i18n.language === 'en' ? 'en-US' : localeTag()

  const resolveInitialViewMonth = useCallback(() => {
    const parsed = parseIsoDate(value) ?? parseIsoDate(effectiveToday) ?? parseIsoDate(max ?? '') ?? parseIsoDate(min ?? '')
    return parsed ? { year: parsed.year, month: parsed.month } : { year: new Date().getFullYear(), month: new Date().getMonth() + 1 }
  }, [value, effectiveToday, max, min])

  const [open, setOpen] = useState(false)
  const [view, setView] = useState<PickerView>('day')
  const [side, setSide] = useState<'bottom' | 'top'>('bottom')
  const [cursor, setCursor] = useState<{ year: number; month: number }>(resolveInitialViewMonth)

  const closePopover = useCallback((opts?: { restoreFocus?: boolean }) => {
    setOpen(false)
    if (opts?.restoreFocus) {
      requestAnimationFrame(() => triggerRef.current?.focus())
    }
  }, [])

  const openPopover = useCallback(() => {
    if (disabled) return
    setCursor(resolveInitialViewMonth())
    setView('day')
    setOpen(true)
  }, [disabled, resolveInitialViewMonth])

  if (disabled && open) {
    setOpen(false)
  }

  // 根据触发器与视口底部距离自适应向上/向下弹出，避免短视口裁剪
  useLayoutEffect(() => {
    if (!open || !triggerRef.current) return
    const rect = triggerRef.current.getBoundingClientRect()
    const spaceBelow = window.innerHeight - rect.bottom
    const spaceAbove = rect.top
    setSide(spaceBelow < POPOVER_ESTIMATED_HEIGHT && spaceAbove > spaceBelow ? 'top' : 'bottom')
  }, [open])

  // 点外部、焦点移出（Tab 切走）或按 Escape 时关闭；window capture 抢在 Radix Dialog 的 document capture 之前，
  // 保证按 Escape 只收起日历浮层而不连带关掉外层弹窗
  useEffect(() => {
    if (!open) return
    const onPointerDown = (e: PointerEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    const onFocusIn = (e: FocusEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        e.stopPropagation()
        closePopover({ restoreFocus: true })
      }
    }
    document.addEventListener('pointerdown', onPointerDown)
    document.addEventListener('focusin', onFocusIn)
    window.addEventListener('keydown', onKeyDown, { capture: true })
    return () => {
      document.removeEventListener('pointerdown', onPointerDown)
      document.removeEventListener('focusin', onFocusIn)
      window.removeEventListener('keydown', onKeyDown, { capture: true })
    }
  }, [open, closePopover])

  const weekdayLabels = useMemo(() => {
    // 2026-11-01 为周日，连续 7 天即周日..周六
    return Array.from({ length: 7 }, (_, idx) => {
      const date = new Date(Date.UTC(2026, 10, 1 + idx))
      const full = date.toLocaleDateString(locale, { weekday: 'long', timeZone: 'UTC' })
      const short =
        locale === 'en-US'
          ? date.toLocaleDateString(locale, { weekday: 'short', timeZone: 'UTC' }).slice(0, 2)
          : date.toLocaleDateString(locale, { weekday: 'narrow', timeZone: 'UTC' })
      return { short, full }
    })
  }, [locale])

  const monthNames = useMemo(() => {
    return Array.from({ length: 12 }, (_, idx) =>
      new Date(Date.UTC(2026, idx, 1)).toLocaleDateString(locale, { month: 'short', timeZone: 'UTC' }),
    )
  }, [locale])

  const headerYearMonthLabel = useMemo(() => {
    return new Date(Date.UTC(cursor.year, cursor.month - 1, 1)).toLocaleDateString(locale, {
      year: 'numeric',
      month: 'long',
      timeZone: 'UTC',
    })
  }, [cursor.year, cursor.month, locale])

  const cells = useMemo(() => buildCalendarCells(cursor.year, cursor.month), [cursor.year, cursor.month])
  const decadeStart = yearGridStart(cursor.year)
  const years = useMemo(() => Array.from({ length: 12 }, (_, idx) => decadeStart + idx), [decadeStart])

  const prevMonth = shiftMonth(cursor.year, cursor.month, -1)
  const nextMonth = shiftMonth(cursor.year, cursor.month, 1)
  const prevMonthDisabled = isMonthDisabled(prevMonth.year, prevMonth.month, min, max)
  const nextMonthDisabled = isMonthDisabled(nextMonth.year, nextMonth.month, min, max)
  const prevYearDisabled = isYearDisabled(cursor.year - 1, min, max)
  const nextYearDisabled = isYearDisabled(cursor.year + 1, min, max)
  const prevDecadeDisabled = isYearRangeDisabled(decadeStart - 12, 12, min, max)
  const nextDecadeDisabled = isYearRangeDisabled(decadeStart + 12, 12, min, max)
  const todayDisabled = isDateDisabled(effectiveToday, min, max)

  const hasValidRange = Boolean(rangeStart && rangeEnd && rangeStart <= rangeEnd)

  const selectDate = (date: string) => {
    if (isDateDisabled(date, min, max)) return
    onChange(date)
    closePopover({ restoreFocus: true })
  }

  const focusDayCell = (targetDate: string) => {
    const parts = parseIsoDate(targetDate)
    if (!parts) return
    if (parts.year !== cursor.year || parts.month !== cursor.month) {
      setCursor({ year: parts.year, month: parts.month })
    }
    requestAnimationFrame(() => {
      const btn = popoverRef.current?.querySelector<HTMLButtonElement>(`button[data-date="${targetDate}"]`)
      btn?.focus()
    })
  }

  const handleDayKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, date: string) => {
    const deltaByKey: Record<string, number> = {
      ArrowLeft: -1,
      ArrowRight: 1,
      ArrowUp: -7,
      ArrowDown: 7,
    }
    const delta = deltaByKey[e.key]
    if (delta === undefined) return
    e.preventDefault()
    const nextDate = shiftIsoDate(date, delta)
    if (!nextDate || isDateDisabled(nextDate, min, max)) return
    focusDayCell(nextDate)
  }

  return (
    <div ref={rootRef} className={cn('relative', open && 'z-50')}>
      <button
        ref={triggerRef}
        id={id}
        type="button"
        disabled={disabled}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? popoverId : undefined}
        aria-label={ariaLabel}
        onClick={() => (open ? closePopover() : openPopover())}
        onKeyDown={(e) => {
          if (!open && e.key === 'ArrowDown') {
            e.preventDefault()
            openPopover()
          }
        }}
        className={cn(
          'flex h-9 w-full items-center justify-between gap-2 rounded-[7px] border bg-surface-2 px-3 font-mono text-[12.5px] tabular-nums shadow-hair outline-none transition-colors',
          open
            ? 'border-brand ring-1 ring-brand'
            : 'border-transparent hover:ring-1 hover:ring-hairline-2 focus-visible:border-brand focus-visible:ring-1 focus-visible:ring-brand',
          'disabled:cursor-not-allowed disabled:opacity-50',
          value ? 'text-ink' : 'text-ink-3',
          className,
        )}
      >
        <span className="truncate">{value || placeholder || t('datePicker.placeholder')}</span>
        <Calendar className="size-[15px] shrink-0 text-ink-3" strokeWidth={1.75} aria-hidden="true" />
      </button>

      {open && (
        <div
          ref={popoverRef}
          id={popoverId}
          role="dialog"
          aria-label={ariaLabel ?? t('datePicker.dialogLabel')}
          tabIndex={-1}
          className={cn(
            'absolute z-50 w-[280px] max-w-[calc(100vw-3rem)] rounded-[14px] border border-transparent bg-glass-pop p-3.5 shadow-pop outline-none',
            'animate-popover-in motion-reduce:animate-none',
            side === 'top' ? 'bottom-full mb-1.5' : 'top-full mt-1.5',
            align === 'end'
              ? side === 'top'
                ? 'left-0 origin-bottom-left sm:left-auto sm:right-0 sm:origin-bottom-right'
                : 'left-0 origin-top-left sm:left-auto sm:right-0 sm:origin-top-right'
              : side === 'top'
                ? 'left-0 origin-bottom-left'
                : 'left-0 origin-top-left',
          )}
        >
          {/* 顶栏：年月切换触发器 + 上下翻页按钮 */}
          <div className="mb-2.5 flex items-center justify-between gap-1">
            {view === 'day' ? (
              <button
                type="button"
                onClick={() => setView('month')}
                aria-label={t('datePicker.selectMonthYear')}
                className="inline-flex h-7 items-center gap-1 rounded-[7px] px-2 text-[13px] font-semibold tracking-tight text-ink transition-colors hover:bg-surface-3 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand"
              >
                <span>{headerYearMonthLabel}</span>
                <ChevronDown className="size-3.5 text-ink-3" strokeWidth={1.9} aria-hidden="true" />
              </button>
            ) : view === 'month' ? (
              <button
                type="button"
                onClick={() => setView('year')}
                aria-label={t('datePicker.selectYear')}
                className="inline-flex h-7 items-center gap-1 rounded-[7px] px-2 font-mono text-[13px] font-semibold tabular-nums tracking-tight text-ink transition-colors hover:bg-surface-3 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand"
              >
                <span>{t('datePicker.yearLabel', { year: cursor.year })}</span>
                <ChevronDown className="size-3.5 text-ink-3" strokeWidth={1.9} aria-hidden="true" />
              </button>
            ) : (
              <button
                type="button"
                onClick={() => setView('day')}
                aria-label={t('datePicker.backToDays')}
                className="inline-flex h-7 items-center gap-1 rounded-[7px] px-2 font-mono text-[12.5px] font-semibold tabular-nums tracking-tight text-ink transition-colors hover:bg-surface-3 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand"
              >
                <span>
                  {decadeStart} – {decadeStart + 11}
                </span>
                <ChevronDown className="size-3.5 rotate-180 text-ink-3" strokeWidth={1.9} aria-hidden="true" />
              </button>
            )}

            <div className="flex items-center gap-0.5">
              <button
                type="button"
                disabled={
                  view === 'day'
                    ? prevMonthDisabled
                    : view === 'month'
                      ? prevYearDisabled
                      : prevDecadeDisabled
                }
                aria-label={
                  view === 'day'
                    ? t('datePicker.prevMonth')
                    : view === 'month'
                      ? t('datePicker.prevYear')
                      : t('datePicker.prevDecade')
                }
                onClick={() => {
                  if (view === 'day') setCursor(prevMonth)
                  else if (view === 'month') setCursor((c) => ({ ...c, year: c.year - 1 }))
                  else setCursor((c) => ({ ...c, year: c.year - 12 }))
                }}
                className={NAV_BTN_CLASS}
              >
                <ChevronLeft className="size-[14px]" strokeWidth={1.9} aria-hidden="true" />
              </button>
              <button
                type="button"
                disabled={
                  view === 'day'
                    ? nextMonthDisabled
                    : view === 'month'
                      ? nextYearDisabled
                      : nextDecadeDisabled
                }
                aria-label={
                  view === 'day'
                    ? t('datePicker.nextMonth')
                    : view === 'month'
                      ? t('datePicker.nextYear')
                      : t('datePicker.nextDecade')
                }
                onClick={() => {
                  if (view === 'day') setCursor(nextMonth)
                  else if (view === 'month') setCursor((c) => ({ ...c, year: c.year + 1 }))
                  else setCursor((c) => ({ ...c, year: c.year + 12 }))
                }}
                className={NAV_BTN_CLASS}
              >
                <ChevronRight className="size-[14px]" strokeWidth={1.9} aria-hidden="true" />
              </button>
            </div>
          </div>

          {/* 视图一：日历 7×6 网格 */}
          {view === 'day' && (
            <>
              <div className="mb-1 grid grid-cols-7 gap-1">
                {weekdayLabels.map((wd, idx) => (
                  <div
                    key={idx}
                    title={wd.full}
                    aria-label={wd.full}
                    className="py-1 text-center text-[10.5px] font-semibold uppercase tracking-[0.05em] text-ink-3"
                  >
                    {wd.short}
                  </div>
                ))}
              </div>

              <div className="grid grid-cols-7 gap-1">
                {cells.map((cell) => {
                  const isDisabled = isDateDisabled(cell.date, min, max)
                  const isSelected = cell.date === value
                  const isToday = cell.date === effectiveToday
                  const isRangeEndpoint =
                    hasValidRange && !isSelected && (cell.date === rangeStart || cell.date === rangeEnd)
                  const isInRange =
                    hasValidRange && !isSelected && !isRangeEndpoint && cell.date > rangeStart! && cell.date < rangeEnd!

                  return (
                    <button
                      key={cell.date}
                      type="button"
                      data-date={cell.date}
                      disabled={isDisabled}
                      aria-label={cell.date}
                      aria-pressed={isSelected}
                      onClick={() => selectDate(cell.date)}
                      onKeyDown={(e) => handleDayKeyDown(e, cell.date)}
                      className={cn(
                        'relative grid h-8 w-full place-items-center rounded-[7px] font-mono text-[12px] tabular-nums transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                        isSelected &&
                          'bg-brand btn-brand-gradient font-semibold text-brand-fg shadow-[inset_0_1px_0_rgba(255,255,255,0.18),var(--shadow-sm)]',
                        !isSelected &&
                          isRangeEndpoint &&
                          'bg-brand-soft font-semibold text-brand ring-1 ring-inset ring-brand-line',
                        !isSelected &&
                          isInRange &&
                          (cell.inCurrentMonth ? 'bg-brand-soft text-ink' : 'bg-brand-soft text-ink-3 opacity-75'),
                        !isSelected &&
                          !isRangeEndpoint &&
                          !isInRange &&
                          (cell.inCurrentMonth
                            ? isToday
                              ? 'font-semibold text-brand ring-1 ring-inset ring-brand-line hover:bg-brand-soft'
                              : 'text-ink hover:bg-surface-3'
                            : 'text-ink-3 opacity-45 hover:bg-surface-3 hover:opacity-85'),
                        isDisabled && 'pointer-events-none opacity-30',
                      )}
                    >
                      <span>{cell.day}</span>
                      {isToday && !isSelected && (
                        <span
                          aria-hidden="true"
                          className="absolute bottom-[3px] left-1/2 size-[3px] -translate-x-1/2 rounded-full bg-brand"
                        />
                      )}
                    </button>
                  )
                })}
              </div>
            </>
          )}

          {/* 视图二：12 月份选择网格 */}
          {view === 'month' && (
            <div className="grid grid-cols-3 gap-1.5 py-1">
              {monthNames.map((label, idx) => {
                const monthNum = idx + 1
                const monthDisabled = isMonthDisabled(cursor.year, monthNum, min, max)
                const selectedParts = parseIsoDate(value)
                const isSelectedMonth = selectedParts?.year === cursor.year && selectedParts?.month === monthNum
                const isCursorMonth = cursor.month === monthNum

                return (
                  <button
                    key={monthNum}
                    type="button"
                    disabled={monthDisabled}
                    onClick={() => {
                      setCursor((c) => ({ ...c, month: monthNum }))
                      setView('day')
                    }}
                    className={cn(
                      'h-9 rounded-[8px] text-[12px] font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                      isSelectedMonth &&
                        'bg-brand btn-brand-gradient font-semibold text-brand-fg shadow-[inset_0_1px_0_rgba(255,255,255,0.18),var(--shadow-sm)]',
                      !isSelectedMonth && isCursorMonth && 'bg-brand-soft font-semibold text-brand ring-1 ring-inset ring-brand-line',
                      !isSelectedMonth && !isCursorMonth && 'text-ink hover:bg-surface-3',
                      monthDisabled && 'pointer-events-none opacity-30',
                    )}
                  >
                    {label}
                  </button>
                )
              })}
            </div>
          )}

          {/* 视图三：12 年份选择网格 */}
          {view === 'year' && (
            <div className="grid grid-cols-3 gap-1.5 py-1">
              {years.map((y) => {
                const yearDisabled = isYearDisabled(y, min, max)
                const selectedParts = parseIsoDate(value)
                const isSelectedYear = selectedParts?.year === y
                const isCursorYear = cursor.year === y

                return (
                  <button
                    key={y}
                    type="button"
                    disabled={yearDisabled}
                    onClick={() => {
                      setCursor((c) => ({ ...c, year: y }))
                      setView('month')
                    }}
                    className={cn(
                      'h-9 rounded-[8px] font-mono text-[12px] font-medium tabular-nums transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                      isSelectedYear &&
                        'bg-brand btn-brand-gradient font-semibold text-brand-fg shadow-[inset_0_1px_0_rgba(255,255,255,0.18),var(--shadow-sm)]',
                      !isSelectedYear && isCursorYear && 'bg-brand-soft font-semibold text-brand ring-1 ring-inset ring-brand-line',
                      !isSelectedYear && !isCursorYear && 'text-ink hover:bg-surface-3',
                      yearDisabled && 'pointer-events-none opacity-30',
                    )}
                  >
                    {y}
                  </button>
                )
              })}
            </div>
          )}

          {/* 页脚：当前日期展示 + 今天快捷选择 */}
          <div className="mt-2.5 flex items-center justify-between gap-2 border-t-[0.5px] border-hairline pt-2.5">
            <span className="truncate font-mono text-[11px] tabular-nums text-ink-3">
              {value || placeholder || 'YYYY-MM-DD'}
            </span>
            <button
              type="button"
              disabled={todayDisabled}
              onClick={() => selectDate(effectiveToday)}
              className="inline-flex h-6 shrink-0 items-center rounded-[6px] px-2 text-[11.5px] font-medium text-brand transition-colors hover:bg-brand-soft focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand disabled:pointer-events-none disabled:opacity-35"
            >
              {t('datePicker.today')}
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
