// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { FootSep, Metric, MetricFoot, MetricValue, MetricsBar } from '@/components/metrics'
import { formatCost, recordCredits } from '@/components/usage-log-table'
import { formatCompactToken, formatTokenCount, localeTag } from '@/lib/locale'
import type { DailySummary, UsageRecord } from '@/types/api'

/** 固定档区间天数（每日统计汇总页与使用日志页共用档位） */
export const RANGE_DAYS: Record<'7' | '14' | '30', number> = { '7': 7, '14': 14, '30': 30 }

/** 已应用的自定义区间（CST 日历日，闭区间含两端） */
export interface CustomRange {
  from: string
  to: string
}

/** Token 展示：不小于 100 万时改用紧凑缩写，避免窄列溢出 */
const tokenText = (n: number) => (n >= 1e6 ? formatCompactToken(n) : formatTokenCount(n))

/** Credits 展示：不小于 1 万时用紧凑缩写，避免 7 列窄格内长整数溢出（与账号详情页 creditsText 同口径） */
const creditsText = (n: number) => (n >= 1e4 ? formatCompactToken(n) : n.toFixed(4))

/** 紧凑脚注文本：在 flex 脚注行内独立截断并带省略号 */
function FootText({ children, title }: { children: ReactNode; title?: string }) {
  return (
    <span className="min-w-0 truncate" title={title}>
      {children}
    </span>
  )
}

/**
 * 七项单行指标条：前 4 项为 CST 闭区间 [from, to] 的日报总计（全部留存记录，不受分页与截断影响），
 * 后 3 项为当前页明细汇总。
 */
