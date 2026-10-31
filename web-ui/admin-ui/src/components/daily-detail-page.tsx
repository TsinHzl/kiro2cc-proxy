// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from '@/lib/toast'
import { RotateCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { UsageMetricsBar } from '@/components/daily-shared'
import { ModelAggPanel } from '@/components/model-agg-panel'
import { PageHead } from '@/components/page-head'
import { PANEL, PANEL_FOOT, PANEL_TITLE, Pager } from '@/components/table-kit'
import { UsageLogTable } from '@/components/usage-log-table'
import { useDailyUsage, useDailyUsageRecords } from '@/hooks/use-credentials'
import { useIpGeo } from '@/hooks/use-ip-geo'
import { extractErrorMessage } from '@/lib/utils'
import { localeTag } from '@/lib/locale'

interface DailyDetailPageProps {
  date: string
  onBack: () => void
}

const PAGE_SIZE = 100

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
  const dailyUsage = useDailyUsage()
  const [refreshing, setRefreshing] = useState(false)

  const records = recordsData?.records ?? []
  const totalRecords = recordsData?.total ?? 0
  const totalPages = Math.max(1, recordsData?.totalPages ?? 1)

  const pageIps = records.map((r) => r.clientIp).filter((ip): ip is string => !!ip)
  const geoMap = useIpGeo(pageIps)

  const label = formatDateLabel(date)

  // refetch 默认不抛异常，失败只体现在返回结果上，须显式检查（与其余页刷新同口径）
  const handleRefresh = async () => {
    setRefreshing(true)
    try {
      const results = await Promise.all([refetch(), dailyUsage.refetch()])
      const failed = results.find((result) => result.isError)
      if (failed) {
        toast.error(extractErrorMessage(failed.error))
        return
      }
      toast.success(t('dailyStats.toastDetailRefreshed'))
    } catch (error) {
      toast.error(extractErrorMessage(error))
    } finally {
      setRefreshing(false)
    }
  }

  return (
    <div className="flex min-h-[calc(100vh-56px)] flex-col">
      <PageHead
        crumb={[t('dashboard.navMain'), t('dailyStats.pageTitle'), label]}
        title={t('dailyStats.detailTitle', { date: label })}
        note={t('dailyStats.detailNote', { size: PAGE_SIZE })}
        onBack={onBack}
        actions={
          <Button variant="outline" onClick={handleRefresh} disabled={isLoading || refreshing}>
            <RotateCw className={isLoading || refreshing ? 'animate-spin' : ''} aria-hidden="true" />
            {t('common.refresh')}
          </Button>
        }
      />

      <UsageMetricsBar
        daily={dailyUsage.data}
        from={date}
        to={date}
        isDailyLoading={dailyUsage.isLoading}
        isDailyError={dailyUsage.isError}
        records={records}
        totalRecords={totalRecords}
        page={page}
        totalPages={totalPages}
      />

      {/* 按模型分组（当前页）：与详情页同款可折叠面板 */}
      <ModelAggPanel records={records} />

      {/* 请求日志（.panel + .tscroll + .panel-foot），与 API Key 详情页共用同一张表 */}
      <div className="mt-[15px] flex min-h-0 flex-1 flex-col">
        <h3 className={PANEL_TITLE}>{t('common.requestLog')}</h3>
        <section className={`${PANEL} flex min-h-0 flex-1 flex-col`}>
          <div className="min-h-0 flex-1 overflow-y-auto">
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
