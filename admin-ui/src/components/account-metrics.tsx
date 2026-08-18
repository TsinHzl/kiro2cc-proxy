// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useId, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

export interface AccountMetricsProps {
  /** 账号总数 */
  total: number
  /** 未禁用账号数 */
  enabledCount: number
  /** 已禁用账号数 */
  disabledCount: number
  /** 异常账号数（deriveAccountState 为 error / warning；禁用与待查询不计入） */
  abnormalCount: number
  /** 已查询账号的剩余积分合计；null = 本会话尚无任何余额结果 */
  creditsTotal: number | null
  /** 已查询到余额的账号数 */
  creditsQueried: number
  /** 启用且已查询余额账号的平均剩余百分比；null = 无数据 */
  avgRemainingPercent: number | null
  /** 剩余百分比最低的启用账号；null = 无数据 */
  lowestAccount: { name: string; percent: number } | null
  /** 今日调用次数；null = 日用量数据未加载 */
  todayRequests: number | null
  /** 今日相对昨日的环比百分比；null = 无昨日基线 */
  requestsDeltaPercent: number | null
  /** 最近 7 天调用次数（日期升序）；不足 2 点不渲染 sparkline */
  requestTrend: number[]
  /** 账号池累计失败数 */
  cumulativeFailures: number
  /** 账号池累计失败率（百分比）；null = 尚无任何调用 */
  cumulativeFailureRate: number | null
  /** 点击「今日调用」段进入日用量列表 */
  onTodayClick: () => void
}

/** 剩余百分比按 60/30 阈值切色（环形图与脚注同源；设计稿把最低值硬编码为 warn，此处改为阈值联动） */
function remainingTone(percent: number) {
  if (percent >= 60) return { stroke: 'stroke-ok', text: 'text-ok' }
  if (percent >= 30) return { stroke: 'stroke-warn', text: 'text-warn' }
  return { stroke: 'stroke-danger', text: 'text-danger' }
}

/** 脚注分隔（设计稿 .m-foot .sep） */
function FootSep() {
  return <span aria-hidden="true" className="h-[9px] w-px shrink-0 bg-hairline-2" />
}

/** 指标段外壳（设计稿 .metric）：before 伪元素 + first:before:hidden 等价于 .metric + .metric::before */
function Metric({ label, onClick, children }: { label: string; onClick?: () => void; children: ReactNode }) {
  const shell =
    "relative flex flex-col gap-0.5 px-4 pb-3.5 pt-[13px] text-left before:absolute before:bottom-3 before:left-0 before:top-3 before:w-px before:bg-hairline before:content-[''] first:before:hidden"
  const inner = (
    <>
      <div className="text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3">{label}</div>
      {children}
    </>
  )
  // 可点击段落渲染为 button（保留旧统计卡片进入日用量的交互，设计稿本身无 hover 态）
  return onClick ? (
    <button
      type="button"
      onClick={onClick}
      className={`${shell} transition-colors hover:bg-surface-3 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand`}
    >
      {inner}
    </button>
  ) : (
    <div className={shell}>{inner}</div>
  )
}

/** 段落右侧浮层（设计稿 .m-aside） */
function MetricAside({ children }: { children: ReactNode }) {
  return <div className="absolute right-[14px] top-1/2 -translate-y-1/2">{children}</div>
}

/** 主值行（设计稿 .m-row + .m-val.mono + .m-unit；tabular-nums 对应 .mono 的 font-variant-numeric） */
function MetricValue({ value, unit, trailing }: { value: string; unit?: string; trailing?: ReactNode }) {
  return (
    <div className="mt-px flex items-baseline gap-[7px]">
      <span className="font-mono text-[23px] font-semibold leading-[1.1] tracking-[-.03em] tabular-nums">{value}</span>
      {unit && <span className="text-[11.5px] font-medium text-ink-3">{unit}</span>}
      {trailing}
    </div>
  )
}

const RING_CIRCUMFERENCE = 106.8 // 2π × r(17)，与设计稿 stroke-dasharray 同值

/** 42px 环形进度（设计稿 .ring）；纯装饰，百分数已由主值文本给出 */
function Ring({ percent }: { percent: number | null }) {
  const clamped = percent === null ? 0 : Math.min(100, Math.max(0, percent))
  return (
    <svg viewBox="0 0 42 42" className="h-[42px] w-[42px] -rotate-90" aria-hidden="true">
      <circle cx="21" cy="21" r="17" fill="none" strokeWidth="4.5" className="stroke-track" />
      {percent !== null && (
        <circle
          cx="21"
          cy="21"
          r="17"
          fill="none"
          strokeWidth="4.5"
          strokeLinecap="round"
          strokeDasharray={RING_CIRCUMFERENCE}
          strokeDashoffset={RING_CIRCUMFERENCE * (1 - clamped / 100)}
          className={remainingTone(clamped).stroke}
        />
      )}
    </svg>
  )
}

const SPARK_W = 74
const SPARK_H = 26
const SPARK_PAD = 3.3 // 与设计稿路径的上下留白一致，避免 1.6px 描边被裁切

