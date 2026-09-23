// Copyright (c) 2026 Harllan He. Licensed under MIT.
// API Keys 面板共享常量与工具（自 api-keys-panel.tsx 拆出，纯代码搬移）
import type { KeyStatus } from '@/components/api-key-row'

export type KeyStatusFilter = 'all' | KeyStatus
export type SortBy = 'newest' | 'cost-desc' | 'cost-asc'

/** 到期预警阈值：距今 ≤ 7 天 */
export const EXPIRING_SOON_MS = 7 * 24 * 60 * 60 * 1000

/** 每页行数，与账号管理页一致 */
export const ITEMS_PER_PAGE = 50

/** 本地时区 YYYY-MM-DD（与日用量接口的日期口径一致） */
export function formatLocalDate(d: Date): string {
  const month = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${d.getFullYear()}-${month}-${day}`
}
