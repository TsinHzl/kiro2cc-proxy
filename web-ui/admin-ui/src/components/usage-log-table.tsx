// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import { Brain, DatabaseZap, Loader2, SearchX } from 'lucide-react'
import { CELL, TH_BASE } from '@/components/table-kit'
import { formatDateTime, formatTokenCount } from '@/lib/locale'
import type { GeoInfo } from '@/hooks/use-ip-geo'
import type { UsageRecord } from '@/types/api'

/**
 * 请求日志表（API Key 详情页 / 每日明细页共用）。
 *
 * 两页的列结构与单元格渲染完全一致，差异仅在数据来源与外层组装，
 * 故只提取表体；`.panel` 外壳、`.tscroll` 滚动容器与 `.panel-foot` 由调用方给出。
 */

/** 模型着色走设计令牌：无紫色令牌，opus 归 brand，sonnet 归 ok，haiku 走中性 ink-2 */
const MODEL_COLORS: Record<string, string> = {
  opus: 'text-brand',
  sonnet: 'text-ok',
  haiku: 'text-ink-2',
}

export function getModelColor(model: string): string {
  const lower = model.toLowerCase()
  for (const [key, cls] of Object.entries(MODEL_COLORS)) {
    if (lower.includes(key)) return cls
  }
  return 'text-ink-3'
}

export function formatCost(cost: number): string {
  return `$${cost.toFixed(4)}`
}

/** UA 短标识：取首个空格前段（如 claude-cli/2.1.5），完整原文走 title 悬停展示 */
function uaShort(ua: string): string {
  return ua.split(/\s+/)[0] || ua
}

/**
 * 缓存命中百分比胶囊的展示开关。
 *
 * meteringEvent 当前不返回 cache_read 真值，命中率由 credits 反推；反推在
 * 「实际 credits 高于全未命中基线」时被 clamp 成 0%，会把缓存写入轮误显成 0%。
 * 口径校准前先隐藏，改回 true 即可恢复（图标、函数、i18n 键均保留）。
 */
const SHOW_CACHE_HIT_RATE = false

/**
 * 缓存命中率 = 缓存读取 / 输入总计。
 *
 * 分母取 `inputTokens`（输入总计，本身已含缓存读取），与同一单元格第三、四行
 * 展示的「缓存读取」和「输入总计」同源，保证三个数字互相自洽。旧记录可能缺
 * `cacheReadInputTokens`（按 0 计）；`inputTokens` 为 0 时无意义，返回 null 不渲染。
 */
export function cacheHitRate(record: UsageRecord): number | null {
  const total = record.inputTokens
  if (!total || total <= 0) return null
  const read = record.cacheReadInputTokens ?? 0
  return Math.min(100, Math.max(0, (read / total) * 100))
}

/**
 * 上游未回传 `creditsUsed` 时的降级估算（USD → Kiro credits）。
 *
 * 仅用于旧记录：后端自 metering 事件落库后 `creditsUsed` 恒有值，
 * 界面上以 ✓ 区分「真实值」与本估算值。
 *
 * 两条路径按可用信息分工，不可互换：
 * - 已知具体模型 → `getKRef(model)`，与后端同档位
 * - 只有跨模型聚合总费用（汇总卡）→ `fallbackCredits`，退化为单一均值除数
 */
const USD_TO_CREDITS_RATE = 0.72

export function fallbackCredits(cost: number): number {
  return cost / USD_TO_CREDITS_RATE
}

/**
 * 模型档位换算率，逐字镜像后端 `src/model/usage.rs::get_k_ref`（各档为实测值）。
 * 判定顺序即优先级：新 opus → 旧 opus → 未知 opus/fable 兜底 → sonnet/haiku 默认。
 * 仅经 `recordCredits` 对外生效 —— 消费方一律走 `recordCredits(record)`，不直接取档位。
 */
function getKRef(model: string): number {
  const m = model.toLowerCase()
  if (
    m.includes('opus-4-7') ||
    m.includes('opus-4.7') ||
    m.includes('opus-4-8') ||
    m.includes('opus-4.8') ||
    m.includes('opus-5') ||
    m.includes('opus.5')
  ) {
    return 2.36
  }
  if (m.includes('opus-4-5') || m.includes('opus-4.5') || m.includes('opus-4-6') || m.includes('opus-4.6')) {
    return 1.9
  }
  if (m.includes('opus') || m.includes('fable')) {
    return 2.36
  }
  return 1.43
}

/** 单条记录的积分：优先真实值，缺失时按该记录模型的档位估算 */
export function recordCredits(record: UsageRecord): number {
  return record.creditsUsed ?? record.estimatedCost * getKRef(record.model)
}

