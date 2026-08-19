// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import { Loader2, SearchX } from 'lucide-react'
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

const LOG_COLS: { labelKey: string; num?: boolean }[] = [
  { labelKey: 'common.colTime' },
  { labelKey: 'common.colIp' },
  { labelKey: 'common.colAccount' },
  { labelKey: 'common.colModel' },
  { labelKey: 'common.colTokenUsage' },
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
            return (
              <tr key={`${record.createdAt}-${record.model}-${idx}`} className="transition-colors hover:bg-surface-2">
                <td className={`${CELL} whitespace-nowrap text-[11.5px] text-ink-2`}>
                  {formatDateTime(record.createdAt)}
                </td>
                <td className={`${CELL} whitespace-nowrap text-[11.5px] text-ink-2`}>
                  {record.clientIp ? (
                    <span title={record.clientIp}>
                      <span className="font-mono">{geo?.displayIp ?? record.clientIp}</span>
                      {/* city 缺失颗粒度的 IP（如 8.8.8.8）返回空串，拼接会留下悬空分隔符 */}
                      {geo?.country && (
                        <span className="ml-1 text-ink-3">
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
                </td>
                <td className={`${CELL} font-mono text-[11.5px] ${getModelColor(record.model)}`}>{record.model}</td>
                <td className={`${CELL} whitespace-nowrap text-[11px] text-ink-2`}>
                  <div className="flex flex-col gap-px">
                    <span>
                      {t('common.inputTokensLabel')}
                      <span className="font-mono tabular-nums">
                        {formatTokenCount(Math.max(0, record.inputTokens - (record.cacheReadInputTokens ?? 0)))}
                      </span>
                    </span>
                    <span>
                      {t('common.outputTokensLabel')}
                      <span className="font-mono tabular-nums">{formatTokenCount(record.outputTokens)}</span>
                    </span>
                    <span className="text-ok">
                      {t('common.cacheReadLabel')}
                      <span className="font-mono tabular-nums">
                        {formatTokenCount(record.cacheReadInputTokens ?? 0)}
                      </span>
                    </span>
                    <span className="font-medium text-ink">
                      {t('common.totalInputLabel')}
                      <span className="font-mono tabular-nums">{formatTokenCount(record.inputTokens)}</span>
                    </span>
                  </div>
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