export function UsageMetricsBar({
  daily,
  from,
  to,
  isDailyLoading,
  isDailyError,
  records,
  totalRecords,
  page,
  totalPages,
}: {
  daily: DailySummary[] | undefined
  from: string
  to: string
  isDailyLoading: boolean
  isDailyError: boolean
  records: UsageRecord[]
  totalRecords: number
  page: number
  totalPages: number
}) {
  const { t } = useTranslation()
  const totals = (daily ?? [])
    .filter((day) => from <= day.date && day.date <= to)
    .reduce(
      (sum, day) => ({
        requests: sum.requests + day.totalRequests,
        cost: sum.cost + day.totalCost,
        credits: sum.credits + day.totalCredits,
        tokens: sum.tokens + day.totalInputTokens + day.totalOutputTokens,
      }),
      { requests: 0, cost: 0, credits: 0, tokens: 0 },
    )
  const hasDaily = daily !== undefined
  const pageCost = records.reduce((s, r) => s + r.estimatedCost, 0)
  const pageCredits = records.reduce((s, r) => s + recordCredits(r), 0)
  const pageTokens = records.reduce((s, r) => s + r.inputTokens + r.outputTokens, 0)
  const pageInput = records.reduce((s, r) => s + r.inputTokens, 0)
  const pageCacheRead = records.reduce((s, r) => s + (r.cacheReadInputTokens ?? 0), 0)
  const cacheShare = pageInput > 0 ? (pageCacheRead / pageInput) * 100 : 0
  const avgCost = records.length > 0 ? pageCost / records.length : 0
  const avgCredits = records.length > 0 ? pageCredits / records.length : 0
  const note = isDailyError
    ? t(hasDaily ? 'dailyStats.totalsRefreshFailed' : 'common.loadFailed')
    : isDailyLoading
      ? t('common.loading')
      : t('dailyStats.totalsRetainedNote')

  return (
    <div className="shrink-0" aria-busy={isDailyLoading}>
      <div className="mb-2 flex flex-wrap items-baseline justify-between gap-2 text-[11px] text-ink-3">
        <span role={isDailyError ? 'alert' : undefined}>{note}</span>
        <span className="font-mono tabular-nums">
          {from === to ? from : `${from} — ${to}`} · UTC+8
        </span>
      </div>
      <MetricsBar cols={7}>
        <Metric label={t('common.statTotalRequests')}>
          <MetricValue value={(hasDaily ? totals.requests : totalRecords).toLocaleString(localeTag())} />
          <MetricFoot className="min-w-0">
            <FootText>{t('common.pageInfoSimple', { current: page, total: totalPages })}</FootText>
          </MetricFoot>
        </Metric>
        <Metric label={t('dailyStats.metricTotalTokensLabel')}>
          <MetricValue value={hasDaily ? tokenText(totals.tokens) : '—'} />
          <MetricFoot className="min-w-0">
            <FootText title={t('dailyStats.totalTokensHint')}>{t('dailyStats.totalTokensHint')}</FootText>
          </MetricFoot>
        </Metric>
        <Metric label={t('dailyStats.metricTotalCreditsLabel')}>
          <MetricValue
            value={hasDaily ? creditsText(totals.credits) : '—'}
            unit={t('dailyStats.unitCredits')}
          />
          <MetricFoot className="min-w-0">
            <FootText title={t('dailyStats.totalCreditsHint')}>{t('dailyStats.totalCreditsHint')}</FootText>
          </MetricFoot>
        </Metric>
        <Metric label={t('dailyStats.metricTotalCostLabel')}>
          <MetricValue value={hasDaily ? formatCost(totals.cost) : '—'} />
          <MetricFoot className="min-w-0">
            <FootText title={t('dailyStats.totalCostHint')}>{t('dailyStats.totalCostHint')}</FootText>
          </MetricFoot>
        </Metric>
        <Metric label={t('dailyStats.metricPageTokensLabel')}>
          <MetricValue value={tokenText(pageTokens)} />
          <MetricFoot className="min-w-0">
            <FootText>
              {t('dailyStats.metricCacheRead')}{' '}
              <b className="font-semibold text-ok">{formatCompactToken(pageCacheRead)}</b>
            </FootText>
            <FootSep />
            <span className="shrink-0">{cacheShare.toFixed(1)}%</span>
          </MetricFoot>
        </Metric>
        <Metric label={t('common.pageCredits')}>
          <MetricValue value={creditsText(pageCredits)} unit={t('dailyStats.unitCredits')} />
          <MetricFoot className="min-w-0">
            <FootText>
              {t('dailyStats.detailAvgCredits')}{' '}
              <b className="font-semibold text-ink-2">{creditsText(avgCredits)}</b>
            </FootText>
          </MetricFoot>
        </Metric>
        <Metric label={t('common.pageCost')}>
          <MetricValue value={formatCost(pageCost)} />
          <MetricFoot className="min-w-0">
            <FootText>
              {t('dailyStats.detailAvgCost')}{' '}
              <b className="font-semibold text-ink-2">{formatCost(avgCost)}</b>
            </FootText>
          </MetricFoot>
        </Metric>
      </MetricsBar>
    </div>
  )
}

const DAY_MS = 86_400_000
/**
 * 后端 `get_daily_summaries()`（src/model/usage.rs:738）按 CST(UTC+8) 聚合，
 * 前端日期窗口必须同口径 —— 按 UTC 推算会让 CST 00:00–08:00 的当日数据落在窗口外。
 */
const CST_OFFSET_MS = 8 * 3_600_000

/** daysAgo 天前的 CST 日历日（YYYY-MM-DD） */
export function cstDateString(daysAgo: number): string {
  const cstMidnightMs = Math.floor((Date.now() + CST_OFFSET_MS) / DAY_MS) * DAY_MS
  return new Date(cstMidnightMs - daysAgo * DAY_MS).toISOString().slice(0, 10)
}

export interface DayRow {
  date: string
  credits: number
  cost: number
  requests: number
}

/**
 * 区间内每一天的零填充序列（升序）。
 *
 * 有意不做 useMemo —— 结果依赖「当前时刻」，memo 会把窗口冻结在首次渲染，
 * 跨 CST 午夜后滞后一天。`useDailyUsage` 的 refetchInterval 为 60s，重算成本可忽略。
 */
