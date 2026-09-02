// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { useCredentialModels } from '@/hooks/use-credentials'
import { parseError } from '@/lib/utils'
import {
  FAMILY_AUTO,
  FAMILY_OTHER,
  formatRate,
  formatRateRange,
  groupByFamily,
  rateRange,
} from '@/lib/model-family'

interface ModelsDialogProps {
  credentialId: number | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function ModelsDialog({ credentialId, open, onOpenChange }: ModelsDialogProps) {
  const { t } = useTranslation()
  const { data, isLoading, error } = useCredentialModels(credentialId)

  const familyLabel = (name: string): string => {
    if (name === FAMILY_AUTO) return t('models.familyAuto')
    return name === FAMILY_OTHER ? t('models.familyOther') : name
  }

  const groups = data ? groupByFamily(data.data) : []

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {t('credentials.modelsDialogTitle', { id: credentialId })}
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

        {data && (
          <div className="space-y-3 max-h-[60vh] overflow-y-auto">
            {groups.map((group) => (
              <div key={group.name} className="border border-hairline rounded-md p-3">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-[13px] font-semibold text-ink-1">
                    {familyLabel(group.name)}
                  </span>
                  <span className="text-[11.5px] text-ink-3">
                    {formatRateRange(rateRange(group.models))}
                  </span>
                </div>
                <ul className="space-y-1">
                  {group.models.map((model) => (
                    <li key={model.id} className="flex items-center justify-between text-[12px]">
                      <span className="font-mono text-ink-2 truncate">{model.id}</span>
                      <span className="font-mono tabular-nums text-ink-3 ml-2 shrink-0">
                        {model.rate_multiplier != null ? formatRate(model.rate_multiplier) : '—'}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
