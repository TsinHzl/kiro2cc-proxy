// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Calendar, ChevronDown, ChevronLeft, ChevronRight } from 'lucide-react'
import {
  type CalendarCell,
  buildCalendarCells,
  formatIsoDate,
  isDateDisabled,
  isMonthDisabled,
  isYearDisabled,
  isYearRangeDisabled,
  parseIsoDate,
  resolveTabStopDate,
  shiftIsoDate,
  shiftMonth,
  utcDate,
  yearGridStart,
} from '@/lib/calendar'
import { localeTag } from '@/lib/locale'
import { cn } from '@/lib/utils'

type PickerView = 'day' | 'month' | 'year'
type PopoverAlign = 'start' | 'end'
type PopoverSide = 'top' | 'bottom'

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
  align?: PopoverAlign
  id?: string
  'aria-label'?: string
  className?: string
}

const DEFAULT_MIN_DATE = '1000-01-01'
const DEFAULT_MAX_DATE = '9999-12-31'
const POPOVER_ESTIMATED_HEIGHT = 336

const NAV_BTN_CLASS =
  'grid size-7 shrink-0 place-items-center rounded-[7px] text-ink-3 transition-all hover:bg-surface-3 hover:text-ink active:scale-95 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand disabled:pointer-events-none disabled:opacity-35'

const SELECTED_CELL_CLASS =
  'bg-brand btn-brand-gradient font-semibold text-brand-fg shadow-[inset_0_1px_0_rgba(255,255,255,0.18),var(--shadow-sm)]'

const CURSOR_CELL_CLASS = 'bg-brand-soft font-semibold text-brand ring-1 ring-inset ring-brand-line'

const POPOVER_POSITION_CLASS: Record<`${PopoverAlign}-${PopoverSide}`, string> = {
  'start-top': 'bottom-full mb-1.5 left-0 origin-bottom-left',
  'start-bottom': 'top-full mt-1.5 left-0 origin-top-left',
  'end-top': 'bottom-full mb-1.5 left-0 origin-bottom-left sm:left-auto sm:right-0 sm:origin-bottom-right',
  'end-bottom': 'top-full mt-1.5 left-0 origin-top-left sm:left-auto sm:right-0 sm:origin-top-right',
}

const DAY_ARROW_DELTA: Record<string, number> = {
  ArrowLeft: -1,
  ArrowRight: 1,
  ArrowUp: -7,
  ArrowDown: 7,
}

function localTodayString(): string {
  const now = new Date()
  return formatIsoDate(now.getFullYear(), now.getMonth() + 1, now.getDate())
}

interface NavAction {
  disabled: boolean
  label: string
  onClick: () => void
}

interface DatePickerHeaderProps {
  view: PickerView
  titleLabel: string
  titleAriaLabel: string
  onCycleView: () => void
  prev: NavAction
  next: NavAction
}

/** 浮层顶栏：年月/decade 视图切换按钮 + 上/下翻页控制 */
function DatePickerHeader({ view, titleLabel, titleAriaLabel, onCycleView, prev, next }: DatePickerHeaderProps) {
  return (
    <div className="mb-2.5 flex items-center justify-between gap-1">
      <button
        type="button"
        onClick={onCycleView}
        aria-label={titleAriaLabel}
        className={cn(
          'inline-flex h-7 items-center gap-1 rounded-[7px] px-2 font-semibold tracking-tight text-ink transition-colors hover:bg-surface-3 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
          view === 'day' ? 'text-[13px]' : 'font-mono tabular-nums',
          view === 'month' && 'text-[13px]',
          view === 'year' && 'text-[12.5px]',
        )}
      >
        <span>{titleLabel}</span>
        <ChevronDown
          className={cn('size-3.5 text-ink-3', view === 'year' && 'rotate-180')}
          strokeWidth={1.9}
          aria-hidden="true"
        />
      </button>

      <div className="flex items-center gap-0.5">
        <button type="button" disabled={prev.disabled} aria-label={prev.label} onClick={prev.onClick} className={NAV_BTN_CLASS}>
          <ChevronLeft className="size-[14px]" strokeWidth={1.9} aria-hidden="true" />
        </button>
        <button type="button" disabled={next.disabled} aria-label={next.label} onClick={next.onClick} className={NAV_BTN_CLASS}>
          <ChevronRight className="size-[14px]" strokeWidth={1.9} aria-hidden="true" />
        </button>
      </div>
    </div>
  )
}

