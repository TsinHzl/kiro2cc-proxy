// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Progress, QuotaPercentBadge } from '@/components/ui/progress'
import { useCredentialBalance } from '@/hooks/use-credentials'
import { parseError, getSubscriptionColor } from '@/lib/utils'
import { localeTag } from '@/lib/locale'

interface BalanceDialogProps {
  credentialId: number | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function BalanceDialog({ credentialId, open, onOpenChange }: BalanceDialogProps) {
  const { t } = useTranslation()
  const { data: balance, isLoading, error } = useCredentialBalance(credentialId)

  const formatDate = (timestamp: number | null) => {
    if (!timestamp) return t('credentials.unknown')
    return new Date(timestamp * 1000).toLocaleString(localeTag())
  }

  const formatNumber = (num: number) => {
    return num.toLocaleString(localeTag(), { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {t('credentials.balanceDialogTitle', { id: credentialId })}
          </DialogTitle>
        </DialogHeader>

        {isLoading && (
          <div className="flex items-center justify-center py-8">
            <div className="h-8 w-8 animate-spin rounded-full border-b-2 border-brand"></div>
          </div>
        )}

        {error && (() => {
          const parsed = parseError(error)
          return (
            <div className="py-6 space-y-3">
              <div className="flex items-center justify-center gap-2 text-danger">
                <svg className="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
                  <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clipRule="evenodd" />
                </svg>
                <span className="font-medium">{parsed.title}</span>
              </div>
              {parsed.detail && (
                <div className="px-4 text-center text-[11.5px] leading-[1.55] text-ink-3">
                  {parsed.detail}
                </div>
              )}
            </div>
          )
        })()}

        {balance && (
          <div className="space-y-4">
            {/* 订阅类型 */}
            <div className="text-center">
              <span className={`text-lg font-semibold ${balance.subscriptionTitle ? getSubscriptionColor(balance.subscriptionTitle) : 'text-ink-3'}`}>
                {balance.subscriptionTitle || t('credentials.unknownSubscription')}
              </span>
            </div>

            {/* 使用进度 */}
            <div className="space-y-2">
              <div className="flex justify-between text-sm">
                <span>{t('credentials.usedLabel', { amount: formatNumber(balance.currentUsage) })}</span>
                <span>{t('credentials.limitLabel', { amount: formatNumber(balance.usageLimit) })}</span>
              </div>
              <Progress value={balance.usagePercentage} />
              <div className="flex items-center justify-center gap-2 text-[11.5px] text-ink-3">
                <span>{t('credentials.usagePercentUsed', { percent: balance.usagePercentage.toFixed(1) })}</span>
                <QuotaPercentBadge percent={balance.usagePercentage} />
              </div>
            </div>

            {/* 详细信息 */}
            <div className="grid grid-cols-2 gap-4 border-t border-hairline pt-4 text-[12px]">
              <div>
                <span className="text-ink-3">{t('credentials.remainingQuotaLabel')}</span>
                <span className="font-medium text-ok">
                  ${formatNumber(balance.remaining)}
                </span>
              </div>
              <div>
                <span className="text-ink-3">{t('credentials.nextResetLabel')}</span>
                <span className="font-medium">
                  {formatDate(balance.nextResetAt)}
                </span>
              </div>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
