import { useState } from 'react'
import { ArrowLeft, BarChart3, DollarSign, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { useApiKeys, useAllUsage, useKeyUsageRecords } from '@/hooks/use-credentials'
import { useIpGeo } from '@/hooks/use-ip-geo'
import type { ApiKeyItem } from '@/types/api'

interface ApiKeyDetailPageProps {
  keyId: number
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

function getKeyStatus(key: ApiKeyItem): 'active' | 'disabled' | 'expired' | 'pending' {
  if (!key.enabled) return 'disabled'
  if (key.expiresAt && new Date(key.expiresAt) <= new Date()) return 'expired'
  if (key.durationDays != null && !key.activatedAt) return 'pending'
  return 'active'
}

const PAGE_SIZE = 50

export function ApiKeyDetailPage({ keyId, onBack }: ApiKeyDetailPageProps) {
  const [page, setPage] = useState(1)

  const { data: apiKeys } = useApiKeys()
  const { data: allUsage } = useAllUsage()
  const { data: recordsData, isLoading, refetch } = useKeyUsageRecords(keyId, page, PAGE_SIZE)

  const apiKey = apiKeys?.find((k) => k.id === keyId)
  const summary = allUsage?.find((u) => u.apiKeyId === keyId)
  const status = apiKey ? getKeyStatus(apiKey) : null

  const pageIps = (recordsData?.records ?? []).map((r) => r.clientIp).filter((ip): ip is string => !!ip)
  const geoMap = useIpGeo(pageIps)

  return (
    <div className="space-y-4">
      {/* 顶部导航 */}
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="sm" onClick={onBack} className="gap-1">
          <ArrowLeft className="h-4 w-4" />
          返回
        </Button>
        {apiKey && (
          <div className="flex items-center gap-2 flex-wrap">
            <code className="text-xs text-muted-foreground font-mono">
              #{String(apiKey.id).padStart(3, '0')}
            </code>
            <span className="font-semibold">{apiKey.name}</span>
            {status && (
              <Badge variant={status === 'active' ? 'success' : status === 'pending' ? 'secondary' : status === 'expired' ? 'warning' : 'destructive'}>
                {status === 'active' ? '启用' : status === 'pending' ? '待激活' : status === 'expired' ? '已过期' : '已禁用'}
              </Badge>
            )}
          </div>
        )}
      </div>

      {/* 汇总卡片 */}
      <div className="grid gap-4 grid-cols-2 sm:grid-cols-3 lg:grid-cols-5">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
              <BarChart3 className="h-3.5 w-3.5" />
              总请求数
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{summary?.totalRequests ?? 0}</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">Input Tokens</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{formatTokens(summary?.totalInputTokens ?? 0)}</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">Output Tokens</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{formatTokens(summary?.totalOutputTokens ?? 0)}</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
              <DollarSign className="h-3.5 w-3.5" />
              总费用
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-orange-600 dark:text-orange-400">
              {formatCost(summary?.totalCost ?? 0)}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">总 Credits</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-blue-600 dark:text-blue-400">
              {((summary?.totalCost ?? 0) / 0.72).toFixed(4)}
            </div>
            {summary?.totalCreditsSaved != null && summary.totalCreditsSaved > 0 && (
              <div className="text-xs text-green-600 dark:text-green-400 mt-0.5">
                省 {summary.totalCreditsSaved.toFixed(4)}
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* 按模型分组 */}
      {summary && summary.byModel.length > 0 && (
        <div>
          <h3 className="text-sm font-medium text-muted-foreground mb-2">按模型分组</h3>
          <div className="grid gap-2 grid-cols-1 sm:grid-cols-2 md:grid-cols-3">
            {summary.byModel.map((m) => (
              <Card key={m.model}>
                <CardContent className="py-3 px-4">
                  <div className={`text-sm font-medium truncate ${getModelColor(m.model)}`}>{m.model}</div>
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
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">凭据</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">模型</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">Input</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">Output</th>
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
