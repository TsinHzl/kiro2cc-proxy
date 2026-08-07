// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ArrowLeft, BarChart3, DollarSign, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { useCredentials, useCredentialUsageRecords, useCredentialTodaySummary } from '@/hooks/use-credentials'
import { useIpGeo } from '@/hooks/use-ip-geo'
import { formatDateTime, formatTokenCount } from '@/lib/locale'

interface CredentialDetailPageProps {
  credentialId: number
  onBack: () => void
}

const MODEL_COLORS: Record<string, string> = {
  opus: 'text-purple-600 dark:text-purple-400',
  sonnet: 'text-blue-600 dark:text-blue-400',
  haiku: 'text-green-600 dark:text-green-400',
}

function getModelColor(model: string): string {
  const lower = model.toLowerCase()
  for (const [key, cls] of Object.entries(MODEL_COLORS)) {
    if (lower.includes(key)) return cls
  }
  return 'text-muted-foreground'
}

function formatCost(cost: number): string {
  return `$${cost.toFixed(4)}`
}

// 镜像后端 src/model/usage.rs::get_k_ref 的模型档位换算率，用于旧记录（无 creditsUsed）的估算回退
function getKRef(model: string): number {
  const m = model.toLowerCase()
  if (m.includes('opus-4-7') || m.includes('opus-4.7') || m.includes('opus-4-8') || m.includes('opus-4.8') || m.includes('opus-5') || m.includes('opus.5')) {
    return 2.36
  }
  if (m.includes('opus-4-5') || m.includes('opus-4.5') || m.includes('opus-4-6') || m.includes('opus-4.6')) {
    return 1.90
  }
  if (m.includes('opus') || m.includes('fable')) {
    return 2.36
  }
  return 1.43
}

const PAGE_SIZE = 50

