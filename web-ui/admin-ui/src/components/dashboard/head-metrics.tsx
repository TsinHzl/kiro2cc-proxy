// Copyright (c) 2026 Harllan He. Licensed under MIT.
// Dashboard 页头 + 指标条区块（自 dashboard.tsx 拆出，纯代码搬移）
import { useTranslation } from 'react-i18next'
import { FileText } from 'lucide-react'

import { AccountMetrics } from '@/components/account-metrics'
import { PageHead } from '@/components/page-head'

interface HeadMetricsProps {
  total: number
  enabledCount: number
  disabledCredentialCount: number
  abnormalCount: number
  creditsTotal: number | null
  creditsQueried: number
  avgRemainingPercent: number | null
  consumedCreditsTotal: number | null
  consumableUsageLimitSum: number | null
  todayRequests: number | null
  requestsDeltaPercent: number | null
  requestTrend: number[]
  todayCredits: number | null
  /** 今日缓存命中率（%），口径 = cacheRead / input；无数据时 null */
  todayCacheRatePercent: number | null
  creditsDeltaPercent: number | null
  creditsTrend: number[]
  cumulativeFailures: number
  cumulativeFailureRate: number | null
  onTodayClick: () => void
  /** 打开使用文档页（账号管理页头右上角唯一入口） */
  onOpenDocs: () => void
}

export function DashboardHeadMetrics({
  total,
  enabledCount,
  disabledCredentialCount,
  abnormalCount,
  creditsTotal,
  creditsQueried,
  avgRemainingPercent,
  consumedCreditsTotal,
  consumableUsageLimitSum,
  todayRequests,
  requestsDeltaPercent,
  requestTrend,
  todayCredits,
  todayCacheRatePercent,
  creditsDeltaPercent,
  creditsTrend,
  cumulativeFailures,
  cumulativeFailureRate,
  onTodayClick,
  onOpenDocs,
}: HeadMetricsProps) {
  const { t } = useTranslation()

  return (
    <>
        {/* 页头（设计稿 .head）：面包屑 + 19px 标题 + 同基线副标题 + 右侧文档入口 */}
        <PageHead
          crumb={[t('dashboard.navMain'), t('dashboard.navCredentials')]}
          title={t('dashboard.navCredentials')}
          note={t('dashboard.pageSubtitle')}
          actions={
            <button
              type="button"
              onClick={onOpenDocs}
              className="group inline-flex h-[31px] items-center gap-1.5 rounded-[7px] px-[11px] text-[12.5px] font-medium text-ink-2 transition-colors hover:bg-surface-3 hover:text-ink"
            >
              <FileText className="h-3.5 w-3.5 text-ink-3 transition-colors group-hover:text-ink-2" />
              {t('dashboard.navDocs')}
            </button>
          }
        />
        {/* 指标条（设计稿 .metrics） */}
        <div className="mb-[15px]">
          <AccountMetrics
            total={total}
            enabledCount={enabledCount}
            disabledCount={disabledCredentialCount}
            abnormalCount={abnormalCount}
            creditsTotal={creditsTotal}
            creditsQueried={creditsQueried}
            avgRemainingPercent={avgRemainingPercent}
            consumedCreditsTotal={consumedCreditsTotal}
            consumptionLimitTotal={consumableUsageLimitSum}
            todayRequests={todayRequests}
            requestsDeltaPercent={requestsDeltaPercent}
            requestTrend={requestTrend}
            todayCredits={todayCredits}
            todayCacheRatePercent={todayCacheRatePercent}
            creditsDeltaPercent={creditsDeltaPercent}
            creditsTrend={creditsTrend}
            cumulativeFailures={cumulativeFailures}
            cumulativeFailureRate={cumulativeFailureRate}
            onTodayClick={onTodayClick}
          />
        </div>
    </>
  )
}
