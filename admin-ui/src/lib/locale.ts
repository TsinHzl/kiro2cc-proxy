// Copyright (c) 2026 Harllan He. Licensed under MIT.
import i18n from '@/i18n'

export function localeTag(): string {
  return i18n.language === 'en' ? 'en-US' : 'zh-CN'
}

export function formatTokenCount(n: number): string {
  return n.toLocaleString(localeTag())
}

export function formatDateTime(dateStr: string): string {
  return new Date(dateStr).toLocaleString(localeTag(), {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}