interface DayGridProps {
  cells: CalendarCell[]
  weekdayLabels: Array<{ short: string; full: string }>
  value: string
  today: string
  min: string
  max: string
  rangeStart?: string
  rangeEnd?: string
  tabStopDate: string | null
  onSelect: (date: string) => void
  onFocusDate: (date: string) => void
  onDayKeyDown: (e: React.KeyboardEvent<HTMLButtonElement>, date: string) => void
}

/** 视图一：周日至周六 7×6 日期网格（roving tabindex + 方向键导航） */
function DayGrid({
  cells,
  weekdayLabels,
  value,
  today,
  min,
  max,
  rangeStart,
  rangeEnd,
  tabStopDate,
  onSelect,
  onFocusDate,
  onDayKeyDown,
}: DayGridProps) {
  const validRange = rangeStart && rangeEnd && rangeStart <= rangeEnd ? { start: rangeStart, end: rangeEnd } : null

  return (
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
          const isToday = cell.date === today
          const isRangeEndpoint =
            validRange !== null && !isSelected && (cell.date === validRange.start || cell.date === validRange.end)
          const isInRange =
            validRange !== null && !isSelected && !isRangeEndpoint && cell.date > validRange.start && cell.date < validRange.end

          return (
            <button
              key={cell.date}
              type="button"
              data-date={cell.date}
              tabIndex={!isDisabled && cell.date === tabStopDate ? 0 : -1}
              disabled={isDisabled}
              aria-label={cell.date}
              aria-pressed={isSelected}
              onClick={() => onSelect(cell.date)}
              onFocus={() => onFocusDate(cell.date)}
              onKeyDown={(e) => onDayKeyDown(e, cell.date)}
              className={cn(
                'relative grid h-8 w-full place-items-center rounded-[7px] font-mono text-[12px] tabular-nums transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
                isSelected && SELECTED_CELL_CLASS,
                !isSelected && isRangeEndpoint && CURSOR_CELL_CLASS,
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
  )
}

interface MonthGridProps {
  year: number
  cursorMonth: number
  monthNames: string[]
  selectedYear?: number
  selectedMonth?: number
  min: string
  max: string
  onSelectMonth: (month: number) => void
}

/** 视图二：12 月份选择网格 */
function MonthGrid({ year, cursorMonth, monthNames, selectedYear, selectedMonth, min, max, onSelectMonth }: MonthGridProps) {
  return (
    <div className="grid grid-cols-3 gap-1.5 py-1">
      {monthNames.map((label, idx) => {
        const monthNum = idx + 1
        const monthDisabled = isMonthDisabled(year, monthNum, min, max)
        const isSelected = selectedYear === year && selectedMonth === monthNum
        const isCursor = cursorMonth === monthNum

        return (
          <button
            key={monthNum}
            type="button"
            disabled={monthDisabled}
            onClick={() => onSelectMonth(monthNum)}
            className={cn(
              'h-9 rounded-[8px] text-[12px] font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
              isSelected && SELECTED_CELL_CLASS,
              !isSelected && isCursor && CURSOR_CELL_CLASS,
              !isSelected && !isCursor && 'text-ink hover:bg-surface-3',
              monthDisabled && 'pointer-events-none opacity-30',
            )}
          >
            {label}
          </button>
        )
      })}
    </div>
  )
}

interface YearGridProps {
  years: number[]
  cursorYear: number
  selectedYear?: number
  min: string
  max: string
  onSelectYear: (year: number) => void
}

/** 视图三：12 年份选择网格 */
function YearGrid({ years, cursorYear, selectedYear, min, max, onSelectYear }: YearGridProps) {
  return (
    <div className="grid grid-cols-3 gap-1.5 py-1">
      {years.map((y) => {
        const yearDisabled = isYearDisabled(y, min, max)
        const isSelected = selectedYear === y
        const isCursor = cursorYear === y

        return (
          <button
            key={y}
            type="button"
            disabled={yearDisabled}
            onClick={() => onSelectYear(y)}
            className={cn(
              'h-9 rounded-[8px] font-mono text-[12px] font-medium tabular-nums transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand',
              isSelected && SELECTED_CELL_CLASS,
              !isSelected && isCursor && CURSOR_CELL_CLASS,
              !isSelected && !isCursor && 'text-ink hover:bg-surface-3',
              yearDisabled && 'pointer-events-none opacity-30',
            )}
          >
            {y}
          </button>
        )
      })}
    </div>
  )
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
  // 未传 min/max 时兜底到 [1000, 9999]（若调用方显式传了早于 1000 的值或 min 则放开至 0000）
  const effectiveMin = useMemo(() => {
    if (min) return min < '0000-01-01' ? '0000-01-01' : min
    return value && value < DEFAULT_MIN_DATE ? '0000-01-01' : DEFAULT_MIN_DATE
  }, [min, value])
  const effectiveMax = useMemo(() => {
    if (max) return max > DEFAULT_MAX_DATE ? DEFAULT_MAX_DATE : max
    return DEFAULT_MAX_DATE
  }, [max])

  const locale = i18n.language === 'en' ? 'en-US' : localeTag()

  const resolveInitialViewMonth = useCallback(() => {
    const parsed = parseIsoDate(value) ?? parseIsoDate(effectiveToday) ?? parseIsoDate(effectiveMax) ?? parseIsoDate(effectiveMin)
    return parsed ? { year: parsed.year, month: parsed.month } : { year: new Date().getFullYear(), month: new Date().getMonth() + 1 }
  }, [value, effectiveToday, effectiveMax, effectiveMin])

  const [open, setOpen] = useState(false)
  const [view, setView] = useState<PickerView>('day')
  const [side, setSide] = useState<PopoverSide>('bottom')
  const [cursor, setCursor] = useState<{ year: number; month: number }>(resolveInitialViewMonth)
  const [focusedDate, setFocusedDate] = useState<string | null>(null)

  const closePopover = useCallback((opts?: { restoreFocus?: boolean }) => {
    setOpen(false)
    if (opts?.restoreFocus) {
      requestAnimationFrame(() => triggerRef.current?.focus())
    }
  }, [])

  const openPopover = useCallback(() => {
    if (disabled) return
    setCursor(resolveInitialViewMonth())
    setFocusedDate(null)
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

  // 打开浮层或从月视图切回日视图时，将焦点移入当前 tabStop 日期格，使 ArrowDown / 方向键即开即用
  useEffect(() => {
    if (!open || view !== 'day') return
    const rafId = requestAnimationFrame(() => {
      const target =
        popoverRef.current?.querySelector<HTMLButtonElement>('button[data-date][tabindex="0"]:not(:disabled)') ??
        popoverRef.current?.querySelector<HTMLButtonElement>('button[data-date]:not(:disabled)')
      target?.focus({ preventScroll: true })
    })
    return () => cancelAnimationFrame(rafId)
  }, [open, view])

  // 点外部、焦点移出（Tab 切走）或按 Escape 时关闭；window capture 抢在 Radix Dialog 的 document capture 之前
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
      const date = utcDate(2026, 11, 1 + idx)
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
      utcDate(2026, idx + 1, 1).toLocaleDateString(locale, { month: 'short', timeZone: 'UTC' }),
    )
  }, [locale])

  const headerYearMonthLabel = useMemo(() => {
    return utcDate(cursor.year, cursor.month, 1).toLocaleDateString(locale, {
      year: 'numeric',
      month: 'long',
      timeZone: 'UTC',
    })
  }, [cursor.year, cursor.month, locale])

  const selectedParts = useMemo(() => parseIsoDate(value), [value])
  const cells = useMemo(() => buildCalendarCells(cursor.year, cursor.month), [cursor.year, cursor.month])
  const decadeStart = yearGridStart(cursor.year)
  const years = useMemo(() => Array.from({ length: 12 }, (_, idx) => decadeStart + idx), [decadeStart])

  const prevMonth = shiftMonth(cursor.year, cursor.month, -1)
  const nextMonth = shiftMonth(cursor.year, cursor.month, 1)
  const todayDisabled = isDateDisabled(effectiveToday, effectiveMin, effectiveMax)

  const tabStopDate = useMemo(
    () => resolveTabStopDate(cells, focusedDate, value, effectiveToday, effectiveMin, effectiveMax),
    [cells, focusedDate, value, effectiveToday, effectiveMin, effectiveMax],
  )

  const headerConfigByView: Record<PickerView, Omit<DatePickerHeaderProps, 'view'>> = {
    day: {
      titleLabel: headerYearMonthLabel,
      titleAriaLabel: t('datePicker.selectMonthYear'),
      onCycleView: () => setView('month'),
      prev: {
        disabled: isMonthDisabled(prevMonth.year, prevMonth.month, effectiveMin, effectiveMax),
        label: t('datePicker.prevMonth'),
        onClick: () => {
          setFocusedDate(null)
          setCursor(prevMonth)
        },
      },
      next: {
        disabled: isMonthDisabled(nextMonth.year, nextMonth.month, effectiveMin, effectiveMax),
        label: t('datePicker.nextMonth'),
        onClick: () => {
          setFocusedDate(null)
          setCursor(nextMonth)
        },
      },
    },
    month: {
      titleLabel: t('datePicker.yearLabel', { year: cursor.year }),
      titleAriaLabel: t('datePicker.selectYear'),
      onCycleView: () => setView('year'),
      prev: {
        disabled: isYearDisabled(cursor.year - 1, effectiveMin, effectiveMax),
        label: t('datePicker.prevYear'),
        onClick: () => {
          setFocusedDate(null)
          setCursor((c) => ({ ...c, year: c.year - 1 }))
        },
      },
      next: {
        disabled: isYearDisabled(cursor.year + 1, effectiveMin, effectiveMax),
        label: t('datePicker.nextYear'),
        onClick: () => {
          setFocusedDate(null)
          setCursor((c) => ({ ...c, year: c.year + 1 }))
        },
      },
    },
    year: {
      titleLabel: `${decadeStart} – ${decadeStart + 11}`,
      titleAriaLabel: t('datePicker.backToDays'),
      onCycleView: () => setView('day'),
      prev: {
        disabled: isYearRangeDisabled(decadeStart - 12, 12, effectiveMin, effectiveMax),
        label: t('datePicker.prevDecade'),
        onClick: () => {
          setFocusedDate(null)
          setCursor((c) => ({ ...c, year: c.year - 12 }))
        },
      },
      next: {
        disabled: isYearRangeDisabled(decadeStart + 12, 12, effectiveMin, effectiveMax),
        label: t('datePicker.nextDecade'),
        onClick: () => {
          setFocusedDate(null)
          setCursor((c) => ({ ...c, year: c.year + 12 }))
        },
      },
    },
  }

  const selectDate = (date: string) => {
    if (isDateDisabled(date, effectiveMin, effectiveMax)) return
    onChange(date)
    closePopover({ restoreFocus: true })
  }

  const focusDayCell = (targetDate: string) => {
    const parts = parseIsoDate(targetDate)
    if (!parts) return
    setFocusedDate(targetDate)
    if (parts.year !== cursor.year || parts.month !== cursor.month) {
      setCursor({ year: parts.year, month: parts.month })
    }
    requestAnimationFrame(() => {
      popoverRef.current?.querySelector<HTMLButtonElement>(`button[data-date="${targetDate}"]`)?.focus({ preventScroll: true })
    })
  }

  const handleDayKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>, date: string) => {
    const delta = DAY_ARROW_DELTA[e.key]
    if (delta === undefined) return
    e.preventDefault()
    const nextDate = shiftIsoDate(date, delta)
    if (!nextDate || isDateDisabled(nextDate, effectiveMin, effectiveMax)) return
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
            POPOVER_POSITION_CLASS[`${align}-${side}`],
          )}
        >
          <DatePickerHeader view={view} {...headerConfigByView[view]} />

          {view === 'day' && (
            <DayGrid
              cells={cells}
              weekdayLabels={weekdayLabels}
              value={value}
              today={effectiveToday}
              min={effectiveMin}
              max={effectiveMax}
              rangeStart={rangeStart}
              rangeEnd={rangeEnd}
              tabStopDate={tabStopDate}
              onSelect={selectDate}
              onFocusDate={setFocusedDate}
              onDayKeyDown={handleDayKeyDown}
            />
          )}

          {view === 'month' && (
            <MonthGrid
              year={cursor.year}
              cursorMonth={cursor.month}
              monthNames={monthNames}
              selectedYear={selectedParts?.year}
              selectedMonth={selectedParts?.month}
              min={effectiveMin}
              max={effectiveMax}
              onSelectMonth={(monthNum) => {
                setFocusedDate(null)
                setCursor((c) => ({ ...c, month: monthNum }))
                setView('day')
              }}
            />
          )}

          {view === 'year' && (
            <YearGrid
              years={years}
              cursorYear={cursor.year}
              selectedYear={selectedParts?.year}
              min={effectiveMin}
              max={effectiveMax}
              onSelectYear={(y) => {
                setFocusedDate(null)
                setCursor((c) => ({ ...c, year: y }))
                setView('month')
              }}
            />
          )}

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
