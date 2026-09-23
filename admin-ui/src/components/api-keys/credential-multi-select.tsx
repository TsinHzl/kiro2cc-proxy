// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 账号绑定多选下拉（自 api-keys-panel.tsx 拆出，纯代码搬移）
import { useTranslation } from 'react-i18next'
import { Check, ChevronDown, Search, X } from 'lucide-react'
import type React from 'react'
import type { BalanceResponse, CredentialStatusItem } from '@/types/api'

interface CredentialMultiSelectProps {
  credentials: CredentialStatusItem[]
  balanceMap: Map<number, BalanceResponse>
  selected: number[]
  onChange: (ids: number[]) => void
  dropdownRef: React.RefObject<HTMLDivElement>
  open: boolean
  onOpenChange: (open: boolean) => void
  searchQuery: string
  onSearchChange: (q: string) => void
}

export function CredentialMultiSelect({
  credentials,
  balanceMap,
  selected,
  onChange,
  dropdownRef,
  open,
  onOpenChange,
  searchQuery,
  onSearchChange,
}: CredentialMultiSelectProps) {
  const { t } = useTranslation()
  const filtered = credentials.filter((c) => {
    if (!searchQuery.trim()) return true
    const q = searchQuery.trim().toLowerCase()
    return (
      String(c.id).includes(q) ||
      (c.email ?? '').toLowerCase().includes(q)
    )
  })

  const toggle = (id: number) => {
    onChange(selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id])
  }

  return (
    <div className="relative mt-2" ref={dropdownRef}>
      {/* 触发器 */}
      <button
        type="button"
        onClick={() => { onOpenChange(!open); onSearchChange('') }}
        className="w-full flex items-center justify-between gap-2 rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm hover:bg-accent/50 transition-colors"
      >
        <div className="flex flex-wrap gap-1 flex-1 min-w-0">
          {selected.length === 0 ? (
            <span className="text-muted-foreground">{t('apiKeys.globalPolicyNoBind')}</span>
          ) : (
            selected.map((id) => {
              const cred = credentials.find((c) => c.id === id)
              return (
                <span
                  key={id}
                  className="inline-flex items-center gap-1 rounded-full bg-violet-100 dark:bg-violet-900/50 text-violet-700 dark:text-violet-300 border border-violet-200 dark:border-violet-700 px-2 py-0.5 text-xs font-medium"
                >
                  {cred?.email ?? `#${id}`}
                  <span
                    role="button"
                    tabIndex={0}
                    className="hover:text-destructive cursor-pointer"
                    onClick={(e) => { e.stopPropagation(); toggle(id) }}
                    onKeyDown={(e) => e.key === 'Enter' && toggle(id)}
                  >
                    <X className="h-3 w-3" />
                  </span>
                </span>
              )
            })
          )}
        </div>
        <ChevronDown className={`h-4 w-4 shrink-0 text-muted-foreground transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>

      {/* 下拉面板 */}
      {open && (
        <div className="absolute z-50 mt-1 w-full rounded-md border bg-popover shadow-md">
          <div className="p-2 border-b">
            <div className="relative">
              <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
              <input
                autoFocus
                type="text"
                placeholder={t('apiKeys.searchCredentialPlaceholder')}
                value={searchQuery}
                onChange={(e) => onSearchChange(e.target.value)}
                className="w-full rounded-sm border-0 bg-transparent pl-7 pr-2 py-1 text-sm outline-none placeholder:text-muted-foreground"
              />
            </div>
          </div>
          <div className="max-h-48 overflow-y-auto py-1">
            {filtered.length === 0 ? (
              <div className="px-3 py-2 text-sm text-muted-foreground">{t('apiKeys.noMatchingAccounts')}</div>
            ) : (
              filtered.map((cred) => {
                const bal = balanceMap.get(cred.id)
                const isSelected = selected.includes(cred.id)
                return (
                  <button
                    key={cred.id}
                    type="button"
                    onClick={() => toggle(cred.id)}
                    className={`w-full flex items-start gap-2 px-3 py-2 text-sm hover:bg-accent transition-colors text-left ${isSelected ? 'bg-violet-50 dark:bg-violet-950/30' : ''}`}
                  >
                    <div className={`mt-0.5 h-4 w-4 shrink-0 rounded border flex items-center justify-center ${isSelected ? 'bg-violet-600 border-violet-600 text-white' : 'border-input'}`}>
                      {isSelected && <Check className="h-3 w-3" />}
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-1.5 flex-wrap">
                        <span className="font-medium">{cred.email ?? t('credentials.accountFallbackName', { id: cred.id })}</span>
                        <span className="text-xs text-muted-foreground">#{cred.id}</span>
                        {cred.disabled && <span className="text-xs text-destructive">{t('apiKeys.statusDisabled')}</span>}
                      </div>
                      {bal ? (
                        <div className="text-xs text-muted-foreground mt-0.5">
                          {t('apiKeys.remainingUsageLabel', { remaining: bal.remaining.toFixed(2), limit: bal.usageLimit.toFixed(2) })}
                          <span className="ml-1">{t('apiKeys.remainingPercentSuffix', { percent: Math.max(0, 100 - bal.usagePercentage).toFixed(1) })}</span>
                        </div>
                      ) : (
                        <div className="text-xs text-muted-foreground mt-0.5">{t('apiKeys.balanceNotLoaded')}</div>
                      )}
                    </div>
                  </button>
                )
              })
            )}
          </div>
        </div>
      )}
    </div>
  )
}

