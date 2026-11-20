// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useId, type ReactNode } from 'react'

/** 4 列统一通栏指标条 */
export function MetricsBar({ children }: { children: ReactNode }) {
  return (
    <section
      className="grid shrink-0 grid-cols-2 overflow-hidden rounded-[14px] border border-transparent bg-chat-canvas shadow-hair md:grid-cols-4 [&>*:last-child_.metric-divider]:hidden max-md:[&>*:nth-child(2n)_.metric-divider]:hidden max-md:[&>*:nth-child(-n+2)]:border-b-[0.5px] max-md:[&>*:nth-child(-n+2)]:border-hairline"
    >
      {children}
    </section>
  )
}

/** 指标段外壳：与 admin-ui 保持统一视觉语言 */
export function Metric({
  label,
  icon,
  children,
}: {
  label: string
  icon?: ReactNode
  children: ReactNode
}) {
  return (
    <div className="relative grid grid-rows-[auto_1fr_auto] gap-2 px-5 py-4 text-left">
      <div className="flex items-center gap-2 text-[12.5px] font-medium text-ink-2">
        {icon && (
          <span className="grid size-[22px] shrink-0 place-items-center rounded-[6px] bg-surface-3/80 text-ink-3 [&>svg]:h-[13px] [&>svg]:w-[13px]">
            {icon}
          </span>
        )}
        <span className="min-w-0 truncate">{label}</span>
      </div>
      {children}
      <span
        aria-hidden="true"
        className="metric-divider absolute bottom-[14px] right-0 top-[14px] w-[0.5px] bg-hairline"
      />
    </div>
  )
}

export function MetricValue({
  value,
  unit,
  valueClass = '',
}: {
  value: string
  unit?: string
  valueClass?: string
}) {
  return (
    <div className="flex items-baseline gap-1.5 self-center whitespace-nowrap">
      <span
        className={`text-[26px] font-bold leading-[1.1] tracking-[-0.02em] tabular-nums ${valueClass}`}
        title={value}
      >
        {value}
      </span>
      {unit && <span className="text-[12.5px] font-medium text-ink-3">{unit}</span>}
    </div>
  )
}

export function MetricFoot({ className = '', children }: { className?: string; children: ReactNode }) {
  return (
    <div className={`flex items-center gap-2 truncate text-[12px] text-ink-3 ${className}`}>
      {children}
    </div>
  )
}

/** 环形进度（渐变描边 + 光晕） */
export function Ring({
  percent,
  size = 56,
  children,
}: {
  percent: number | null
  size?: 48 | 56 | 64
  children?: ReactNode
}) {
  const gradientId = `ring-${useId().replace(/:/g, '')}`
  const clamped = percent === null ? 0 : Math.min(100, Math.max(0, percent))
  const geo =
    size === 64
      ? { vb: 64, cx: 32, r: 25, circ: 157.08 }
      : size === 56
        ? { vb: 56, cx: 28, r: 22, circ: 138.23 }
        : { vb: 48, cx: 24, r: 19, circ: 119.38 }
  const sizeClass =
    size === 64 ? 'h-[64px] w-[64px]' : size === 56 ? 'h-[56px] w-[56px]' : 'h-[48px] w-[48px]'
  return (
    <div className={`relative flex shrink-0 items-center justify-center ${sizeClass}`}>
      <svg viewBox={`0 0 ${geo.vb} ${geo.vb}`} className="h-full w-full -rotate-90" aria-hidden="true">
        <defs>
          <linearGradient id={gradientId} x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" stopColor="var(--ring-grad-from)" />
            <stop offset="1" stopColor="var(--ring-grad-to)" />
          </linearGradient>
        </defs>
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
            stroke={`url(#${gradientId})`}
            style={{ filter: 'drop-shadow(0 0 3px var(--ring-glow))' }}
          />
        )}
      </svg>
      {children && (
        <div className="absolute inset-0 flex flex-col items-center justify-center text-center">{children}</div>
      )}
    </div>
  )
}
