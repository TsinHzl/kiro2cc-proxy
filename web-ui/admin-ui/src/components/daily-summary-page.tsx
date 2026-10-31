// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from '@/lib/toast'
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
import {
  CustomRangeDialog,
  RANGE_DAYS,
  type CustomRange,
  type DayRow,
  buildWindow,
  weekdayLabel,
} from '@/components/daily-shared'

export type SummaryRangeKey = '7' | '14' | '30' | 'custom'

type UsageFilter = 'active' | 'all'

const DAY_MS = 86_400_000

/** 自定义区间的最大跨度（闭区间含两端天数），超长区间折线图与表格不可用 */
const CUSTOM_RANGE_MAX_DAYS = 366

/** 解析 YYYY-MM-DD 为 UTC 毫秒；非法格式返回 NaN */
function toUtcMs(date: string): number {
  return Date.parse(`${date}T00:00:00Z`)
}

/** 闭区间跨度（含两端天数）；非法日期或 from > to 返回 NaN */
function spanDaysOf(from: string, to: string): number {
  const fromMs = toUtcMs(from)
  const toMs = toUtcMs(to)
  if (Number.isNaN(fromMs) || Number.isNaN(toMs) || fromMs > toMs) return Number.NaN
  return Math.floor((toMs - fromMs) / DAY_MS) + 1
}

/**
 * 绝对日期闭区间的零填充序列（升序），与 buildWindow 同口径但接受固定日期。
 * custom 区间不随 CST 午夜滚动（用户选定即固定），无需 cstDateString 偏移重算。
 */
