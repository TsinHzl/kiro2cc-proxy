// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { RotateCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { FootSep, Metric, MetricFoot, MetricValue, MetricsBar } from '@/components/metrics'
import { PageHead } from '@/components/page-head'
import { CELL, PANEL, PANEL_FOOT, PANEL_TITLE, Pager, TH_BASE } from '@/components/table-kit'
import { UsageLogTable, formatCost, getModelColor, recordCredits } from '@/components/usage-log-table'
import { useCredentials, useCredentialUsageRecords, useCredentialTodaySummary } from '@/hooks/use-credentials'
import { useIpGeo } from '@/hooks/use-ip-geo'
import { ACCOUNT_STATE_VISUAL, accountLabel, deriveAccountState } from '@/lib/account-state'
import { extractErrorMessage } from '@/lib/utils'
import { formatTokenCount, localeTag } from '@/lib/locale'

interface CredentialDetailPageProps {
  credentialId: number
  onBack: () => void
}

const PAGE_SIZE = 50

/** 状态标签（设计稿 .tag）：20px 高 + 5px pip，配色取自账号管理页同一套派生状态 */
const TAG =
  'inline-flex h-5 items-center gap-[5px] whitespace-nowrap rounded-[6px] border px-[7px] text-[11px] font-semibold'

/** 逐模型用量表头：模型 + 5 个右对齐数值列 */
const MODEL_COLS: { labelKey: string; num?: boolean }[] = [
  { labelKey: 'common.colModel' },
  { labelKey: 'apiKeys.colRequests', num: true },
  { labelKey: 'apiKeys.colInputTokens', num: true },
  { labelKey: 'apiKeys.colOutputTokens', num: true },
  { labelKey: 'common.colCost', num: true },
  { labelKey: 'apiKeys.colCredits', num: true },
]

interface ModelAgg {
  requests: number
  inputTokens: number
  outputTokens: number
  cost: number
  credits: number
  creditsSaved: number
}

export function CredentialDetailPage({ credentialId, onBack }: CredentialDetailPageProps) {
  const { t } = useTranslation()
  const [page, setPage] = useState(1)

  const { data: credentialsData } = useCredentials()
  const { data: recordsData, isLoading, refetch } = useCredentialUsageRecords(credentialId, page, PAGE_SIZE)
  const { data: todaySummary } = useCredentialTodaySummary(credentialId)

  const credential = credentialsData?.credentials.find((c) => c.id === credentialId)
  // 详情页无余额查询上下文，hasBalance 恒 false：未查额度且零成功调用的账号显示为「待验证」
  const visual = credential ? ACCOUNT_STATE_VISUAL[deriveAccountState(credential, false)] : null
  const title = credential ? accountLabel(credential) : `#${credentialId}`

  const records = recordsData?.records ?? []
  const pageIps = records.map((r) => r.clientIp).filter((ip): ip is string => !!ip)
  const geoMap = useIpGeo(pageIps)
  const totalRequests = recordsData?.total ?? 0
  const totalPages = Math.max(1, recordsData?.totalPages ?? 1)

  // 逐模型聚合仅覆盖当前页（全量聚合需后端新接口），标题文案已含「（当前页）」限定
  const byModel = records.reduce<Record<string, ModelAgg>>((acc, r) => {
    const entry =
      acc[r.model] ?? { requests: 0, inputTokens: 0, outputTokens: 0, cost: 0, credits: 0, creditsSaved: 0 }
    entry.requests += 1
    entry.inputTokens += r.inputTokens
    entry.outputTokens += r.outputTokens
    entry.cost += r.estimatedCost
    entry.credits += recordCredits(r)
    entry.creditsSaved += r.creditsSaved ?? 0
    acc[r.model] = entry
    return acc
  }, {})

  const pageInput = records.reduce((s, r) => s + r.inputTokens, 0)
  const pageOutput = records.reduce((s, r) => s + r.outputTokens, 0)
  const pageCost = records.reduce((s, r) => s + r.estimatedCost, 0)
  const pageCredits = records.reduce((s, r) => s + recordCredits(r), 0)
  const pageSaved = records.reduce((s, r) => s + (r.creditsSaved ?? 0), 0)
  const todaySaved = todaySummary?.totalCreditsSaved ?? 0
  const todayRequests = todaySummary?.totalRequests ?? 0

  // refetch 默认不抛异常，失败只体现在返回结果上，须显式检查（与 API Key 详情页同口径）
  const handleRefresh = async () => {
    const result = await refetch()
    if (result.isError) {
      toast.error(extractErrorMessage(result.error))
      return
    }
    toast.success(t('apiKeys.toastLogRefreshed'))
  }

  const savedTrailing = (amount: number) =>
    amount > 0 ? (
      <span className="text-[11px] font-medium text-ok">
        {t('common.savedPrefix', { amount: amount.toFixed(4) })}
      </span>
    ) : undefined

  return (
    <div>
      {/* 页头（设计稿 .head）：三级面包屑 + 返回钮 + 序号副标题 + 右侧状态标签与刷新 */}
      <PageHead
        crumb={[t('dashboard.navMain'), t('dashboard.navCredentials'), title]}
        title={title}
        note={<code className="font-mono">#{credentialId}</code>}
        onBack={onBack}
        actions={
          <>
            {visual && (
              <span className={`${TAG} ${visual.tagClass}`}>
                <span className={`size-[5px] flex-none rounded-full ${visual.pipClass}`} aria-hidden="true" />
                {t(visual.labelKey)}
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
          <MetricValue value={totalRequests.toLocaleString(localeTag())} />
        </Metric>
        <Metric label={t('credentials.pageTokens')}>
          <MetricValue value={formatTokenCount(pageInput + pageOutput)} />
          <MetricFoot>
            <span>{t('common.inboundPrefix', { value: formatTokenCount(pageInput) })}</span>
            <FootSep />
            <span>{t('common.outboundPrefix', { value: formatTokenCount(pageOutput) })}</span>
          </MetricFoot>
        </Metric>
        <Metric label={t('common.pageCost')}>
          <MetricValue value={formatCost(pageCost)} />
        </Metric>
        <Metric label={t('common.pageCredits')}>
          <MetricValue value={pageCredits.toFixed(4)} unit="cr" trailing={savedTrailing(pageSaved)} />
        </Metric>
        <Metric label={t('credentials.todayCredits')}>
          <MetricValue
            value={(todaySummary?.totalCredits ?? 0).toFixed(4)}
            unit="cr"
            trailing={savedTrailing(todaySaved)}
          />
          {todayRequests > 0 && (
            <MetricFoot>{t('credentials.todayRequestsCount', { count: todayRequests })}</MetricFoot>
          )}
        </Metric>
      </MetricsBar>

      {/* 逐模型用量（原卡片网格改表格，与请求日志同一套表头/单元格原语） */}
      {Object.keys(byModel).length > 0 && (
        <div className="mt-[15px]">
          <h3 className={PANEL_TITLE}>{t('credentials.groupByModel')}</h3>
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
                  {Object.entries(byModel).map(([model, m]) => (
                    <tr key={model} className="transition-colors hover:bg-surface-2">
                      <td className={CELL}>
                        <span className={`font-mono text-[12px] ${getModelColor(model)}`}>{model}</span>
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
              <span>{t('apiKeys.footRecords', { count: totalRequests })}</span>
              {totalRequests > 0 && (
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
