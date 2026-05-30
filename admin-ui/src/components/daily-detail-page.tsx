// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { ArrowLeft, BarChart3, DollarSign, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { useDailyUsageRecords } from '@/hooks/use-credentials'
import { useIpGeo } from '@/hooks/use-ip-geo'

interface DailyDetailPageProps {
  date: string
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

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

function formatCost(cost: number): string {
  return `$${cost.toFixed(4)}`
}

function formatDateTime(dateStr: string): string {
  return new Date(dateStr).toLocaleString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}

function formatDateLabel(dateStr: string): string {
  return new Date(dateStr + 'T00:00:00Z').toLocaleDateString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit', timeZone: 'UTC',
  })
}

const PAGE_SIZE = 50

export function DailyDetailPage({ date, onBack }: DailyDetailPageProps) {
  const [page, setPage] = useState(1)
  const { data: recordsData, isLoading, refetch } = useDailyUsageRecords(date, page, PAGE_SIZE)

  const records = recordsData?.records ?? []
  const pageIps = records.map((r) => r.clientIp).filter((ip): ip is string => !!ip)
  const geoMap = useIpGeo(pageIps)

  const totalCost = records.reduce((s, r) => s + r.estimatedCost, 0)
  const totalCredits = records.reduce((s, r) => s + (r.creditsUsed ?? r.estimatedCost / 0.72), 0)
  const totalCreditsSaved = records.reduce((s, r) => s + (r.creditsSaved ?? 0), 0)

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="sm" onClick={onBack} className="gap-1">
          <ArrowLeft className="h-4 w-4" />
          返回
        </Button>
        <span className="font-semibold">{formatDateLabel(date)} 用量详情</span>
      </div>

      <div className="grid gap-4 grid-cols-2 sm:grid-cols-3">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
              <BarChart3 className="h-3.5 w-3.5" />
              总请求数
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{recordsData?.total ?? 0}</div>
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
              {formatCost(totalCost)}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">本页 Credits</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-blue-600 dark:text-blue-400">
              {totalCredits.toFixed(4)}
            </div>
            {totalCreditsSaved > 0 && (
              <div className="text-xs text-green-600 dark:text-green-400 mt-0.5">
                省 {totalCreditsSaved.toFixed(4)}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      <div>
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-sm font-medium text-muted-foreground">
            请求日志
            {recordsData && <span className="ml-1">（共 {recordsData.total} 条）</span>}
          </h3>
          <Button variant="ghost" size="sm" onClick={() => refetch()} disabled={isLoading}>
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
        </div>

        <Card>
          <CardContent className="p-0">
            {isLoading ? (
              <div className="py-8 text-center text-muted-foreground text-sm">加载中...</div>
            ) : records.length === 0 ? (
              <div className="py-8 text-center text-muted-foreground text-sm">暂无请求记录</div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/50">
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">时间</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">IP</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">账号</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">模型</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">Input</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">Output</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">费用</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">Credits</th>
                    </tr>
                  </thead>
                  <tbody>
                    {records.map((record, idx) => {
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
                                {geo?.country && <span className="ml-1 text-muted-foreground/60">{geo.country}·{geo.city}</span>}
                              </span>
                            ) : '—'}
                          </td>
                          <td className="px-4 py-2 text-xs text-muted-foreground max-w-[120px] truncate" title={record.credentialLabel}>
                            {record.credentialLabel ?? '—'}
                          </td>
                          <td className={`px-4 py-2 font-mono text-xs ${getModelColor(record.model)}`}>
                            {record.model}
                          </td>
                          <td className="px-4 py-2 text-right tabular-nums">{formatTokens(record.inputTokens)}</td>
                          <td className="px-4 py-2 text-right tabular-nums">{formatTokens(record.outputTokens)}</td>
                          <td className="px-4 py-2 text-right tabular-nums font-medium text-orange-600 dark:text-orange-400">
                            {formatCost(record.estimatedCost)}
                          </td>
                          <td className="px-4 py-2 text-right tabular-nums font-medium text-blue-600 dark:text-blue-400">
                            {record.creditsUsed != null ? record.creditsUsed.toFixed(4) : (record.estimatedCost / 0.72).toFixed(4)}
                            {record.creditsUsed != null && <span className="ml-1 text-xs text-green-500">✓</span>}
                            {record.creditsSaved != null && record.creditsSaved > 0 && (
                              <span className="ml-1 text-xs text-green-600 dark:text-green-400">
                                (省 {record.creditsSaved.toFixed(4)})
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

        {recordsData && recordsData.totalPages > 1 && (
          <div className="flex justify-center items-center gap-4 mt-4">
            <Button variant="outline" size="sm" onClick={() => setPage((p) => Math.max(1, p - 1))} disabled={page === 1}>
              上一页
            </Button>
            <span className="text-sm text-muted-foreground">第 {page} / {recordsData.totalPages} 页</span>
            <Button variant="outline" size="sm" onClick={() => setPage((p) => Math.min(recordsData.totalPages, p + 1))} disabled={page === recordsData.totalPages}>
              下一页
            </Button>
          </div>
        )}
      </div>
    </div>
  )
}