/** 74×26 折线图（设计稿 .spark）；纯装饰 */
function Sparkline({ values }: { values: number[] }) {
  const gradientId = useId()
  // 自保：单点时 (i / (length-1)) 会得到 NaN，画出非法 path
  if (values.length < 2) return null
  const max = Math.max(...values)
  const min = Math.min(...values)
  const span = max - min
  const points = values.map((v, i) => ({
    x: (i / (values.length - 1)) * SPARK_W,
    // 全平（span=0）时走中线，避免除零
    y: span === 0 ? SPARK_H / 2 : SPARK_PAD + (1 - (v - min) / span) * (SPARK_H - SPARK_PAD * 2),
  }))
  const line = points.map((p, i) => `${i === 0 ? 'M' : 'L'}${p.x.toFixed(1)} ${p.y.toFixed(1)}`).join(' ')
  const last = points[points.length - 1]
  return (
    <svg viewBox={`0 0 ${SPARK_W} ${SPARK_H}`} preserveAspectRatio="none" className="h-[26px] w-[74px] overflow-visible" aria-hidden="true">
      <defs>
        <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="var(--brand)" stopOpacity=".22" />
          <stop offset="1" stopColor="var(--brand)" stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d={`${line} L${SPARK_W} ${SPARK_H} L0 ${SPARK_H}Z`} fill={`url(#${gradientId})`} />
      <path d={line} fill="none" stroke="var(--brand)" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
      <circle cx={last.x} cy={last.y} r="2.4" fill="var(--brand)" />
    </svg>
  )
}

/** 环比徽标（设计稿 .delta.up/.flat/.dn） */
function Delta({ percent }: { percent: number }) {
  const flat = Math.abs(percent) < 0.05
  const tone = flat ? 'bg-surface-3 text-ink-3' : percent > 0 ? 'bg-ok-soft text-ok' : 'bg-danger-soft text-danger'
  return (
    <span className={`inline-flex items-center gap-0.5 rounded-[5px] px-[5px] py-px text-[10.5px] font-semibold tabular-nums ${tone}`}>
      {flat ? '—' : percent > 0 ? '↑' : '↓'} {Math.abs(percent).toFixed(1)}%
    </span>
  )
}

/** 指标条（设计稿 .metrics）：账号池 / 全局剩余积分 / 启用账号平均剩余 / 今日调用 */
export function AccountMetrics({
  total,
  enabledCount,
  disabledCount,
  abnormalCount,
  creditsTotal,
  creditsQueried,
  avgRemainingPercent,
  lowestAccount,
  todayRequests,
  requestsDeltaPercent,
  requestTrend,
  cumulativeFailures,
  cumulativeFailureRate,
  onTodayClick,
}: AccountMetricsProps) {
  const { t } = useTranslation()
  // 展示层兜底：上游若给出 NaN / Infinity，一律降级为「无数据」，绝不把 "NaN" 吐给用户
  const credits = Number.isFinite(creditsTotal) ? creditsTotal! : null
  const avg = Number.isFinite(avgRemainingPercent) ? avgRemainingPercent! : null
  // shrink-0 对应设计稿 .metrics{flex:none}，避免挂进 flex 容器后被压缩
  return (
    <section className="grid shrink-0 grid-cols-4 overflow-hidden rounded-[11px] border border-hairline bg-surface shadow-hair">
      <Metric label={t('credentials.metricPoolLabel')}>
        <MetricValue value={String(total)} unit={t('credentials.metricPoolUnit')} />
        <div className="mt-[3px] flex items-center gap-1.5 text-[11px] text-ink-3">
          <span>
            <b className="font-medium text-ink-2">{enabledCount}</b> {t('credentials.metricEnabled')}
          </span>
          <FootSep />
          <span>
            <b className="font-medium text-ink-2">{disabledCount}</b> {t('credentials.metricDisabled')}
          </span>
          {abnormalCount > 0 && (
            <>
              <FootSep />
              <span className="font-semibold text-warn">{t('credentials.metricAbnormal', { count: abnormalCount })}</span>
            </>
          )}
        </div>
      </Metric>

      <Metric label={t('credentials.metricCreditsLabel')}>
        <MetricValue value={credits === null ? '—' : credits.toFixed(1)} />
        <div className="mt-[3px] text-[11px] text-ink-3">
          {credits === null
            ? t('credentials.metricCreditsHint')
            : t('credentials.metricCreditsCovered', { queried: creditsQueried, total })}
        </div>
      </Metric>

      <Metric label={t('credentials.metricAvgLabel')}>
        <MetricValue value={avg === null ? '—' : String(Math.round(avg))} unit={avg === null ? undefined : '%'} />
        {lowestAccount && (
          <div className="mt-[3px] truncate pr-14 text-[11px] text-ink-3">
            {t('credentials.metricAvgLowest')} <b className="font-medium text-ink-2">{lowestAccount.name}</b>
            {' · '}
            <span className={`font-semibold ${remainingTone(lowestAccount.percent).text}`}>
              {Math.round(lowestAccount.percent)}%
            </span>
          </div>
        )}
        <MetricAside>
          <Ring percent={avg} />
        </MetricAside>
      </Metric>

      <Metric label={t('credentials.metricCallsLabel')} onClick={onTodayClick}>
        <MetricValue
          value={todayRequests === null ? '—' : todayRequests.toLocaleString()}
          trailing={requestsDeltaPercent === null ? undefined : <Delta percent={requestsDeltaPercent} />}
        />
        <div className="mt-[3px] flex items-center gap-1.5 truncate pr-[92px] text-[11px] text-ink-3">
          <span>
            {t('credentials.metricFailPrefix')}{' '}
            <b className={cumulativeFailures > 0 ? 'font-medium text-ink-2' : 'font-medium'}>{cumulativeFailures}</b>
          </span>
          {cumulativeFailureRate !== null && (
            <>
              <FootSep />
              <span>
                {t('credentials.metricFailRatePrefix')}{' '}
                <b className={cumulativeFailures > 0 ? 'font-medium text-ink-2' : 'font-medium'}>
                  {cumulativeFailureRate.toFixed(2)}%
                </b>
              </span>
            </>
          )}
        </div>
        {requestTrend.length >= 2 && (
          <MetricAside>
            <Sparkline values={requestTrend} />
          </MetricAside>
        )}
      </Metric>
    </section>
  )
}
