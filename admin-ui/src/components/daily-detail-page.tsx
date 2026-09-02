// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { RotateCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { FootSep, Metric, MetricAside, MetricFoot, MetricValue, MetricsBar, Ring } from '@/components/metrics'
import { PageHead } from '@/components/page-head'
import { PANEL, PANEL_FOOT, PANEL_TITLE, Pager } from '@/components/table-kit'
import { UsageLogTable, formatCost, recordCredits } from '@/components/usage-log-table'
import { useDailyUsageRecords } from '@/hooks/use-credentials'
import { useIpGeo } from '@/hooks/use-ip-geo'
import { extractErrorMessage } from '@/lib/utils'
import { formatTokenCount, localeTag } from '@/lib/locale'

interface DailyDetailPageProps {
  date: string
  onBack: () => void
}

const PAGE_SIZE = 50

/** 按 UTC 解析 + 按 UTC 格式化，避免浏览器本地时区把 CST 日历日挪一天 */
function formatDateLabel(date: string): string {
  return new Date(`${date}T00:00:00Z`).toLocaleDateString(localeTag(), {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    timeZone: 'UTC',
  })
}

export function DailyDetailPage({ date, onBack }: DailyDetailPageProps) {
  const { t } = useTranslation()
  const [page, setPage] = useState(1)
  const { data: recordsData, isLoading, refetch } = useDailyUsageRecords(date, page, PAGE_SIZE)

  const records = recordsData?.records ?? []
  const totalRecords = recordsData?.total ?? 0
  const totalPages = Math.max(1, recordsData?.totalPages ?? 1)

  const pageIps = records.map((r) => r.clientIp).filter((ip): ip is string => !!ip)
  const geoMap = useIpGeo(pageIps)

  // 后端为服务端分页，前端只持有当页 50 条 —— 除请求数外的指标均为「当页」汇总，标签已显式写明
  const pageCost = records.reduce((s, r) => s + r.estimatedCost, 0)
  const pageCredits = records.reduce((s, r) => s + recordCredits(r), 0)
  const pageSaved = records.reduce((s, r) => s + (r.creditsSaved ?? 0), 0)
  const pageInput = records.reduce((s, r) => s + r.inputTokens, 0)
  const pageOutput = records.reduce((s, r) => s + r.outputTokens, 0)
  const pageCacheRead = records.reduce((s, r) => s + (r.cacheReadInputTokens ?? 0), 0)
  const cacheShare = pageInput > 0 ? (pageCacheRead / pageInput) * 100 : 0
  const avgCost = records.length > 0 ? pageCost / records.length : 0

  const label = formatDateLabel(date)

  // refetch 默认不抛异常，失败只体现在返回结果上，须显式检查（与其余页刷新同口径）
  const handleRefresh = async () => {
    const result = await refetch()
    if (result.isError) {
      toast.error(extractErrorMessage(result.error))
      return
    }
    toast.success(t('dailyStats.toastDetailRefreshed'))
  }

  return (
    <div>
      <PageHead
        crumb={[t('dashboard.navMain'), t('dailyStats.pageTitle'), label]}
        title={t('dailyStats.detailTitle', { date: label })}
        note={t('dailyStats.detailNote', { size: PAGE_SIZE })}
        onBack={onBack}
        actions={
          <Button variant="outline" onClick={handleRefresh} disabled={isLoading}>
            <RotateCw className={isLoading ? 'animate-spin' : ''} aria-hidden="true" />
            {t('common.refresh')}
          </Button>
        }
      />

      <MetricsBar>
        <Metric label={t('common.statTotalRequests')}>
          <MetricValue value={totalRecords.toLocaleString(localeTag())} />
          <MetricFoot className="truncate">
            <span>{t('dailyStats.metricPageOf', { shown: records.length, total: totalRecords })}</span>
            <FootSep />
            <span>{t('common.pageInfoSimple', { current: page, total: totalPages })}</span>
          </MetricFoot>
        </Metric>

        <Metric label={t('common.pageCost')}>
          <MetricValue value={formatCost(pageCost)} />
          <MetricFoot className="truncate">
            <span>
              {t('dailyStats.detailAvgCost')} <b className="font-semibold text-ink-2">{formatCost(avgCost)}</b>
            </span>
          </MetricFoot>
        </Metric>

        <Metric label={t('common.pageCredits')}>
          <MetricValue value={pageCredits.toFixed(4)} unit={t('dailyStats.unitCredits')} />
          <MetricFoot className="truncate">
            {pageSaved > 0 ? (
              <span className="text-ok">{t('common.savedPrefix', { amount: pageSaved.toFixed(4) })}</span>
            ) : (
              <span>{t('dailyStats.detailNoSaving')}</span>
            )}
          </MetricFoot>
        </Metric>

        <Metric label={t('dailyStats.metricPageTokensLabel')}>
          <MetricValue value={formatTokenCount(pageInput + pageOutput)} />
          {/* pr 给 .m-aside 的 42px 环形图让位 */}
          <MetricFoot className="truncate pr-[62px]">
            <span>
              {t('dailyStats.metricCacheRead')}{' '}
              <b className="font-semibold text-ok">{formatTokenCount(pageCacheRead)}</b>
            </span>
            <FootSep />
            <span>{cacheShare.toFixed(1)}%</span>
          </MetricFoot>
          <MetricAside>
            <Ring percent={cacheShare} tone="stroke-ok" size={42} />
          </MetricAside>
        </Metric>
      </MetricsBar>

      {/* 请求日志（.panel + .tscroll + .panel-foot），与 API Key 详情页共用同一张表 */}
      <div className="mt-[15px]">
        <h3 className={PANEL_TITLE}>{t('common.requestLog')}</h3>
        <section className={PANEL}>
          <div className="max-h-[70vh] overflow-y-auto">
            <UsageLogTable records={records} geoMap={geoMap} isLoading={isLoading} />
          </div>

          <div className={PANEL_FOOT}>
            <div className="ml-auto flex flex-none items-center gap-2">
              <span>{t('apiKeys.footRecords', { count: totalRecords })}</span>
              {totalRecords > 0 && (
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
