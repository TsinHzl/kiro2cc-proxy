// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ArrowLeft, RefreshCw, ChevronLeft, ChevronRight, BarChart3, DollarSign } from 'lucide-react'
import { getUsageRecords } from '@/api/user'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { useIpGeo } from '@/hooks/use-ip-geo'

interface UsageLogPageProps {
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

function formatCost(n: number) {
  return `$${n.toFixed(4)}`
}

function formatTokens(n: number) {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return n.toString()
}

function formatDate(iso: string) {
  return new Date(iso).toLocaleString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}

export function UsageLogPage({ onBack }: UsageLogPageProps) {
  const [page, setPage] = useState(1)
  const pageSize = 50

  const { data, isLoading, isFetching, refetch } = useQuery({
    queryKey: ['usageRecords', page, pageSize],
    queryFn: () => getUsageRecords(page, pageSize),
  })

  const allRecords = data?.records ?? []
  const pageIps = allRecords.map((r) => r.clientIp).filter((ip): ip is string => !!ip)
  const geoMap = useIpGeo(pageIps)

  const byModel = allRecords.reduce<Record<string, { requests: number; inputTokens: number; outputTokens: number; cost: number }>>((acc, r) => {
    const entry = acc[r.model] ?? { requests: 0, inputTokens: 0, outputTokens: 0, cost: 0 }
    entry.requests += 1
    entry.inputTokens += r.inputTokens
    entry.outputTokens += r.outputTokens
    entry.cost += r.estimatedCost
    acc[r.model] = entry
    return acc
  }, {})

  const pageCost = allRecords.reduce((s, r) => s + r.estimatedCost, 0)
  const pageCredits = allRecords.reduce((s, r) => s + (r.creditsUsed ?? r.estimatedCost / 0.72), 0)
  const pageCreditsSaved = allRecords.reduce((s, r) => s + (r.creditsSaved ?? 0), 0)

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="max-w-6xl mx-auto px-4 py-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Button variant="ghost" size="icon" onClick={onBack} aria-label="返回">
              <ArrowLeft className="h-4 w-4" />
            </Button>
            <h1 className="text-xl font-semibold">请求日志</h1>
            {data && (
              <span className="text-sm text-muted-foreground">共 {data.total} 条</span>
            )}
          </div>
          <Button variant="ghost" size="icon" onClick={() => refetch()} disabled={isFetching} aria-label="刷新">
            <RefreshCw className={`h-4 w-4 ${isFetching ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </header>

      <main className="max-w-6xl mx-auto px-4 py-6 space-y-4">
        {isLoading ? (
          <div className="flex justify-center py-20">
            <RefreshCw className="h-8 w-8 animate-spin text-muted-foreground" />
          </div>
        ) : !data || data.total === 0 ? (
          <Card>
            <CardContent className="py-12 text-center text-muted-foreground">
              暂无请求日志
            </CardContent>
          </Card>
        ) : (
          <>
            {/* 汇总卡片 */}
            <div className="grid gap-4 grid-cols-2 md:grid-cols-4">
              <Card>
                <CardHeader className="pb-2">
                  <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
                    <BarChart3 className="h-3.5 w-3.5" />
                    总请求数
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="text-2xl font-bold">{data.total}</div>
                </CardContent>
              </Card>
              <Card>
                <CardHeader className="pb-2">
                  <CardTitle className="text-sm font-medium text-muted-foreground">本页 Tokens</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="text-sm font-bold">
                    入 {formatTokens(allRecords.reduce((s, r) => s + r.inputTokens, 0))} /
                    出 {formatTokens(allRecords.reduce((s, r) => s + r.outputTokens, 0))}
                  </div>
                </CardContent>
              </Card>
              <Card>
                <CardHeader className="pb-2">
                  <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
                    <DollarSign className="h-3.5 w-3.5" />
                    本页费用
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="text-2xl font-bold text-orange-600 dark:text-orange-400">
                    {formatCost(pageCost)}
                  </div>
                </CardContent>
              </Card>
              <Card>
                <CardHeader className="pb-2">
                  <CardTitle className="text-sm font-medium text-muted-foreground">本页 Credits</CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="text-2xl font-bold text-blue-600 dark:text-blue-400">
                    {pageCredits.toFixed(4)}
                  </div>
                  {pageCreditsSaved > 0 && (
                    <div className="text-xs text-green-600 dark:text-green-400 mt-0.5">
                      省 {pageCreditsSaved.toFixed(4)}
                    </div>
                  )}
                </CardContent>
              </Card>
            </div>

            {/* 按模型分组 */}
            {Object.keys(byModel).length > 0 && (
              <div>
                <h3 className="text-sm font-medium text-muted-foreground mb-2">按模型分组（当前页）</h3>
                <div className="grid gap-2 grid-cols-1 sm:grid-cols-2 md:grid-cols-3">
                  {Object.entries(byModel).map(([model, m]) => (
                    <Card key={model}>
                      <CardContent className="py-3 px-4">
                        <div className={`text-sm font-medium truncate ${getModelColor(model)}`}>{model}</div>
                        <div className="flex flex-wrap gap-x-3 gap-y-0.5 mt-1 text-xs text-muted-foreground">
                          <span>{m.requests} 次</span>
                          <span>入 {formatTokens(m.inputTokens)}</span>
                          <span>出 {formatTokens(m.outputTokens)}</span>
                          <span className="font-medium text-orange-600 dark:text-orange-400">{formatCost(m.cost)}</span>
                        </div>
                      </CardContent>
                    </Card>
                  ))}
                </div>
              </div>
            )}

            {/* 日志表格 */}
            <Card>
              <CardContent className="p-0">
                <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="border-b bg-muted/50">
                        <th className="text-left px-4 py-3 font-medium text-muted-foreground">时间</th>
                        <th className="text-left px-4 py-3 font-medium text-muted-foreground">IP</th>
                        <th className="text-left px-4 py-3 font-medium text-muted-foreground">模型</th>
                        <th className="text-right px-4 py-3 font-medium text-muted-foreground">Input</th>
                        <th className="text-right px-4 py-3 font-medium text-muted-foreground">Output</th>
                        <th className="text-right px-4 py-3 font-medium text-muted-foreground">缓存读取</th>
                        <th className="text-right px-4 py-3 font-medium text-muted-foreground">费用</th>
                        <th className="text-right px-4 py-3 font-medium text-muted-foreground">Kiro Credits</th>
                      </tr>
                    </thead>
                    <tbody>
                      {allRecords.map((r, i) => {
                        const geo = r.clientIp ? geoMap.get(r.clientIp) : undefined
                        return (
                          <tr key={i} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                            <td className="px-4 py-3 text-xs text-muted-foreground whitespace-nowrap">
                              {formatDate(r.createdAt)}
                            </td>
                            <td className="px-4 py-3 text-xs text-muted-foreground whitespace-nowrap">
                              {r.clientIp ? (
                                <span title={r.clientIp}>
                                  <span className="font-mono">{geo?.displayIp ?? r.clientIp}</span>
                                  {geo && geo.country && (
                                    <span className="ml-1 text-muted-foreground/60">{geo.country}·{geo.city}</span>
                                  )}
                                </span>
                              ) : '—'}
                            </td>
                            <td className={`px-4 py-3 font-mono text-xs max-w-[200px] truncate ${getModelColor(r.model)}`} title={r.model}>
                              {r.model}
                            </td>
                            <td className="px-4 py-3 text-right tabular-nums">
                              {formatTokens(r.inputTokens)}
                            </td>
                            <td className="px-4 py-3 text-right tabular-nums">
                              {formatTokens(r.outputTokens)}
                            </td>
                            <td className="px-4 py-3 text-right tabular-nums text-green-600 dark:text-green-400">
                              {r.cacheReadInputTokens != null ? formatTokens(r.cacheReadInputTokens) : '—'}
                            </td>
                            <td className="px-4 py-3 text-right tabular-nums font-medium text-orange-600 dark:text-orange-400">
                              {formatCost(r.estimatedCost)}
                            </td>
                            <td className="px-4 py-3 text-right tabular-nums font-medium text-blue-600 dark:text-blue-400">
                              {r.creditsUsed != null ? r.creditsUsed.toFixed(4) : (r.estimatedCost / 0.72).toFixed(4)}
                              {r.creditsUsed != null && <span className="ml-1 text-xs text-green-500">✓</span>}
                              {r.creditsSaved != null && r.creditsSaved > 0 && (
                                <span className="ml-1 text-xs text-green-600 dark:text-green-400">
                                  (省 {r.creditsSaved.toFixed(4)})
                                </span>
                              )}
                            </td>
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                </div>

                {data.totalPages > 1 && (
                  <div className="flex items-center justify-between px-4 py-3 border-t">
                    <span className="text-sm text-muted-foreground">
                      第 {data.page} / {data.totalPages} 页
                    </span>
                    <div className="flex items-center gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => setPage((p) => Math.max(1, p - 1))}
                        disabled={data.page <= 1 || isFetching}
                      >
                        <ChevronLeft className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => setPage((p) => Math.min(data.totalPages, p + 1))}
                        disabled={data.page >= data.totalPages || isFetching}
                      >
                        <ChevronRight className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                )}
              </CardContent>
            </Card>
          </>
        )}
      </main>
    </div>
  )
}
