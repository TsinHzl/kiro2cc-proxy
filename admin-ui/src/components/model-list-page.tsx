// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { useModels } from '@/hooks/use-credentials'
import type { ModelItem } from '@/types/api'

const PROVIDER_STYLES: Record<string, { text: string; dot: string }> = {
  anthropic: { text: 'text-purple-600 dark:text-purple-400', dot: 'bg-purple-500' },
  openai: { text: 'text-green-600 dark:text-green-400', dot: 'bg-green-500' },
  deepseek: { text: 'text-blue-600 dark:text-blue-400', dot: 'bg-blue-500' },
  minimax: { text: 'text-orange-600 dark:text-orange-400', dot: 'bg-orange-500' },
  glm: { text: 'text-pink-600 dark:text-pink-400', dot: 'bg-pink-500' },
  qwen: { text: 'text-cyan-600 dark:text-cyan-400', dot: 'bg-cyan-500' },
  kiro: { text: 'text-muted-foreground', dot: 'bg-muted-foreground' },
}

const DEFAULT_PROVIDER_STYLE = { text: 'text-foreground', dot: 'bg-muted-foreground' }

function getProviderStyle(ownedBy: string): { text: string; dot: string } {
  return PROVIDER_STYLES[ownedBy] ?? DEFAULT_PROVIDER_STYLE
}

function getProviderRateRanges(models: ModelItem[]): Record<string, { min: number; max: number }> {
  const ranges: Record<string, { min: number; max: number }> = {}
  for (const m of models) {
    if (m.rate_multiplier == null) continue
    const r = ranges[m.owned_by]
    if (!r) {
      ranges[m.owned_by] = { min: m.rate_multiplier, max: m.rate_multiplier }
    } else {
      r.min = Math.min(r.min, m.rate_multiplier)
      r.max = Math.max(r.max, m.rate_multiplier)
    }
  }
  return ranges
}

export function ModelListPage() {
  const { t } = useTranslation()
  const { data, isLoading, refetch } = useModels()
  const models = data?.data ?? []
  const providerRateRanges = useMemo(() => getProviderRateRanges(models), [models])

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <h2 className="text-xl font-semibold">{t('models.pageTitle')}</h2>
        <Button variant="ghost" size="sm" onClick={() => refetch()} disabled={isLoading} className="ml-auto">
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </Button>
      </div>

      <Card>
        <CardContent className="p-0">
          {isLoading ? (
            <div className="py-8 text-center text-muted-foreground text-sm">{t('common.loading')}</div>
          ) : models.length === 0 ? (
            <div className="py-8 text-center text-muted-foreground text-sm">{t('models.emptyNoModels')}</div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b bg-muted/50">
                    <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('models.colModelId')}</th>
                    <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('models.colDisplayName')}</th>
                    <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('models.colProvider')}</th>
                    <th className="text-right px-4 py-2 font-medium text-muted-foreground">Max Tokens</th>
                    <th className="text-right px-4 py-2 font-medium text-muted-foreground">{t('models.colRateMultiplier')}</th>
                  </tr>
                </thead>
                <tbody>
                  {models.map((model) => {
                    const style = getProviderStyle(model.owned_by)
                    const range = providerRateRanges[model.owned_by]
                    const isCheapest = range && range.min !== range.max && model.rate_multiplier === range.min
                    const isMostExpensive = range && range.min !== range.max && model.rate_multiplier === range.max
                    return (
                    <tr key={model.id} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                      <td className={`px-4 py-2 font-mono text-xs ${style.text}`}>{model.id}</td>
                      <td className="px-4 py-2">{model.display_name}</td>
                      <td className="px-4 py-2 text-muted-foreground">
                        <span className="inline-flex items-center gap-1.5">
                          <span className={`inline-block w-2 h-2 rounded-full ${style.dot}`} />
                          {model.owned_by}
                        </span>
                      </td>
                      <td className="px-4 py-2 text-right tabular-nums">{model.max_tokens.toLocaleString()}</td>
                      <td className="px-4 py-2 text-right tabular-nums">
                        {model.rate_multiplier != null ? `${model.rate_multiplier.toFixed(2)}x` : '—'}
                        {isCheapest && (
                          <span className="ml-1.5 text-[10px] px-1 py-0.5 rounded bg-green-500/15 text-green-600 dark:text-green-400">{t('models.cheapestBadge')}</span>
                        )}
                        {isMostExpensive && (
                          <span className="ml-1.5 text-[10px] px-1 py-0.5 rounded bg-red-500/15 text-red-600 dark:text-red-400">{t('models.mostExpensiveBadge')}</span>
                        )}
                      </td>
                    </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
