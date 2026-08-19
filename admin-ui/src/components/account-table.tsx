// Copyright (c) 2026 Harllan He. Licensed under MIT.
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { ArrowDown, ArrowUp, ChevronsUpDown, SearchX } from 'lucide-react'
import { DataCheckbox, TH_BASE } from '@/components/table-kit'
import type { AccountSortKey, SortDirection } from '@/lib/account-state'

/** 列宽（设计稿 colgroup）：账号列为 undefined = auto，吃掉剩余宽度 */
const COLUMN_WIDTHS: (number | undefined)[] = [34, 46, undefined, 92, 104, 172, 112, 60, 96, 150]
/** 空状态行需要横跨全部列 */
export const ACCOUNT_TABLE_COLUMNS = COLUMN_WIDTHS.length

/** 第 2 列起的列定义，顺序与 COLUMN_WIDTHS[1..] 一一对应；首列是全选复选框，无标题 */
const COLUMNS: { labelKey: string; num?: boolean; sortKey?: AccountSortKey }[] = [
  { labelKey: 'credentials.colId' },
  { labelKey: 'credentials.colAccount', sortKey: 'account' },
  { labelKey: 'credentials.colState' },
  { labelKey: 'credentials.colPlan' },
  { labelKey: 'credentials.colRemaining', sortKey: 'remaining' },
  { labelKey: 'credentials.colCalls', num: true },
  { labelKey: 'credentials.colRpm', num: true },
  { labelKey: 'credentials.colLastUsed' },
  { labelKey: 'credentials.colOps', num: true },
]

export interface AccountTableProps {
  /** 表体行：AccountRow 列表 */
  children: ReactNode
  /** 当前页行数；0 时改渲染空状态 */
  rowCount: number
  /** 当前页是否已全选 */
  allSelected: boolean
  /** 当前页存在已选但未全选 → 全选框显示半选态 */
  someSelected: boolean
  onToggleSelectAll: () => void
  /** null = 未排序，保持后端返回顺序（表头显示中性双向箭头） */
  sortKey: AccountSortKey | null
  sortDir: SortDirection
  onSort: (key: AccountSortKey) => void
  /** 搜索或状态筛选生效中：空状态额外给出清除入口 */
  isFiltered: boolean
  onClearFilters: () => void
  /** 面板脚（设计稿 .panel-foot）：置于滚动区之外，固定在面板底部不随表体滚动 */
  footer?: ReactNode
}

/** 账号表格外壳（设计稿 .panel + .tscroll）：sticky 表头 + 可排序列 + 空状态，行由 children 提供 */
export function AccountTable({
  children,
  rowCount,
  allSelected,
  someSelected,
  onToggleSelectAll,
  sortKey,
  sortDir,
  onSort,
  isFiltered,
  onClearFilters,
  footer,
}: AccountTableProps) {
  const { t } = useTranslation()

  return (
    <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-[11px] border border-hairline bg-surface shadow-panel">
      {/* .tscroll：sticky 表头依赖这里的滚动容器。整页撑满的 flex 外壳不在本轮范围，
          先用视口比例给出滚动上限（每页 50 行时必然触发内滚） */}
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
              {COLUMNS.map((col) => {
                const colSort = col.sortKey
                const active = colSort !== undefined && colSort === sortKey
                return (
                  <th
                    key={col.labelKey}
                    className={`${TH_BASE} ${col.num ? 'text-right' : ''}`}
                    aria-sort={active ? (sortDir === 'asc' ? 'ascending' : 'descending') : undefined}
                  >
                    {colSort ? (
                      <button
                        type="button"
                        onClick={() => onSort(colSort)}
                        className={`inline-flex items-center gap-1 rounded-[4px] transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand ${
                          active ? 'text-ink' : 'text-ink-2 hover:text-ink'
                        }`}
                      >
                        {t(col.labelKey)}
                        {active ? (
                          sortDir === 'asc' ? (
                            <ArrowUp className="size-3 shrink-0 text-brand" />
                          ) : (
                            <ArrowDown className="size-3 shrink-0 text-brand" />
                          )
                        ) : (
                          <ChevronsUpDown className="size-3 shrink-0 text-ink-3" />
                        )}
                      </button>
                    ) : (
                      t(col.labelKey)
                    )}
                  </th>
                )
              })}
            </tr>
          </thead>
          {/* 末行不画分隔线（设计稿 tbody tr:last-child td{border-bottom:0}），行组件无需自行处理 */}
          <tbody className="[&_tr:last-child>td]:border-b-0">
            {rowCount === 0 ? (
              <tr>
                <td colSpan={ACCOUNT_TABLE_COLUMNS} className="px-3 py-14">
                  <div className="flex flex-col items-center gap-2.5 text-ink-3">
                    <SearchX className="size-6" aria-hidden="true" />
                    <span className="text-[12.5px]">{t('credentials.emptyFiltered')}</span>
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
