// Copyright (c) 2026 Harllan He. Licensed under MIT.
import i18n from '@/i18n'

export function localeTag(): string {
  return i18n.language === 'en' ? 'en-US' : 'zh-CN'
}