/** 毫秒 → 「X.Xs」展示（≥60s 转分秒「Xm XXs」，先取整总秒数避免 60s 进位残留） */
function formatDurationMs(ms: number): string {
  const total = Math.round(ms / 1000)
  if (total >= 60) {
    return `${Math.floor(total / 60)}m ${String(total % 60).padStart(2, '0')}s`
  }
  return `${(ms / 1000).toFixed(1)}s`
}

/** 快慢分档阈值（ms）：首字 ≤1s 快 / ≥5s 慢，耗时 ≤10s 快 / ≥60s 慢 */
const FIRST_TOKEN_FAST_MS = 1_000
const FIRST_TOKEN_SLOW_MS = 5_000
const DURATION_FAST_MS = 10_000
const DURATION_SLOW_MS = 60_000

/** 快/中/慢三档底色（设计令牌，无新增自定义颜色） */
function durationTone(ms: number, fastMs: number, slowMs: number): string {
  if (ms <= fastMs) return 'bg-ok-soft text-ok'
  if (ms >= slowMs) return 'bg-warn-soft text-warn'
  return 'bg-surface-3 text-ink-2'
}

/** 徽章胶囊统一样式：细边框占宽度、小号加粗字号 */
const BADGE_CLS = 'rounded-[4px] border border-transparent px-1 text-[9.5px] font-semibold'

/** 「用时」单元格：流式双徽章（首字/耗时），非流式仅耗时，缺数据以 — 占位保证列宽稳定 */
export function DurationCell({ record }: { record: UsageRecord }) {
  const { t } = useTranslation()
  const first = record.firstTokenMs
  const dur = record.durationMs
  if (first == null && dur == null) return <span className="text-ink-3">—</span>
  return (
    <div className="flex flex-col items-start gap-px">
      {first != null && (
        <span
          className={`${BADGE_CLS} ${durationTone(first, FIRST_TOKEN_FAST_MS, FIRST_TOKEN_SLOW_MS)}`}
          title={t('common.firstTokenLabel')}
        >
          {t('common.firstTokenLabel')} {formatDurationMs(first)}
        </span>
      )}
      {dur != null && (
        <span
          className={`${BADGE_CLS} ${durationTone(dur, DURATION_FAST_MS, DURATION_SLOW_MS)}`}
          title={t('common.durationLabel')}
        >
          {t('common.durationLabel')} {formatDurationMs(dur)}
        </span>
      )}
    </div>
  )
}

const LOG_COLS: { labelKey: string; num?: boolean }[] = [
  { labelKey: 'common.colTime' },
  { labelKey: 'common.colIp' },
  { labelKey: 'common.colAccount' },
  { labelKey: 'common.colModel' },
  { labelKey: 'common.colTokenUsage' },
  { labelKey: 'common.colDuration' },
  { labelKey: 'common.colCost', num: true },
  { labelKey: 'common.colCredits', num: true },
]

