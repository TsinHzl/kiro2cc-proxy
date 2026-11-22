// Copyright (c) 2026 Harllan He. Licensed under MIT.
// Dashboard 凭据操作条区块（自 dashboard.tsx 拆出，纯代码搬移）
import { useTranslation } from 'react-i18next'
import { RefreshCw, CheckCircle2, Plus } from 'lucide-react'
import { ACTION_BTN, ACTION_BTN_PRIMARY } from '@/components/dashboard/panel-constants'

interface CredentialActionBarProps {
  allCredentials: unknown[]
  handleQueryCurrentPageInfo: () => void
  queryingInfo: boolean
  queryInfoProgress: { current: number; total: number }
  openBatchImport: () => void
  verifying: boolean
  verifyDialogOpen: boolean
  openVerifyDialog: () => void
  verifyProgress: { current: number; total: number }
  openAddDialog: () => void
}

function BatchImportIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="2">
      <path d="M6 3h8l3 3v6" />
      <path d="M3 7h8l3 3v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" />
      <path d="M14 11v6m-2-2 2 2 2-2m-6 3h8" />
    </svg>
  )
}

export function CredentialActionBar({
  allCredentials,
  handleQueryCurrentPageInfo,
  queryingInfo,
  queryInfoProgress,
  openBatchImport,
  verifying,
  verifyDialogOpen,
  openVerifyDialog,
  verifyProgress,
  openAddDialog,
}: CredentialActionBarProps) {
  const { t } = useTranslation()

  return (
    <div className="flex shrink-0 flex-wrap items-center gap-[7px]">
              {allCredentials.length > 0 && (
                <button
                  type="button"
                  onClick={handleQueryCurrentPageInfo}
                  disabled={queryingInfo}
                  aria-label={t('common.refresh')}
                  className={ACTION_BTN}
                >
                  <RefreshCw className={queryingInfo ? 'animate-spin' : ''} />
                  <span className="hidden sm:inline">
                    {queryingInfo
                      ? t('dashboard.queryingProgress', { current: queryInfoProgress.current, total: queryInfoProgress.total })
                      : t('common.refresh')}
                  </span>
                </button>
              )}
              <button
                type="button"
                onClick={openAddDialog}
                aria-label={t('dashboard.addAccount')}
                className={ACTION_BTN}
              >
                <Plus />
                <span className="hidden sm:inline">{t('dashboard.addAccount')}</span>
              </button>
              <button
                type="button"
                onClick={openBatchImport}
                aria-label={t('dashboard.batchImport')}
                className={`${ACTION_BTN_PRIMARY} ml-auto`}
              >
                <BatchImportIcon />
                <span className="hidden sm:inline">{t('dashboard.batchImport')}</span>
              </button>
              {/* 验活进度浮动入口：设计稿无此项，为保留既有能力挂在主按钮左侧 */}
              {verifying && !verifyDialogOpen && (
                <button type="button" onClick={openVerifyDialog} className={ACTION_BTN}>
                  <CheckCircle2 className="animate-spin" />
                  {t('dashboard.verifyingProgress', { current: verifyProgress.current, total: verifyProgress.total })}
                </button>
              )}
            </div>
  )
}
