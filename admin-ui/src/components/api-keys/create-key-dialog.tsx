// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 创建 API Key 对话框（自 api-keys-panel.tsx 拆出，纯代码搬移）
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
import type { BalanceResponse, CredentialStatusItem } from '@/types/api'
import { CredentialMultiSelect } from '@/components/api-keys/credential-multi-select'

const quickDurationOptions = [
  { value: 1, unit: 'hours' as const },
  { value: 3, unit: 'hours' as const },
  { value: 6, unit: 'hours' as const },
  { value: 12, unit: 'hours' as const },
  { value: 1, unit: 'days' as const },
  { value: 3, unit: 'days' as const },
  { value: 7, unit: 'days' as const },
]

interface CreateKeyDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  newName: string
  setNewName: (v: string) => void
  nameConflict: boolean
  newMode: 'date' | 'quota'
  setNewMode: (v: 'date' | 'quota') => void
  newDuration: number | null
  setNewDuration: (v: number | null) => void
  newDurationUnit: 'days' | 'hours'
  setNewDurationUnit: (v: 'days' | 'hours') => void
  newSpendingLimit: number
  setNewSpendingLimit: (v: number) => void
  newLimitUnit: 'usd' | 'credits'
  setNewLimitUnit: (v: 'usd' | 'credits') => void
  newUnlimited: boolean
  setNewUnlimited: (v: boolean) => void
  newBoundCredentialIds: number[]
  setNewBoundCredentialIds: (ids: number[]) => void
  credentials: CredentialStatusItem[] | undefined
  credentialBalanceMap: Map<number, BalanceResponse>
  createCredDropdownRef: React.RefObject<HTMLDivElement>
  createCredDropdownOpen: boolean
  setCreateCredDropdownOpen: (v: boolean) => void
  credSearchQuery: string
  setCredSearchQuery: (q: string) => void
  unitLabel: (unit: 'days' | 'hours') => string
  handleCreate: () => void
  isCreating: boolean
}

