// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useId } from 'react'
import { useTranslation } from 'react-i18next'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import { FOOT_BTN, PAGER_BTN, pageWindow } from '@/components/table-kit'

export interface AccountPanelFootProps {
  /** 跨页累计的已选数量；0 时左区整体不渲染 */
  selectedCount: number
  /** 已选中处于禁用态的数量；0 时批量删除不可用 */
  selectedDisabledCount: number
  onBatchVerify: () => void
  onBatchRestore: () => void
  onBatchDelete: () => void
  onDeselectAll: () => void
  /** 当前筛选结果总数（分页依据），筛选生效时小于账号总量 */
  totalCount: number
  /** 搜索或状态筛选生效中：计数文案改为「筛选出 N 个」，避免被读成账号总量 */
  isFiltered: boolean
  page: number
  totalPages: number
  itemsPerPage: number
  onPageChange: (page: number) => void
}

/** 面板脚（设计稿 .panel-foot）：左区批量操作（仅有选中时出现），右区计数 + 页码 + 每页条数 */
export function AccountPanelFoot({
  selectedCount,
  selectedDisabledCount,
  onBatchVerify,
  onBatchRestore,
  onBatchDelete,
  onDeselectAll,
  totalCount,
  isFiltered,
  page,
  totalPages,
  itemsPerPage,
  onPageChange,
}: AccountPanelFootProps) {
  const { t } = useTranslation()
  const deleteBlocked = selectedDisabledCount === 0
  const blockedReasonId = useId()

  return (
    <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1.5 border-t border-hairline bg-surface-2 px-[13px] py-[9px] text-[11.5px] text-ink-3">
      {selectedCount > 0 && (
        <>
          <span className="flex-none">{t('dashboard.selectedCount', { count: selectedCount })}</span>
          <button type="button" className={FOOT_BTN} onClick={onBatchVerify}>
            {t('dashboard.batchVerify')}
          </button>
          <button type="button" className={FOOT_BTN} onClick={onBatchRestore}>
            {t('dashboard.batchRestore')}
          </button>
          {/* 原生 disabled button 不派发 hover 事件，title 挂外层 span 才能读到禁用原因；
              祖先 title 不被屏幕阅读器播报，另挂 aria-describedby 指向 sr-only 文本 */}
          <span className="flex-none" title={deleteBlocked ? t('dashboard.deleteDisabledOnly') : undefined}>
            <button
              type="button"
              className={`${FOOT_BTN} text-danger hover:bg-danger-soft hover:text-danger`}
              onClick={onBatchDelete}
              disabled={deleteBlocked}
              aria-describedby={deleteBlocked ? blockedReasonId : undefined}
            >
              {t('dashboard.batchDelete')}
            </button>
            {deleteBlocked && (
              <span id={blockedReasonId} className="sr-only">
                {t('dashboard.deleteDisabledOnly')}
              </span>
            )}
          </span>
          <button type="button" className={FOOT_BTN} onClick={onDeselectAll}>
            {t('dashboard.deselectAll')}
          </button>
        </>
      )}

      <div className="ml-auto flex flex-none items-center gap-2">
        <span>{t(isFiltered ? 'credentials.footFiltered' : 'credentials.footTotal', { count: totalCount })}</span>
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
