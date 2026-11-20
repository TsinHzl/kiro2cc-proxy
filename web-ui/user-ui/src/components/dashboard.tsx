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
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Progress, QuotaPercentBadge } from '@/components/ui/progress'
import { UsageLogPage } from '@/components/usage-log-page'
import { PageHead } from '@/components/page-head'
import { Metric, MetricFoot, MetricValue, MetricsBar, Ring } from '@/components/metrics'
import { CELL, PANEL, PANEL_TITLE, ROW_HOVER, TH_BASE } from '@/components/table-kit'
import { useTheme } from '@/hooks/use-theme'

interface DashboardProps {
  onLogout: () => void
}

function getModelTone(model: string): { text: string; pip: string } {
  const lower = model.toLowerCase()
  if (lower.includes('opus')) return { text: 'text-brand', pip: 'bg-brand' }
  if (lower.includes('sonnet')) return { text: 'text-ok', pip: 'bg-ok' }
  if (lower.includes('haiku')) return { text: 'text-warn', pip: 'bg-warn' }
  return { text: 'text-ink-2', pip: 'bg-ink-3' }
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
      if (expired) {
        return (
          <Badge variant="destructive">
            <span className="size-[5px] rounded-full bg-danger" aria-hidden="true" />
            已过期
          </Badge>
        )
      }
    }
    if (data.spendingLimit && usedAmount >= data.spendingLimit) {
      return (
        <Badge variant="destructive">
          <span className="size-[5px] rounded-full bg-danger" aria-hidden="true" />
          额度已用完
        </Badge>
      )
    }
    return (
      <Badge variant="success">
        <span className="size-[5px] rounded-full bg-ok" aria-hidden="true" />
        正常
      </Badge>
    )
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

  const maxModelCost = data?.byModel.length
    ? Math.max(...data.byModel.map((m) => m.cost), 0)
    : 0

  return (
    <div className="min-h-screen bg-background">
      <div className="mx-auto w-[90%] max-w-[1280px] px-4 py-7 max-md:w-full max-md:px-3.5 max-md:py-4">
        <PageHead
          crumb={['Kiro2CCProxy', '额度用量监控']}
          title="额度用量监控"
          note={getStatusBadge() ?? undefined}
          actions={
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => setShowLog(true)}
              >
                <FileText className="h-3.5 w-3.5" />
                查看日志
              </Button>
              <Button
                variant="outline"
                size="icon"
                onClick={() => refetch()}
                disabled={isRefetching}
                aria-label="刷新"
              >
                <RefreshCw className={`h-3.5 w-3.5 ${isRefetching ? 'animate-spin' : ''}`} />
              </Button>
              <Button
                variant="outline"
                size="icon"
                onClick={toggleTheme}
                aria-label={theme === 'dark' ? '切换到白天模式' : '切换到夜间模式'}
              >
                {theme === 'dark' ? <Sun className="h-3.5 w-3.5" /> : <Moon className="h-3.5 w-3.5" />}
              </Button>
              <Button variant="ghost" size="sm" onClick={handleLogout}>
                <LogOut className="h-3.5 w-3.5" />
                退出
              </Button>
            </div>
          }
        />

        <div className="space-y-4">
          {/* Key 信息与额度概览 */}
          {data && (
            <section className="overflow-hidden rounded-[14px] border border-transparent bg-chat-canvas shadow-panel">
              <div className="flex flex-wrap items-center justify-between gap-3 border-b-[0.5px] border-hairline px-5 py-4">
                <div className="flex min-w-0 items-center gap-3">
                  <span className="grid size-9 shrink-0 place-items-center rounded-[10px] bg-brand-soft text-brand ring-[0.5px] ring-inset ring-brand-line">
                    <Zap className="h-4 w-4" />
                  </span>
                  <div className="min-w-0">
                    <div className="truncate text-[15px] font-semibold tracking-[-0.01em] text-ink">
                      {data.name}
                    </div>
                    {(data.activatedAt || data.expiresAt) && (
                      <div className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11.5px] text-ink-3">
                        {data.activatedAt && (
                          <span className="inline-flex items-center gap-1.5 tabular-nums">
                            <Clock className="h-3 w-3" />
                            激活时间: {formatDate(data.activatedAt)}
                          </span>
                        )}
                        {data.expiresAt && (
                          <span className="inline-flex items-center gap-1.5 tabular-nums">
                            <Clock className="h-3 w-3" />
                            到期时间: {formatDate(data.expiresAt)}
                          </span>
                        )}
                      </div>
                    )}
                  </div>
                </div>

                {data.spendingLimit && spendingPercent !== null && (
                  <div className="flex items-center gap-3">
                    <div className="text-right">
                      <div className="text-[11px] font-medium text-ink-3">
                        额度使用{isCredits ? '（credits）' : ''}
                      </div>
                      <div className="mt-0.5 flex items-center justify-end gap-2 font-mono text-[13px] font-semibold tabular-nums text-ink">
                        <span>
                          {formatLimitAmount(usedAmount)}
                          <span className="font-normal text-ink-3"> / {formatLimitAmount(data.spendingLimit)}</span>
                        </span>
                        <QuotaPercentBadge percent={spendingPercent} />
                      </div>
                    </div>
                    <Ring percent={spendingPercent} size={48}>
                      <span className="font-mono text-[11px] font-bold tabular-nums text-ink">
                        {spendingPercent.toFixed(0)}%
                      </span>
                    </Ring>
                  </div>
                )}
              </div>

              {data.spendingLimit && spendingPercent !== null && (
                <div className="bg-[color-mix(in_srgb,var(--surface-2)_35%,transparent)] px-5 py-3">
                  <Progress value={spendingPercent} className="h-2" />
                </div>
              )}
            </section>
          )}

          {/* 一键导入 cc-switch：由 cc-switch 桌面端接管客户端配置文件，无需手写 ~/.codex/.env */}
          {data && (
            <section className="flex flex-wrap items-center justify-between gap-4 rounded-[14px] border border-transparent bg-chat-canvas px-5 py-4 shadow-hair">
              <div className="flex min-w-0 items-start gap-3">
                <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-[8px] bg-surface-3/80 text-ink-2">
                  <Download className="h-4 w-4" />
                </span>
                <div className="min-w-0">
                  <div className="text-[13.5px] font-semibold tracking-[-0.01em] text-ink">
                    一键导入 cc-switch
                  </div>
                  <p className="mt-0.5 text-[11.5px] leading-relaxed text-ink-3">
                    需先安装 cc-switch 桌面端。导入后只会新增供应商条目，不会自动切换当前供应商。
                  </p>
                </div>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <Button variant="outline" size="sm" onClick={() => handleImportToCcSwitch('claude')}>
                  <ClaudeIcon className="h-4 w-4" />
                  导入 Claude Code
                </Button>
                <Button variant="outline" size="sm" onClick={() => handleImportToCcSwitch('codex')}>
                  <ChatGptIcon className="h-4 w-4" />
                  导入 Codex
                </Button>
              </div>
            </section>
          )}

          {/* 用量概览 */}
          {data && (
            <MetricsBar>
              <Metric label="总请求数" icon={<Activity />}>
                <MetricValue value={data.totalRequests.toLocaleString()} />
                <MetricFoot>累计代理请求次数</MetricFoot>
              </Metric>
              <Metric label="输入 Tokens" icon={<ArrowUpFromLine />}>
                <MetricValue value={formatTokens(data.totalInputTokens)} />
                <MetricFoot>累计输入 Token 消耗</MetricFoot>
              </Metric>
              <Metric label="输出 Tokens" icon={<ArrowDownToLine />}>
                <MetricValue value={formatTokens(data.totalOutputTokens)} />
                <MetricFoot>累计输出 Token 消耗</MetricFoot>
              </Metric>
              <Metric label="总费用" icon={<DollarSign />}>
                <MetricValue value={formatCost(data.totalCost)} />
                <MetricFoot>累计估算费用</MetricFoot>
              </Metric>
            </MetricsBar>
          )}

          {/* 按模型分组 */}
          {data && data.byModel.length > 0 && (
            <div className="pt-1">
              <h3 className={PANEL_TITLE}>按模型分组</h3>
              <section className={PANEL}>
                <div className="overflow-x-auto">
                  <table className="w-full border-separate border-spacing-0">
                    <thead>
                      <tr>
                        <th className={TH_BASE}>模型</th>
                        <th className={`${TH_BASE} text-right`}>请求数</th>
                        <th className={`${TH_BASE} text-right`}>输入 / 输出 Tokens</th>
                        <th className={`${TH_BASE} w-[180px]`}>费用占比</th>
                        <th className={`${TH_BASE} text-right`}>费用</th>
                      </tr>
                    </thead>
                    <tbody className="[&_tr:last-child>td]:border-b-0">
                      {data.byModel.map((m) => {
                        const tone = getModelTone(m.model)
                        const barPct = maxModelCost > 0 ? (m.cost / maxModelCost) * 100 : 0
                        return (
                          <tr key={m.model} className={ROW_HOVER}>
                            <td className={CELL}>
                              <span className="inline-flex items-center gap-2 font-mono text-[12.5px] font-medium">
                                <span className={`size-2 shrink-0 rounded-full ${tone.pip}`} aria-hidden="true" />
                                <span className={tone.text}>{m.model}</span>
                              </span>
                            </td>
                            <td className={`${CELL} text-right font-mono text-[12px] tabular-nums text-ink-2`}>
                              {m.requests} 次请求
                            </td>
                            <td className={`${CELL} text-right font-mono text-[12px] tabular-nums text-ink-2`}>
                              {formatTokens(m.inputTokens)} in / {formatTokens(m.outputTokens)} out
                            </td>
                            <td className={CELL}>
                              <span className="block h-[5px] w-full overflow-hidden rounded-full bg-track">
                                <span
                                  className="block h-full rounded-full bg-brand transition-all duration-300"
                                  style={{ width: `${Math.min(100, Math.max(0, barPct))}%` }}
                                />
                              </span>
                            </td>
                            <td className={`${CELL} text-right font-mono text-[12.5px] font-semibold tabular-nums text-ink`}>
                              {formatCost(m.cost)}
                            </td>
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                </div>
              </section>
            </div>
          )}

          {/* 无数据提示 */}
          {data && data.totalRequests === 0 && (
            <Card>
              <CardContent className="py-12 text-center text-[13px] text-ink-3">
                暂无用量数据
              </CardContent>
            </Card>
          )}
        </div>
      </div>
    </div>
  )
}
