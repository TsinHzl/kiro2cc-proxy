// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Loader2, RotateCw, SearchX, type LucideIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Metric, MetricFoot, MetricValue, MetricsBar } from '@/components/metrics'
import { PageHead } from '@/components/page-head'
import { CELL, PANEL, PANEL_FOOT, PANEL_TITLE, Pager, TH_BASE } from '@/components/table-kit'
import { useCredentials } from '@/hooks/use-credentials'
import { accountLabel } from '@/lib/account-state'
import { extractErrorMessage } from '@/lib/utils'
import { formatDateTime, localeTag } from '@/lib/locale'
import type { CredentialStatusItem } from '@/types/api'

/**
 * 账号事件日志页（限流日志 / 失败日志共用）。
 *
 * 两页的页头、指标条、表格列、分页与详情对话框完全一致，差异只有色调、图标、
 * 6 个 i18n 键与累计计数取哪个字段 —— 故全部展示收敛在此，调用方只负责取数
 * 并注入这些差异（design.md 决策 8：原语只承载展示，语义由调用方注入）。
 */

export const PAGE_SIZE = 50

/** FIFO 淘汰阈值，与后端日志表上限一致；两页文案相同故不作为注入项 */
const RECORD_LIMIT = 500

/** 两页共用的记录形状 —— `ThrottleLogRecord` 与 `FailureLogRecord` 字段完全一致 */
interface EventLogRecord {
  requestType: string
  statusCode: number
  responseBody: string
  createdAt: string
}

/** 色调白名单：动态拼接类名会被 Tailwind purge，故用静态 Record */
const TONE = {
  warn: { value: 'text-warn', tag: 'text-warn bg-warn-soft border-warn-line' },
  danger: { value: 'text-danger', tag: 'text-danger bg-danger-soft border-danger-line' },
} as const

/** 标签（设计稿 .tag） */
const TAG =
  'inline-flex h-5 items-center gap-[5px] whitespace-nowrap rounded-[6px] border px-[7px] text-[11px] font-semibold'

const COLS = ['common.colTime', 'logs.colRequestType', 'logs.colStatusCode', 'logs.colResponseSummary']

