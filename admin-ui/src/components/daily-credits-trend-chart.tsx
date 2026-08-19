// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useId } from 'react'
import { useTranslation } from 'react-i18next'

export interface TrendPoint {
  /** CST(UTC+8) 日历日，格式 YYYY-MM-DD */
  date: string
  credits: number
}

/** Y 轴 4 条刻度线 = 3 等分（设计稿 top: 0 / 33.33% / 66.67% / 100%） */
const AXIS_DIVISIONS = 3
/** 点数超过此值时 X 轴标签抽稀，否则 30 天档标签必然重叠 */
const XLABEL_DENSE_THRESHOLD = 20
const XLABEL_STEP = 3
/** 数值标注只给前 2 名有消耗日（设计稿 2 个 .plot-val） */
const ANNOTATED_COUNT = 2

/** 上界取「好数」并保证能被 3 整除；全零区间取 1，避免 credits/axisMax 除零 */
function niceAxisMax(value: number): number {
  if (value <= 0) return 1
  const rawStep = value / AXIS_DIVISIONS
  const magnitude = Math.pow(10, Math.floor(Math.log10(rawStep)))
  const normalized = rawStep / magnitude
  const nice = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10
  return nice * magnitude * AXIS_DIVISIONS
}

function formatAxisValue(value: number): string {
  if (value === 0) return '0'
  // 小量级区间（axisMax < 1）保留 4 位，否则 0.003 会显示成 0.00
  return String(Number(value.toFixed(value >= 1 ? 2 : 4)))
}

const pct = (n: number) => `${n.toFixed(3)}%`

/**
 * 积分消耗趋势（设计稿 .chart-card）。
 *
 * 几何走 SVG（`preserveAspectRatio="none"` 非等比拉伸 + `vector-effect="non-scaling-stroke"`
 * 保线宽），文字 / 圆点 / 区间带走百分比绝对定位的 HTML，避免字号与圆点被一起压扁。
 */
