// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from '@/lib/toast'
import { RotateCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { FootSep, Metric, MetricFoot, MetricValue, MetricsBar } from '@/components/metrics'
import { ModelAggPanel } from '@/components/model-agg-panel'
import { PageHead } from '@/components/page-head'
import { PANEL, PANEL_FOOT, PANEL_TITLE, Pager } from '@/components/table-kit'
import { UsageLogTable, formatCost, recordCredits } from '@/components/usage-log-table'
import {
  useCredentials,
  useCredentialUsageRecords,
  useCredentialUsageSummary,
  useCredentialTodaySummary,
} from '@/hooks/use-credentials'
import { useIpGeo } from '@/hooks/use-ip-geo'
import { ACCOUNT_STATE_VISUAL, accountLabel, deriveAccountState } from '@/lib/account-state'
import { extractErrorMessage } from '@/lib/utils'
import { formatCompactToken, formatTokenCount, localeTag } from '@/lib/locale'

interface CredentialDetailPageProps {
  credentialId: number
  onBack: () => void
}

const PAGE_SIZE = 100

/** Token 展示：不小于 100 万时改用紧凑缩写，避免窄列溢出（与 daily-shared 同口径） */
const tokenText = (n: number) => (n >= 1e6 ? formatCompactToken(n) : formatTokenCount(n))

/** Credits 展示：不小于 1 万时用紧凑缩写，避免 8 列窄格内长整数溢出 */
const creditsText = (n: number) => (n >= 1e4 ? formatCompactToken(n) : n.toFixed(4))

/** 状态标签（设计稿 .tag）：20px 高 + 5px pip，配色取自账号管理页同一套派生状态 */
const TAG =
  'inline-flex h-5 items-center gap-[5px] whitespace-nowrap rounded-[6px] border px-[7px] text-[11px] font-semibold'

export function CredentialDetailPage({ credentialId, onBack }: CredentialDetailPageProps) {
  const { t } = useTranslation()
  const [page, setPage] = useState(1)
  const [refreshing, setRefreshing] = useState(false)

  const { data: credentialsData } = useCredentials()
  const { data: recordsData, isLoading, refetch } = useCredentialUsageRecords(credentialId, page, PAGE_SIZE)
  // 两个汇总查询保留 query 对象以便刷新时一并 refetch
  const todayQuery = useCredentialTodaySummary(credentialId)
  const summaryQuery = useCredentialUsageSummary(credentialId)
  const todaySummary = todayQuery.data
  const usageSummary = summaryQuery.data

  const credential = credentialsData?.credentials.find((c) => c.id === credentialId)
  // 详情页无余额查询上下文，hasBalance 恒 false：未查额度且零成功调用的账号显示为「待验证」
  const visual = credential ? ACCOUNT_STATE_VISUAL[deriveAccountState(credential, false)] : null
  const title = credential ? accountLabel(credential) : `#${credentialId}`

  const records = recordsData?.records ?? []
  const pageIps = records.map((r) => r.clientIp).filter((ip): ip is string => !!ip)
  const geoMap = useIpGeo(pageIps)
  const totalRequests = recordsData?.total ?? 0
  const totalPages = Math.max(1, recordsData?.totalPages ?? 1)

  const pageInput = records.reduce((s, r) => s + r.inputTokens, 0)
  const pageOutput = records.reduce((s, r) => s + r.outputTokens, 0)
  const pageCost = records.reduce((s, r) => s + r.estimatedCost, 0)
  const pageCredits = records.reduce((s, r) => s + recordCredits(r), 0)
  const pageSaved = records.reduce((s, r) => s + (r.creditsSaved ?? 0), 0)
  const todaySaved = todaySummary?.totalCreditsSaved ?? 0
  const todayRequests = todaySummary?.totalRequests ?? 0

  // 全量总计：与该页「总请求数」同口径，只统计仍留存于明细缓冲的记录
  const totalTokens = (usageSummary?.totalInputTokens ?? 0) + (usageSummary?.totalOutputTokens ?? 0)
  const totalCost = usageSummary?.totalCost ?? 0
  const totalCredits = usageSummary?.totalCredits ?? 0
  const totalCreditsSaved = usageSummary?.totalCreditsSaved ?? 0
  const hasUsageSummary = usageSummary !== undefined

  // refetch 默认不抛异常，失败只体现在返回结果上，须显式检查（与 API Key 详情页同口径）。
  // 明细与两个汇总查询一并刷新，避免同屏数据时间点不一致；refreshing 重入锁与
  // daily-detail / daily-stats 两页同口径，防止进行期间重复触发并发请求
  const handleRefresh = async () => {
    setRefreshing(true)
    try {
      const results = await Promise.all([refetch(), summaryQuery.refetch(), todayQuery.refetch()])
      const failed = results.find((result) => result.isError)
      if (failed) {
        toast.error(extractErrorMessage(failed.error))
        return
      }
      toast.success(t('apiKeys.toastLogRefreshed'))
    } catch (error) {
      toast.error(extractErrorMessage(error))
    } finally {
      setRefreshing(false)
    }
  }

  const savedTrailing = (amount: number) =>
    amount > 0 ? (
      <span className="text-[11px] font-medium text-ok">
        {t('common.savedPrefix', { amount: amount.toFixed(4) })}
      </span>
    ) : undefined

  return (
    <div className="flex h-[calc(100vh-56px)] min-h-[480px] flex-col">
      {/* 页头（设计稿 .head）：三级面包屑 + 返回钮 + 序号副标题 + 右侧状态标签与刷新 */}
      <PageHead
        crumb={[t('dashboard.navMain'), t('dashboard.navCredentials'), title]}
        title={title}
        note={<code className="font-mono">#{credentialId}</code>}
        onBack={onBack}
        actions={
          <>
            {visual && (
              <span className={`${TAG} ${visual.tagClass}`}>
                <span className={`size-[5px] flex-none rounded-full ${visual.pipClass}`} aria-hidden="true" />
                {t(visual.labelKey)}
              </span>
            )}
            <Button variant="outline" onClick={handleRefresh} disabled={isLoading || refreshing}>
              <RotateCw className={isLoading || refreshing ? 'animate-spin' : ''} aria-hidden="true" />
              {t('common.refresh')}
            </Button>
          </>
        }
      />

      {/* 指标条（设计稿 .metrics）：主值统一 ink 色，费用/积分不再单独染色 */}
      <MetricsBar cols={8}>
        <Metric label={t('common.statTotalRequests')}>
          <MetricValue value={totalRequests.toLocaleString(localeTag())} />
        </Metric>
        <Metric label={t('dailyStats.metricTotalTokensLabel')}>
          <MetricValue value={hasUsageSummary ? tokenText(totalTokens) : '—'} />
          <MetricFoot className="min-w-0">
            <span className="min-w-0 truncate" title={t('dailyStats.totalTokensHint')}>
              {t('dailyStats.totalTokensHint')}
            </span>
          </MetricFoot>
        </Metric>
        <Metric label={t('dailyStats.metricTotalCreditsLabel')}>
          <MetricValue
            value={hasUsageSummary ? creditsText(totalCredits) : '—'}
            unit={t('dailyStats.unitCredits')}
          />
          <MetricFoot className="min-w-0">
            <span className="min-w-0 truncate" title={t('dailyStats.totalCreditsHint')}>
              {t('dailyStats.totalCreditsHint')}
            </span>
            {totalCreditsSaved > 0 && <FootSep />}
            {savedTrailing(totalCreditsSaved)}
          </MetricFoot>
        </Metric>
        <Metric label={t('dailyStats.metricTotalCostLabel')}>
          <MetricValue value={hasUsageSummary ? formatCost(totalCost) : '—'} />
          <MetricFoot className="min-w-0">
            <span className="min-w-0 truncate" title={t('dailyStats.totalCostHint')}>
              {t('dailyStats.totalCostHint')}
            </span>
          </MetricFoot>
        </Metric>
        <Metric label={t('credentials.pageTokens')}>
          <MetricValue value={formatTokenCount(pageInput + pageOutput)} />
          <MetricFoot className="whitespace-nowrap">
            <span>{t('common.inboundPrefix', { value: formatCompactToken(pageInput) })}</span>
            <FootSep />
            <span>{t('common.outboundPrefix', { value: formatCompactToken(pageOutput) })}</span>
          </MetricFoot>
        </Metric>
        <Metric label={t('common.pageCredits')}>
          <MetricValue value={creditsText(pageCredits)} unit={t('dailyStats.unitCredits')} />
          <MetricFoot>{savedTrailing(pageSaved)}</MetricFoot>
        </Metric>
        <Metric label={t('common.pageCost')}>
          <MetricValue value={formatCost(pageCost)} />
        </Metric>
        <Metric label={t('credentials.todayCredits')}>
          <MetricValue
            value={creditsText(todaySummary?.totalCredits ?? 0)}
            unit={t('dailyStats.unitCredits')}
          />
          <MetricFoot>
            {savedTrailing(todaySaved)}
            {todayRequests > 0 && (
              <span>{t('credentials.todayRequestsCount', { count: todayRequests })}</span>
            )}
          </MetricFoot>
        </Metric>
      </MetricsBar>

      {/* 逐模型用量（可折叠面板，聚合与表格结构在 ModelAggPanel 内） */}
      <ModelAggPanel records={records} />

      {/* 请求日志（.panel + .tscroll + .panel-foot） */}
      <div className="mt-[15px] flex min-h-0 flex-1 flex-col">
        <h3 className={PANEL_TITLE}>{t('common.requestLog')}</h3>
        <section className={`${PANEL} flex min-h-0 flex-1 flex-col`}>
          <div className="relative min-h-0 flex-1 overflow-y-auto">
            <UsageLogTable records={records} geoMap={geoMap} isLoading={isLoading} />
          </div>

          {/* 面板脚（设计稿 .panel-foot）：右区计数 + 页码 + 每页条数；服务端分页 */}
          <div className={PANEL_FOOT}>
            <div className="ml-auto flex flex-none items-center gap-2">
              <span>{t('apiKeys.footRecords', { count: totalRequests })}</span>
              {totalRequests > 0 && (
                <>
                  <Pager page={page} totalPages={totalPages} onPage={setPage} />
                  <span>{t('credentials.footPerPage', { size: PAGE_SIZE })}</span>
                </>
              )}
            </div>
          </div>
        </section>
      </div>
    </div>
  )
}