export function CredentialDetailPage({ credentialId, onBack }: CredentialDetailPageProps) {
  const { t } = useTranslation()
  const [page, setPage] = useState(1)

  const { data: credentialsData } = useCredentials()
  const { data: recordsData, isLoading, refetch } = useCredentialUsageRecords(credentialId, page, PAGE_SIZE)
  const { data: todaySummary } = useCredentialTodaySummary(credentialId)

  const credential = credentialsData?.credentials.find((c) => c.id === credentialId)

  const allRecords = recordsData?.records ?? []
  const pageIps = allRecords.map((r) => r.clientIp).filter((ip): ip is string => !!ip)
  const geoMap = useIpGeo(pageIps)
  const totalRequests = recordsData?.total ?? 0

  // Per-model aggregation from current page (approximate — full aggregation would need all pages)
  const byModel = allRecords.reduce<Record<string, { requests: number; inputTokens: number; outputTokens: number; cost: number; credits: number; creditsSaved: number }>>((acc, r) => {
    const entry = acc[r.model] ?? { requests: 0, inputTokens: 0, outputTokens: 0, cost: 0, credits: 0, creditsSaved: 0 }
    entry.requests += 1
    entry.inputTokens += r.inputTokens
    entry.outputTokens += r.outputTokens
    entry.cost += r.estimatedCost
    entry.credits += r.creditsUsed ?? r.estimatedCost * getKRef(r.model)
    entry.creditsSaved += r.creditsSaved ?? 0
    acc[r.model] = entry
    return acc
  }, {})

  return (
    <div className="space-y-4">
      {/* 顶部导航 */}
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="sm" onClick={onBack} className="gap-1">
          <ArrowLeft className="h-4 w-4" />
          {t('common.back')}
        </Button>
        {credential && (
          <div className="flex items-center gap-2 flex-wrap">
            <code className="text-xs text-muted-foreground font-mono">#{credential.id}</code>
            <span className="font-semibold">{credential.nickname || credential.email || t('credentials.accountFallbackName', { id: credential.id })}</span>
            <Badge variant={credential.disabled ? 'destructive' : 'success'}>
              {credential.disabled ? t('credentials.healthDisabled') : t('credentials.enabled')}
            </Badge>
          </div>
        )}
      </div>

      {/* 汇总卡片 */}
      <div className="grid gap-4 grid-cols-2 md:grid-cols-3 lg:grid-cols-5">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
              <BarChart3 className="h-3.5 w-3.5" />
              {t('common.statTotalRequests')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{totalRequests}</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">{t('credentials.pageTokens')}</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-sm font-bold">
              {t('common.inboundPrefix', { value: formatTokenCount(allRecords.reduce((s, r) => s + r.inputTokens, 0)) })} /
              {t('common.outboundPrefix', { value: formatTokenCount(allRecords.reduce((s, r) => s + r.outputTokens, 0)) })}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
              <DollarSign className="h-3.5 w-3.5" />
              {t('common.pageCost')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-orange-600 dark:text-orange-400">
              {formatCost(allRecords.reduce((s, r) => s + r.estimatedCost, 0))}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">{t('common.pageCredits')}</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-blue-600 dark:text-blue-400">
              {allRecords.reduce((s, r) => s + (r.creditsUsed ?? r.estimatedCost * getKRef(r.model)), 0).toFixed(4)}
            </div>
            {(() => {
              const savedTotal = allRecords.reduce((s, r) => s + (r.creditsSaved ?? 0), 0)
              return savedTotal > 0 ? (
                <div className="text-xs text-green-600 dark:text-green-400 mt-0.5">
                  {t('common.savedPrefix', { amount: savedTotal.toFixed(4) })}
                </div>
              ) : null
            })()}
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">{t('credentials.todayCredits')}</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-cyan-600 dark:text-cyan-400">
              {(todaySummary?.totalCredits ?? 0).toFixed(4)}
            </div>
            {(todaySummary?.totalCreditsSaved ?? 0) > 0 && (
              <div className="text-xs text-green-600 dark:text-green-400 mt-0.5">
                {t('common.savedPrefix', { amount: (todaySummary?.totalCreditsSaved ?? 0).toFixed(4) })}
              </div>
            )}
            {(todaySummary?.totalRequests ?? 0) > 0 && (
              <div className="text-xs text-muted-foreground mt-0.5">
                {t('credentials.todayRequestsCount', { count: todaySummary?.totalRequests })}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* 按模型分组（当前页） */}
      {Object.keys(byModel).length > 0 && (
        <div>
          <h3 className="text-sm font-medium text-muted-foreground mb-2">{t('credentials.groupByModel')}</h3>
          <div className="grid gap-2 grid-cols-1 sm:grid-cols-2 md:grid-cols-3">
            {Object.entries(byModel).map(([model, m]) => (
              <Card key={model}>
                <CardContent className="py-3 px-4">
                  <div className={`text-sm font-medium truncate ${getModelColor(model)}`}>{model}</div>
                  <div className="flex flex-wrap gap-x-3 gap-y-0.5 mt-1 text-xs text-muted-foreground">
                    <span>{t('common.requestCountSuffix', { count: m.requests })}</span>
                    <span>{t('common.inboundPrefix', { value: formatTokenCount(m.inputTokens) })}</span>
                    <span>{t('common.outboundPrefix', { value: formatTokenCount(m.outputTokens) })}</span>
                    <span className="font-medium text-orange-600 dark:text-orange-400">{formatCost(m.cost)}</span>
                    <span className="font-medium text-blue-600 dark:text-blue-400">
                      {m.credits.toFixed(4)} credits
                      {m.creditsSaved > 0 && (
                        <span className="ml-1 text-green-600 dark:text-green-400">({t('common.savedPrefix', { amount: m.creditsSaved.toFixed(4) })})</span>
                      )}
                    </span>
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        </div>
      )}

      {/* 原始日志表格 */}
      <div>
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-sm font-medium text-muted-foreground">
            {t('common.requestLog')}
            {recordsData && <span className="ml-1">{t('common.totalCountSuffix', { count: recordsData.total })}</span>}
          </h3>
          <Button variant="ghost" size="sm" onClick={() => refetch()} disabled={isLoading}>
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
        </div>

        <Card>
          <CardContent className="p-0">
            {isLoading ? (
              <div className="py-8 text-center text-muted-foreground text-sm">{t('common.loading')}</div>
            ) : !recordsData || recordsData.records.length === 0 ? (
              <div className="py-8 text-center text-muted-foreground text-sm">{t('common.noRecords')}</div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/50">
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('common.colTime')}</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('common.colIp')}</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('common.colAccount')}</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('common.colModel')}</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('common.colTokenUsage')}</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">{t('common.colCost')}</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">{t('credentials.colCredits')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {/* records are returned newest-first by the API */}
                    {recordsData.records.map((record, idx) => {
                      const geo = record.clientIp ? geoMap.get(record.clientIp) : undefined
                      return (
                      <tr key={`${record.createdAt}-${record.model}-${idx}`} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                        <td className="px-4 py-2 text-xs text-muted-foreground whitespace-nowrap">
                          {formatDateTime(record.createdAt)}
                        </td>
                        <td className="px-4 py-2 text-xs text-muted-foreground whitespace-nowrap">
                          {record.clientIp ? (
                            <span title={record.clientIp}>
                              <span className="font-mono">{geo?.displayIp ?? record.clientIp}</span>
                              {geo && geo.country && <span className="ml-1 text-muted-foreground dark:text-muted-foreground/60">{geo.country}·{geo.city}</span>}
                            </span>
                          ) : '—'}
                        </td>
                        <td className="px-4 py-2 text-xs text-muted-foreground max-w-[120px] truncate" title={record.credentialLabel}>
                          {record.credentialLabel ?? '—'}
                        </td>
                        <td className={`px-4 py-2 font-mono text-xs ${getModelColor(record.model)}`}>
                          {record.model}
                        </td>
                        <td className="px-4 py-2 text-xs whitespace-nowrap">
                          <div className="space-y-0.5 text-left">
                            <div>{t('common.inputTokensLabel')}<span className="tabular-nums">{formatTokenCount(Math.max(0, record.inputTokens - (record.cacheReadInputTokens ?? 0)))}</span></div>
                            <div>{t('common.outputTokensLabel')}<span className="tabular-nums">{formatTokenCount(record.outputTokens)}</span></div>
                            <div className="text-green-600 dark:text-green-400">{t('common.cacheReadLabel')}<span className="tabular-nums">{formatTokenCount(record.cacheReadInputTokens ?? 0)}</span></div>
                            <div className="font-medium">{t('common.totalInputLabel')}<span className="tabular-nums">{formatTokenCount(record.inputTokens)}</span></div>
                          </div>
                        </td>
                        <td className="px-4 py-2 text-right tabular-nums font-medium text-orange-600 dark:text-orange-400">
                          {formatCost(record.estimatedCost)}
                        </td>
                        <td className="px-4 py-2 text-right tabular-nums font-medium text-blue-600 dark:text-blue-400">
                          {record.creditsUsed != null ? record.creditsUsed.toFixed(4) : (record.estimatedCost * getKRef(record.model)).toFixed(4)}
                          {record.creditsUsed != null && <span className="ml-1 text-xs text-green-500">✓</span>}
                          {record.creditsSaved != null && record.creditsSaved > 0 && (
                            <span className="ml-1 text-xs text-green-600 dark:text-green-400">
                              ({t('common.savedPrefix', { amount: record.creditsSaved.toFixed(4) })})
                            </span>
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

        {/* 分页控件 */}
        {recordsData && recordsData.totalPages > 1 && (
          <div className="flex justify-center items-center gap-4 mt-4">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              disabled={page === 1}
            >
              {t('common.prevPage')}
            </Button>
            <span className="text-sm text-muted-foreground">
              {t('common.pageInfoSimple', { current: page, total: recordsData.totalPages })}
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setPage((p) => Math.min(recordsData.totalPages, p + 1))}
              disabled={page === recordsData.totalPages}
            >
              {t('common.nextPage')}
            </Button>
          </div>
        )}
      </div>
    </div>
  )
}
