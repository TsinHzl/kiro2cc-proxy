// Copyright (c) 2026 Harllan He. Licensed under MIT.
import i18n from '@/i18n'

export function localeTag(): string {
  return i18n.language === 'en' ? 'en-US' : 'zh-CN'
}

export function formatTokenCount(n: number): string {
  return n.toLocaleString(localeTag())
}

/** 紧凑 SI 缩写：764573906 → 764.57M，用于表格内避免超长数字撑高行高 */
export function formatCompactToken(n: number): string {
  const trim = (s: string) => s.replace(/(\.\d*?)0+$/, '$1').replace(/\.$/, '')
  if (n >= 1e9) return `${trim((n / 1e9).toFixed(2))}B`
  if (n >= 1e6) return `${trim((n / 1e6).toFixed(2))}M`
  if (n >= 1e3) return `${trim((n / 1e3).toFixed(1))}K`
  return n.toLocaleString(localeTag())
}

export function formatDateTime(dateStr: string): string {
  return new Date(dateStr).toLocaleString(localeTag(), {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}
