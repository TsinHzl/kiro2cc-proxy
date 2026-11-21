// Copyright (c) 2026 Harllan He. Licensed under MIT.
import type { TFunction } from 'i18next'

const MINUTE_MS = 60_000
const HOUR_MS = 60 * MINUTE_MS
const DAY_MS = 24 * HOUR_MS

/**
 * 格式化额度重置倒计时：
 * - ≥ 1 天：展示剩余天数
 * - < 1 天：展示剩余小时与分钟（不足 1 小时仅展示分钟，不足 1 分钟提示即将重置）
 * - 已过期 / 无效时间戳：安全降级
 */
export function formatQuotaResetCountdown(
  nextResetAtSeconds: number | null | undefined,
  t: TFunction,
  nowMs: number = Date.now(),
): string | null {
  if (typeof nextResetAtSeconds !== 'number' || !Number.isFinite(nextResetAtSeconds) || nextResetAtSeconds <= 0) {
    return null
  }
  const diffMs = nextResetAtSeconds * 1000 - nowMs
  if (diffMs <= 0) {
    return t('credentials.quotaResetSoon')
  }
  if (diffMs >= DAY_MS) {
    const days = Math.floor(diffMs / DAY_MS)
    return t('credentials.quotaResetInDays', { days })
  }
  const totalMinutes = Math.floor(diffMs / MINUTE_MS)
  if (totalMinutes <= 0) {
    return t('credentials.quotaResetLessThanMinute')
  }
  const hours = Math.floor(totalMinutes / 60)
  const minutes = totalMinutes % 60
  if (hours === 0) {
    return t('credentials.quotaResetInMinutes', { minutes })
  }
  return t('credentials.quotaResetInHoursMinutes', { hours, minutes })
}
