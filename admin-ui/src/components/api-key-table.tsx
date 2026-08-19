// Copyright (c) 2026 Harllan He. Licensed under MIT.
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Loader2, SearchX } from 'lucide-react'
import { DataCheckbox, TH_BASE } from '@/components/table-kit'

/** 列宽（设计稿 #p-keys colgroup）：名称列为 undefined = auto，吃掉剩余宽度 */
const COLUMN_WIDTHS: (number | undefined)[] = [34, undefined, 200, 88, 150, 168, 108, 76, 112, 124]
/** 空状态行需要横跨全部列 */
export const API_KEY_TABLE_COLUMNS = COLUMN_WIDTHS.length

/**
 * 第 2 列起的列定义，顺序与 COLUMN_WIDTHS[1..] 一一对应；首列是全选复选框，无标题。
 * 设计稿「名称」「日配额」两列带 sortable 图标，本页排序已由 .actionbar 的 Segmented
 * 三档承担，不再引入第二套表头排序机制。
 */
const COLUMNS: { labelKey: string; num?: boolean }[] = [
  { labelKey: 'apiKeys.colName' },
  { labelKey: 'apiKeys.colKey' },
  { labelKey: 'apiKeys.colStatus' },
  { labelKey: 'apiKeys.colBound' },
  { labelKey: 'apiKeys.colSpendLimit' },
  { labelKey: 'apiKeys.colRequests', num: true },
  { labelKey: 'apiKeys.colRpm', num: true },
  { labelKey: 'apiKeys.colActivated' },
  { labelKey: 'apiKeys.colOps', num: true },
]

export function ApiKeyTable({
  children,
  rowCount,
  emptyText,
  emptyLoading = false,
  allSelected,
  someSelected,
  onToggleSelectAll,
  isFiltered,
  onClearFilters,
  footer,
}: {
  children: ReactNode
  rowCount: number
  emptyText: string
  emptyLoading?: boolean
  allSelected: boolean
  someSelected: boolean
  onToggleSelectAll: () => void
  isFiltered: boolean
  onClearFilters: () => void
  footer?: ReactNode
}) {
  const { t } = useTranslation()
  return (
    <section className="mt-[13px] flex min-h-0 flex-1 flex-col overflow-hidden rounded-[11px] border border-hairline bg-surface shadow-panel">
      {/* .tscroll：sticky 表头依赖这里的滚动容器 */}
      <div className="max-h-[70vh] min-h-0 flex-1 overflow-y-auto">
        <table className="w-full border-separate border-spacing-0">
          <colgroup>
            {COLUMN_WIDTHS.map((width, index) => (
              <col key={index} style={width ? { width } : undefined} />
            ))}
          </colgroup>
          <thead>
            <tr>
              <th className={TH_BASE}>
                <DataCheckbox
                  checked={allSelected}
                  indeterminate={someSelected}
                  disabled={rowCount === 0}
                  onToggle={onToggleSelectAll}
                  label={t('credentials.selectPage')}
                />
              </th>
              {COLUMNS.map((col) => (
                <th key={col.labelKey} className={`${TH_BASE} ${col.num ? 'text-right' : ''}`}>
                  {t(col.labelKey)}
                </th>
              ))}
            </tr>
          </thead>
          {/* 末行不画分隔线（设计稿 tbody tr:last-child td{border-bottom:0}） */}
          <tbody className="[&_tr:last-child>td]:border-b-0">
            {rowCount === 0 ? (
              <tr>
                <td colSpan={API_KEY_TABLE_COLUMNS} className="px-3 py-14">
                  <div className="flex flex-col items-center gap-2.5 text-ink-3">
                    {emptyLoading ? (
                      <Loader2 className="size-6 animate-spin" aria-hidden="true" />
                    ) : (
                      <SearchX className="size-6" aria-hidden="true" />
                    )}
                    <span className="text-[12.5px]">{emptyText}</span>
                    {isFiltered && (
                      <button
                        type="button"
                        onClick={onClearFilters}
                        className="rounded-[6px] border border-hairline-2 bg-surface px-2.5 py-1 text-[11.5px] font-medium text-ink-2 shadow-hair transition-colors hover:text-ink"
                      >
                        {t('credentials.clearFilters')}
                      </button>
                    )}
                  </div>
                </td>
              </tr>
            ) : (
              children
            )}
          </tbody>
        </table>
      </div>
      {footer}
    </section>
  )
}