export function DailyCreditsTrendChart({ points }: { points: TrendPoint[] }) {
  const { t } = useTranslation()
  // useId() 产生形如 `:r1:` 的 id，冒号在 SVG `url(#…)` 引用上存在跨浏览器风险，须剔除
  const gradientId = `credits-trend-${useId().replace(/:/g, '')}`

  const lastIndex = points.length - 1
  // 点数 < 2 时 i / lastIndex 得到 NaN / Infinity，整段路径退化，改出空态
  const plottable = points.length >= 2
  const axisMax = niceAxisMax(plottable ? Math.max(...points.map((p) => p.credits)) : 0)

  const coords = plottable
    ? points.map((p, i) => ({ ...p, x: (i / lastIndex) * 100, bottom: (p.credits / axisMax) * 100 }))
    : []
  const linePath = coords
    .map((c, i) => `${i === 0 ? 'M' : 'L'}${c.x.toFixed(3)} ${(100 - c.bottom).toFixed(3)}`)
    .join(' ')

  // 零消耗日不落点（设计稿）；全零时 annotated 为空 → 不渲染 .is-peak 与 .plot-val
  const active = coords.filter((c) => c.credits > 0)
  const annotated = [...active].sort((a, b) => b.credits - a.credits).slice(0, ANNOTATED_COUNT)
  const peakDate = annotated[0]?.date ?? null

  // 从区间末尾往前数的连续零消耗天数；≥2 天才画带（单日不成「区间」）
  let zeroStreak = 0
  for (let i = lastIndex; i >= 0 && points[i].credits === 0; i--) zeroStreak++
  const bandStart = points.length - zeroStreak
  const band =
    plottable && zeroStreak >= 2
      ? { left: (bandStart / lastIndex) * 100, labelLeft: (((bandStart + lastIndex) / 2) / lastIndex) * 100 }
      : null

  const gridTops = Array.from({ length: AXIS_DIVISIONS + 1 }, (_, i) => (i / AXIS_DIVISIONS) * 100)
  const dense = points.length > XLABEL_DENSE_THRESHOLD
  // 抽稀时保证末点与峰值日不被跳过
  const xLabels = coords.filter(
    (c, i) => !dense || i % XLABEL_STEP === 0 || i === lastIndex || c.date === peakDate,
  )

  return (
    <section className="shrink-0 overflow-hidden rounded-[11px] border border-hairline bg-surface shadow-panel">
      {/* 设计稿 .chart-head */}
      <div className="flex flex-wrap items-center gap-2.5 px-4 pt-[13px]">
        <div>
          <div className="text-[13px] font-semibold tracking-[-.01em]">{t('dailyStats.trendTitle')}</div>
          <div className="text-[11px] text-ink-3">{t('dailyStats.chartSub')}</div>
        </div>
        <div className="ml-auto flex flex-wrap items-center gap-x-[13px] gap-y-1 text-[11px] text-ink-3">
          <span className="flex items-center gap-[5px]">
            <i aria-hidden="true" className="block size-[9px] rounded-[2px] bg-brand" />
            {t('dailyStats.legendDaily')}
          </span>
          <span className="flex items-center gap-[5px]">
            <i aria-hidden="true" className="block size-[9px] rounded-[2px] border border-hairline-2 bg-surface-3" />
            {t('dailyStats.legendZeroBand')}
          </span>
        </div>
      </div>

      {/* 设计稿 .chart-body */}
      <div className="px-4 pb-3.5 pt-2.5">
        {!plottable ? (
          <div className="grid h-[184px] place-items-center text-[12.5px] text-ink-3">
            {t('dailyStats.chartEmpty')}
          </div>
        ) : (
          <>
            {/* 设计稿 .plot；ml-[44px] 给 .plot-y 让位（其 left:-44px 挂在此坐标系上） */}
            <div className="relative ml-[44px] h-[184px]">
              <div className="plot-grid">
                {gridTops.slice(0, -1).map((top) => (
                  <i key={top} style={{ top: pct(top) }} />
                ))}
                <i className="base" style={{ bottom: 0 }} />
              </div>

              {gridTops.map((top) => (
                <span key={top} className="plot-y" style={{ top: pct(top) }}>
                  {formatAxisValue(axisMax * (1 - top / 100))}
                </span>
              ))}

              {band && (
                <>
                  <div className="plot-band" style={{ left: pct(band.left), right: 0 }} />
                  <span
                    className="absolute top-[9px] z-[2] -translate-x-1/2 whitespace-nowrap text-[10.5px] text-ink-3"
                    style={{ left: pct(band.labelLeft) }}
                  >
                    {t('dailyStats.zeroBandLabel', { days: zeroStreak })}
                  </span>
                </>
              )}

              <svg
                viewBox="0 0 100 100"
                preserveAspectRatio="none"
                className="absolute inset-0 z-[1] h-full w-full overflow-visible"
                aria-hidden="true"
              >
                <defs>
                  <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0" stopColor="var(--brand)" stopOpacity=".26" />
                    <stop offset="1" stopColor="var(--brand)" stopOpacity="0" />
                  </linearGradient>
                </defs>
                <path d={`${linePath}L100 100L0 100Z`} fill={`url(#${gradientId})`} />
                <path
                  d={linePath}
                  fill="none"
                  stroke="var(--brand)"
                  strokeWidth="1.7"
                  vectorEffect="non-scaling-stroke"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>

              {/* 设计稿 .plot-dot / .plot-dot.is-peak */}
              {active.map((c) => (
                <span
                  key={c.date}
                  className={`absolute z-[3] -translate-x-1/2 translate-y-1/2 rounded-full border-brand bg-surface ${
                    c.date === peakDate
                      ? 'size-[9px] border-[2.5px] shadow-[0_0_0_4px_var(--brand-soft)]'
                      : 'size-[7px] border-2'
                  }`}
                  style={{ left: pct(c.x), bottom: pct(c.bottom) }}
                />
              ))}

              {/* 设计稿 .plot-val */}
              {annotated.map((c) => (
                <span
                  key={c.date}
                  className="absolute z-[3] whitespace-nowrap font-mono text-[10.5px] font-semibold tabular-nums text-brand [transform:translate(-50%,-9px)]"
                  style={{ left: pct(c.x), bottom: pct(c.bottom) }}
                >
                  {c.credits.toFixed(2)}
                </span>
              ))}
            </div>

            <div className="plot-x">
              {xLabels.map((c) => (
                <span key={c.date} className={c.date === peakDate ? 'on' : undefined} style={{ left: pct(c.x) }}>
                  {c.date.slice(5)}
                </span>
              ))}
            </div>
          </>
        )}
      </div>
    </section>
  )
}
