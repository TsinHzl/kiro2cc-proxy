import * as React from 'react'
import { cn } from '@/lib/utils'

interface ProgressProps extends React.HTMLAttributes<HTMLDivElement> {
  value?: number
  max?: number
}

/** 四档配色阈值边界（剩余百分比），取自 quota_progress_bar.html 设计稿；account-metrics.tsx 的脚注文案档位共用同一常量源 */
export const QUOTA_TIER_THRESHOLDS = { safe: 60, mod: 30, warn: 15 } as const

/** 剩余额度四档渐变配色，阈值与色值取自 quota_progress_bar.html 设计稿 */
export function quotaTone(remainingPct: number): { grad: string; badgeBg: string; badgeText: string } {
  if (remainingPct > QUOTA_TIER_THRESHOLDS.safe) {
    return { grad: 'var(--quota-safe-grad)', badgeBg: 'var(--quota-safe-badge-bg)', badgeText: 'var(--quota-safe-badge-text)' }
  }
  if (remainingPct > QUOTA_TIER_THRESHOLDS.mod) {
    return { grad: 'var(--quota-mod-grad)', badgeBg: 'var(--quota-mod-badge-bg)', badgeText: 'var(--quota-mod-badge-text)' }
  }
  if (remainingPct > QUOTA_TIER_THRESHOLDS.warn) {
    return { grad: 'var(--quota-warn-grad)', badgeBg: 'var(--quota-warn-badge-bg)', badgeText: 'var(--quota-warn-badge-text)' }
  }
  return { grad: 'var(--quota-danger-grad)', badgeBg: 'var(--quota-danger-badge-bg)', badgeText: 'var(--quota-danger-badge-text)' }
}

const Progress = React.forwardRef<HTMLDivElement, ProgressProps>(
  ({ className, value = 0, max = 100, ...props }, ref) => {
    const usedPct = Math.min(Math.max((value / max) * 100, 0), 100)
    const remainingPct = 100 - usedPct

    return (
      <div
        ref={ref}
        className={cn('relative h-1 w-full overflow-hidden rounded-[3px] bg-track', className)}
        {...props}
      >
        <div
          className="h-full rounded-[3px] transition-all"
          style={{ width: `${remainingPct}%`, backgroundImage: quotaTone(remainingPct).grad }}
        />
      </div>
    )
  }
)
Progress.displayName = 'Progress'

/** 消费额度百分比胶囊徽章：与 Progress 共用四档配色阈值 */
export function QuotaPercentBadge({ percent }: { percent: number }) {
  const usedPct = Math.min(Math.max(percent, 0), 100)
  const tone = quotaTone(100 - usedPct)
  return (
    <span
      className="inline-flex items-center rounded-full px-[7px] py-px font-mono text-[11px] font-semibold tabular-nums"
      style={{ background: tone.badgeBg, color: tone.badgeText }}
    >
      {usedPct.toFixed(0)}%
    </span>
  )
}

export { Progress }
