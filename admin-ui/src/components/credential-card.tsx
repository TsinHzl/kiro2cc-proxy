// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from 'sonner'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import { RefreshCw, Wallet, Trash2, Loader2, Pencil, FileText } from 'lucide-react'
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
import { Input } from '@/components/ui/input'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type { CredentialStatusItem, BalanceResponse } from '@/types/api'
import { getSubscriptionColor } from '@/lib/utils'
import {
  useSetDisabled,
  useSetPriority,
  useResetFailure,
  useDeleteCredential,
} from '@/hooks/use-credentials'
import { EditCredentialDialog } from './edit-credential-dialog'

interface CredentialCardProps {
  credential: CredentialStatusItem
  onViewBalance: (id: number) => void
  onViewDetail: (id: number) => void
  onViewThrottleLog: (id: number) => void
  onViewFailureLog: (id: number) => void
  selected: boolean
  onToggleSelect: () => void
  balance: BalanceResponse | null
  loadingBalance: boolean
  rpm?: number
}

function formatLastUsed(lastUsedAt: string | null, t: TFunction): string {
  if (!lastUsedAt) return t('credentials.neverUsed')
  const date = new Date(lastUsedAt)
  const now = new Date()
  const diff = now.getTime() - date.getTime()
  if (diff < 0) return t('credentials.justNow')
  const seconds = Math.floor(diff / 1000)
  if (seconds < 60) return t('credentials.secondsAgo', { count: seconds })
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return t('credentials.minutesAgo', { count: minutes })
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return t('credentials.hoursAgo', { count: hours })
  const days = Math.floor(hours / 24)
  return t('credentials.daysAgo', { count: days })
}

type HealthStatus = CredentialStatusItem['healthStatus']

const HEALTH_CONFIG: Record<HealthStatus, { labelKey: string; className: string; dotClass: string }> = {
  healthy:   { labelKey: 'credentials.healthHealthy',   className: 'bg-green-100 text-green-700 border-green-300 dark:bg-neon-green/10 dark:text-neon-green dark:border-neon-green/30',     dotClass: 'bg-green-400' },
  warning:   { labelKey: 'credentials.healthWarning',   className: 'bg-yellow-100 text-yellow-700 border-yellow-300 dark:bg-neon-yellow/10 dark:text-neon-yellow dark:border-neon-yellow/30', dotClass: 'bg-yellow-400' },
  degraded:  { labelKey: 'credentials.healthDegraded',  className: 'bg-orange-100 text-orange-700 border-orange-300 dark:bg-orange-500/10 dark:text-orange-400 dark:border-orange-500/30', dotClass: 'bg-orange-400' },
  unhealthy: { labelKey: 'credentials.healthUnhealthy', className: 'bg-red-100 text-red-700 border-red-300 dark:bg-neon-red/10 dark:text-neon-red dark:border-neon-red/30',          dotClass: 'bg-red-400' },
  disabled:  { labelKey: 'credentials.healthDisabled',  className: 'bg-gray-100 text-gray-700 border-gray-300 dark:bg-gray-500/10 dark:text-gray-400 dark:border-gray-500/30',       dotClass: 'bg-gray-400' },
}

