// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { CalendarDays, Loader2, RotateCw, SearchX } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { DailyCreditsTrendChart } from '@/components/daily-credits-trend-chart'
import { Delta, FootSep, Metric, MetricAside, MetricFoot, MetricValue, MetricsBar, Ring } from '@/components/metrics'
import { PageHead } from '@/components/page-head'
import { SearchBox, Segmented, Toolbar, UpdatedAgo } from '@/components/toolbar'
import { CELL, PANEL_FOOT, TH_BASE } from '@/components/table-kit'
import { useDailyUsage } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import { localeTag } from '@/lib/locale'
import type { DailySummary } from '@/types/api'

interface DailyStatsPageProps {
  onBack: () => void
  onViewDay: (date: string) => void
  showBack?: boolean
}

type RangeKey = '7' | '14' | '30'
const RANGE_DAYS: Record<RangeKey, number> = { '7': 7, '14': 14, '30': 30 }

type UsageFilter = 'active' | 'all'

const DAY_MS = 86_400_000
/**
 * 后端 `get_daily_summaries()`（src/model/usage.rs:738）按 CST(UTC+8) 聚合，
 * 前端日期窗口必须同口径 —— 按 UTC 推算会让 CST 00:00–08:00 的当日数据落在窗口外。
 */
const CST_OFFSET_MS = 8 * 3_600_000

/** daysAgo 天前的 CST 日历日（YYYY-MM-DD） */
function cstDateString(daysAgo: number): string {
  const cstMidnightMs = Math.floor((Date.now() + CST_OFFSET_MS) / DAY_MS) * DAY_MS
  return new Date(cstMidnightMs - daysAgo * DAY_MS).toISOString().slice(0, 10)
}

interface DayRow {
  date: string
  credits: number
  cost: number
  requests: number
  saved: number
}

/**
 * 区间内每一天的零填充序列（升序）。
 *
 * 有意不做 useMemo —— 结果依赖「当前时刻」，memo 会把窗口冻结在首次渲染，
 * 跨 CST 午夜后滞后一天。`useDailyUsage` 的 refetchInterval 为 60s，重算成本可忽略。
 */
function buildWindow(data: DailySummary[] | undefined, days: number, offset: number): DayRow[] {
  const byDate = new Map((data ?? []).map((d) => [d.date, d]))
  return Array.from({ length: days }, (_, i) => {
    const date = cstDateString(offset + days - 1 - i)
    const row = byDate.get(date)
    return {
      date,
      credits: row?.totalCredits ?? 0,
      cost: row?.totalCost ?? 0,
      requests: row?.totalRequests ?? 0,
      saved: row?.totalCreditsSaved ?? 0,
    }
  })
}

/** 按 UTC 解析 + 按 UTC 取星期，避免浏览器本地时区把 CST 日历日挪一天 */
function weekdayLabel(date: string): string {
  return new Date(`${date}T00:00:00Z`).toLocaleDateString(localeTag(), { weekday: 'short', timeZone: 'UTC' })
}

function median(values: number[]): number {
  if (values.length === 0) return 0
  const sorted = [...values].sort((a, b) => a - b)
  const mid = Math.floor(sorted.length / 2)
  return sorted.length % 2 === 0 ? (sorted[mid - 1] + sorted[mid]) / 2 : sorted[mid]
}

/** 占比 ≥ 此值视为区间主导日（设计稿 56.8% 用 accent 高亮） */
const SHARE_TONE_STRONG = 50
/** 占比 < 此值视为可忽略量（设计稿 0.2% 用 text-3 弱化） */
const SHARE_TONE_WEAK = 1

/** 占区间百分比着色：设计稿 56.8% 用 accent、20% 档用 text-2、0.2% 用 text-3 */
function shareTone(share: number): string {
  if (share >= SHARE_TONE_STRONG) return 'font-semibold text-brand'
  return share >= SHARE_TONE_WEAK ? 'text-ink-2' : 'text-ink-3'
}

const COLUMN_WIDTHS: (number | undefined)[] = [130, 78, undefined, 120]

