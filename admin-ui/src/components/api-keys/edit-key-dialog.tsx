// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 编辑 API Key 对话框（自 api-keys-panel.tsx 拆出，纯代码搬移）
import { useTranslation } from 'react-i18next'
import { Clock, DollarSign } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type { ApiKeyItem, BalanceResponse, CredentialStatusItem } from '@/types/api'
import { CredentialMultiSelect } from '@/components/api-keys/credential-multi-select'
import { localeTag } from '@/lib/locale'

const quickDurationOptions = [
  { value: 1, unit: 'hours' as const },
  { value: 3, unit: 'hours' as const },
  { value: 6, unit: 'hours' as const },
  { value: 12, unit: 'hours' as const },
  { value: 1, unit: 'days' as const },
  { value: 3, unit: 'days' as const },
  { value: 7, unit: 'days' as const },
]

interface EditKeyDialogProps {
  editingKey: ApiKeyItem | null
  setEditingKey: (key: ApiKeyItem | null) => void
  editName: string
  setEditName: (v: string) => void
  editMode: 'date' | 'quota'
  setEditMode: (v: 'date' | 'quota') => void
  editDuration: number | null | string
  setEditDuration: (v: number | null | string) => void
  /** 用户是否主动改过有效期；未改过时不提交 durationDays/expiresAt，避免旧版固定到期 Key 被静默清除 */
  setEditExpiryDirty: (v: boolean) => void
  editDurationUnit: 'days' | 'hours'
  setEditDurationUnit: (v: 'days' | 'hours') => void
  editBoundCredentialIds: number[]
  setEditBoundCredentialIds: (ids: number[]) => void
  editSpendingLimit: number
  setEditSpendingLimit: (v: number) => void
  editLimitUnit: 'usd' | 'credits'
  setEditLimitUnit: (v: 'usd' | 'credits') => void
  credentials: CredentialStatusItem[] | undefined
  credentialBalanceMap: Map<number, BalanceResponse>
  editCredDropdownRef: React.RefObject<HTMLDivElement>
  editCredDropdownOpen: boolean
  setEditCredDropdownOpen: (v: boolean) => void
  credSearchQuery: string
  setCredSearchQuery: (q: string) => void
  unitLabel: (unit: 'days' | 'hours') => string
  formatDuration: (days: number) => string
  formatDate: (dateStr: string) => string
  getKeyStatus: (key: ApiKeyItem) => 'active' | 'expired' | 'pending' | 'disabled'
  handleUpdate: () => void
}

