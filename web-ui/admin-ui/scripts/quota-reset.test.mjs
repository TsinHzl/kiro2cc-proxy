// Copyright (c) 2026 Harllan He. Licensed under MIT.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const { formatQuotaResetCountdown, isValidResetTimestamp } = await import('../src/lib/quota-reset.ts')
const zh = JSON.parse(readFileSync(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8'))
const en = JSON.parse(readFileSync(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8'))

function makeT(locale) {
  return (key, vars = {}) => {
    const template = key.split('.').reduce((acc, part) => acc?.[part], locale)
    assert.equal(typeof template, 'string', `missing i18n key: ${key}`)
    return template.replace(/\{\{(\w+)\}\}/g, (_, name) => String(vars[name] ?? ''))
  }
}

const tZh = makeT(zh)
const tEn = makeT(en)

test('无效或缺失时间戳返回 null', () => {
  for (const val of [null, undefined, 0, -100, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.equal(isValidResetTimestamp(val), false)
    assert.equal(formatQuotaResetCountdown(val, tZh, 1_700_000_000_000), null)
  }
  assert.equal(isValidResetTimestamp(1_700_000_000), true)
})

test('剩余时间大于等于 1 天时展示天数', () => {
  const nowMs = 1_700_000_000_000
  const oneDayLater = nowMs / 1000 + 86_400
  const nineDaysSixteenHoursLater = nowMs / 1000 + 9 * 86_400 + 16 * 3_600

  assert.equal(formatQuotaResetCountdown(oneDayLater, tZh, nowMs), '还剩 1 天重置')
  assert.equal(formatQuotaResetCountdown(oneDayLater, tEn, nowMs), 'Resets in 1d')
  assert.equal(formatQuotaResetCountdown(nineDaysSixteenHoursLater, tZh, nowMs), '还剩 9 天重置')
})

test('剩余时间少于 1 天时展示小时与分钟', () => {
  const nowMs = 1_700_000_000_000
  const almostOneDay = nowMs / 1000 + 23 * 3_600 + 59 * 60 + 45
  const exactHours = nowMs / 1000 + 5 * 3_600
  const onlyMinutes = nowMs / 1000 + 42 * 60 + 20

  assert.equal(formatQuotaResetCountdown(almostOneDay, tZh, nowMs), '还剩 23 小时 59 分钟重置')
  assert.equal(formatQuotaResetCountdown(almostOneDay, tEn, nowMs), 'Resets in 23h 59m')
  assert.equal(formatQuotaResetCountdown(exactHours, tZh, nowMs), '还剩 5 小时 0 分钟重置')
  assert.equal(formatQuotaResetCountdown(onlyMinutes, tZh, nowMs), '还剩 42 分钟重置')
  assert.equal(formatQuotaResetCountdown(onlyMinutes, tEn, nowMs), 'Resets in 42m')
})

test('不足 1 分钟与已过期时间安全降级', () => {
  const nowMs = 1_700_000_000_000
  assert.equal(formatQuotaResetCountdown(nowMs / 1000 + 30, tZh, nowMs), '还剩不到 1 分钟重置')
  assert.equal(formatQuotaResetCountdown(nowMs / 1000 + 30, tEn, nowMs), 'Resets in <1m')
  assert.equal(formatQuotaResetCountdown(nowMs / 1000, tZh, nowMs), '即将重置')
  assert.equal(formatQuotaResetCountdown(nowMs / 1000 - 120, tEn, nowMs), 'Resetting soon')
})