export function DailyStatsPage({ onBack, onViewDay, showBack = true }: DailyStatsPageProps) {
  const { t } = useTranslation()
  const { data, isLoading, refetch, dataUpdatedAt } = useDailyUsage()
  const [range, setRange] = useState<RangeKey>('14')
  const [filter, setFilter] = useState<UsageFilter>('active')
  const [query, setQuery] = useState('')

  const days = RANGE_DAYS[range]
  const rows = buildWindow(data, days, 0)
  const prevRows = buildWindow(data, days, days)

  const total = rows.reduce((s, r) => s + r.credits, 0)
  const prevTotal = prevRows.reduce((s, r) => s + r.credits, 0)
  const totalRequests = rows.reduce((s, r) => s + r.requests, 0)
  const activeRows = rows.filter((r) => r.credits > 0)
  const peak = activeRows.reduce<DayRow | null>(
    (best, r) => (best === null || r.credits > best.credits ? r : best),
    null,
  )

  // 从区间末尾往前数的连续零消耗天数
  let zeroStreak = 0
  for (let i = rows.length - 1; i >= 0 && rows[i].credits === 0; i--) zeroStreak++
  const zeroStreakSince = zeroStreak > 0 ? rows[rows.length - zeroStreak].date : null

  const avgActive = activeRows.length > 0 ? total / activeRows.length : 0
  const avgAll = total / days
  const medianActive = median(activeRows.map((r) => r.credits))
  const from = rows[0].date
  const to = rows[rows.length - 1].date

  const q = query.trim()
  const tableRows = (filter === 'active' ? activeRows : rows)
    .filter((r) => q === '' || r.date.includes(q))
    // 日期降序（最新在前）：设计稿原为积分降序，但日期乱序不利于按天核对，故改为时间轴倒序
    .sort((a, b) => b.date.localeCompare(a.date))

  // refetch 默认不抛异常，失败只体现在返回结果上，须显式检查（与其余页刷新同口径）
  const handleRefresh = async () => {
    const result = await refetch()
    if (result.isError) {
      toast.error(extractErrorMessage(result.error))
      return
    }
    toast.success(t('dailyStats.toastRefreshed'))
  }

  return (
    <div className="flex min-h-0 flex-col">
      <PageHead
        crumb={[t('dashboard.navMain'), t('dailyStats.pageTitle')]}
        title={t('dailyStats.pageTitle')}
        note={t('dailyStats.headNote')}
        onBack={showBack ? onBack : undefined}
        actions={
          <>
            <Segmented
              value={range}
              onChange={setRange}
              groupLabel={t('dailyStats.rangeGroupLabel')}
              options={[
                { key: '7', label: t('dailyStats.rangeDays', { days: 7 }) },
                { key: '14', label: t('dailyStats.rangeDays', { days: 14 }) },
                { key: '30', label: t('dailyStats.rangeDays', { days: 30 }) },
              ]}
            />
            {/* 设计稿此处为日历按钮；无日期选择器需求支撑，降级为非交互的区间信息 */}
            <span
              className="inline-flex h-[31px] flex-none items-center gap-[7px] rounded-[7px] border border-hairline-2 bg-surface px-2.5 font-mono text-[11.5px] tabular-nums text-ink-2 shadow-hair"
              title={t('dailyStats.rangeSpanLabel')}
            >
              <CalendarDays className="size-[15px] flex-none text-ink-3" strokeWidth={1.7} aria-hidden="true" />
              {from.slice(5)} — {to.slice(5)}
            </span>
            <Button variant="outline" onClick={handleRefresh} disabled={isLoading}>
              <RotateCw className={isLoading ? 'animate-spin' : ''} aria-hidden="true" />
              {t('common.refresh')}
            </Button>
          </>
        }
      />

      <MetricsBar>
        <Metric label={t('dailyStats.metricTotalLabel')}>
          <MetricValue value={total.toFixed(2)} unit={t('dailyStats.unitCredits')} />
          <MetricFoot className="truncate">
            <span>{t('dailyStats.rangeDays', { days })}</span>
            <FootSep />
            <span className="flex items-center gap-1">
              {t('dailyStats.metricVsPrev')}
              {/* 上一区间为 0 时百分比无意义（除零），不渲染 */}
              {prevTotal > 0 && (
                <span title={t('dailyStats.metricVsPrevHint')}>
                  <Delta percent={((total - prevTotal) / prevTotal) * 100} />
                </span>
              )}
            </span>
          </MetricFoot>
        </Metric>

        <Metric label={t('dailyStats.metricPeakLabel')}>
          <MetricValue value={peak ? peak.credits.toFixed(2) : '—'} unit={peak ? peak.date.slice(5) : undefined} />
          <MetricFoot className="truncate">
            {peak && total > 0 ? (
              <>
                <span title={t('dailyStats.metricPeakShareHint')}>
                  {t('dailyStats.metricPeakShare')}{' '}
                  <b className="font-semibold text-ink-2">{((peak.credits / total) * 100).toFixed(1)}%</b>
                </span>
                <FootSep />
                <span>
                  {t('dailyStats.metricPeakRequests')}{' '}
                  <b className="font-semibold text-ink-2">{peak.requests.toLocaleString(localeTag())}</b>
                </span>
              </>
            ) : (
              <span>{t('dailyStats.metricNoUsage')}</span>
            )}
          </MetricFoot>
        </Metric>

        <Metric label={t('dailyStats.metricActiveDaysLabel')}>
          <MetricValue value={String(activeRows.length)} unit={t('dailyStats.metricOfDays', { days })} />
          {/* pr 给 .m-aside 的 42px 环形图让位 */}
          <MetricFoot className="truncate pr-[62px]">
            {zeroStreakSince ? (
              <>
                <span>
                  {t('dailyStats.metricZeroStreak')}{' '}
                  <b className="font-semibold text-warn">{t('dailyStats.rangeDays', { days: zeroStreak })}</b>
                </span>
                <FootSep />
                <span>{t('dailyStats.metricZeroStreakSince', { date: zeroStreakSince.slice(5) })}</span>
              </>
            ) : (
              <span>{t('dailyStats.metricNoZeroStreak')}</span>
            )}
          </MetricFoot>
          <MetricAside>
            <Ring percent={(activeRows.length / days) * 100} tone="stroke-warn" />
          </MetricAside>
        </Metric>

        <Metric label={t('dailyStats.metricAvgActiveLabel')}>
          <MetricValue value={avgActive.toFixed(2)} unit={t('dailyStats.unitCreditsPerDay')} />
          <MetricFoot className="truncate">
            <span>
              {t('dailyStats.metricAvgAll')} <b className="font-semibold text-ink-2">{avgAll.toFixed(2)}</b>
            </span>
            <FootSep />
            <span>
              {t('dailyStats.metricMedian')} <b className="font-semibold text-ink-2">{medianActive.toFixed(2)}</b>
            </span>
          </MetricFoot>
        </Metric>
      </MetricsBar>

      <div className="mt-4">
        <DailyCreditsTrendChart points={rows} />
      </div>

      <Toolbar>
        <SearchBox
          value={query}
          onChange={setQuery}
          placeholder={t('dailyStats.searchPlaceholder')}
          clearLabel={t('dailyStats.searchClear')}
        />
        <Segmented
          value={filter}
          onChange={setFilter}
          groupLabel={t('dailyStats.segGroupLabel')}
          options={[
            { key: 'active', label: t('dailyStats.segActive'), count: activeRows.length },
            { key: 'all', label: t('dailyStats.segAll'), count: rows.length },
          ]}
        />
        <div className="ml-auto flex flex-wrap items-center gap-2.5">
          <span className="text-[11px] text-ink-3">{t('dailyStats.toolNote')}</span>
          <UpdatedAgo dataUpdatedAt={dataUpdatedAt} />
        </div>
      </Toolbar>

      {/* 设计稿 .panel.is-static：不吃剩余高度、不内滚，行数受区间上限 30 约束 */}
      <section className="flex-none overflow-hidden rounded-[11px] border border-hairline bg-surface shadow-panel">
        <table className="w-full border-collapse text-[12.5px]">
          <colgroup>
            {COLUMN_WIDTHS.map((w, i) => (
              <col key={i} style={w ? { width: w } : undefined} />
            ))}
          </colgroup>
          <thead>
            <tr>
              <th className={TH_BASE}>{t('dailyStats.colDate')}</th>
              <th className={TH_BASE}>{t('dailyStats.colWeekday')}</th>
              <th className={TH_BASE}>
                <div className="flex items-baseline gap-1.5">
                  <span>{t('dailyStats.colCreditsUsage')}</span>
                  <span className="font-normal normal-case tracking-normal text-ink-3">
                    {t('dailyStats.colCreditsUsageShare', { days })}
                  </span>
                </div>
              </th>
              <th className={`${TH_BASE} text-right`}>{t('dailyStats.colRequests')}</th>
            </tr>
          </thead>
          <tbody className="[&_tr:last-child>td]:border-b-0">
            {tableRows.length === 0 ? (
              <tr>
                <td colSpan={COLUMN_WIDTHS.length} className="px-3 py-14">
                  <div className="flex flex-col items-center gap-2.5 text-ink-3">
                    {isLoading ? (
                      <Loader2 className="size-6 animate-spin" aria-hidden="true" />
                    ) : (
                      <SearchX className="size-6" aria-hidden="true" />
                    )}
                    <span className="text-[12.5px]">
                      {isLoading
                        ? t('common.loading')
                        : q !== '' || filter === 'active'
                          ? t('dailyStats.emptyNoMatch')
                          : t('dailyStats.emptyNoRecords')}
                    </span>
                  </div>
                </td>
              </tr>
            ) : (
              tableRows.map((row) => {
                const share = total > 0 ? (row.credits / total) * 100 : 0
                // .bar 长度相对峰值日（设计稿 25.14 → 100%、10.57 → 42%）
                const barWidth = peak && peak.credits > 0 ? (row.credits / peak.credits) * 100 : 0
                return (
                  <tr
                    key={row.date}
                    tabIndex={0}
                    onClick={() => onViewDay(row.date)}
                    onKeyDown={(e) => {
                      if (e.key !== 'Enter' && e.key !== ' ') return
                      e.preventDefault()
                      onViewDay(row.date)
                    }}
                    title={t('dailyStats.viewDetailTitle')}
                    className="cursor-pointer transition-colors hover:bg-surface-2 focus-visible:outline focus-visible:-outline-offset-2 focus-visible:outline-brand"
                  >
                    <td className={CELL}>
                      <span className="whitespace-nowrap font-mono text-[12px] tabular-nums text-ink">{row.date}</span>
                    </td>
                    <td className={CELL}>
                      <span className="whitespace-nowrap text-[11.5px] text-ink-3">{weekdayLabel(row.date)}</span>
                    </td>
                    {/* 设计稿 .quota：绝对值 + 占区间百分比 + .bar；USD 与省额降为副行，保证费用信息不丢 */}
                    <td className={CELL}>
                      <div className="flex min-w-[190px] flex-col gap-[5px]">
                        <div className="flex items-baseline justify-between gap-2 font-mono text-[11.5px] tabular-nums">
                          <span className="text-ink-2">
                            <b className="font-semibold text-ink">{row.credits.toFixed(4)}</b>{' '}
                            <span className="text-ink-3">{t('dailyStats.unitCredits')}</span>
                          </span>
                          <span className={shareTone(share)} title={t('dailyStats.colCreditsUsageShareTitle')}>
                            {share.toFixed(1)}%
                          </span>
                        </div>
                        <span className="block h-1 overflow-hidden rounded-[3px] bg-track">
                          <span
                            className="block h-full rounded-[3px] bg-brand"
                            style={{ width: `${Math.min(100, Math.max(0, barWidth))}%` }}
                          />
                        </span>
                        <span className="font-mono text-[10px] tabular-nums text-ink-3">
                          ${row.cost.toFixed(4)}
                          {row.saved > 0 && (
                            <span className="ml-1.5 text-ok">
                              {t('common.savedPrefix', { amount: row.saved.toFixed(4) })}
                            </span>
                          )}
                        </span>
                      </div>
                    </td>
                    <td className={`${CELL} text-right`}>
                      <span className="font-mono text-[12px] tabular-nums text-ink-2">
                        {row.requests.toLocaleString(localeTag())}
                      </span>
                    </td>
                  </tr>
                )
              })
            )}
          </tbody>
        </table>

        <div className={PANEL_FOOT}>
          <span>
            {t('dailyStats.footRange')} <b className="font-semibold text-ink-2">{from}</b> —{' '}
            <b className="font-semibold text-ink-2">{to}</b>
          </span>
          <span className="ml-auto">
            {t('dailyStats.footTotals', {
              credits: total.toFixed(4),
              requests: totalRequests.toLocaleString(localeTag()),
            })}
          </span>
        </div>
      </section>
    </div>
  )
}