function HealthBadge({ status }: { status: HealthStatus }) {
  const { t } = useTranslation()
  const cfg = HEALTH_CONFIG[status] ?? HEALTH_CONFIG.disabled
  return (
    <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium border ${cfg.className}`}>
      <span className="w-1.5 h-1.5 rounded-full bg-current opacity-80" />
      {t(cfg.labelKey)}
    </span>
  )
}

export function CredentialCard({
  credential,
  onViewBalance,
  onViewDetail,
  onViewThrottleLog,
  onViewFailureLog,
  selected,
  onToggleSelect,
  balance,
  loadingBalance,
  rpm = 0,
}: CredentialCardProps) {
  const { t } = useTranslation()
  const [editingPriority, setEditingPriority] = useState(false)
  const [priorityValue, setPriorityValue] = useState(String(credential.priority))
  const [showDeleteDialog, setShowDeleteDialog] = useState(false)
  const [showEditDialog, setShowEditDialog] = useState(false)

  const setDisabled = useSetDisabled()
  const setPriority = useSetPriority()
  const resetFailure = useResetFailure()
  const deleteCredential = useDeleteCredential()

  const handleToggleDisabled = () => {
    setDisabled.mutate(
      { id: credential.id, disabled: !credential.disabled },
      {
        onSuccess: (res) => toast.success(res.message),
        onError: (err) => toast.error(t('credentials.toastOpFailed', { message: (err as Error).message })),
      }
    )
  }

  const handlePriorityChange = () => {
    const newPriority = parseInt(priorityValue, 10)
    if (isNaN(newPriority) || newPriority < 0) {
      toast.error(t('credentials.toastPriorityInvalid'))
      return
    }
    setPriority.mutate(
      { id: credential.id, priority: newPriority },
      {
        onSuccess: (res) => {
          toast.success(res.message)
          setEditingPriority(false)
        },
        onError: (err) => toast.error(t('credentials.toastOpFailed', { message: (err as Error).message })),
      }
    )
  }

  const handleReset = () => {
    resetFailure.mutate(credential.id, {
      onSuccess: (res) => toast.success(res.message),
      onError: (err) => toast.error(t('credentials.toastOpFailed', { message: (err as Error).message })),
    })
  }

  const handleDelete = () => {
    if (!credential.disabled) {
      toast.error(t('credentials.toastDisableFirst'))
      setShowDeleteDialog(false)
      return
    }
    deleteCredential.mutate(credential.id, {
      onSuccess: (res) => {
        toast.success(res.message)
        setShowDeleteDialog(false)
      },
      onError: (err) => toast.error(t('credentials.toastDeleteFailed', { message: (err as Error).message })),
    })
  }

  return (
    <>
      <Card className={[
        credential.disabled ? 'opacity-60' : '',
      ].filter(Boolean).join(' ')}>
        <CardContent className="py-3 px-3 sm:px-4">
          <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2">
            {/* 左侧信息区 */}
            <div className="flex items-start gap-2.5 min-w-0 flex-1">
              <Checkbox
                checked={selected}
                onCheckedChange={onToggleSelect}
                className="mt-0.5 shrink-0"
              />
              <div className="min-w-0 flex-1">
                {/* 行1：标识 + 状态徽章 */}
                <div className="flex items-center gap-2 flex-wrap">
                  <code className="text-xs text-muted-foreground font-mono">#{String(credential.id).padStart(3, '0')}</code>
                  <span className="font-medium truncate">
                    {credential.nickname || t('credentials.accountFallbackName', { id: credential.id })}
                  </span>
                  <HealthBadge status={credential.healthStatus} />
                  {credential.disabled && <Badge variant="destructive">{t('credentials.healthDisabled')}</Badge>}
                </div>

                {/* 行2：账号 + 最后调用 */}
                <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 mt-1 text-xs text-muted-foreground">
                  {credential.email && <span>{credential.email}</span>}
                  <span>{t('credentials.lastCallLabel', { time: formatLastUsed(credential.lastUsedAt, t) })}</span>
                  {credential.hasProxy && credential.proxyUrl && (
                    <span className="text-blue-500 truncate max-w-[200px]">{t('credentials.proxyLabel', { url: credential.proxyUrl })}</span>
                  )}
                  {credential.hasProfileArn && (
                    <Badge variant="secondary" className="text-xs h-4">Profile ARN</Badge>
                  )}
                </div>

                {/* 行3：数值统计 */}
                <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 mt-1 text-xs">
                  <span className="text-muted-foreground flex items-center gap-1">
                    {t('credentials.priorityLabel')}
                    {editingPriority ? (
                      <span className="inline-flex items-center gap-1">
                        <Input
                          type="number"
                          value={priorityValue}
                          onChange={(e) => setPriorityValue(e.target.value)}
                          className="w-14 h-6 text-xs px-1"
                          min="0"
                          autoFocus
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') handlePriorityChange()
                            if (e.key === 'Escape') { setEditingPriority(false); setPriorityValue(String(credential.priority)) }
                          }}
                        />
                        <button className="text-green-500 hover:text-green-400" onClick={handlePriorityChange} disabled={setPriority.isPending}>✓</button>
                        <button className="text-muted-foreground hover:text-foreground" onClick={() => { setEditingPriority(false); setPriorityValue(String(credential.priority)) }}>✕</button>
                      </span>
                    ) : (
                      <span
                        className="font-medium text-foreground cursor-pointer hover:underline"
                        onClick={() => setEditingPriority(true)}
                      >
                        {credential.priority}
                      </span>
                    )}
                  </span>
                  <span
                    className={`cursor-pointer hover:underline ${credential.failureCount > 0 ? 'text-red-500 font-medium' : 'text-muted-foreground'}`}
                    onClick={() => onViewFailureLog(credential.id)}
                    title={t('credentials.viewFailureLog')}
                  >
                    {t('credentials.failureLabel', { count: credential.failureCount })}
                  </span>
                  <span className="text-muted-foreground">{t('credentials.successLabel', { count: credential.successCount })}</span>
                  <span
                    className={`cursor-pointer hover:underline ${credential.throttleCount > 0 ? 'text-orange-500 font-medium' : 'text-muted-foreground'}`}
                    onClick={() => onViewThrottleLog(credential.id)}
                    title={t('credentials.viewThrottleLog')}
                  >
                    {t('credentials.throttleLabel', { count: credential.throttleCount })}
                  </span>
                  <span className="text-blue-600 dark:text-blue-400 font-medium">RPM {rpm}</span>
                  <span className="text-muted-foreground">
                    {t('credentials.remainingLabel')}
                    {loadingBalance ? (
                      <Loader2 className="inline w-3 h-3 animate-spin ml-1" />
                    ) : balance ? (
                      <span className={`font-medium ${(100 - balance.usagePercentage) >= 50 ? 'text-green-600' : (100 - balance.usagePercentage) >= 20 ? 'text-yellow-500' : 'text-red-500'}`}>
                        {balance.remaining.toFixed(1)}/{balance.usageLimit.toFixed(1)}
                        <span className="ml-1">{t('credentials.remainingPercent', { percent: (100 - balance.usagePercentage).toFixed(0) })}</span>
                      </span>
                    ) : (
                      <span>{t('credentials.unknown')}</span>
                    )}
                  </span>
                  {balance?.subscriptionTitle && (
                    <span className={`text-xs font-medium ${getSubscriptionColor(balance.subscriptionTitle)}`}>
                      {balance.subscriptionTitle}
                    </span>
                  )}
                </div>
              </div>
            </div>

            {/* 右侧操作区 */}
            <div className="flex items-center gap-1 sm:ml-2 self-end sm:self-auto shrink-0">
              <Switch
                checked={!credential.disabled}
                onCheckedChange={handleToggleDisabled}
                disabled={setDisabled.isPending}
              />
              <Button
                variant="ghost" size="sm"
                className="h-8 w-8 p-0"
                onClick={() => onViewBalance(credential.id)}
                title={t('credentials.viewBalance')}
              >
                <Wallet className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost" size="sm"
                className="h-8 w-8 p-0"
                onClick={() => onViewDetail(credential.id)}
                title={t('credentials.viewLog')}
              >
                <FileText className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost" size="sm"
                className="h-8 w-8 p-0"
                onClick={() => setShowEditDialog(true)}
                title={t('common.edit')}
              >
                <Pencil className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost" size="sm"
                className="h-8 w-8 p-0 text-muted-foreground hover:text-foreground"
                onClick={handleReset}
                disabled={resetFailure.isPending || credential.failureCount === 0}
                title={t('credentials.resetFailureCount')}
              >
                <RefreshCw className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost" size="sm"
                className="h-8 w-8 p-0 text-destructive hover:text-destructive"
                onClick={() => setShowDeleteDialog(true)}
                disabled={!credential.disabled}
                title={!credential.disabled ? t('credentials.deleteNeedsDisableTitle') : t('common.delete')}
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <Dialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('credentials.confirmDeleteTitle')}</DialogTitle>
            <DialogDescription>
              {t('credentials.confirmDeleteDesc', { id: credential.id })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowDeleteDialog(false)} disabled={deleteCredential.isPending}>
              {t('common.cancel')}
            </Button>
            <Button variant="destructive" onClick={handleDelete} disabled={deleteCredential.isPending || !credential.disabled}>
              {t('credentials.confirmDeleteButton')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <EditCredentialDialog
        open={showEditDialog}
        onOpenChange={setShowEditDialog}
        credential={credential}
      />
    </>
  )
}
