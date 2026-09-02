// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import { Activity, Coins, CreditCard, Hash, Wallet } from 'lucide-react'
import {
  Delta,
  FootSep,
  Metric,
  MetricBody,
  MetricFoot,
  MetricValue,
  MetricsBar,
  Ring,
  Sparkline,
  StatusDot,
} from '@/components/metrics'

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
  /** 已消费积分合计 Σ(usageLimit − remaining)（仅 usageLimit > 0 账号）；null = 无已查询账号 */
  consumedCreditsTotal: number | null
  /** 今日调用次数；null = 日用量数据未加载 */
  todayRequests: number | null
  /** 今日相对昨日的环比百分比；null = 无昨日基线 */
  requestsDeltaPercent: number | null
  /** 最近 7 天调用次数（日期升序）；不足 2 点不渲染 sparkline */
  requestTrend: number[]
  /** 今日消耗 credits；null = 日用量数据未加载 */
  todayCredits: number | null
  /** 今日经缓存命中节省的 credits，须为非负（后端偶发负值由调用方裁剪）；null = 日用量数据未加载 */
  todayCreditsSaved: number | null
  /** 今日 credits 相对昨日的环比百分比；null = 无昨日基线 */
  creditsDeltaPercent: number | null
  /** 最近 7 天消耗 credits（日期升序）；不足 2 点不渲染 sparkline */
  creditsTrend: number[]
  /** 账号池累计失败数 */
  cumulativeFailures: number
  /** 账号池累计失败率（百分比）；null = 尚无任何调用 */
  cumulativeFailureRate: number | null
  /** 点击「今日调用」段进入日用量列表 */
  onTodayClick: () => void
}

/** 剩余百分比按 60/30 阈值切色（仅用于脚注 label 映射；环形图统一走 brand/ok 渐变，不随阈值变色） */
function remainingTone(percent: number): 'ok' | 'warn' | 'danger' {
  if (percent >= 60) return 'ok'
  if (percent >= 30) return 'warn'
  return 'danger'
}

function remainingStatusLabel(t: (k: string) => string, percent: number): string {
  const tone = remainingTone(percent)
  if (tone === 'ok') return t('credentials.metricRemainingStatusAbundant')
  if (tone === 'warn') return t('credentials.metricRemainingStatusTight')
  return t('credentials.metricRemainingStatusCritical')
}

/** 指标条（方案1 unified-card-bar）：账号池 / 全局剩余积分 / 全局消费积分 / 今日调用 / 今日消耗积分 */
export function AccountMetrics({
  total,
  enabledCount,
  disabledCount,
  abnormalCount,
  creditsTotal,
  creditsQueried,
  avgRemainingPercent,
  consumedCreditsTotal,
  todayRequests,
  requestsDeltaPercent,
  requestTrend,
  todayCredits,
  todayCreditsSaved,
  creditsDeltaPercent,
  creditsTrend,
  cumulativeFailures,
  cumulativeFailureRate,
  onTodayClick,
}: AccountMetricsProps) {
  const { t } = useTranslation()
  // 展示层兜底：上游若给出 NaN / Infinity，一律降级为「无数据」，绝不把 "NaN" 吐给用户
  const credits = Number.isFinite(creditsTotal) ? creditsTotal! : null
  const avg = Number.isFinite(avgRemainingPercent) ? avgRemainingPercent! : null
  const consumed = Number.isFinite(consumedCreditsTotal) ? consumedCreditsTotal! : null
  const usedCredits = Number.isFinite(todayCredits) ? todayCredits! : null
  const savedCredits = Number.isFinite(todayCreditsSaved) ? todayCreditsSaved! : null
  return (
    <MetricsBar cols={5}>
      {/* 卡 1：账号池 */}
      <Metric label={t('credentials.metricPoolLabel')} icon={<Hash />}>
        <MetricBody>
          <MetricValue value={String(total)} unit={t('credentials.metricPoolUnit')} />
        </MetricBody>
        <MetricFoot>
          <StatusDot tone={abnormalCount > 0 ? 'muted' : 'ok'} />
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

      {/* 卡 2：全局剩余积分（环形图：avg 剩余百分比 + 充裕/紧张/告急 label） */}
      <Metric label={t('credentials.metricCreditsLabel')} icon={<Wallet />}>
        <MetricBody>
          <MetricValue value={credits === null ? '—' : credits.toFixed(1)} />
          {avg !== null && (
            <Ring percent={avg}>
              <span className="text-[13px] font-bold leading-none tracking-[-0.02em] tabular-nums text-ink">
                {Math.round(avg)}%
              </span>
              <span className="mt-px text-[9px] text-ink-3">{remainingStatusLabel(t, avg)}</span>
            </Ring>
          )}
        </MetricBody>
        <MetricFoot>
          {credits === null
            ? t('credentials.metricCreditsHint')
            : t('credentials.metricCreditsCovered', { queried: creditsQueried, total })}
        </MetricFoot>
      </Metric>

      {/* 卡 3：全局消费积分 */}
      <Metric label={t('credentials.metricConsumedLabel')} icon={<Coins />}>
        <MetricBody>
          <MetricValue value={consumed === null ? '—' : consumed.toFixed(1)} />
        </MetricBody>
        <MetricFoot>
          {consumed === null
            ? t('credentials.metricCreditsHint')
            : t('credentials.metricCreditsCovered', { queried: creditsQueried, total })}
        </MetricFoot>
      </Metric>

      {/* 卡 4：今日调用（sparkline: brand） */}
      <Metric label={t('credentials.metricCallsLabel')} icon={<Activity />} onClick={onTodayClick}>
        <MetricBody>
          <MetricValue
            value={todayRequests === null ? '—' : todayRequests.toLocaleString()}
            trailing={requestsDeltaPercent === null ? undefined : <Delta percent={requestsDeltaPercent} />}
          />
          {requestTrend.length >= 2 && <Sparkline values={requestTrend} tone="brand" />}
        </MetricBody>
        <MetricFoot className="truncate">
          <StatusDot tone={cumulativeFailures > 0 ? 'muted' : 'ok'} />
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
      </Metric>

      {/* 卡 5：今日消耗积分（sparkline: ok；与「今日调用」同源于日用量接口，故共用同一跳转） */}
      <Metric label={t('credentials.metricCreditsUsedLabel')} icon={<CreditCard />} onClick={onTodayClick}>
        <MetricBody>
          <MetricValue
            value={usedCredits === null ? '—' : usedCredits.toFixed(1)}
            trailing={creditsDeltaPercent === null ? undefined : <Delta percent={creditsDeltaPercent} />}
          />
          {creditsTrend.length >= 2 && <Sparkline values={creditsTrend} tone="ok" />}
        </MetricBody>
        <MetricFoot className="truncate">
          <span>
            {t('credentials.metricCreditsSavedPrefix')}{' '}
            <b className={savedCredits !== null && savedCredits > 0 ? 'font-medium text-ink-2' : 'font-medium'}>
              {savedCredits === null ? '—' : savedCredits.toFixed(1)}
            </b>
          </span>
        </MetricFoot>
      </Metric>
    </MetricsBar>
  )
}