export function EventLogPage({
  credentialId,
  onBack,
  page,
  onPage,
  data,
  isLoading,
  refetch,
  tone,
  icon: Icon,
  badgeKey,
  totalLabelKey,
  cumulativeLabelKey,
  cumulativeHintKey,
  cumulativeOf,
  titleKey,
  emptyKey,
}: {
  credentialId: number
  onBack: () => void
  page: number
  onPage: (page: number) => void
  data: { records: EventLogRecord[]; total: number; totalPages: number } | undefined
  isLoading: boolean
  /** 传入 TanStack 的 refetch；宽松结构类型只取失败判定所需的两个字段 */
  refetch: () => Promise<{ isError: boolean; error: unknown }>
  tone: keyof typeof TONE
  icon: LucideIcon
  badgeKey: string
  totalLabelKey: string
  cumulativeLabelKey: string
  cumulativeHintKey: string
  cumulativeOf: (credential: CredentialStatusItem) => number
  titleKey: string
  emptyKey: string
}) {
  const { t } = useTranslation()
  const [selectedBody, setSelectedBody] = useState<string | null>(null)

  const { data: credentialsData } = useCredentials()
  const credential = credentialsData?.credentials.find((c) => c.id === credentialId)
  const title = credential ? accountLabel(credential) : `#${credentialId}`

  const records = data?.records ?? []
  const total = data?.total ?? 0
  const totalPages = Math.max(1, data?.totalPages ?? 1)
  const visual = TONE[tone]

  // refetch 默认不抛异常，失败只体现在返回结果上，须显式检查（与详情页同口径）
  const handleRefresh = async () => {
    const result = await refetch()
    if (result.isError) {
      toast.error(extractErrorMessage(result.error))
      return
    }
    toast.success(t('logs.toastLogRefreshed'))
  }

  return (
    <div>
      {/* 页头（设计稿 .head）：三级面包屑 + 返回钮 + 序号副标题 + 日志类型标签与刷新 */}
      <PageHead
        crumb={[t('dashboard.navMain'), t('dashboard.navCredentials'), title]}
        title={title}
        note={<code className="font-mono">#{credentialId}</code>}
        onBack={onBack}
        actions={
          <>
            <span className={`${TAG} ${visual.tag}`}>
              <Icon className="size-3" aria-hidden="true" />
              {t(badgeKey)}
            </span>
            <Button variant="outline" onClick={handleRefresh} disabled={isLoading}>
              <RotateCw className={isLoading ? 'animate-spin' : ''} />
              {t('common.refresh')}
            </Button>
          </>
        }
      />

      {/* 指标条（设计稿 .metrics）：本页记录数按色调着色，另两项为中性参照值 */}
      <MetricsBar cols={3}>
        <Metric label={t(totalLabelKey)}>
          <MetricValue value={total.toLocaleString(localeTag())} valueClass={visual.value} />
        </Metric>
        <Metric label={t(cumulativeLabelKey)}>
          <MetricValue value={(credential ? cumulativeOf(credential) : 0).toLocaleString(localeTag())} />
          <MetricFoot className="min-w-0">
            <span className="truncate" title={t(cumulativeHintKey)}>
              {t(cumulativeHintKey)}
            </span>
          </MetricFoot>
        </Metric>
        <Metric label={t('logs.recordLimitLabel')}>
          <MetricValue value={RECORD_LIMIT.toLocaleString(localeTag())} />
          <MetricFoot>{t('logs.limitExceededHint')}</MetricFoot>
        </Metric>
      </MetricsBar>

      {/* 事件表（.panel + .tscroll + .panel-foot） */}
      <div className="mt-[15px]">
        <h3 className={PANEL_TITLE}>{t(titleKey)}</h3>
        <section className={PANEL}>
          <div className="max-h-[70vh] overflow-y-auto">
            <table className="w-full border-separate border-spacing-0">
              <thead>
                <tr>
                  {COLS.map((key) => (
                    <th key={key} className={TH_BASE}>
                      {t(key)}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody className="[&_tr:last-child>td]:border-b-0">
                {records.length === 0 ? (
                  <tr>
                    <td colSpan={COLS.length} className="px-3 py-14">
                      <div className="flex flex-col items-center gap-2.5 text-ink-3">
                        {isLoading ? (
                          <Loader2 className="size-6 animate-spin" aria-hidden="true" />
                        ) : (
                          <SearchX className="size-6" aria-hidden="true" />
                        )}
                        <span className="text-[12.5px]">{isLoading ? t('common.loading') : t(emptyKey)}</span>
                      </div>
                    </td>
                  </tr>
                ) : (
                  records.map((record, idx) => (
                    <tr key={`${record.createdAt}-${idx}`} className="transition-colors hover:bg-surface-2">
                      <td className={`${CELL} whitespace-nowrap font-mono text-[11.5px] text-ink-2`}>
                        {formatDateTime(record.createdAt)}
                      </td>
                      <td className={CELL}>
                        <span className={`${TAG} border-hairline-2 bg-surface-3 text-ink-2`}>
                          {record.requestType.toUpperCase()}
                        </span>
                      </td>
                      <td className={`${CELL} font-mono text-[12px] font-medium tabular-nums ${visual.value}`}>
                        {record.statusCode}
                      </td>
                      <td className={CELL}>
                        {/* 改版前挂在 td 上的 onClick 键盘不可达，改为 button 并补 title 显示全文 */}
                        <button
                          type="button"
                          onClick={() => setSelectedBody(record.responseBody)}
                          title={record.responseBody}
                          className="block w-full truncate text-left text-[11.5px] text-ink-2 transition-colors hover:text-ink hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand"
                        >
                          {record.responseBody}
                        </button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>

          {/* 面板脚（设计稿 .panel-foot）：右区计数 + 页码 + 每页条数；服务端分页 */}
          <div className={PANEL_FOOT}>
            <div className="ml-auto flex flex-none items-center gap-2">
              <span>{t('apiKeys.footRecords', { count: total })}</span>
              {total > 0 && (
                <>
                  <Pager page={page} totalPages={totalPages} onPage={onPage} />
                  <span>{t('credentials.footPerPage', { size: PAGE_SIZE })}</span>
                </>
              )}
            </div>
          </div>
        </section>
      </div>

      <Dialog
        open={selectedBody !== null}
        onOpenChange={(open) => {
          if (!open) setSelectedBody(null)
        }}
      >
        <DialogContent className="flex max-h-[80vh] max-w-2xl flex-col">
          <DialogHeader>
            <DialogTitle>{t('logs.fullResponseTitle')}</DialogTitle>
          </DialogHeader>
          <pre className="flex-1 overflow-y-auto whitespace-pre-wrap break-all rounded-[8px] border border-hairline bg-surface-2 p-3 font-mono text-[11.5px] leading-[1.55] text-ink-2">
            {selectedBody}
          </pre>
        </DialogContent>
      </Dialog>
    </div>
  )
}
