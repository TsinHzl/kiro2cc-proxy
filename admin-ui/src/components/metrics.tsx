// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useId, type ReactNode } from 'react'

/**
 * 方案1「现代化一体式通栏卡片条 (Unified Modern Card Bar)」视觉原语。
 * 对齐截图：13px label（非大写）、icon 在 label 左侧、主值 28px/700、脚注 12px。
 */

const BAR_COLS_5 = '1.1fr 1.35fr 1fr 1.25fr 1.1fr'
const BAR_COLS_3 = '1fr 1.3fr 1fr'
const BAR_COLS_4 = '1fr 1fr 1fr 1fr'

/** 指标条外壳（方案1 .unified-card-bar）；非等宽 fr 列走内联 style */
export function MetricsBar({ cols = 4, children }: { cols?: 3 | 4 | 5; children: ReactNode }) {
  const template = cols === 5 ? BAR_COLS_5 : cols === 3 ? BAR_COLS_3 : BAR_COLS_4
  return (
    <section
      className="grid shrink-0 overflow-hidden rounded-[16px] border border-hairline bg-surface shadow-hair [&>*:last-child_.metric-divider]:hidden"
      style={{ gridTemplateColumns: template }}
    >
      {children}
    </section>
  )
}

/** 指标段外壳（方案1 .metric-cell）：三段 flex-col justify-between + 右侧 divider span */
export function Metric({
  label,
  icon,
  onClick,
  children,
}: {
  label: string
  icon?: ReactNode
  onClick?: () => void
  children: ReactNode
}) {
  const shell = 'relative flex flex-col justify-between px-6 py-3.5 text-left'
  const inner = (
    <>
      {/* cell-header：icon 在 label 左侧，space-between 对齐 */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-1.5 text-[13px] font-medium text-ink-2">
          {icon && <span className="text-ink-3 [&>svg]:h-[14px] [&>svg]:w-[14px]">{icon}</span>}
          {label}
        </div>
      </div>
      {children}
      {/* cell ::after divider 等价：右侧 1px 竖线，top/bottom 18px；末列由 MetricsBar 的 last:hidden 自动隐藏 */}
      <span
        aria-hidden="true"
        className="metric-divider absolute bottom-[14px] right-0 top-[14px] w-px bg-hairline"
      />
    </>
  )
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

/** 段落右侧浮层（向后兼容：absolute 右侧垂直居中） */
export function MetricAside({ children }: { children: ReactNode }) {
  return <div className="absolute right-[14px] top-1/2 -translate-y-1/2">{children}</div>
}

/** 段落主体区（方案1 .cell-body）：主值与右侧图表内联并排 */
export function MetricBody({ children }: { children: ReactNode }) {
  return <div className="flex items-center justify-between gap-3">{children}</div>
}

/** 主值行（28px/700，对齐截图） */
export function MetricValue({
  value,
  unit,
  trailing,
  valueClass = '',
  unitClass = 'text-[13px] font-medium text-ink-3',
}: {
  value: string
  unit?: string
  trailing?: ReactNode
  valueClass?: string
  unitClass?: string
}) {
  return (
    <div className="flex items-baseline gap-1.5">
      <span
        className={`text-[28px] font-bold leading-[1.1] tracking-[-0.02em] tabular-nums ${valueClass}`}
      >
        {value}
      </span>
      {unit && <span className={unitClass}>{unit}</span>}
      {trailing}
    </div>
  )
}

/** 脚注行（12px，对齐截图） */
export function MetricFoot({ className = '', children }: { className?: string; children: ReactNode }) {
  return <div className={`flex items-center gap-2 text-[12px] text-ink-3 ${className}`}>{children}</div>
}

/** 脚注分隔 */
export function FootSep() {
  return <span aria-hidden="true" className="h-[9px] w-px shrink-0 bg-hairline-2" />
}

/** 状态点（方案1 .status-dot）：6px 圆点，ok/muted 二态 */
export function StatusDot({ tone = 'ok' }: { tone?: 'ok' | 'muted' }) {
  return (
    <span
      aria-hidden="true"
      className={`mr-0.5 inline-block h-1.5 w-1.5 shrink-0 rounded-full ${tone === 'ok' ? 'bg-ok' : 'bg-ink-3'}`}
    />
  )
}

/**
 * 环形进度（方案1 .ring-chart-wrap；旧页兼容 42px）。
 * - 传 `tone`（stroke-* 类名）→ 走纯色描边（旧页兼容）
 * - 不传 `tone` → 走方案1 渐变描边 + drop-shadow 光晕
 */
export function Ring({
  percent,
  tone,
  size = 64,
  children,
}: {
  percent: number | null
  tone?: string
  size?: 42 | 52 | 64
  children?: ReactNode
}) {
  const gradientId = `ring-${useId().replace(/:/g, '')}`
  const clamped = percent === null ? 0 : Math.min(100, Math.max(0, percent))
  // 几何参数：viewBox=size+4padding, cx=cy=viewBox/2, r=viewBox/2-5(边距), circumference=2πr
  const geo =
    size === 64
      ? { vb: 64, cx: 32, r: 25, circ: 157.08 }
      : size === 52
        ? { vb: 48, cx: 24, r: 19, circ: 119.38 }
        : { vb: 42, cx: 21, r: 17, circ: 106.8 }
  const sizeClass =
    size === 64 ? 'h-[64px] w-[64px]' : size === 52 ? 'h-[52px] w-[52px]' : 'h-[42px] w-[42px]'
  const useGradient = !tone
  return (
    <div className={`relative flex shrink-0 items-center justify-center ${sizeClass}`}>
      <svg
        viewBox={`0 0 ${geo.vb} ${geo.vb}`}
        className="h-full w-full -rotate-90"
        aria-hidden="true"
      >
        {useGradient && (
          <defs>
            <linearGradient id={gradientId} x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="var(--ring-grad-from)" />
              <stop offset="1" stopColor="var(--ring-grad-to)" />
            </linearGradient>
          </defs>
        )}
        <circle cx={geo.cx} cy={geo.cx} r={geo.r} fill="none" strokeWidth="4.5" className="stroke-track" />
        {percent !== null && (
          <circle
            cx={geo.cx}
            cy={geo.cx}
            r={geo.r}
            fill="none"
            strokeWidth="4.5"
            strokeLinecap="round"
            strokeDasharray={geo.circ}
            strokeDashoffset={geo.circ * (1 - clamped / 100)}
            stroke={useGradient ? `url(#${gradientId})` : undefined}
            className={tone}
            style={useGradient ? { filter: 'drop-shadow(0 0 3px var(--ring-glow))' } : undefined}
          />
        )}
      </svg>
      {children && (
        <div className="absolute inset-0 flex flex-col items-center justify-center text-center">{children}</div>
      )}
    </div>
  )
}

const SPARK_W = 80
const SPARK_H = 36
const SPARK_PAD = 3.3

/** 80×36 折线图（方案1 .sparkline-wrap）：Catmull-Rom 平滑贝塞尔 + 渐变区域 + 终点双光点 */
export function Sparkline({ values, tone = 'brand' }: { values: number[]; tone?: 'brand' | 'ok' }) {
  const gradientId = `spark-${useId().replace(/:/g, '')}`
  const strokeVar = tone === 'ok' ? 'var(--ok)' : 'var(--brand)'
  if (values.length < 2) return null
  const max = Math.max(...values)
  const min = Math.min(...values)
  const span = max - min
  const points = values.map((v, i) => ({
    x: (i / (values.length - 1)) * SPARK_W,
    y: span === 0 ? SPARK_H / 2 : SPARK_PAD + (1 - (v - min) / span) * (SPARK_H - SPARK_PAD * 2),
  }))
  let line = `M${points[0].x.toFixed(1)} ${points[0].y.toFixed(1)}`
  for (let i = 0; i < points.length - 1; i++) {
    const p0 = points[i === 0 ? 0 : i - 1]
    const p1 = points[i]
    const p2 = points[i + 1]
    const p3 = points[i + 2 < points.length ? i + 2 : i + 1]
    const c1x = p1.x + (p2.x - p0.x) / 6
    const c1y = p1.y + (p2.y - p0.y) / 6
    const c2x = p2.x - (p3.x - p1.x) / 6
    const c2y = p2.y - (p3.y - p1.y) / 6
    line += ` C${c1x.toFixed(1)} ${c1y.toFixed(1)} ${c2x.toFixed(1)} ${c2y.toFixed(1)} ${p2.x.toFixed(1)} ${p2.y.toFixed(1)}`
  }
  const last = points[points.length - 1]
  return (
    <svg
      viewBox={`0 0 ${SPARK_W} ${SPARK_H}`}
      preserveAspectRatio="none"
      className="h-[36px] w-[80px] shrink-0 overflow-visible"
      aria-hidden="true"
    >
      <defs>
        <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor={strokeVar} stopOpacity=".22" />
          <stop offset="1" stopColor={strokeVar} stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d={`${line} L${SPARK_W} ${SPARK_H} L0 ${SPARK_H}Z`} fill={`url(#${gradientId})`} />
      <path d={line} fill="none" stroke={strokeVar} strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
      <circle cx={last.x} cy={last.y} r="5" fill={strokeVar} opacity="0.3" />
      <circle cx={last.x} cy={last.y} r="3" fill={strokeVar} />
    </svg>
  )
}

/** 环比胶囊（方案1 .trend-badge）：rounded-[6px] px-[7px] py-0.5 */
export function Delta({ percent }: { percent: number }) {
  const flat = Math.abs(percent) < 0.05
  const tone = flat ? 'bg-surface-3 text-ink-3' : percent > 0 ? 'bg-danger-soft text-danger' : 'bg-ok-soft text-ok'
  return (
    <span className={`inline-flex items-center gap-0.5 rounded-[6px] px-[7px] py-0.5 text-[11px] font-semibold tabular-nums ${tone}`}>
      {flat ? '—' : percent > 0 ? '↑' : '↓'} {Math.abs(percent).toFixed(1)}%
    </span>
  )
}
