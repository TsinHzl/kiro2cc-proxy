// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useId, type ReactNode } from 'react'

/**
 * 设计稿 `.metrics` / `.metric` 系视觉原语（design.md 决策 8）。
 *
 * 只承载展示，不含任何领域字段与阈值语义 —— 取色阈值（如「剩余越高越好」与
 * 「占用越高越坏」）留在调用方，通过 `Ring` 的 `tone` 参数注入。
 */

/** 指标条外壳（设计稿 .metrics）；shrink-0 对应 .metrics{flex:none}，避免挂进 flex 容器后被压缩 */
export function MetricsBar({ cols = 4, children }: { cols?: 3 | 4 | 5; children: ReactNode }) {
  // 动态拼接类名会被 Tailwind purge，故走白名单三元
  const grid = cols === 5 ? 'grid-cols-5' : cols === 3 ? 'grid-cols-3' : 'grid-cols-4'
  return (
    <section className={`grid shrink-0 ${grid} overflow-hidden rounded-[11px] border border-hairline bg-surface shadow-hair`}>
      {children}
    </section>
  )
}

/** 指标段外壳（设计稿 .metric）：before 伪元素 + first:before:hidden 等价于 .metric + .metric::before */
export function Metric({ label, onClick, children }: { label: string; onClick?: () => void; children: ReactNode }) {
  const shell =
    "relative flex flex-col gap-0.5 px-4 pb-3.5 pt-[13px] text-left before:absolute before:bottom-3 before:left-0 before:top-3 before:w-px before:bg-hairline before:content-[''] first:before:hidden"
  const inner = (
    <>
      <div className="text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3">{label}</div>
      {children}
    </>
  )
  // 可点击段落渲染为 button（设计稿本身无 hover 态，此处为保留既有跳转交互）
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
export function MetricAside({ children }: { children: ReactNode }) {
  return <div className="absolute right-[14px] top-1/2 -translate-y-1/2">{children}</div>
}

/** 主值行（设计稿 .m-row + .m-val.mono + .m-unit；tabular-nums 对应 .mono 的 font-variant-numeric） */
export function MetricValue({
  value,
  unit,
  trailing,
  /** 主值着色类名（如 text-danger）；阈值判定属领域语义，留在调用方 */
  valueClass = '',
}: {
  value: string
  unit?: string
  trailing?: ReactNode
  valueClass?: string
}) {
  return (
    <div className="mt-px flex items-baseline gap-[7px]">
      <span className={`font-mono text-[23px] font-semibold leading-[1.1] tracking-[-.03em] tabular-nums ${valueClass}`}>
        {value}
      </span>
      {unit && <span className="text-[11.5px] font-medium text-ink-3">{unit}</span>}
      {trailing}
    </div>
  )
}

/** 脚注行（设计稿 .m-foot）；className 用于按需追加 truncate / pr-* 让位给 .m-aside */
export function MetricFoot({ className = '', children }: { className?: string; children: ReactNode }) {
  return <div className={`mt-[3px] flex items-center gap-1.5 text-[11px] text-ink-3 ${className}`}>{children}</div>
}

/** 脚注分隔（设计稿 .m-foot .sep） */
export function FootSep() {
  return <span aria-hidden="true" className="h-[9px] w-px shrink-0 bg-hairline-2" />
}

const RING_CIRCUMFERENCE = 106.8 // 2π × r(17)，与设计稿 stroke-dasharray 同值

/**
 * 42px 环形进度（设计稿 .ring）；纯装饰，百分数应由主值文本给出。
 * tone 传 stroke-* 类名 —— 阈值判定属领域语义，不在原语内做。
 */
export function Ring({ percent, tone = 'stroke-brand' }: { percent: number | null; tone?: string }) {
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
          className={tone}
        />
      )}
    </svg>
  )
}

const SPARK_W = 74
const SPARK_H = 26
const SPARK_PAD = 3.3 // 与设计稿路径的上下留白一致，避免 1.6px 描边被裁切

/** 74×26 折线图（设计稿 .spark）；纯装饰 */
export function Sparkline({ values }: { values: number[] }) {
  // useId() 产生形如 `:r1:` 的 id，冒号在 SVG `url(#…)` 引用上存在跨浏览器风险，须剔除
  const gradientId = `spark-${useId().replace(/:/g, '')}`
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
export function Delta({ percent }: { percent: number }) {
  const flat = Math.abs(percent) < 0.05
  const tone = flat ? 'bg-surface-3 text-ink-3' : percent > 0 ? 'bg-ok-soft text-ok' : 'bg-danger-soft text-danger'
  return (
    <span className={`inline-flex items-center gap-0.5 rounded-[5px] px-[5px] py-px text-[10.5px] font-semibold tabular-nums ${tone}`}>
      {flat ? '—' : percent > 0 ? '↑' : '↓'} {Math.abs(percent).toFixed(1)}%
    </span>
  )
}
