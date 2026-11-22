// Copyright (c) 2026 Harllan He. Licensed under MIT.

export interface DateParts {
  year: number
  month: number // 1-12
  day: number // 1-31
}

export interface CalendarCell {
  date: string
  year: number
  month: number
  day: number
  inCurrentMonth: boolean
}

const ISO_DATE_RE = /^(\d{4})-(\d{2})-(\d{2})$/
const DAY_MS = 86_400_000
const GRID_CELL_COUNT = 42

/** 指定公历年月包含的天数（month 为 1–12） */
export function daysInMonth(year: number, month: number): number {
  return new Date(Date.UTC(year, month, 0)).getUTCDate()
}

/** 指定公历年月 1 号的星期（0 = 周日 .. 6 = 周六，按 UTC 算术避免本地时区漂移） */
export function firstWeekdayOfMonth(year: number, month: number): number {
  return new Date(Date.UTC(year, month - 1, 1)).getUTCDay()
}

/** 格式化公历日期为 YYYY-MM-DD */
export function formatIsoDate(year: number, month: number, day: number): string {
  return `${String(year).padStart(4, '0')}-${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`
}

/** 严格解析 YYYY-MM-DD（校验真实公历日，拒绝如 2026-02-29 等越界日期） */
export function parseIsoDate(value: string): DateParts | null {
  const match = ISO_DATE_RE.exec(value)
  if (!match) return null
  const year = Number(match[1])
  const month = Number(match[2])
  const day = Number(match[3])
  if (month < 1 || month > 12) return null
  if (day < 1 || day > daysInMonth(year, month)) return null
  return { year, month, day }
}

/** 月份偏移（支持跨年正负偏移，month 为 1–12） */
export function shiftMonth(year: number, month: number, deltaMonths: number): { year: number; month: number } {
  const total = year * 12 + (month - 1) + deltaMonths
  const nextYear = Math.floor(total / 12)
  const nextMonth = ((total % 12) + 12) % 12 + 1
  return { year: nextYear, month: nextMonth }
}

/** 日期按天数偏移（UTC 算术，无夏令时与本地时区偏移问题） */
export function shiftIsoDate(dateStr: string, deltaDays: number): string | null {
  const parts = parseIsoDate(dateStr)
  if (!parts) return null
  const ms = Date.UTC(parts.year, parts.month - 1, parts.day) + deltaDays * DAY_MS
  return new Date(ms).toISOString().slice(0, 10)
}

/** 判断日历日是否超出 [min, max] 允许区间 */
export function isDateDisabled(date: string, min?: string, max?: string): boolean {
  if (min && date < min) return true
  if (max && date > max) return true
  return false
}

/** 判断整月是否完全落在 [min, max] 之外 */
export function isMonthDisabled(year: number, month: number, min?: string, max?: string): boolean {
  const start = formatIsoDate(year, month, 1)
  const end = formatIsoDate(year, month, daysInMonth(year, month))
  if (min && end < min) return true
  if (max && start > max) return true
  return false
}

/** 判断整年是否完全落在 [min, max] 之外 */
export function isYearDisabled(year: number, min?: string, max?: string): boolean {
  const start = formatIsoDate(year, 1, 1)
  const end = formatIsoDate(year, 12, 31)
  if (min && end < min) return true
  if (max && start > max) return true
  return false
}

/** 判断连续年份区间是否全部禁用 */
export function isYearRangeDisabled(startYear: number, count: number, min?: string, max?: string): boolean {
  for (let i = 0; i < count; i++) {
    if (!isYearDisabled(startYear + i, min, max)) return false
  }
  return true
}

/** 12 年网格起始年份（4×3 布局，按 12 年对齐） */
export function yearGridStart(year: number): number {
  return Math.floor(year / 12) * 12
}

/** 构建固定 6 行 × 7 列（42 格）的月历单元格（周日起始），切月时弹窗高度保持稳定 */
export function buildCalendarCells(year: number, month: number): CalendarCell[] {
  const leadingCount = firstWeekdayOfMonth(year, month)
  const currentDays = daysInMonth(year, month)
  const prev = shiftMonth(year, month, -1)
  const prevDays = daysInMonth(prev.year, prev.month)
  const next = shiftMonth(year, month, 1)

  const cells: CalendarCell[] = []
  for (let i = leadingCount - 1; i >= 0; i--) {
    const day = prevDays - i
    cells.push({
      date: formatIsoDate(prev.year, prev.month, day),
      year: prev.year,
      month: prev.month,
      day,
      inCurrentMonth: false,
    })
  }
  for (let day = 1; day <= currentDays; day++) {
    cells.push({
      date: formatIsoDate(year, month, day),
      year,
      month,
      day,
      inCurrentMonth: true,
    })
  }
  const trailingCount = GRID_CELL_COUNT - cells.length
  for (let day = 1; day <= trailingCount; day++) {
    cells.push({
      date: formatIsoDate(next.year, next.month, day),
      year: next.year,
      month: next.month,
      day,
      inCurrentMonth: false,
    })
  }
  return cells
}
