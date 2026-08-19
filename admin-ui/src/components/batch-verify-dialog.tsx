// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'

export interface VerifyResult {
  id: number
  status: 'pending' | 'verifying' | 'success' | 'failed'
  usage?: string
  error?: string
}

interface BatchVerifyDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  verifying: boolean
  progress: { current: number; total: number }
  results: Map<number, VerifyResult>
  onCancel: () => void
}

export function BatchVerifyDialog({
  open,
  onOpenChange,
  verifying,
  progress,
  results,
  onCancel,
}: BatchVerifyDialogProps) {
  const { t } = useTranslation()
  const resultsArray = Array.from(results.values())
  const successCount = resultsArray.filter(r => r.status === 'success').length
  const failedCount = resultsArray.filter(r => r.status === 'failed').length

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('credentials.batchVerifyDialogTitle')}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {/* 进度显示 */}
          {verifying && (
            <div className="space-y-2">
              <div className="flex justify-between text-sm">
                <span>{t('credentials.verifyingProgressLabel')}</span>
                <span>{progress.current} / {progress.total}</span>
              </div>
              <div className="h-1 w-full overflow-hidden rounded-[3px] bg-track">
                <div
                  className="h-full rounded-[3px] bg-brand transition-all"
                  style={{ width: `${(progress.current / progress.total) * 100}%` }}
                />
              </div>
            </div>
          )}

          {/* 统计信息 */}
          {results.size > 0 && (
            <div className="flex justify-between text-sm font-medium">
              <span>{t('credentials.verifyResultLabel')}</span>
              <span>
                {t('credentials.statSuccessLabel')}: {successCount} / {t('credentials.statFailedLabel')}: {failedCount}
              </span>
            </div>
          )}

          {/* 结果列表 */}
          {results.size > 0 && (
            <div className="max-h-[400px] space-y-1 overflow-y-auto rounded-[8px] border border-hairline p-2">
              {resultsArray.map((result) => (
                <div
                  key={result.id}
                  className={`rounded-[6px] p-2 text-[12px] ${
                    result.status === 'success'
                      ? 'bg-ok-soft text-ok'
                      : result.status === 'failed'
                      ? 'bg-danger-soft text-danger'
                      : result.status === 'verifying'
                      ? 'bg-brand-soft text-brand'
                      : 'bg-surface-3 text-ink-2'
                  }`}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <span className="font-medium">{t('credentials.accountFallbackName', { id: result.id })}</span>
                      {result.status === 'success' && result.usage && (
                        <Badge variant="secondary" className="text-xs">
                          {result.usage}
                        </Badge>
                      )}
                    </div>
                    <span>
                      {result.status === 'success' && '✓'}
                      {result.status === 'failed' && '✗'}
                      {result.status === 'verifying' && '⏳'}
                      {result.status === 'pending' && '⋯'}
                    </span>
                  </div>
                  {result.error && (
                    <div className="text-xs mt-1 opacity-90">
                      {t('credentials.errorLabel', { error: result.error })}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}

          {/* 提示信息 */}
          {verifying && (
            <p className="text-[11px] leading-[1.55] text-ink-3">
              {t('credentials.verifyBackgroundHint')}
            </p>
          )}
        </div>

        <div className="flex justify-end gap-2">
          {verifying ? (
            <>
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
              >
                {t('credentials.runInBackgroundButton')}
              </Button>
              <Button
                type="button"
                variant="destructive"
                onClick={onCancel}
              >
                {t('credentials.cancelVerifyButton')}
              </Button>
            </>
          ) : (
            <Button
              type="button"
              onClick={() => onOpenChange(false)}
            >
              {t('common.close')}
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