export function EditKeyDialog({
  editingKey,
  setEditingKey,
  editName,
  setEditName,
  editMode,
  setEditMode,
  editDuration,
  setEditDuration,
  setEditExpiryDirty,
  editDurationUnit,
  setEditDurationUnit,
  editBoundCredentialIds,
  setEditBoundCredentialIds,
  editSpendingLimit,
  setEditSpendingLimit,
  editLimitUnit,
  setEditLimitUnit,
  credentials,
  credentialBalanceMap,
  editCredDropdownRef,
  editCredDropdownOpen,
  setEditCredDropdownOpen,
  credSearchQuery,
  setCredSearchQuery,
  unitLabel,
  formatDuration,
  formatDate,
  getKeyStatus,
  handleUpdate,
}: EditKeyDialogProps) {
  const { t } = useTranslation()

  return (
    <Dialog open={!!editingKey} onOpenChange={(open) => !open && setEditingKey(null)}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('apiKeys.editDialogTitle')}</DialogTitle>
          <DialogDescription>{t('apiKeys.editDialogDesc')}</DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div>
            <label className="text-sm font-medium">{t('apiKeys.remarkNameLabel')}</label>
            <Input
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
            />
          </div>
          <div>
            <label className="text-sm font-medium">{t('apiKeys.limitModeLabel')}</label>
            <div className="flex gap-2 mt-2">
              <Button
                type="button"
                size="sm"
                variant={editMode === 'date' ? 'default' : 'outline'}
                onClick={() => setEditMode('date')}
              >
                <Clock className="h-3.5 w-3.5 mr-1.5" />
                {t('apiKeys.byDateButton')}
              </Button>
              <Button
                type="button"
                size="sm"
                variant={editMode === 'quota' ? 'default' : 'outline'}
                onClick={() => setEditMode('quota')}
              >
                <DollarSign className="h-3.5 w-3.5 mr-1.5" />
                {t('apiKeys.byQuotaButton')}
              </Button>
            </div>
          </div>
          {editMode === 'date' ? (
            <div>
              <label className="text-sm font-medium">{t('apiKeys.renewDurationLabel')}</label>
              {editingKey?.activatedAt ? (
                <div className="text-xs text-muted-foreground mt-1">
                  {t('apiKeys.activatedAtLabel', { date: formatDate(editingKey.activatedAt) })}
                  {editingKey.expiresAt && t('apiKeys.expiresSuffix', { date: formatDate(editingKey.expiresAt) })}
                </div>
              ) : editingKey?.durationDays != null ? (
                <div className="text-xs text-muted-foreground mt-1">
                  {t('apiKeys.pendingWithDuration', { duration: formatDuration(editingKey.durationDays) })}
                </div>
              ) : editingKey?.expiresAt && new Date(editingKey.expiresAt) > new Date() ? (
                <div className="text-xs text-muted-foreground mt-1">
                  {t('apiKeys.currentExpiryLabel', { date: new Date(editingKey.expiresAt).toLocaleString(localeTag(), { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) })}
                </div>
              ) : null}
              <div className="flex flex-wrap gap-2 mt-2">
                {quickDurationOptions.map((opt) => (
                  <Button
                    key={`${opt.value}-${opt.unit}`}
                    type="button"
                    size="sm"
                    variant={editDuration === opt.value && editDurationUnit === opt.unit ? 'default' : 'outline'}
                    onClick={() => { setEditDuration(opt.value); setEditDurationUnit(opt.unit); setEditExpiryDirty(true) }}
                  >
                    {opt.value} {unitLabel(opt.unit)}
                  </Button>
                ))}
                <Button
                  type="button"
                  size="sm"
                  variant={editDuration === null ? 'default' : 'outline'}
                  onClick={() => { setEditDuration(null); setEditExpiryDirty(true) }}
                >
                  {t('apiKeys.neverExpires')}
                </Button>
              </div>
              {editDuration !== null && (
                <div className="flex items-center gap-2 mt-2">
                  <Input
                    type="number"
                    min={1}
                    value={editDuration}
                    onChange={(e) => {
                      const v = e.target.value
                      setEditDuration(v === '' ? '' : Math.max(1, Number(v)))
                      setEditExpiryDirty(true)
                    }}
                    className="w-24"
                  />
                  <div className="flex gap-1">
                    <Button type="button" size="sm" variant={editDurationUnit === 'hours' ? 'default' : 'outline'} onClick={() => setEditDurationUnit('hours')}>{t('apiKeys.hoursUnit')}</Button>
                    <Button type="button" size="sm" variant={editDurationUnit === 'days' ? 'default' : 'outline'} onClick={() => setEditDurationUnit('days')}>{t('apiKeys.daysUnit')}</Button>
                  </div>
                </div>
              )}
              <div className="text-xs text-muted-foreground mt-2">
                <Clock className="h-3 w-3 inline mr-1" />
                {editDuration !== null && editDuration !== ''
                  ? (editingKey && getKeyStatus(editingKey) === 'active'
                      ? t('apiKeys.renewOnCurrentExpiry', { value: editDuration, unit: unitLabel(editDurationUnit) })
                      : t('apiKeys.activatesAfterFirstUse', { value: editDuration, unit: unitLabel(editDurationUnit) }))
                  : t('apiKeys.neverExpires')}
              </div>
            </div>
          ) : (
            <div>
              <label className="text-sm font-medium">{t('apiKeys.meteringUnitLabel')}</label>
              <div className="flex gap-2 mt-2">
                <Button type="button" size="sm" variant={editLimitUnit === 'usd' ? 'default' : 'outline'} onClick={() => setEditLimitUnit('usd')}>{t('apiKeys.usdEstimate')}</Button>
                <Button type="button" size="sm" variant={editLimitUnit === 'credits' ? 'default' : 'outline'} onClick={() => setEditLimitUnit('credits')}>{t('apiKeys.realCredits')}</Button>
              </div>
              <label className="text-sm font-medium mt-3 block">
                {t('apiKeys.quotaLimitLabel', { unit: editLimitUnit === 'credits' ? 'credits' : t('apiKeys.unitUsd') })}
              </label>
              <div className="flex items-center gap-2 mt-2">
                <span className="text-sm text-muted-foreground">{editLimitUnit === 'credits' ? '' : '$'}</span>
                <Input
                  type="number"
                  min={1}
                  step={1}
                  value={editSpendingLimit}
                  onChange={(e) => setEditSpendingLimit(Number(e.target.value))}
                  className="w-32"
                />
              </div>
              <div className="text-xs text-muted-foreground mt-2">
                <DollarSign className="h-3 w-3 inline mr-1" />
                {t('apiKeys.quotaAutoStopHint', { amount: editLimitUnit === 'credits' ? `${editSpendingLimit} credits` : `$${editSpendingLimit}` })}
              </div>
            </div>
          )}
          {credentials && credentials.length > 0 && (
            <div>
              <label className="text-sm font-medium">{t('apiKeys.boundAccountsLabel')}</label>
              <p className="text-xs text-muted-foreground mt-0.5">{t('apiKeys.bindAccountsHint')}</p>
              <CredentialMultiSelect
                credentials={credentials}
                balanceMap={credentialBalanceMap}
                selected={editBoundCredentialIds}
                onChange={setEditBoundCredentialIds}
                dropdownRef={editCredDropdownRef}
                open={editCredDropdownOpen}
                onOpenChange={setEditCredDropdownOpen}
                searchQuery={credSearchQuery}
                onSearchChange={setCredSearchQuery}
              />
            </div>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => setEditingKey(null)}>{t('common.cancel')}</Button>
          <Button onClick={handleUpdate}>{t('common.save')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