export function CreateKeyDialog({
  open,
  onOpenChange,
  newName,
  setNewName,
  nameConflict,
  newMode,
  setNewMode,
  newDuration,
  setNewDuration,
  newDurationUnit,
  setNewDurationUnit,
  newSpendingLimit,
  setNewSpendingLimit,
  newLimitUnit,
  setNewLimitUnit,
  newUnlimited,
  setNewUnlimited,
  newBoundCredentialIds,
  setNewBoundCredentialIds,
  credentials,
  credentialBalanceMap,
  createCredDropdownRef,
  createCredDropdownOpen,
  setCreateCredDropdownOpen,
  credSearchQuery,
  setCredSearchQuery,
  unitLabel,
  handleCreate,
  isCreating,
}: CreateKeyDialogProps) {
  const { t } = useTranslation()

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('apiKeys.createDialogTitle')}</DialogTitle>
          <DialogDescription>{t('apiKeys.createDialogDesc')}</DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div>
            <label className="text-sm font-medium">{t('apiKeys.serialLabel')}</label>
            <Input
              placeholder={t('apiKeys.serialPlaceholder')}
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
            />
            {nameConflict && (
              <p className="text-xs text-destructive mt-1">{t('apiKeys.serialConflict')}</p>
            )}
          </div>
          <div>
            <label className="text-sm font-medium">{t('apiKeys.limitModeLabel')}</label>
            <div className="flex gap-2 mt-2">
              <Button
                type="button"
                size="sm"
                variant={newMode === 'date' ? 'default' : 'outline'}
                onClick={() => setNewMode('date')}
              >
                <Clock className="h-3.5 w-3.5 mr-1.5" />
                {t('apiKeys.byDateButton')}
              </Button>
              <Button
                type="button"
                size="sm"
                variant={newMode === 'quota' ? 'default' : 'outline'}
                onClick={() => setNewMode('quota')}
              >
                <DollarSign className="h-3.5 w-3.5 mr-1.5" />
                {t('apiKeys.byQuotaButton')}
              </Button>
            </div>
          </div>
          {newMode === 'date' ? (
            <div>
              <label className="text-sm font-medium">{t('apiKeys.validityLabel')}</label>
              <div className="flex flex-wrap gap-2 mt-2">
                {quickDurationOptions.map((opt) => (
                  <Button
                    key={`${opt.value}-${opt.unit}`}
                    type="button"
                    size="sm"
                    variant={newDuration === opt.value && newDurationUnit === opt.unit ? 'default' : 'outline'}
                    onClick={() => { setNewDuration(opt.value); setNewDurationUnit(opt.unit) }}
                  >
                    {opt.value} {unitLabel(opt.unit)}
                  </Button>
                ))}
                <Button
                  type="button"
                  size="sm"
                  variant={newDuration === null ? 'default' : 'outline'}
                  onClick={() => setNewDuration(null)}
                >
                  {t('apiKeys.neverExpires')}
                </Button>
              </div>
              {newDuration !== null && (
                <div className="flex items-center gap-2 mt-2">
                  <Input
                    type="number"
                    min={1}
                    value={newDuration}
                    onChange={(e) => setNewDuration(Math.max(1, Number(e.target.value)))}
                    className="w-24"
                  />
                  <div className="flex gap-1">
                    <Button type="button" size="sm" variant={newDurationUnit === 'hours' ? 'default' : 'outline'} onClick={() => setNewDurationUnit('hours')}>{t('apiKeys.hoursUnit')}</Button>
                    <Button type="button" size="sm" variant={newDurationUnit === 'days' ? 'default' : 'outline'} onClick={() => setNewDurationUnit('days')}>{t('apiKeys.daysUnit')}</Button>
                  </div>
                </div>
              )}
              <div className="text-xs text-muted-foreground mt-2">
                <Clock className="h-3 w-3 inline mr-1" />
                {newDuration !== null ? t('apiKeys.activatesAfterFirstUse', { value: newDuration, unit: unitLabel(newDurationUnit) }) : t('apiKeys.neverExpires')}
              </div>
            </div>
          ) : (
            <div>
              <label className="text-sm font-medium">{t('apiKeys.meteringUnitLabel')}</label>
              <div className="flex gap-2 mt-2">
                <Button type="button" size="sm" variant={newLimitUnit === 'usd' ? 'default' : 'outline'} onClick={() => setNewLimitUnit('usd')}>{t('apiKeys.usdEstimate')}</Button>
                <Button type="button" size="sm" variant={newLimitUnit === 'credits' ? 'default' : 'outline'} onClick={() => setNewLimitUnit('credits')}>{t('apiKeys.realCredits')}</Button>
              </div>
              <label className="text-sm font-medium mt-3 block">
                {t('apiKeys.quotaLimitLabel', { unit: newLimitUnit === 'credits' ? 'credits' : t('apiKeys.unitUsd') })}
              </label>
              <div className="flex flex-wrap gap-2 mt-2">
                <Button
                  type="button"
                  size="sm"
                  variant={newUnlimited ? 'default' : 'outline'}
                  onClick={() => setNewUnlimited(true)}
                >
                  {t('apiKeys.unlimitedQuotaButton')}
                </Button>
                {(newLimitUnit === 'credits' ? [1000, 5000, 10000] : [100, 500, 1000]).map((amount) => (
                  <Button
                    key={amount}
                    type="button"
                    size="sm"
                    variant={!newUnlimited && newSpendingLimit === amount ? 'default' : 'outline'}
                    onClick={() => { setNewUnlimited(false); setNewSpendingLimit(amount) }}
                  >
                    {newLimitUnit === 'credits' ? amount : `$${amount}`}
                  </Button>
                ))}
              </div>
              {!newUnlimited && (
                <div className="flex items-center gap-2 mt-2">
                  <span className="text-sm text-muted-foreground">
                    {newLimitUnit === 'credits' ? t('apiKeys.customCredits') : t('apiKeys.customUsd')}
                  </span>
                  <Input
                    type="text"
                    inputMode="numeric"
                    value={newSpendingLimit || ''}
                    onChange={(e) => {
                      const v = e.target.value.replace(/\D/g, '')
                      setNewSpendingLimit(v === '' ? 0 : Number(v))
                    }}
                    onFocus={(e) => e.target.select()}
                    className="w-32"
                  />
                </div>
              )}
              <div className="text-xs text-muted-foreground mt-2">
                <DollarSign className="h-3 w-3 inline mr-1" />
                {newUnlimited
                  ? t('apiKeys.unlimitedQuotaHint')
                  : t('apiKeys.quotaAutoStopHint', { amount: newLimitUnit === 'credits' ? `${newSpendingLimit} credits` : `$${newSpendingLimit}` })}
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
                selected={newBoundCredentialIds}
                onChange={setNewBoundCredentialIds}
                dropdownRef={createCredDropdownRef}
                open={createCredDropdownOpen}
                onOpenChange={setCreateCredDropdownOpen}
                searchQuery={credSearchQuery}
                onSearchChange={setCredSearchQuery}
              />
            </div>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>{t('common.cancel')}</Button>
          <Button onClick={handleCreate} disabled={!newName.trim() || nameConflict || isCreating}>
            {isCreating ? t('apiKeys.creatingButton') : t('apiKeys.createConfirmButton')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
