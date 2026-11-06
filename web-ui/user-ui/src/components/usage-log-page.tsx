// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Brain, RefreshCw, BarChart3, DollarSign } from 'lucide-react'
import { getUsageRecords } from '@/api/user'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { useIpGeo } from '@/hooks/use-ip-geo'
import { PageHead } from '@/components/page-head'
import { CELL, PANEL, PANEL_FOOT, PANEL_TITLE, Pager, TH_BASE } from '@/components/table-kit'

interface UsageLogPageProps {
  onBack: () => void
}

/** 模型色调：按名称映射到令牌色（brand/ok/warn/danger/ink-2 兜底），替代原硬编码 orange/blue/green */
const MODEL_TONE: Record<string, string> = {
  opus: 'text-brand',
  sonnet: 'text-ok',
  haiku: 'text-warn',
}

function getModelTone(model: string): string {
  const lower = model.toLowerCase()
  for (const [key, cls] of Object.entries(MODEL_TONE)) {
    if (lower.includes(key)) return cls
  }
  return 'text-ink-2'
}

function formatCost(n: number) {
  return `$${n.toFixed(4)}`
}

/** 流式/非流徽章基类：与 admin-ui BADGE_CLS 同构（user-ui 独立 app，不跨仓共享） */
const BADGE_CLS = 'rounded-[4px] px-1 py-px text-[9.5px] font-semibold leading-tight'

/** UA 短标识：取首个空格前段（如 claude-cli/2.1.5），完整原文走 title 悬停展示 */
function uaShort(ua: string): string {
  return ua.split(/\s+/)[0] || ua
}

function formatTokens(n: number) {
  return n.toLocaleString('zh-CN')
}

function formatDate(iso: string) {
  return new Date(iso).toLocaleString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}

