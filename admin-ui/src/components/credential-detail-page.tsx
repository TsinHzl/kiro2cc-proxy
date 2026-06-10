// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { ArrowLeft, BarChart3, DollarSign, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { useCredentials, useCredentialUsageRecords } from '@/hooks/use-credentials'
import { useIpGeo } from '@/hooks/use-ip-geo'

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

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

function formatCost(cost: number): string {
  return `$${cost.toFixed(4)}`
}

function formatDate(dateStr: string): string {
  return new Date(dateStr).toLocaleString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}

const PAGE_SIZE = 50

export function CredentialDetailPage({ credentialId, onBack }: CredentialDetailPageProps) {
  const [page, setPage] = useState(1)

  const { data: credentialsData } = useCredentials()
  const { data: recordsData, isLoading, refetch } = useCredentialUsageRecords(credentialId, page, PAGE_SIZE)

  const credential = credentialsData?.credentials.find((c) => c.id === credentialId)

  const allRecords = recordsData?.records ?? []
  const pageIps = allRecords.map((r) => r.clientIp).filter((ip): ip is string => !!ip)
  const geoMap = useIpGeo(pageIps)
  const totalRequests = recordsData?.total ?? 0

  // Per-model aggregation from current page (approximate — full aggregation would need all pages)
  const byModel = allRecords.reduce<Record<string, { requests: number; inputTokens: number; outputTokens: number; cost: number }>>((acc, r) => {
    const entry = acc[r.model] ?? { requests: 0, inputTokens: 0, outputTokens: 0, cost: 0 }
    entry.requests += 1
    entry.inputTokens += r.inputTokens
    entry.outputTokens += r.outputTokens
    entry.cost += r.estimatedCost
    acc[r.model] = entry
    return acc
  }, {})

  return (
    <div className="space-y-4">
      {/* 顶部导航 */}
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="sm" onClick={onBack} className="gap-1">
          <ArrowLeft className="h-4 w-4" />
          返回
        </Button>
        {credential && (
          <div className="flex items-center gap-2 flex-wrap">
            <code className="text-xs text-muted-foreground font-mono">#{credential.id}</code>
            <span className="font-semibold">{credential.nickname || credential.email || `账号 #${credential.id}`}</span>
            <Badge variant={credential.disabled ? 'destructive' : 'success'}>
              {credential.disabled ? '已禁用' : '启用'}
            </Badge>
          </div>
        )}
      </div>

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
            <div className="text-2xl font-bold">{totalRequests}</div>
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
              {formatCost(allRecords.reduce((s, r) => s + r.estimatedCost, 0))}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">本页 Credits</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-blue-600 dark:text-blue-400">
              {allRecords.reduce((s, r) => s + (r.creditsUsed ?? r.estimatedCost / 0.72), 0).toFixed(4)}
            </div>
            {(() => {
              const savedTotal = allRecords.reduce((s, r) => s + (r.creditsSaved ?? 0), 0)
              return savedTotal > 0 ? (
                <div className="text-xs text-green-600 dark:text-green-400 mt-0.5">
                  省 {savedTotal.toFixed(4)}
                </div>
              ) : null
            })()}
          </CardContent>
        </Card>
      </div>

      {/* 按模型分组（当前页） */}
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

      {/* 原始日志表格 */}
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
            ) : !recordsData || recordsData.records.length === 0 ? (
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
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">缓存读取</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">费用</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">Kiro Credits</th>
                    </tr>
                  </thead>
                  <tbody>
                    {/* records are returned newest-first by the API */}
                    {recordsData.records.map((record, idx) => {
                      const geo = record.clientIp ? geoMap.get(record.clientIp) : undefined
                      return (
                      <tr key={`${record.createdAt}-${record.model}-${idx}`} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                        <td className="px-4 py-2 text-xs text-muted-foreground whitespace-nowrap">
                          {formatDate(record.createdAt)}
                        </td>
                        <td className="px-4 py-2 text-xs text-muted-foreground whitespace-nowrap">
                          {record.clientIp ? (
                            <span title={record.clientIp}>
                              <span className="font-mono">{geo?.displayIp ?? record.clientIp}</span>
                              {geo && geo.country && <span className="ml-1 text-muted-foreground/60">{geo.country}·{geo.city}</span>}
                            </span>
                          ) : '—'}
                        </td>
                        <td className="px-4 py-2 text-xs text-muted-foreground max-w-[120px] truncate" title={record.credentialLabel}>
                          {record.credentialLabel ?? '—'}
                        </td>
                        <td className={`px-4 py-2 font-mono text-xs ${getModelColor(record.model)}`}>
                          {record.model}
                        </td>
                        <td className="px-4 py-2 text-right tabular-nums">
                          {formatTokens(record.inputTokens)}
                        </td>
                        <td className="px-4 py-2 text-right tabular-nums">
                          {formatTokens(record.outputTokens)}
                        </td>
                        <td className="px-4 py-2 text-right tabular-nums text-green-600 dark:text-green-400">
                          {record.cacheReadInputTokens != null ? formatTokens(record.cacheReadInputTokens) : '—'}
                        </td>
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

        {/* 分页控件 */}
        {recordsData && recordsData.totalPages > 1 && (
          <div className="flex justify-center items-center gap-4 mt-4">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              disabled={page === 1}
            >
              上一页
            </Button>
            <span className="text-sm text-muted-foreground">
              第 {page} / {recordsData.totalPages} 页
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setPage((p) => Math.min(recordsData.totalPages, p + 1))}
              disabled={page === recordsData.totalPages}
            >
              下一页
            </Button>
          </div>
        )}
      </div>
    </div>
  )
}
