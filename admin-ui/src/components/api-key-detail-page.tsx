// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { RotateCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Metric, MetricValue, MetricsBar } from '@/components/metrics'
import { PageHead } from '@/components/page-head'
import { STATUS_VISUAL, TAG_BASE, type KeyStatus } from '@/components/api-key-row'
import { CELL, PANEL, PANEL_FOOT, PANEL_TITLE, Pager, TH_BASE } from '@/components/table-kit'
import { UsageLogTable, fallbackCredits, formatCost, getModelColor } from '@/components/usage-log-table'
import { useApiKeys, useAllUsage, useKeyUsageRecords } from '@/hooks/use-credentials'
import { useIpGeo } from '@/hooks/use-ip-geo'
import { extractErrorMessage } from '@/lib/utils'
import { formatTokenCount, localeTag } from '@/lib/locale'
import type { ApiKeyItem } from '@/types/api'

interface ApiKeyDetailPageProps {
  keyId: number
  onBack: () => void
}

function getKeyStatus(key: ApiKeyItem): KeyStatus {
  if (!key.enabled) return 'disabled'
  if (key.expiresAt && new Date(key.expiresAt) <= new Date()) return 'expired'
  if (key.durationDays != null && !key.activatedAt) return 'pending'
  return 'active'
}

const PAGE_SIZE = 50

const MODEL_COLS: { labelKey: string; num?: boolean }[] = [
  { labelKey: 'common.colModel' },
  { labelKey: 'apiKeys.colRequests', num: true },
  { labelKey: 'apiKeys.colInputTokens', num: true },
  { labelKey: 'apiKeys.colOutputTokens', num: true },
  { labelKey: 'common.colCost', num: true },
  { labelKey: 'apiKeys.colCredits', num: true },
]