export function UsageLogTable({
  records,
  geoMap,
  isLoading,
}: {
  records: UsageRecord[]
  geoMap: Map<string, GeoInfo | null>
  isLoading: boolean
}) {
  const { t } = useTranslation()
  return (
    <table className="w-full border-separate border-spacing-0">
      <thead>
        <tr>
          {LOG_COLS.map((col) => (
            <th key={col.labelKey} className={`${TH_BASE} ${col.num ? 'text-right' : ''}`}>
              {t(col.labelKey)}
            </th>
          ))}
        </tr>
      </thead>
      <tbody className="[&_tr:last-child>td]:border-b-0">
        {records.length === 0 ? (
          <tr>
            <td colSpan={LOG_COLS.length} className="px-3 py-14">
              <div className="flex flex-col items-center gap-2.5 text-ink-3">
                {isLoading ? (
                  <Loader2 className="size-6 animate-spin" aria-hidden="true" />
                ) : (
                  <SearchX className="size-6" aria-hidden="true" />
                )}
                <span className="text-[12.5px]">{isLoading ? t('common.loading') : t('common.noRecords')}</span>
              </div>
            </td>
          </tr>
        ) : (
          /* records are returned newest-first by the API */
          records.map((record, idx) => {
            const geo = record.clientIp ? geoMap.get(record.clientIp) : undefined
            const hitRate = cacheHitRate(record)
            return (
              <tr key={`${record.createdAt}-${record.model}-${idx}`} className="transition-colors hover:bg-surface-2">
                <td className={`${CELL} whitespace-nowrap text-[11.5px] text-ink-2`}>
                  {formatDateTime(record.createdAt)}
                </td>
                <td className={`${CELL} whitespace-nowrap text-[11.5px] text-ink-2`}>
                  {record.clientIp ? (
                    <span title={record.clientIp} className="flex flex-col items-start">
                      <span className="font-mono">{geo?.displayIp ?? record.clientIp}</span>
                      {/* city 缺失颗粒度的 IP（如 8.8.8.8）返回空串，拼接会留下悬空分隔符 */}
                      {geo?.country && (
                        <span className="text-ink-3">
                          {geo.city ? `${geo.country}·${geo.city}` : geo.country}
                        </span>
                      )}
                    </span>
                  ) : (
                    <span className="text-ink-3">—</span>
                  )}
                </td>
                <td
                  className={`${CELL} max-w-[132px] truncate text-[11.5px] text-ink-2`}
                  title={record.credentialLabel}
                >
                  {record.credentialLabel ?? <span className="text-ink-3">—</span>}
                  {record.userAgent && (
                    <span
                      className="mt-px block truncate font-mono text-[10px] text-ink-3"
                      title={record.userAgent}
                    >
                      {uaShort(record.userAgent)}
                    </span>
                  )}
                </td>
                <td className={`${CELL} text-[11.5px]`}>
                  <div className="flex flex-col items-start gap-px">
                    <span className={`font-mono ${getModelColor(record.model)}`}>{record.model}</span>
                    {(record.isStream != null || record.effort) && (
                      <div className="flex items-center gap-1">
                        {record.isStream != null && (
                          <span
                            className={`${BADGE_CLS} py-px leading-tight ${
                              record.isStream ? 'bg-brand-soft text-brand' : 'bg-surface-3 text-ink-2'
                            }`}
                          >
                            {record.isStream ? t('common.streamBadge') : t('common.nonStreamBadge')}
                          </span>
                        )}
                        {record.effort && (
                          <span
                            className="rounded bg-warn-soft px-1 py-px font-mono text-[10px] leading-tight text-warn"
                            title={`effort: ${record.effort}`}
                          >
                            {record.effort}
                          </span>
                        )}
                      </div>
                    )}
                  </div>
                </td>
                <td className={`${CELL} whitespace-nowrap text-[11px] text-ink-2`}>
                  <div className="flex flex-col gap-px">
                    <span>
                      {t('common.inputTokensLabel')}
                      <span className="font-mono tabular-nums">
                        {formatTokenCount(Math.max(0, record.inputTokens - (record.cacheReadInputTokens ?? 0)))}
                      </span>
                    </span>
                    <span className={(record.thinkingTokenCount ?? 0) > 0 ? 'flex items-center gap-1.5' : undefined}>
                      <span>
                        {t('common.outputTokensLabel')}
                        <span className="font-mono tabular-nums">{formatTokenCount(record.outputTokens)}</span>
                      </span>
                      {record.thinkingTokenCount != null && record.thinkingTokenCount > 0 && (
                        <span
                          className="inline-flex items-center gap-0.5 rounded-full bg-warn-soft px-1.5 py-px font-mono text-[10px] font-semibold tabular-nums text-warn"
                          title={t('common.thinkingBadge')}
                        >
                          <span className="sr-only">{t('common.thinkingLabel')}</span>
                          <Brain className="size-3" aria-hidden="true" />
                          <span>{formatTokenCount(record.thinkingTokenCount)}</span>
                        </span>
                      )}
                    </span>
                    <span className="text-ok">
                      {t('common.cacheReadLabel')}
                      <span className="font-mono tabular-nums">
                        {formatTokenCount(record.cacheReadInputTokens ?? 0)}
                      </span>
                      {SHOW_CACHE_HIT_RATE && hitRate != null && (
                        <span
                          className="ml-1.5 inline-flex items-center gap-0.5 rounded-full bg-brand-soft px-1.5 py-px align-middle font-mono text-[10px] font-semibold tabular-nums text-brand"
                          title={t('common.cacheHitRateTitle', { rate: hitRate.toFixed(1) })}
                        >
                          <DatabaseZap className="size-3" aria-hidden="true" />
                          {Math.round(hitRate)}%
                        </span>
                      )}
                    </span>
                    <span className="font-medium text-ink">
                      {t('common.totalInputLabel')}
                      <span className="font-mono tabular-nums">{formatTokenCount(record.inputTokens)}</span>
                    </span>
                  </div>
                </td>
                <td className={`${CELL} whitespace-nowrap text-[11px]`}>
                  <DurationCell record={record} />
                </td>
                <td className={`${CELL} text-right font-mono text-[12px] font-medium tabular-nums text-warn`}>
                  {formatCost(record.estimatedCost)}
                </td>
                <td className={`${CELL} text-right font-mono text-[12px] font-medium tabular-nums text-brand`}>
                  {recordCredits(record).toFixed(4)}
                  {record.creditsUsed != null && <span className="ml-1 text-[11px] text-ok">✓</span>}
                  {record.creditsSaved != null && record.creditsSaved > 0 && (
                    <small className="block text-[10px] font-normal text-ok">
                      {t('common.savedPrefix', { amount: record.creditsSaved.toFixed(4) })}
                    </small>
                  )}
                </td>
              </tr>
            )
          })
        )}
      </tbody>
    </table>
  )
}
