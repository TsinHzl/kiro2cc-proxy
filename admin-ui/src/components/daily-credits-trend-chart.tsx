// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Card, CardContent } from '@/components/ui/card'
import type { DailySummary } from '@/types/api'

interface DailyCreditsTrendChartProps {
  data: DailySummary[]
}

const DAYS = 14
const WIDTH = 720
const HEIGHT = 150
const PADDING = { top: 36, right: 16, bottom: 28, left: 40 }

// "今天"取当前 UTC 日历日，daysAgo 天前同样按 UTC 日历日回推，
// 与后端 DailySummary.date 的 UTC 语义保持一致，不依赖浏览器本地时区。
function utcDateString(daysAgo: number): string {
  const now = new Date()
  const todayUtcMs = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())
  return new Date(todayUtcMs - daysAgo * 86400000).toISOString().slice(0, 10)
}

function niceAxisMax(value: number): number {
  if (value <= 0) return 4
  const rawStep = value / 4
  const magnitude = Math.pow(10, Math.floor(Math.log10(rawStep)))
  const normalized = rawStep / magnitude
  const niceNormalized = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10
  return niceNormalized * magnitude * 4
}

function formatAxisValue(value: number): string {
  return value % 1 === 0 ? String(value) : value.toFixed(4)
}

export function DailyCreditsTrendChart({ data }: DailyCreditsTrendChartProps) {
  const { t } = useTranslation()
  const points = useMemo(() => {
    const byDate = new Map(data.map((d) => [d.date, d.totalCredits]))
    return Array.from({ length: DAYS }, (_, i) => {
      const date = utcDateString(DAYS - 1 - i)
      return { date, credits: byDate.get(date) ?? 0 }
    })
  }, [data])

  const maxCredits = Math.max(...points.map((p) => p.credits))
  const axisMax = niceAxisMax(maxCredits)

  const innerWidth = WIDTH - PADDING.left - PADDING.right
  const innerHeight = HEIGHT - PADDING.top - PADDING.bottom
  const xStep = innerWidth / (points.length - 1)

  const coords = points.map((p, i) => ({
    ...p,
    x: PADDING.left + i * xStep,
    y: PADDING.top + innerHeight - (p.credits / axisMax) * innerHeight,
  }))

  const linePath = coords.map((c, i) => `${i === 0 ? 'M' : 'L'} ${c.x} ${c.y}`).join(' ')
  const baseline = PADDING.top + innerHeight
  const areaPath = `${linePath} L ${coords[coords.length - 1].x} ${baseline} L ${coords[0].x} ${baseline} Z`

  const gridLines = [0, 1, 2, 3, 4].map((i) => ({
    value: (axisMax * i) / 4,
    y: PADDING.top + innerHeight - (i / 4) * innerHeight,
  }))

  return (
    <Card>
      <CardContent className="pt-4">
        <div className="text-sm font-medium mb-2">{t('dailyStats.trendTitle')}</div>
        <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} className="w-full">
          <defs>
            <linearGradient id="dailyCreditsAreaGradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="currentColor" stopOpacity={0.25} className="text-blue-500" />
              <stop offset="100%" stopColor="currentColor" stopOpacity={0} className="text-blue-500" />
            </linearGradient>
          </defs>

          {gridLines.map((g) => (
            <g key={g.value}>
              <line
                x1={PADDING.left}
                x2={WIDTH - PADDING.right}
                y1={g.y}
                y2={g.y}
                stroke="currentColor"
                strokeOpacity={0.1}
              />
              <text
                x={PADDING.left - 8}
                y={g.y}
                textAnchor="end"
                dominantBaseline="middle"
                fontSize={10}
                fill="currentColor"
                className="text-muted-foreground"
              >
                {formatAxisValue(g.value)}
              </text>
            </g>
          ))}

          <path d={areaPath} fill="url(#dailyCreditsAreaGradient)" />
          <path d={linePath} fill="none" stroke="currentColor" strokeWidth={2} className="text-blue-500" />

          {coords.map((c) => (
            <g key={c.date}>
              <circle cx={c.x} cy={c.y} r={3} fill="currentColor" className="text-blue-500" />
              <text
                x={c.x}
                y={c.y - 8}
                textAnchor="middle"
                fontSize={9}
                fill="currentColor"
                className="text-blue-600 dark:text-blue-400 font-medium"
              >
                {c.credits.toFixed(4)}
              </text>
              <text
                x={c.x}
                y={HEIGHT - 8}
                textAnchor="middle"
                fontSize={9}
                fill="currentColor"
                className="text-muted-foreground"
              >
                {c.date.slice(5)}
              </text>
            </g>
          ))}
        </svg>

        <div className="flex items-center gap-1.5 mt-1 text-xs text-muted-foreground">
          <span className="inline-block w-2 h-2 rounded-full bg-blue-500" />
          {t('dailyStats.creditsUsageLegend')}
        </div>
      </CardContent>
    </Card>
  )
}
