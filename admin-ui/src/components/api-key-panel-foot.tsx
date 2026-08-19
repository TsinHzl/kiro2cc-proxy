// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useId } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import { FOOT_BTN, PAGER_BTN, pageWindow } from '@/components/table-kit'

export interface ApiKeyPanelFootProps {
  /** 跨页累计的已选数量；0 时左区整体不渲染 */
  selectedCount: number
  /** 已选中处于启用态的数量；0 时批量停用不可用 */
  selectedEnabledCount: number
  /** 批量操作进行中，禁用两个批量按钮 */
  busy: boolean
  onBatchDisable: () => void
  onBatchResetUsage: () => void
  onDeselectAll: () => void
  /** 当前筛选结果总数（分页依据），筛选生效时小于 Key 总量 */
  totalCount: number
  isFiltered: boolean
  page: number
  totalPages: number
  itemsPerPage: number
  onPageChange: (page: number) => void
}

export function ApiKeyPanelFoot({
  selectedCount,
  selectedEnabledCount,
  busy,
  onBatchDisable,
  onBatchResetUsage,
  onDeselectAll,
  totalCount,
  isFiltered,
  page,
  totalPages,
  itemsPerPage,
  onPageChange,
}: ApiKeyPanelFootProps) {
  const { t } = useTranslation()
  const disableBlocked = selectedEnabledCount === 0
  const blockedReasonId = useId()

  return (
    <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5 border-t border-hairline bg-surface-2 px-[13px] py-[9px] text-[11.5px] text-ink-3">
      {selectedCount > 0 && (
        <>
          <span className="flex-none">{t('dashboard.selectedCount', { count: selectedCount })}</span>
          {/* 原生 disabled button 不派发 hover 事件，title 挂外层 span 才能读到禁用原因；
              祖先 title 不被屏幕阅读器播报，另挂 aria-describedby 指向 sr-only 文本 */}
          <span className="flex-none" title={disableBlocked ? t('apiKeys.batchDisableBlocked') : undefined}>
            <button
              type="button"
              className={FOOT_BTN}
              onClick={onBatchDisable}
              disabled={busy || disableBlocked}
              aria-describedby={disableBlocked ? blockedReasonId : undefined}
            >
              {t('apiKeys.batchDisable')}
            </button>
            {disableBlocked && (
              <span id={blockedReasonId} className="sr-only">
                {t('apiKeys.batchDisableBlocked')}
              </span>
            )}
          </span>
          <button type="button" className={FOOT_BTN} onClick={onBatchResetUsage} disabled={busy}>
            {t('apiKeys.batchResetUsage')}
          </button>
          <button type="button" className={FOOT_BTN} onClick={onDeselectAll}>
            {t('dashboard.deselectAll')}
          </button>
        </>
      )}

      <div className="ml-auto flex flex-none items-center gap-2">
        <span>{t(isFiltered ? 'apiKeys.footFiltered' : 'apiKeys.footTotal', { count: totalCount })}</span>
        {/* 筛选无结果时表体已给出空状态与清除入口，页码与每页条数在此无意义 */}
        {totalCount > 0 && (
          <>
            <div className="flex gap-[2px]">
              <button
                type="button"
                className={PAGER_BTN}
                onClick={() => onPageChange(page - 1)}
                disabled={page <= 1}
                aria-label={t('common.prevPage')}
              >
                <ChevronLeft className="size-[13px]" strokeWidth={1.8} />
              </button>
              {pageWindow(page, totalPages).map((n) => (
                <button
                  key={n}
                  type="button"
                  onClick={() => onPageChange(n)}
                  aria-current={n === page ? 'page' : undefined}
                  className={`${PAGER_BTN} ${n === page ? 'bg-surface-3 font-semibold text-ink' : ''}`}
                >
                  {n}
                </button>
              ))}
              <button
                type="button"
                className={PAGER_BTN}
                onClick={() => onPageChange(page + 1)}
                disabled={page >= totalPages}
                aria-label={t('common.nextPage')}
              >
                <ChevronRight className="size-[13px]" strokeWidth={1.8} />
              </button>
            </div>
            <span>{t('credentials.footPerPage', { size: itemsPerPage })}</span>
          </>
        )}
      </div>
    </div>
  )
}
