// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { toast } from 'sonner'
import {
  LogOut, RefreshCw, Activity, Zap, DollarSign, Clock,
  ArrowUpFromLine, ArrowDownToLine, FileText, Sun, Moon,
  Download,
} from 'lucide-react'
import { getUsage } from '@/api/user'
import { storage } from '@/lib/storage'
import { copyToClipboard } from '@/lib/clipboard'
import { importToCcSwitch, type CcSwitchApp } from '@/lib/ccswitch'
import { ChatGptIcon, ClaudeIcon } from '@/components/brand-icons'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import { UsageLogPage } from '@/components/usage-log-page'
import { PageHead } from '@/components/page-head'
import { useTheme } from '@/hooks/use-theme'

interface DashboardProps {
  onLogout: () => void
}

export function Dashboard({ onLogout }: DashboardProps) {
  const { theme, toggleTheme } = useTheme()
  const [showLog, setShowLog] = useState(false)
  const { data, isLoading, refetch, isRefetching } = useQuery({
    queryKey: ['usage'],
    queryFn: getUsage,
    refetchInterval: 30000,
  })

  const handleLogout = () => {
    storage.removeApiKey()
    onLogout()
  }
  /** 跳转 cc-switch 导入当前 Key；协议未注册时退化为提示 + 复制链接 */
  const handleImportToCcSwitch = (app: CcSwitchApp) => {
    const apiKey = storage.getApiKey()
    if (!apiKey) {
      toast.error('登录状态已失效，请重新登录')
      return
    }
    importToCcSwitch(
      app,
      { apiKey, keyName: data?.name },
      {
        onNotInstalled: (link) => {
          toast.error('未检测到 cc-switch，请先安装并注册 ccswitch:// 协议', {
            action: {
              label: '复制链接',
              onClick: () => {
                void copyToClipboard(link)
              },
            },
          })
        },
      }
    )
  }

  const formatTokens = (n: number) => n.toLocaleString('zh-CN')

  const formatCost = (n: number) => `$${n.toFixed(4)}`

  const formatDate = (iso: string | null) => {
    if (!iso) return null
    const d = new Date(iso)
    return d.toLocaleString('zh-CN', {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit',
    })
  }

  const isCredits = data?.limitUnit === 'credits'
  const usedAmount = isCredits ? data?.totalCredits ?? 0 : data?.totalCost ?? 0

  const getStatusBadge = () => {
    if (!data) return null
    if (data.expiresAt) {
      const expired = new Date(data.expiresAt) < new Date()
      if (expired) return <Badge variant="destructive">已过期</Badge>
    }
    if (data.spendingLimit && usedAmount >= data.spendingLimit) {
      return <Badge variant="destructive">额度已用完</Badge>
    }
    return <Badge variant="success">正常</Badge>
  }

  const spendingPercent = data?.spendingLimit
    ? Math.min((usedAmount / data.spendingLimit) * 100, 100)
    : null

  const formatLimitAmount = (n: number) => (isCredits ? n.toFixed(2) : formatCost(n))

  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background">
        <RefreshCw className="h-8 w-8 animate-spin text-ink-3" />
      </div>
    )
  }

  if (showLog) {
    return <UsageLogPage onBack={() => setShowLog(false)} />
  }

  return (
    <div className="min-h-screen bg-background">
      <div className="w-[90%] mx-auto px-4 py-6">
        <PageHead
          crumb={['额度用量监控']}
          title="额度用量监控"
          note={getStatusBadge() ?? undefined}
          actions={
            <div className="flex items-center gap-2">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setShowLog(true)}
              >
                <FileText className="h-4 w-4 mr-1" />
                查看日志
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => refetch()}
                disabled={isRefetching}
                aria-label="刷新"
              >
                <RefreshCw className={`h-4 w-4 ${isRefetching ? 'animate-spin' : ''}`} />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={toggleTheme}
                aria-label={theme === 'dark' ? '切换到白天模式' : '切换到夜间模式'}
              >
                {theme === 'dark' ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
              </Button>
              <Button variant="ghost" size="sm" onClick={handleLogout}>
                <LogOut className="h-4 w-4 mr-1" />
                退出
              </Button>
            </div>
          }
        />

        <div className="space-y-6">
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
                    <div className="flex items-center gap-2 text-ink-3">
                      <Clock className="h-3.5 w-3.5" />
                      激活时间: {formatDate(data.activatedAt)}
                    </div>
                  )}
                  {data.expiresAt && (
                    <div className="flex items-center gap-2 text-ink-3">
                      <Clock className="h-3.5 w-3.5" />
                      到期时间: {formatDate(data.expiresAt)}
                    </div>
                  )}
                </div>
                {/* 额度进度条 */}
                {data.spendingLimit && spendingPercent !== null && (
                  <div className="space-y-1.5">
                    <div className="flex justify-between text-sm">
                      <span className="text-ink-3">
                        额度使用{isCredits ? '（credits）' : ''}
                      </span>
                      <span>{formatLimitAmount(usedAmount)} / {formatLimitAmount(data.spendingLimit)}</span>
                    </div>
                    <Progress value={spendingPercent} />
                  </div>
                )}
              </CardContent>
            </Card>
          )}

          {/* 一键导入 cc-switch：由 cc-switch 桌面端接管客户端配置文件，无需手写 ~/.codex/.env */}
          {data && (
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-base font-medium flex items-center gap-2">
                  <Download className="h-4 w-4" />
                  一键导入 cc-switch
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="flex flex-wrap gap-2">
                  <Button variant="outline" size="sm" onClick={() => handleImportToCcSwitch('claude')}>
                    <ClaudeIcon className="h-4 w-4 mr-1" />
                    导入 Claude Code
                  </Button>
                  <Button variant="outline" size="sm" onClick={() => handleImportToCcSwitch('codex')}>
                    <ChatGptIcon className="h-4 w-4 mr-1" />
                    导入 Codex
                  </Button>
                </div>
                <p className="text-xs text-ink-3">
                  需先安装 cc-switch 桌面端。导入后只会新增供应商条目，不会自动切换当前供应商。
                </p>
              </CardContent>
            </Card>
          )}
          {/* 用量概览 */}
          {data && (
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <Card>
                <CardContent className="pt-6">
                  <div className="flex items-center gap-2 text-ink-3 text-sm mb-1">
                    <Activity className="h-3.5 w-3.5" />
                    总请求数
                  </div>
                  <div className="text-2xl font-bold">{data.totalRequests.toLocaleString()}</div>
                </CardContent>
              </Card>
              <Card>
                <CardContent className="pt-6">
                  <div className="flex items-center gap-2 text-ink-3 text-sm mb-1">
                    <ArrowUpFromLine className="h-3.5 w-3.5" />
                    输入 Tokens
                  </div>
                  <div className="text-2xl font-bold">{formatTokens(data.totalInputTokens)}</div>
                </CardContent>
              </Card>
              <Card>
                <CardContent className="pt-6">
                  <div className="flex items-center gap-2 text-ink-3 text-sm mb-1">
                    <ArrowDownToLine className="h-3.5 w-3.5" />
                    输出 Tokens
                  </div>
                  <div className="text-2xl font-bold">{formatTokens(data.totalOutputTokens)}</div>
                </CardContent>
              </Card>
              <Card>
                <CardContent className="pt-6">
                  <div className="flex items-center gap-2 text-ink-3 text-sm mb-1">
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
                    <div key={m.model} className="flex items-center justify-between py-2 border-b border-hairline last:border-0">
                      <div>
                        <div className="font-medium text-sm">{m.model}</div>
                        <div className="text-xs text-ink-3 mt-0.5">
                          {m.requests} 次请求
                        </div>
                      </div>
                      <div className="text-right">
                        <div className="text-sm font-medium">{formatCost(m.cost)}</div>
                        <div className="text-xs text-ink-3 mt-0.5">
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
              <CardContent className="py-12 text-center text-ink-3">
                暂无用量数据
              </CardContent>
            </Card>
          )}
        </div>
      </div>
    </div>
  )
}
