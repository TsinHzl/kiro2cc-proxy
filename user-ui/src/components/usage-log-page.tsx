import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ArrowLeft, RefreshCw, ChevronLeft, ChevronRight } from 'lucide-react'
import { getUsageRecords } from '@/api/user'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'

interface UsageLogPageProps {
  onBack: () => void
}

export function UsageLogPage({ onBack }: UsageLogPageProps) {
  const [page, setPage] = useState(1)
  const pageSize = 50

  const { data, isLoading, isFetching, refetch } = useQuery({
    queryKey: ['usageRecords', page, pageSize],
    queryFn: () => getUsageRecords(page, pageSize),
  })

  const formatCost = (n: number) => `$${n.toFixed(6)}`
  const formatTokens = (n: number) => {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
    return n.toString()
  }
  const formatDate = (iso: string) =>
    new Date(iso).toLocaleString('zh-CN', {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit', second: '2-digit',
    })

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="max-w-4xl mx-auto px-4 py-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Button variant="ghost" size="icon" onClick={onBack} aria-label="返回">
              <ArrowLeft className="h-4 w-4" />
            </Button>
            <h1 className="text-xl font-semibold">请求日志</h1>
            {data && (
              <span className="text-sm text-muted-foreground">共 {data.total} 条</span>
            )}
          </div>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => refetch()}
            disabled={isFetching}
            aria-label="刷新"
          >
            <RefreshCw className={`h-4 w-4 ${isFetching ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </header>

      <main className="max-w-4xl mx-auto px-4 py-6">
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
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-base font-medium">请求记录</CardTitle>
            </CardHeader>
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/50">
                      <th className="text-left px-4 py-3 font-medium text-muted-foreground">时间</th>
                      <th className="text-left px-4 py-3 font-medium text-muted-foreground">模型</th>
                      <th className="text-right px-4 py-3 font-medium text-muted-foreground">输入</th>
                      <th className="text-right px-4 py-3 font-medium text-muted-foreground">输出</th>
                      <th className="text-right px-4 py-3 font-medium text-muted-foreground">费用</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.records.map((r, i) => (
                      <tr key={i} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                        <td className="px-4 py-3 text-muted-foreground whitespace-nowrap">
                          {formatDate(r.createdAt)}
                        </td>
                        <td className="px-4 py-3 font-medium max-w-[200px] truncate" title={r.model}>
                          {r.model}
                        </td>
                        <td className="px-4 py-3 text-right tabular-nums">
                          {formatTokens(r.inputTokens)}
                        </td>
                        <td className="px-4 py-3 text-right tabular-nums">
                          {formatTokens(r.outputTokens)}
                        </td>
                        <td className="px-4 py-3 text-right tabular-nums text-green-600 dark:text-green-400">
                          {formatCost(r.estimatedCost)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              {/* 分页 */}
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
        )}
      </main>
    </div>
  )
}