function buildCustomWindow(data: DailySummary[] | undefined, from: string, to: string): DayRow[] {
  const spanDays = spanDaysOf(from, to)
  // NaN 与 <= 0 都不成窗口：倒置/非法日期由对话框 invalid 拦截，此处兜底防空序列
  if (!Number.isFinite(spanDays) || spanDays <= 0) return []
  const fromMs = toUtcMs(from)
  const byDate = new Map((data ?? []).map((d) => [d.date, d]))
  return Array.from({ length: spanDays }, (_, i) => {
    const date = new Date(fromMs + i * DAY_MS).toISOString().slice(0, 10)
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

/**
 * 表格列宽（px，取自设计稿表头）：日期 / 星期 / 积分 / 请求数。
 * 第三列 undefined 即交给表格按剩余宽度均分 —— 该列内含绝对值 + 占比 + 进度条，
 * 需要弹性宽度，写死会在窄屏截断进度条。
 */
const COLUMN_WIDTHS: (number | undefined)[] = [130, 78, undefined, 120]

/**
 * 「每日统计」独立汇总页（3.4.28 版每日统计页形态）：区间指标卡 + 折线图 +
 * 日期汇总表格（点击下钻日明细）。
 *
 * - 区间（7/14/30 + 自定义）由父级持有并经 props 传入（下钻日明细返回后保持），
 *   与使用日志页的 range 完全独立
 * - 数据仍来自 useDailyUsage（60s 轮询），buildWindow 零填充窗口口径不变；
 *   custom 态走 buildCustomWindow（绝对日期闭区间，不随午夜滚动）
 */
export function DailySummaryPage({
  onBack,
  onViewDay,
  sumRange,
  onSumRangeChange,
  customRange,
  onCustomRangeChange,
}: {
  onBack: () => void
  onViewDay: (date: string) => void
  sumRange: SummaryRangeKey
  onSumRangeChange: (range: SummaryRangeKey) => void
  customRange: CustomRange | null
  onCustomRangeChange: (range: CustomRange | null) => void
}) {
  const { t } = useTranslation()
  const { data, isLoading, refetch, dataUpdatedAt } = useDailyUsage()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [filter, setFilter] = useState<UsageFilter>('active')
  const [query, setQuery] = useState('')

  // custom 态区间为用户选定固定日期；固定档 days 为区间实际天数（指标分母）。
  // sumRange 为 'custom' 但 customRange 尚未应用（校验失败）时按默认 7 天窗口兜底
  const isCustom = sumRange === 'custom' && customRange !== null
  const fixedDays = RANGE_DAYS[sumRange === 'custom' ? '7' : sumRange]
  const rows = isCustom
    ? buildCustomWindow(data, customRange.from, customRange.to)
    : buildWindow(data, fixedDays, 0)
  // 环比仅固定档有定义（等长上一滚动窗口）；custom 态不渲染环比，恒 0 跳过 Delta
  const prevRows = isCustom ? [] : buildWindow(data, fixedDays, fixedDays)
  const days = isCustom ? rows.length : fixedDays

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
  const avgAll = days > 0 ? total / days : 0
  const medianActive = median(activeRows.map((r) => r.credits))
  const from = rows.length > 0 ? rows[0].date : ''
  const to = rows.length > 0 ? rows[rows.length - 1].date : ''

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

  // 应用自定义区间：跨度超上限仅提示不切态（弹窗保持打开，用户可直接改日期重试）
  const handleApplyCustom = (r: CustomRange) => {
    const spanDays = spanDaysOf(r.from, r.to)
    // 对话框已拦截非法日期，此处按 NaN / 超上限分别兜底，避免把解析失败误报成区间过长
    if (!Number.isFinite(spanDays)) {
      toast.error(t('dailyStats.customInvalidRange'))
      return
    }
    if (spanDays > CUSTOM_RANGE_MAX_DAYS) {
      toast.error(t('dailyStats.customSpanTooLong', { days: CUSTOM_RANGE_MAX_DAYS }))
      return
    }
    onCustomRangeChange(r)
    onSumRangeChange('custom')
    setDialogOpen(false)
  }

  return (
    <div className="flex min-h-0 flex-col">
      <PageHead
        crumb={[t('dashboard.navMain'), t('dailyStats.summaryPageTitle')]}
        title={t('dailyStats.summaryPageTitle')}
        note={t('dailyStats.summaryHeadNote')}
        onBack={onBack}
        actions={
          <>
            <Segmented
              value={sumRange}
              onChange={onSumRangeChange}
              groupLabel={t('dailyStats.rangeGroupLabel')}
              options={[
                { key: '7', label: t('dailyStats.rangeDays', { days: 7 }) },
                { key: '14', label: t('dailyStats.rangeDays', { days: 14 }) },
                { key: '30', label: t('dailyStats.rangeDays', { days: 30 }) },
              ]}
            />
            {/* 日期范围按钮：点击弹出自定义日期对话框（与使用日志页同款交互） */}
            <Button
              variant="outline"
              className="gap-[7px] px-2.5 font-mono text-[11.5px] tabular-nums"
              title={t('dailyStats.rangeSpanLabel')}
              onClick={() => setDialogOpen(true)}
            >
              <CalendarDays strokeWidth={1.7} aria-hidden="true" />
              {isCustom ? `${customRange.from} — ${customRange.to}` : `${from} — ${to}`}
            </Button>
            <Button variant="outline" onClick={handleRefresh} disabled={isLoading}>
              <RotateCw className={isLoading ? 'animate-spin' : ''} aria-hidden="true" />
              {t('common.refresh')}
            </Button>
          </>
        }
      />

      <SummaryMetrics
        total={total}
        days={days}
        isCustom={isCustom}
        prevTotal={prevTotal}
        peak={peak}
        zeroStreak={zeroStreak}
        zeroStreakSince={zeroStreakSince}
        activeCount={activeRows.length}
        avgActive={avgActive}
        avgAll={avgAll}
        medianActive={medianActive}
      />

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

      <SummaryTable
        tableRows={tableRows}
        isLoading={isLoading}
        isEmptyFiltered={q !== '' || filter === 'active'}
        total={total}
        totalRequests={totalRequests}
        peak={peak}
        days={days}
        from={from}
        to={to}
        onViewDay={onViewDay}
      />

      <CustomRangeDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        onApply={handleApplyCustom}
        initial={customRange}
      />
    </div>
  )
}

/**
 * 区间指标卡四联：总量 / 峰值日 / 活跃天数 / 日均。
 * 纯展示（props 为已算好的标量），不持有状态。未包 memo —— 父组件持有搜索 / 筛选
 * 等本地 state，每次按键都会重渲染本块，而 peak 等对象 props 每轮均由 filter/reduce
 * 新产出、引用必然变化，memo 命中率为零；要优化需先把对象 props 拍平成标量。
 */
function SummaryMetrics({
  total,
  days,
  isCustom,
  prevTotal,
  peak,
  zeroStreak,
  zeroStreakSince,
  activeCount,
  avgActive,
  avgAll,
  medianActive,
}: {
  total: number
  days: number
  isCustom: boolean
  prevTotal: number
  peak: DayRow | null
  zeroStreak: number
  zeroStreakSince: string | null
  activeCount: number
  avgActive: number
  avgAll: number
  medianActive: number
}) {
  const { t } = useTranslation()

  return (
    <MetricsBar>
      <Metric label={t('dailyStats.metricTotalLabel')}>
        <MetricValue value={total.toFixed(2)} unit={t('dailyStats.unitCredits')} />
        <MetricFoot className="truncate">
          <span>{t('dailyStats.rangeDays', { days })}</span>
          {/* 环比仅固定档有定义（等长上一滚动窗口）；custom 态等长前一区间语义模糊，不渲染 */}
          {!isCustom && (
            <>
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
            </>
          )}
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
        <MetricValue value={String(activeCount)} unit={t('dailyStats.metricOfDays', { days })} />
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
          <Ring percent={days > 0 ? (activeCount / days) * 100 : 0} tone="stroke-warn" size={42} />
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
  )
}

/**
 * 日期汇总表格 + 空态 + 页脚合计。
 *
 * 下钻入口为日期单元格内真实 `<button>`：整行 onClick 仅作鼠标热区（省去瞄准日期文字的
 * 成本），键盘与屏幕阅读器走原生 button（自带 role、Enter/Space 激活与可访问名称），
 * 按钮点击 stopPropagation 避免与行 onClick 双触发。
 */
function SummaryTable({
  tableRows,
  isLoading,
  isEmptyFiltered,
  total,
  totalRequests,
  peak,
  days,
  from,
  to,
  onViewDay,
}: {
  tableRows: DayRow[]
  isLoading: boolean
  isEmptyFiltered: boolean
  total: number
  totalRequests: number
  peak: DayRow | null
  days: number
  from: string
  to: string
  onViewDay: (date: string) => void
}) {
  const { t } = useTranslation()

  return (
    /* 设计稿 .panel.is-static：不吃剩余高度、不内滚，custom 态最长 366 行随页面滚动 */
    <section className="flex-none overflow-hidden rounded-[11px] border border-transparent bg-surface shadow-panel">
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
                      : isEmptyFiltered
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
                  onClick={() => onViewDay(row.date)}
                  className="cursor-pointer transition-colors hover:bg-surface-2"
                >
                  <td className={CELL}>
                    <button
                      type="button"
                      title={`${row.date} ${t('dailyStats.viewDetailTitle')}`}
                      aria-label={`${row.date} ${t('dailyStats.viewDetailTitle')}`}
                      onClick={(e) => {
                        e.stopPropagation()
                        onViewDay(row.date)
                      }}
                      className="whitespace-nowrap rounded font-mono text-[12px] tabular-nums text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand"
                    >
                      {row.date}
                    </button>
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
  )
}
