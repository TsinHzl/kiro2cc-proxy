// Copyright (c) 2026 Harllan He. Licensed under MIT.
// Dashboard 凭据操作条区块（自 dashboard.tsx 拆出，纯代码搬移）
import { useTranslation } from 'react-i18next'
import { RefreshCw, Info, Trash2, FileUp, Upload, CheckCircle2, Plus } from 'lucide-react'
import { ACTION_BTN, ACTION_BTN_DANGER, ACTION_BTN_PRIMARY, ACTION_VDIV } from '@/components/dashboard/panel-constants'

interface CredentialActionBarProps {
  allCredentials: unknown[]
  disabledCredentialCount: number
  handleRefresh: () => void
  handleQueryCurrentPageInfo: () => void
  queryingInfo: boolean
  queryInfoProgress: { current: number; total: number }
  handleClearAll: () => void
  openKamImport: () => void
  openBatchImport: () => void
  verifying: boolean
  verifyDialogOpen: boolean
  openVerifyDialog: () => void
  verifyProgress: { current: number; total: number }
  openAddDialog: () => void
}

export function CredentialActionBar({
  allCredentials,
  disabledCredentialCount,
  handleRefresh,
  handleQueryCurrentPageInfo,
  queryingInfo,
  queryInfoProgress,
  handleClearAll,
  openKamImport,
  openBatchImport,
  verifying,
  verifyDialogOpen,
  openVerifyDialog,
  verifyProgress,
  openAddDialog,
}: CredentialActionBarProps) {
  const { t } = useTranslation()

  return (
    <div className="flex flex-wrap items-center gap-[7px]">
              <button type="button" onClick={handleRefresh} aria-label={t('dashboard.refreshList')} className={ACTION_BTN}>
                <RefreshCw />
                <span className="hidden sm:inline">{t('dashboard.refreshList')}</span>
              </button>
              {allCredentials.length > 0 && (
                <button
                  type="button"
                  onClick={handleQueryCurrentPageInfo}
                  disabled={queryingInfo}
                  aria-label={t('dashboard.queryInfo')}
                  className={ACTION_BTN}
                >
                  <Info className={queryingInfo ? 'animate-pulse' : ''} />
                  <span className="hidden sm:inline">
                    {queryingInfo
                      ? t('dashboard.queryingProgress', { current: queryInfoProgress.current, total: queryInfoProgress.total })
                      : t('dashboard.queryInfo')}
                  </span>
                </button>
              )}
              {/* 「清除已禁用」两侧的竖线随按钮一起显隐，空列表时不留孤立分隔线 */}
              {allCredentials.length > 0 && (
                <>
                  <span aria-hidden="true" className={ACTION_VDIV} />
                  <button
                    type="button"
                    onClick={handleClearAll}
                    disabled={disabledCredentialCount === 0}
                    title={disabledCredentialCount === 0 ? t('dashboard.noClearableDisabled') : undefined}
                    aria-label={t('dashboard.clearDisabled')}
                    className={ACTION_BTN_DANGER}
                  >
                    <Trash2 />
                    <span className="hidden sm:inline">{t('dashboard.clearDisabled')}</span>
                  </button>
                  <span aria-hidden="true" className={ACTION_VDIV} />
                </>
              )}
              <button
                type="button"
                onClick={openKamImport}
                aria-label={t('dashboard.kamImport')}
                className={ACTION_BTN}
              >
                <FileUp />
                <span className="hidden sm:inline">{t('dashboard.kamImport')}</span>
              </button>
              <button
                type="button"
                onClick={openBatchImport}
                aria-label={t('dashboard.batchImport')}
                className={ACTION_BTN}
              >
                <Upload />
                <span className="hidden sm:inline">{t('dashboard.batchImport')}</span>
              </button>
              {/* 验活进度浮动入口：设计稿无此项，为保留既有能力挂在主按钮左侧 */}
              {verifying && !verifyDialogOpen && (
                <button type="button" onClick={openVerifyDialog} className={ACTION_BTN}>
                  <CheckCircle2 className="animate-spin" />
                  {t('dashboard.verifyingProgress', { current: verifyProgress.current, total: verifyProgress.total })}
                </button>
              )}
              <button
                type="button"
                onClick={openAddDialog}
                aria-label={t('dashboard.addAccount')}
                className={`${ACTION_BTN_PRIMARY} ml-auto`}
              >
                <Plus />
                <span className="hidden sm:inline">{t('dashboard.addAccount')}</span>
              </button>
            </div>
  )
}
