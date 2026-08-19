// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import { Delta, FootSep, Metric, MetricAside, MetricFoot, MetricValue, MetricsBar, Ring, Sparkline } from '@/components/metrics'

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
  return (
    <MetricsBar>
      <Metric label={t('credentials.metricPoolLabel')}>
        <MetricValue value={String(total)} unit={t('credentials.metricPoolUnit')} />
        <MetricFoot>
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
        </MetricFoot>
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
          <Ring percent={avg} tone={avg === null ? undefined : remainingTone(avg).stroke} />
        </MetricAside>
      </Metric>

      <Metric label={t('credentials.metricCallsLabel')} onClick={onTodayClick}>
        <MetricValue
          value={todayRequests === null ? '—' : todayRequests.toLocaleString()}
          trailing={requestsDeltaPercent === null ? undefined : <Delta percent={requestsDeltaPercent} />}
        />
        <MetricFoot className="truncate pr-[92px]">
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
        </MetricFoot>
        {requestTrend.length >= 2 && (
          <MetricAside>
            <Sparkline values={requestTrend} />
          </MetricAside>
        )}
      </Metric>
    </MetricsBar>
  )
}