export function buildWindow(data: DailySummary[] | undefined, days: number, offset: number): DayRow[] {
  const byDate = new Map((data ?? []).map((d) => [d.date, d]))
  return Array.from({ length: days }, (_, i) => {
    const date = cstDateString(offset + days - 1 - i)
    const row = byDate.get(date)
    return {
      date,
      credits: row?.totalCredits ?? 0,
      cost: row?.totalCost ?? 0,
      requests: row?.totalRequests ?? 0,
    }
  })
}

/** 按 UTC 解析 + 按 UTC 取星期，避免浏览器本地时区把 CST 日历日挪一天 */
export function weekdayLabel(date: string): string {
  return new Date(`${date}T00:00:00Z`).toLocaleDateString(localeTag(), { weekday: 'short', timeZone: 'UTC' })
}

/** 起始 / 结束两个日期输入框共用的样式（改一处即两边同步） */
const DATE_INPUT_CLASS =
  'h-9 rounded-[7px] border border-transparent bg-surface-2 px-3 font-mono text-[12.5px] text-ink shadow-hair outline-none focus-visible:border-brand'

/**
 * 自定义区间日期对话框：两个原生 date input（YYYY-MM-DD 由浏览器保证）+ 应用/取消。
 * 非法（任一为空或起始>结束）时点应用显示红字提示、不关弹窗不发请求；
 * 取消/遮罩/ESC 由 onOpenChange(false) 上抛，调用方 range 不变即回退前一 tab。
 * 重开时用上次已应用区间回填（切走再切回不重弹窗，但重开保留上一次选择）。
 */
export function CustomRangeDialog({
  open,
  onOpenChange,
  onApply,
  initial,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onApply: (range: CustomRange) => void
  initial: CustomRange | null
}) {
  const { t } = useTranslation()
  const [from, setFrom] = useState('')
  const [to, setTo] = useState('')
  const [touched, setTouched] = useState(false)
  // 区间不可延伸到未来（未来日零填充会稀释均值、误导零消耗统计）
  const todayCst = cstDateString(0)

  // 每次打开时回填上次已应用区间（关闭不清，避免应用前的输入在轮询重渲染间丢失）
  const [wasOpen, setWasOpen] = useState(false)
  if (open && !wasOpen) {
    setWasOpen(true)
    setFrom(initial?.from ?? cstDateString(0))
    setTo(initial?.to ?? cstDateString(0))
    setTouched(false)
  } else if (!open && wasOpen) {
    setWasOpen(false)
  }

  const invalid = from === '' || to === '' || from > to || to > todayCst

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t('dailyStats.customDialogTitle')}</DialogTitle>
        </DialogHeader>

        <div className="grid grid-cols-1 gap-4 py-2 sm:grid-cols-2">
          <label className="flex flex-col gap-1.5">
            <span className="text-[12px] font-medium text-ink-2">{t('dailyStats.customFromLabel')}</span>
            <input
              type="date"
              value={from}
              max={todayCst}
              onChange={(e) => {
                setFrom(e.target.value)
                setTouched(true)
              }}
              className={DATE_INPUT_CLASS}
            />
          </label>
          <label className="flex flex-col gap-1.5">
            <span className="text-[12px] font-medium text-ink-2">{t('dailyStats.customToLabel')}</span>
            <input
              type="date"
              value={to}
              max={todayCst}
              onChange={(e) => {
                setTo(e.target.value)
                setTouched(true)
              }}
              className={DATE_INPUT_CLASS}
            />
          </label>
        </div>

        {/* 仅在用户改过且非法时提示（初次打开的空输入不红字轰炸）。
            未来日优先单独提示；其余非法组合走通用提示，二者互斥只出一条 */}
        {touched && to > todayCst && (
          <p className="text-[11.5px] text-danger" role="alert">
            {t('dailyStats.customFutureRange')}
          </p>
        )}
        {touched && to <= todayCst && (from === '' || to === '' || from > to) && (
          <p className="text-[11.5px] text-danger" role="alert">
            {t('dailyStats.customInvalidRange')}
          </p>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button
            disabled={invalid}
            onClick={() => {
              if (invalid) return
              onApply({ from, to })
            }}
          >
            {t('dailyStats.customApply')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