export function ApiKeyDetailPage({ keyId, onBack }: ApiKeyDetailPageProps) {
  const { t } = useTranslation()
  const [page, setPage] = useState(1)

  const { data: apiKeys } = useApiKeys()
  const { data: allUsage } = useAllUsage()
  const { data: recordsData, isLoading, refetch } = useKeyUsageRecords(keyId, page, PAGE_SIZE)

  const apiKey = apiKeys?.find((k) => k.id === keyId)
  const summary = allUsage?.find((u) => u.apiKeyId === keyId)
  const status = apiKey ? getKeyStatus(apiKey) : null
  const serial = `#${String(keyId).padStart(3, '0')}`
  const title = apiKey?.name ?? serial

  const pageIps = (recordsData?.records ?? []).map((r) => r.clientIp).filter((ip): ip is string => !!ip)
  const geoMap = useIpGeo(pageIps)

  const records = recordsData?.records ?? []
  const totalRecords = recordsData?.total ?? 0
  const totalPages = Math.max(1, recordsData?.totalPages ?? 1)
  const savedCredits = summary?.totalCreditsSaved ?? 0

  // refetch 默认不抛异常，失败只体现在返回结果上，须显式检查（与列表页刷新同口径）
  const handleRefresh = async () => {
    const result = await refetch()
    if (result.isError) {
      toast.error(extractErrorMessage(result.error))
      return
    }
    toast.success(t('apiKeys.toastLogRefreshed'))
  }

  return (
    <div>
      {/* 页头（设计稿 .head）：三级面包屑 + 返回钮 + 序号副标题 + 右侧状态标签与刷新 */}
      <PageHead
        crumb={[t('dashboard.navMain'), t('apiKeys.pageTitle'), title]}
        title={title}
        note={<code className="font-mono">{serial}</code>}
        onBack={onBack}
        actions={
          <>
            {status && (
              <span className={`${TAG_BASE} ${STATUS_VISUAL[status].cls}`}>
                <span
                  className={`size-[5px] flex-none rounded-full ${STATUS_VISUAL[status].pip}`}
                  aria-hidden="true"
                />
                {t(STATUS_VISUAL[status].labelKey)}
              </span>
            )}
            <Button variant="outline" onClick={handleRefresh} disabled={isLoading}>
              <RotateCw className={isLoading ? 'animate-spin' : ''} />
              {t('common.refresh')}
            </Button>
          </>
        }
      />

      {/* 指标条（设计稿 .metrics）：主值统一 ink 色，费用/积分不再单独染色 */}
      <MetricsBar cols={5}>
        <Metric label={t('common.statTotalRequests')}>
          <MetricValue value={(summary?.totalRequests ?? 0).toLocaleString(localeTag())} />
        </Metric>
        <Metric label={t('apiKeys.colInputTokens')}>
          <MetricValue value={formatTokenCount(summary?.totalInputTokens ?? 0)} />
        </Metric>
        <Metric label={t('apiKeys.colOutputTokens')}>
          <MetricValue value={formatTokenCount(summary?.totalOutputTokens ?? 0)} />
        </Metric>
        <Metric label={t('apiKeys.totalCostLabel')}>
          <MetricValue value={formatCost(summary?.totalCost ?? 0)} />
        </Metric>
        <Metric label={t('apiKeys.totalCreditsLabel')}>
          <MetricValue
            value={fallbackCredits(summary?.totalCost ?? 0).toFixed(4)}
            unit="cr"
            trailing={
              savedCredits > 0 ? (
                <span className="text-[11px] font-medium text-ok">
                  {t('common.savedPrefix', { amount: savedCredits.toFixed(4) })}
                </span>
              ) : undefined
            }
          />
        </Metric>
      </MetricsBar>

      {/* 逐模型用量（原卡片网格改表格，与列表页同一套表头/单元格原语） */}
      {summary && summary.byModel.length > 0 && (
        <div className="mt-[15px]">
          <h3 className={PANEL_TITLE}>{t('apiKeys.byModelTitle')}</h3>
          <section className={PANEL}>
            <div className="max-h-[42vh] overflow-y-auto">
              <table className="w-full border-separate border-spacing-0">
                <thead>
                  <tr>
                    {MODEL_COLS.map((col) => (
                      <th key={col.labelKey} className={`${TH_BASE} ${col.num ? 'text-right' : ''}`}>
                        {t(col.labelKey)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody className="[&_tr:last-child>td]:border-b-0">
                  {summary.byModel.map((m) => (
                    <tr key={m.model} className="transition-colors hover:bg-surface-2">
                      <td className={CELL}>
                        <span className={`font-mono text-[12px] ${getModelColor(m.model)}`}>{m.model}</span>
                      </td>
                      <td className={`${CELL} text-right font-mono text-[12px] tabular-nums text-ink-2`}>
                        {m.requests.toLocaleString(localeTag())}
                      </td>
                      <td className={`${CELL} text-right font-mono text-[12px] tabular-nums text-ink-2`}>
                        {formatTokenCount(m.inputTokens)}
                      </td>
                      <td className={`${CELL} text-right font-mono text-[12px] tabular-nums text-ink-2`}>
                        {formatTokenCount(m.outputTokens)}
                      </td>
                      <td className={`${CELL} text-right font-mono text-[12px] font-medium tabular-nums text-warn`}>
                        {formatCost(m.cost)}
                      </td>
                      <td className={`${CELL} text-right font-mono text-[12px] font-medium tabular-nums text-brand`}>
                        {m.credits.toFixed(4)}
                        {m.creditsSaved > 0 && (
                          <small className="block text-[10px] font-normal text-ok">
                            {t('common.savedPrefix', { amount: m.creditsSaved.toFixed(4) })}
                          </small>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </div>
      )}

      {/* 请求日志（.panel + .tscroll + .panel-foot） */}
      <div className="mt-[15px]">
        <h3 className={PANEL_TITLE}>{t('common.requestLog')}</h3>
        <section className={PANEL}>
          <div className="max-h-[70vh] overflow-y-auto">
            <UsageLogTable records={records} geoMap={geoMap} isLoading={isLoading} />
          </div>

          {/* 面板脚（设计稿 .panel-foot）：右区计数 + 页码 + 每页条数；服务端分页 */}
          <div className={PANEL_FOOT}>
            <div className="ml-auto flex flex-none items-center gap-2">
              <span>{t('apiKeys.footRecords', { count: totalRecords })}</span>
              {totalRecords > 0 && (
                <>
                  <Pager page={page} totalPages={totalPages} onPage={setPage} />
                  <span>{t('credentials.footPerPage', { size: PAGE_SIZE })}</span>
                </>
              )}
            </div>
          </div>
        </section>
      </div>
    </div>
  )
}
