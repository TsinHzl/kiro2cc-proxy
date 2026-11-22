// Copyright (c) 2026 Harllan He. Licensed under MIT.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const {
  buildCalendarCells,
  daysInMonth,
  firstWeekdayOfMonth,
  formatIsoDate,
  isDateDisabled,
  isMonthDisabled,
  isYearDisabled,
  isYearRangeDisabled,
  parseIsoDate,
  shiftIsoDate,
  shiftMonth,
  yearGridStart,
} = await import('../src/lib/calendar.ts')

const zh = JSON.parse(readFileSync(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8'))
const en = JSON.parse(readFileSync(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8'))

test('parseIsoDate 严格校验公历闰年与无效日期', () => {
  assert.deepEqual(parseIsoDate('2026-11-22'), { year: 2026, month: 11, day: 22 })
  assert.deepEqual(parseIsoDate('2024-02-29'), { year: 2024, month: 2, day: 29 })
  assert.equal(parseIsoDate('2026-02-29'), null)
  assert.equal(parseIsoDate('2026-00-10'), null)
  assert.equal(parseIsoDate('2026-13-01'), null)
  assert.equal(parseIsoDate('2026-04-31'), null)
  assert.equal(parseIsoDate('2026/11/22'), null)
  assert.equal(parseIsoDate(''), null)
})

test('daysInMonth 与 firstWeekdayOfMonth 与截图 2026-11 对齐', () => {
  // 2026-11-01 为周日（weekday = 0），11 月共 30 天
  assert.equal(firstWeekdayOfMonth(2026, 11), 0)
  assert.equal(daysInMonth(2026, 11), 30)
  assert.equal(daysInMonth(2024, 2), 29)
  assert.equal(daysInMonth(2026, 2), 28)
})

test('buildCalendarCells 生成固定 42 格且首尾月份拼接正确', () => {
  const nov = buildCalendarCells(2026, 11)
  assert.equal(nov.length, 42)
  assert.deepEqual(nov[0], { date: '2026-11-01', year: 2026, month: 11, day: 1, inCurrentMonth: true })
  assert.deepEqual(nov[29], { date: '2026-11-30', year: 2026, month: 11, day: 30, inCurrentMonth: true })
  assert.deepEqual(nov[30], { date: '2026-12-01', year: 2026, month: 12, day: 1, inCurrentMonth: false })
  assert.deepEqual(nov[41], { date: '2026-12-12', year: 2026, month: 12, day: 12, inCurrentMonth: false })

  // 跨年补齐（2026-01-01 为周四，前面补 4 天 2025-12-28..31）
  const jan = buildCalendarCells(2026, 1)
  assert.equal(jan.length, 42)
  assert.deepEqual(jan[0], { date: '2025-12-28', year: 2025, month: 12, day: 28, inCurrentMonth: false })
  assert.deepEqual(jan[3], { date: '2025-12-31', year: 2025, month: 12, day: 31, inCurrentMonth: false })
  assert.deepEqual(jan[4], { date: '2026-01-01', year: 2026, month: 1, day: 1, inCurrentMonth: true })
})

test('shiftMonth、shiftIsoDate 与 yearGridStart 跨年/跨月边界正确', () => {
  assert.deepEqual(shiftMonth(2026, 1, -1), { year: 2025, month: 12 })
  assert.deepEqual(shiftMonth(2026, 12, 1), { year: 2027, month: 1 })
  assert.deepEqual(shiftMonth(2026, 3, -15), { year: 2024, month: 12 })

  assert.equal(shiftIsoDate('2026-03-01', -1), '2026-02-28')
  assert.equal(shiftIsoDate('2024-03-01', -1), '2024-02-29')
  assert.equal(shiftIsoDate('2026-11-22', 7), '2026-11-29')
  assert.equal(shiftIsoDate('invalid', 1), null)

  assert.equal(formatIsoDate(2026, 1, 5), '2026-01-05')
  assert.equal(yearGridStart(2026), 2016)
})

test('min/max 禁用判断准确覆盖日、月、年三个层级', () => {
  assert.equal(isDateDisabled('2026-11-22', undefined, '2026-11-22'), false)
  assert.equal(isDateDisabled('2026-11-23', undefined, '2026-11-22'), true)
  assert.equal(isDateDisabled('2026-11-01', '2026-11-05', '2026-11-22'), true)

  assert.equal(isMonthDisabled(2026, 11, undefined, '2026-11-22'), false)
  assert.equal(isMonthDisabled(2026, 12, undefined, '2026-11-22'), true)
  assert.equal(isMonthDisabled(2026, 10, '2026-11-01', '2026-11-22'), true)

  assert.equal(isYearDisabled(2026, undefined, '2026-11-22'), false)
  assert.equal(isYearDisabled(2027, undefined, '2026-11-22'), true)
  assert.equal(isYearDisabled(2025, '2026-01-01', '2026-11-22'), true)

  assert.equal(isYearRangeDisabled(2016, 12, undefined, '2026-11-22'), false)
  assert.equal(isYearRangeDisabled(2028, 12, undefined, '2026-11-22'), true)
  assert.equal(isYearRangeDisabled(2004, 12, '2020-01-01', '2026-11-22'), true)
})

test('datePicker i18n 文案在中英文词典中保持键对齐且非空', () => {
  const zhKeys = Object.keys(zh.datePicker ?? {}).sort()
  const enKeys = Object.keys(en.datePicker ?? {}).sort()
  assert.deepEqual(zhKeys, enKeys)
  assert.ok(zhKeys.length > 0)
  for (const key of zhKeys) {
    assert.equal(typeof zh.datePicker[key], 'string')
    assert.equal(typeof en.datePicker[key], 'string')
    assert.ok(zh.datePicker[key].length > 0)
    assert.ok(en.datePicker[key].length > 0)
  }
})
