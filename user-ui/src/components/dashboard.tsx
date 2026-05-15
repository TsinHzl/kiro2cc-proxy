import { useQuery } from '@tanstack/react-query'
import {
  LogOut, RefreshCw, Activity, Zap, DollarSign, Clock,
  ArrowUpFromLine, ArrowDownToLine,
} from 'lucide-react'
import { getUsage } from '@/api/user'
import { storage } from '@/lib/storage'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'

interface DashboardProps {
  onLogout: () => void
}

export function Dashboard({ onLogout }: DashboardProps) {
  const { data, isLoading, refetch, isRefetching } = useQuery({
    queryKey: ['usage'],
    queryFn: getUsage,
    refetchInterval: 30000,
  })

  const handleLogout = () => {
    storage.removeApiKey()
    onLogout()
  }

  const formatTokens = (n: number) => {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
    return n.toString()
  }

  const formatCost = (n: number) => `$${n.toFixed(4)}`

  const formatDate = (iso: string | null) => {
    if (!iso) return null
    const d = new Date(iso)
    return d.toLocaleString('zh-CN', {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit',
    })
  }

  const getStatusBadge = () => {
    if (!data) return null
    if (data.expiresAt) {
      const expired = new Date(data.expiresAt) < new Date()
      if (expired) return <Badge variant="destructive">已过期</Badge>
    }
    if (data.spendingLimit && data.totalCost >= data.spendingLimit) {
      return <Badge variant="destructive">额度已用完</Badge>
    }
    return <Badge variant="success">正常</Badge>
  }

  const spendingPercent = data?.spendingLimit
    ? Math.min((data.totalCost / data.spendingLimit) * 100, 100)
    : null

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background">
        <RefreshCw className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-background">
      {/* Header */}
      <header className="border-b">
        <div className="max-w-4xl mx-auto px-4 py-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <h1 className="text-xl font-semibold">额度用量监控</h1>
            {getStatusBadge()}
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              size="icon"
              onClick={() => refetch()}
              disabled={isRefetching}
              aria-label="刷新"
            >
              <RefreshCw className={`h-4 w-4 ${isRefetching ? 'animate-spin' : ''}`} />
            </Button>
            <Button variant="ghost" size="sm" onClick={handleLogout}>
              <LogOut className="h-4 w-4 mr-1" />
              退出
            </Button>
          </div>
        </div>
      </header>

      <main className="max-w-4xl mx-auto px-4 py-6 space-y-6">
        {/* Key 信息 */}
        {data && (
          <Card>
            <CardHeader className="pb-3">
              <CardTitle className="text-base font-medium flex items-center gap-2">
                <Zap className="h-4 w-4" />
                {data.name}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="grid grid-cols-2 gap-4 text-sm">
                {data.activatedAt && (
                  <div className="flex items-center gap-2 text-muted-foreground">
                    <Clock className="h-3.5 w-3.5" />
                    激活时间: {formatDate(data.activatedAt)}
                  </div>
                )}
                {data.expiresAt && (
                  <div className="flex items-center gap-2 text-muted-foreground">
                    <Clock className="h-3.5 w-3.5" />
                    到期时间: {formatDate(data.expiresAt)}
                  </div>
                )}
              </div>
              {/* 额度进度条 */}
              {data.spendingLimit && spendingPercent !== null && (
                <div className="space-y-1.5">
                  <div className="flex justify-between text-sm">
                    <span className="text-muted-foreground">额度使用</span>
                    <span>{formatCost(data.totalCost)} / {formatCost(data.spendingLimit)}</span>
                  </div>
                  <Progress value={spendingPercent} />
                </div>
              )}
            </CardContent>
          </Card>
        )}

        {/* 用量概览 */}
        {data && (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <Card>
              <CardContent className="pt-6">
                <div className="flex items-center gap-2 text-muted-foreground text-sm mb-1">
                  <Activity className="h-3.5 w-3.5" />
                  总请求数
                </div>
                <div className="text-2xl font-bold">{data.totalRequests.toLocaleString()}</div>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="pt-6">
                <div className="flex items-center gap-2 text-muted-foreground text-sm mb-1">
                  <ArrowUpFromLine className="h-3.5 w-3.5" />
                  输入 Tokens
                </div>
                <div className="text-2xl font-bold">{formatTokens(data.totalInputTokens)}</div>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="pt-6">
                <div className="flex items-center gap-2 text-muted-foreground text-sm mb-1">
                  <ArrowDownToLine className="h-3.5 w-3.5" />
                  输出 Tokens
                </div>
                <div className="text-2xl font-bold">{formatTokens(data.totalOutputTokens)}</div>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="pt-6">
                <div className="flex items-center gap-2 text-muted-foreground text-sm mb-1">
                  <DollarSign className="h-3.5 w-3.5" />
                  总费用
                </div>
                <div className="text-2xl font-bold">{formatCost(data.totalCost)}</div>
              </CardContent>
            </Card>
          </div>
        )}

        {/* 按模型分组 */}
        {data && data.byModel.length > 0 && (
          <Card>
            <CardHeader>
              <CardTitle className="text-base font-medium">按模型分组</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="space-y-4">
                {data.byModel.map((m) => (
                  <div key={m.model} className="flex items-center justify-between py-2 border-b last:border-0">
                    <div>
                      <div className="font-medium text-sm">{m.model}</div>
                      <div className="text-xs text-muted-foreground mt-0.5">
                        {m.requests} 次请求
                      </div>
                    </div>
                    <div className="text-right">
                      <div className="text-sm font-medium">{formatCost(m.cost)}</div>
                      <div className="text-xs text-muted-foreground mt-0.5">
                        {formatTokens(m.inputTokens)} in / {formatTokens(m.outputTokens)} out
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </CardContent>
          </Card>
        )}

        {/* 无数据提示 */}
        {data && data.totalRequests === 0 && (
          <Card>
            <CardContent className="py-12 text-center text-muted-foreground">
              暂无用量数据
            </CardContent>
          </Card>
        )}
      </main>
    </div>
  )
}
