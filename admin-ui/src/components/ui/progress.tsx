import * as React from 'react'
import { cn } from '@/lib/utils'

interface ProgressProps extends React.HTMLAttributes<HTMLDivElement> {
  value?: number
  max?: number
}

const Progress = React.forwardRef<HTMLDivElement, ProgressProps>(
  ({ className, value = 0, max = 100, ...props }, ref) => {
    const percentage = Math.min(Math.max((value / max) * 100, 0), 100)

    return (
      <div
        ref={ref}
        className={cn(
          // 设计稿 .bar：4px 高 / 3px 圆角 / track 底 / 无边框
          'relative h-1 w-full overflow-hidden rounded-[3px] bg-track',
          className
        )}
        {...props}
      >
        <div
          className={cn(
            'h-full rounded-[3px] transition-all',
            percentage > 80 ? 'bg-danger' : percentage > 60 ? 'bg-warn' : 'bg-ok'
          )}
          style={{ width: `${percentage}%` }}
        />
      </div>
    )
  }
)
Progress.displayName = 'Progress'

export { Progress }