export function UsageLogPage({ onBack }: UsageLogPageProps) {
  const [page, setPage] = useState(1)
  const pageSize = 100

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

  return (
    <div className="min-h-screen bg-background">
      <div className="w-[90%] max-md:w-full mx-auto px-4 max-md:px-3.5 py-6 max-md:py-4">
        <PageHead
          crumb={['请求日志']}
          title="请求日志"
          note={data ? `共 ${data.total} 条` : undefined}
          onBack={onBack}
          actions={
            <Button
              variant="ghost"
              size="icon"
              onClick={() => refetch()}
              disabled={isFetching}
              aria-label="刷新"
            >
              <RefreshCw className={`h-4 w-4 ${isFetching ? 'animate-spin' : ''}`} />
            </Button>
          }
        />

        <div className="space-y-4">
          {isLoading ? (
            <div className="flex justify-center py-20">
              <RefreshCw className="h-8 w-8 animate-spin text-ink-3" />
            </div>
          ) : !data || data.total === 0 ? (
            <Card>
              <CardContent className="py-12 text-center text-ink-3">
                暂无请求日志
              </CardContent>
            </Card>
          ) : (
            <>
              {/* 汇总卡片 */}
              <div className="grid gap-4 grid-cols-2 md:grid-cols-4">
                <Card>
                  <CardHeader className="pb-2">
                    <CardTitle className="text-sm font-medium text-ink-3 flex items-center gap-1">
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
                    <CardTitle className="text-sm font-medium text-ink-3">本页 Tokens</CardTitle>
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
                    <CardTitle className="text-sm font-medium text-ink-3">本页 Credits</CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="text-2xl font-bold text-ok">
                      {pageCredits.toFixed(4)}
                    </div>
                  </CardContent>
                </Card>
                <Card>
                  <CardHeader className="pb-2">
                    <CardTitle className="text-sm font-medium text-ink-3 flex items-center gap-1">
                      <DollarSign className="h-3.5 w-3.5" />
                      本页费用
                    </CardTitle>
                  </CardHeader>
                  <CardContent>
                    <div className="text-2xl font-bold text-warn">
                      {formatCost(pageCost)}
                    </div>
                  </CardContent>
                </Card>
              </div>

              {/* 按模型分组 */}
              {Object.keys(byModel).length > 0 && (
                <div>
                  <h3 className={PANEL_TITLE}>按模型分组（当前页）</h3>
                  <div className="grid gap-2 grid-cols-1 sm:grid-cols-2 md:grid-cols-3">
                    {Object.entries(byModel).map(([model, m]) => (
                      <Card key={model}>
                        <CardContent className="py-3 px-4">
                          <div className={`text-sm font-medium truncate ${getModelTone(model)}`}>{model}</div>
                          <div className="flex flex-wrap gap-x-3 gap-y-0.5 mt-1 text-xs text-ink-3">
                            <span>{m.requests} 次</span>
                            <span>入 {formatTokens(m.inputTokens)}</span>
                            <span>出 {formatTokens(m.outputTokens)}</span>
                            <span className="font-medium text-warn">{formatCost(m.cost)}</span>
                          </div>
                        </CardContent>
                      </Card>
                    ))}
                  </div>
                </div>
              )}

              {/* 日志表格（.panel + table-kit） */}
              <div>
                <h3 className={PANEL_TITLE}>请求明细</h3>
                <section className={PANEL}>
                  <div className="max-h-[70vh] overflow-y-auto">
                    <table className="w-full border-separate border-spacing-0">
                      <thead>
                        <tr>
                          <th className={TH_BASE}>时间</th>
                          <th className={TH_BASE}>IP</th>
                          <th className={TH_BASE}>模型</th>
                          <th className={TH_BASE}>Token 用量</th>
                          <th className={`${TH_BASE} text-right`}>费用</th>
                          <th className={`${TH_BASE} text-right`}>Kiro Credits</th>
                        </tr>
                      </thead>
                      <tbody className="[&_tr:last-child>td]:border-b-0">
                        {allRecords.map((r) => {
                          const geo = r.clientIp ? geoMap.get(r.clientIp) : undefined
                          return (
                            <tr key={r.createdAt} className="transition-colors hover:bg-surface-2">
                              <td className={`${CELL} whitespace-nowrap font-mono text-[11.5px] text-ink-2`}>
                                {formatDate(r.createdAt)}
                              </td>
                              <td className={`${CELL} whitespace-nowrap text-[11.5px] text-ink-2`}>
                                {r.clientIp ? (
                                  <span title={r.clientIp} className="flex flex-col items-start">
                                    <span className="font-mono">{geo?.displayIp ?? r.clientIp}</span>
                                    {geo && geo.country && (
                                      <span className="text-ink-3">
                                        {geo.city ? `${geo.country}·${geo.city}` : geo.country}
                                      </span>
                                    )}
                                  </span>
                                ) : (
                                  <span className="text-ink-3">—</span>
                                )}
                                {r.userAgent && (
                                  <span
                                    className="mt-px block truncate font-mono text-[10px] text-ink-3"
                                    title={r.userAgent}
                                  >
                                    {uaShort(r.userAgent)}
                                  </span>
                                )}
                              </td>
                              <td className={`${CELL} text-[11.5px] max-w-[200px]`}>
                                <div className="flex flex-col items-start gap-px">
                                  <span className={`max-w-full truncate font-mono ${getModelTone(r.model)}`} title={r.model}>
                                    {r.model}
                                  </span>
                                  {r.isStream != null && (
                                    <span
                                      className={`${BADGE_CLS} ${
                                        r.isStream ? 'bg-brand-soft text-brand' : 'bg-surface-3 text-ink-2'
                                      }`}
                                    >
                                      {r.isStream ? '流式' : '非流'}
                                    </span>
                                  )}
                                </div>
                              </td>
                              <td className={CELL}>
                                <div className="space-y-0.5 text-left text-[11.5px]">
                                  <div>输入 Tokens：<span className="tabular-nums">{formatTokens(Math.max(0, r.inputTokens - (r.cacheReadInputTokens ?? 0)))}</span></div>
                                  <div className={(r.thinkingTokenCount ?? 0) > 0 ? 'flex items-center gap-2' : undefined}>
                                    <div>输出 Tokens：<span className="tabular-nums">{formatTokens(r.outputTokens)}</span></div>
                                    {r.thinkingTokenCount != null && r.thinkingTokenCount > 0 && (
                                      <div className="inline-flex items-center gap-0.5 text-warn" title="深度思考 Tokens">
                                        <span className="sr-only">深度思考 Tokens：</span>
                                        <Brain className="size-3" aria-hidden="true" />
                                        <span className="tabular-nums">{formatTokens(r.thinkingTokenCount)}</span>
                                      </div>
                                    )}
                                  </div>
                                  <div className="text-ok">缓存读取：<span className="tabular-nums">{formatTokens(r.cacheReadInputTokens ?? 0)}</span></div>
                                  <div className="font-medium">输入总计：<span className="tabular-nums">{formatTokens(r.inputTokens)}</span></div>
                                </div>
                              </td>
                              <td className={`${CELL} text-right tabular-nums font-medium text-[12px] text-warn`}>
                                {formatCost(r.estimatedCost)}
                              </td>
                              <td className={`${CELL} text-right tabular-nums font-medium text-[12px] text-ok`}>
                                {r.creditsUsed != null ? r.creditsUsed.toFixed(4) : (r.estimatedCost / 0.72).toFixed(4)}
                                {r.creditsUsed != null && <span className="ml-1 text-xs text-ok">✓</span>}
                              </td>
                            </tr>
                          )
                        })}
                      </tbody>
                    </table>
                  </div>

                  {/* 面板脚（.panel-foot）：分页 */}
                  {data.totalPages > 1 && (
                    <div className={PANEL_FOOT}>
                      <div className="ml-auto flex flex-none items-center gap-2">
                        <span>第 {data.page} / {data.totalPages} 页</span>
                        <Pager
                          page={data.page}
                          totalPages={data.totalPages}
                          onPage={(p) => setPage(p)}
                        />
                      </div>
                    </div>
                  )}
                </section>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  )
}
