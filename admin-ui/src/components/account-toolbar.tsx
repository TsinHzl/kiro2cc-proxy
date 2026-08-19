// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import { SearchBox, Segmented, Toolbar, UpdatedAgo, type SegmentedOption } from '@/components/toolbar'

/** 分段筛选口径：设计稿把 warning + error 合并为「异常」一段（与指标条 abnormalCount 同源） */
export type AccountStatusFilter = 'all' | 'healthy' | 'abnormal' | 'disabled' | 'pending'

const SEGMENTS: { key: AccountStatusFilter; labelKey: string; pipClass?: string }[] = [
  { key: 'all', labelKey: 'credentials.filterAll' },
  { key: 'healthy', labelKey: 'credentials.filterHealthy', pipClass: 'bg-ok' },
  { key: 'abnormal', labelKey: 'credentials.filterAbnormal', pipClass: 'bg-warn' },
  { key: 'disabled', labelKey: 'credentials.filterDisabled', pipClass: 'bg-ink-3' },
  { key: 'pending', labelKey: 'credentials.filterPending', pipClass: 'bg-brand' },
]

export interface AccountToolbarProps {
  searchQuery: string
  onSearchChange: (value: string) => void
  statusFilter: AccountStatusFilter
  onStatusFilterChange: (value: AccountStatusFilter) => void
  /** 各分段计数（基于搜索过滤后的集合，由调用方按 deriveAccountState 计算） */
  counts: Record<AccountStatusFilter, number>
  /** TanStack Query 的 dataUpdatedAt；0 = 尚未取到数据，不渲染「更新于」 */
  dataUpdatedAt: number
}

/** 工具栏（设计稿 .toolbar）：搜索 + 状态分段筛选 + 更新时间 */
export function AccountToolbar({
  searchQuery,
  onSearchChange,
  statusFilter,
  onStatusFilterChange,
  counts,
  dataUpdatedAt,
}: AccountToolbarProps) {
  const { t } = useTranslation()
  const options: SegmentedOption<AccountStatusFilter>[] = SEGMENTS.map((seg) => ({
    key: seg.key,
    label: t(seg.labelKey),
    pipClass: seg.pipClass,
    count: counts[seg.key],
  }))

  return (
    <Toolbar>
      <SearchBox
        value={searchQuery}
        onChange={onSearchChange}
        placeholder={t('credentials.searchPlaceholder')}
        clearLabel={t('credentials.searchClear')}
      />
      <Segmented
        value={statusFilter}
        onChange={onStatusFilterChange}
        groupLabel={t('credentials.filterGroupLabel')}
        options={options}
      />
      <UpdatedAgo dataUpdatedAt={dataUpdatedAt} />
    </Toolbar>
  )
}
