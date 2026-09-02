// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { Fragment, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Boxes, Copy, Download, Loader2, RotateCw, SearchX } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { FootSep, Metric, MetricFoot, MetricValue, MetricsBar } from '@/components/metrics'
import { PageHead } from '@/components/page-head'
import { SearchBox, Segmented, Toolbar, UpdatedAgo } from '@/components/toolbar'
import { CELL, ICON_BTN, PANEL, PANEL_FOOT, TH_BASE } from '@/components/table-kit'
import { TAG_BASE } from '@/components/api-key-row'
import { useModels } from '@/hooks/use-credentials'
import { copyToClipboard } from '@/lib/clipboard'
import { extractErrorMessage } from '@/lib/utils'
import { localeTag } from '@/lib/locale'
import type { ModelItem } from '@/types/api'
import {
  FAMILY_AUTO,
  FAMILY_OTHER,
  formatRate,
  formatRateRange,
  groupByFamily,
  rateRange,
} from '@/lib/model-family'

/** 8192 → 8.2K、32000 → 32K（设计稿以 K 为单位展示 Token 上限；formatTokenCount 只做千分位） */
function formatK(n: number): string {
  if (n < 1000) return String(n)
  const k = n / 1000
  return `${Number.isInteger(k) ? k : k.toFixed(1)}K`
}

/** 取首个匹配该倍率的模型名，用于指标卡脚注 */
function rateModelName(models: ModelItem[], rate: number): string {
  const hit = models.find((m) => m.rate_multiplier === rate)
  if (hit === undefined) return '—'
  return hit.display_name !== '' ? hit.display_name : hit.id
}

/** 提供方语义色：主力提供方各占一色，长尾统一弱化（旧实现的 7 色 palette 已收敛到设计令牌） */
const PROVIDER_PIP: Readonly<Record<string, string>> = {
  anthropic: 'bg-brand',
  kiro: 'bg-ok',
  openai: 'bg-ink-2',
}
const PROVIDER_PIP_FALLBACK = 'bg-ink-3'

type RateStatus = 'synced' | 'missing'

/**
 * 「倍率来源」状态 —— 后端在上游 ListAvailableModels 失败时会回退本地静态模型表，
 * 此时 rate_multiplier 全为 null。设计稿的「可用 / 已弃用」无对应字段，故改表此语义。
 */
const RATE_STATUS: Readonly<Record<RateStatus, { cls: string; pip: string; labelKey: string }>> = {
  synced: { cls: 'border-ok-line bg-ok-soft text-ok', pip: 'bg-ok', labelKey: 'models.statusRateSynced' },
  missing: { cls: 'border-warn-line bg-warn-soft text-warn', pip: 'bg-warn', labelKey: 'models.statusRateMissing' },
}

type ModelFilter = 'all' | RateStatus

/** 指标卡脚注最多列出的家族数 */
const FAMILY_PREVIEW = 3
const EXPORT_FILE_NAME = 'kiro2cc-models.json'
const COLUMN_WIDTHS: (number | undefined)[] = [44, undefined, 118, 96, 158, 108, 52]

function downloadJson(models: ModelItem[]): void {
  const url = URL.createObjectURL(new Blob([JSON.stringify(models, null, 2)], { type: 'application/json' }))
  try {
    const link = document.createElement('a')
    link.href = url
    link.download = EXPORT_FILE_NAME
    link.click()
  } finally {
    URL.revokeObjectURL(url)
  }
}

export function ModelListPage() {
  const { t } = useTranslation()
  const { data, isLoading, refetch, dataUpdatedAt } = useModels()
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<ModelFilter>('all')

  const models = data?.data ?? []
  const allRange = rateRange(models)
  const maxRate = allRange?.max ?? 0
  const syncedCount = models.filter((m) => m.rate_multiplier != null).length
  const allGroups = groupByFamily(models)
  const providerCount = new Set(models.map((m) => m.owned_by)).size
  const outputs = models.map((m) => m.max_tokens)

  const familyLabel = (name: string): string => {
    if (name === FAMILY_AUTO) return t('models.familyAuto')
    return name === FAMILY_OTHER ? t('models.familyOther') : name
  }

  const q = query.trim().toLowerCase()
  const filtered = models.filter((m) => {
    const hitQuery = q === '' || m.id.toLowerCase().includes(q) || m.display_name.toLowerCase().includes(q)
    const status: RateStatus = m.rate_multiplier != null ? 'synced' : 'missing'
    return hitQuery && (filter === 'all' || filter === status)
  })
  // 序号跨家族连续（设计稿 01…19），故按筛选后的平铺顺序预先编号
  const serials = new Map(filtered.map((m, i) => [m.id, i + 1]))
  const groups = groupByFamily(filtered)

  // refetch 默认不抛异常，失败只体现在返回结果上，须显式检查（与其余页刷新同口径）
  const handleRefresh = async () => {
    const result = await refetch()
    if (result.isError) {
      toast.error(extractErrorMessage(result.error))
      return
    }
    toast.success(t('models.toastRefreshed'))
  }

  const handleCopyId = async (id: string) => {
    try {
      await copyToClipboard(id)
      toast.success(t('models.toastCopiedId'))
    } catch (error) {
      toast.error(extractErrorMessage(error))
    }
  }

  const handleCopyAll = async () => {
    try {
      await copyToClipboard(models.map((m) => m.id).join('\n'))
      toast.success(t('models.toastCopiedIds', { n: models.length }))
    } catch (error) {
      toast.error(extractErrorMessage(error))
    }
  }

  const handleExport = () => {
    try {
      downloadJson(models)
      toast.success(t('models.toastExported'))
    } catch (error) {
      toast.error(extractErrorMessage(error))
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHead
        crumb={[t('dashboard.navMain'), t('models.pageTitle')]}
        title={t('models.pageTitle')}
        note={t('models.headNote')}
        actions={
          models.length > 0 && (
            /* 清单来源徽章：倍率全缺失即代表后端已回退本地静态表 */
            <span
              className={`${TAG_BASE} ${
                syncedCount > 0 ? 'border-ok-line bg-ok-soft text-ok' : 'border-warn-line bg-warn-soft text-warn'
              }`}
            >
              <span
                className={`size-[5px] flex-none rounded-full ${syncedCount > 0 ? 'bg-ok' : 'bg-warn'}`}
                aria-hidden="true"
              />
              {t(syncedCount > 0 ? 'models.sourceLive' : 'models.sourceFallback')}
            </span>
          )
        }
      />

      <MetricsBar>
        <Metric label={t('models.metricTotalLabel')}>
          <MetricValue value={String(models.length)} unit={t('models.unitModels')} />
          <MetricFoot className="truncate">
            <span>{t('models.metricProviders', { n: providerCount })}</span>
            {outputs.length > 0 && (
              <>
                <FootSep />
                <span>
                  {t('models.metricOutputRange', {
                    min: formatK(Math.min(...outputs)),
                    max: formatK(Math.max(...outputs)),
                  })}
                </span>
              </>
            )}
          </MetricFoot>
        </Metric>

        <Metric label={t('models.metricFamiliesLabel')}>
          <MetricValue value={String(allGroups.length)} unit={t('models.unitFamilies')} />
          <MetricFoot className="truncate">
            {allGroups.length === 0 ? (
              <span>{t('models.emptyNoModels')}</span>
            ) : (
              allGroups.slice(0, FAMILY_PREVIEW).map((group, i) => (
                <span key={group.name} className="flex items-center gap-1.5">
                  {i > 0 && <FootSep />}
                  {familyLabel(group.name)}
                  <b className="font-mono font-semibold tabular-nums text-ink-2">{group.models.length}</b>
                </span>
              ))
            )}
            {allGroups.length > FAMILY_PREVIEW && (
              <>
                <FootSep />
                <span>{t('models.metricFamiliesMore', { n: allGroups.length - FAMILY_PREVIEW })}</span>
              </>
            )}
          </MetricFoot>
        </Metric>

        <Metric label={t('models.metricRateCoverageLabel')}>
          <MetricValue value={String(syncedCount)} unit={t('models.metricRateOfTotal', { total: models.length })} />
          <MetricFoot className="truncate">
            <span>
              {t(
                syncedCount === 0 && models.length > 0
                  ? 'models.metricRateFallbackNote'
                  : 'models.metricRateLiveNote',
              )}
            </span>
          </MetricFoot>
        </Metric>

        <Metric label={t('models.metricRateRangeLabel')}>
          <MetricValue
            value={allRange ? formatRate(allRange.min) : '—'}
            unit={allRange && allRange.min !== allRange.max ? `－ ${formatRate(allRange.max)}` : undefined}
          />
          <MetricFoot className="truncate">
            {allRange ? (
              <>
                <span className="truncate">
                  {t('models.metricRateLowest', { name: rateModelName(models, allRange.min) })}
                </span>
                <FootSep />
                <span className="truncate">
                  {t('models.metricRateHighest', { name: rateModelName(models, allRange.max) })}
                </span>
              </>
            ) : (
              <span>{t('models.metricRateNone')}</span>
            )}
          </MetricFoot>
        </Metric>
      </MetricsBar>

      {/* 操作条（设计稿 .actionbar）：「调整倍率」「映射规则」依赖未实现的后端写接口，不渲染 */}
      <div className="flex flex-wrap items-center gap-[7px] pt-[15px]">
        <Button variant="outline" onClick={handleRefresh} disabled={isLoading}>
          <RotateCw className={isLoading ? 'animate-spin' : ''} aria-hidden="true" />
          {t('models.refreshList')}
        </Button>
        <Button variant="outline" onClick={handleCopyAll} disabled={models.length === 0}>
          <Copy aria-hidden="true" />
          {t('models.copyAllIds')}
        </Button>
        <Button variant="outline" onClick={handleExport} disabled={models.length === 0}>
          <Download aria-hidden="true" />
          {t('models.exportJson')}
        </Button>
      </div>

      <Toolbar>
        <SearchBox
          value={query}
          onChange={setQuery}
          placeholder={t('models.searchPlaceholder')}
          clearLabel={t('models.searchClear')}
        />
        <Segmented
          value={filter}
          onChange={setFilter}
          groupLabel={t('models.filterGroupLabel')}
          options={[
            { key: 'all', label: t('models.filterAll'), count: models.length },
            { key: 'synced', label: t('models.filterSynced'), count: syncedCount, pipClass: 'bg-ok' },
            {
              key: 'missing',
              label: t('models.filterMissing'),
              count: models.length - syncedCount,
              pipClass: 'bg-warn',
            },
          ]}
        />
        <UpdatedAgo dataUpdatedAt={dataUpdatedAt} />
      </Toolbar>

      <section className={`flex min-h-0 flex-1 flex-col ${PANEL}`}>
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full border-collapse text-[12.5px]">
            <colgroup>
              {COLUMN_WIDTHS.map((w, i) => (
                <col key={i} style={w ? { width: w } : undefined} />
              ))}
            </colgroup>
            <thead>
              <tr>
                <th className={TH_BASE}>{t('models.colIndex')}</th>
                <th className={TH_BASE}>{t('models.colModelId')}</th>
                <th className={TH_BASE}>{t('models.colProvider')}</th>
                <th className={`${TH_BASE} text-right`}>{t('models.colMaxTokens')}</th>
                <th className={TH_BASE}>{t('models.colRateMultiplier')}</th>
                <th className={TH_BASE}>{t('models.colStatus')}</th>
                <th className={`${TH_BASE} text-right`}>{t('models.colActions')}</th>
              </tr>
            </thead>
            <tbody className="[&_tr:last-child>td]:border-b-0">
              {filtered.length === 0 ? (
                <tr>
                  <td colSpan={COLUMN_WIDTHS.length} className="px-3 py-14">
                    <div className="flex flex-col items-center gap-2.5 text-ink-3">
                      {isLoading ? (
                        <Loader2 className="size-6 animate-spin" aria-hidden="true" />
                      ) : (
                        <SearchX className="size-6" aria-hidden="true" />
                      )}
                      <span className="text-[12.5px]">
                        {isLoading
                          ? t('common.loading')
                          : models.length === 0
                            ? t('models.emptyNoModels')
                            : t('models.emptyNoMatch')}
                      </span>
                    </div>
                  </td>
                </tr>
              ) : (
                groups.map((group) => (
                  <Fragment key={group.name}>
                    {/* 设计稿 .grp 分组行：家族名 + 数量 + 该组倍率区间 */}
                    <tr className="bg-surface-2">
                      <td colSpan={5} className="border-b border-hairline px-3 py-[7px]">
                        <span className="inline-flex items-center gap-1.5 text-[10.5px] font-semibold uppercase tracking-[0.07em] text-ink-2">
                          <Boxes className="size-[13px] flex-none text-ink-3" strokeWidth={1.7} aria-hidden="true" />
                          {familyLabel(group.name)}
                          <span className="font-mono text-[10.5px] font-normal tabular-nums text-ink-3">
                            {group.models.length}
                          </span>
                        </span>
                      </td>
                      <td colSpan={2} className="border-b border-hairline px-3 py-[7px] text-right">
                        <span className="whitespace-nowrap font-mono text-[10.5px] tabular-nums text-ink-3">
                          {formatRateRange(rateRange(group.models))}
                        </span>
                      </td>
                    </tr>
                    {group.models.map((model) => {
                      const rate = model.rate_multiplier
                      const visual = RATE_STATUS[rate != null ? 'synced' : 'missing']
                      // .mult-bar 宽度按占全清单最大倍率的比例；最大倍率为 0 时全部 0 宽
                      const barWidth = rate != null && maxRate > 0 ? (rate / maxRate) * 100 : 0
                      const isTop = rate != null && maxRate > 0 && rate >= maxRate
                      return (
                        <tr key={model.id} className="transition-colors hover:bg-surface-2">
                          <td className={CELL}>
                            <span className="font-mono text-[11px] tabular-nums text-ink-3">
                              {String(serials.get(model.id) ?? 0).padStart(2, '0')}
                            </span>
                          </td>
                          <td className={CELL}>
                            <div className="flex min-w-0 flex-col gap-[3px]">
                              <span className="truncate font-mono text-[12px] text-ink">{model.id}</span>
                              {model.display_name !== '' && model.display_name !== model.id && (
                                <span className="truncate text-[11px] text-ink-3">{model.display_name}</span>
                              )}
                            </div>
                          </td>
                          <td className={CELL}>
                            <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-[11.5px] text-ink-2">
                              <span
                                className={`size-[6px] flex-none rounded-full ${
                                  PROVIDER_PIP[model.owned_by] ?? PROVIDER_PIP_FALLBACK
                                }`}
                                aria-hidden="true"
                              />
                              {model.owned_by}
                            </span>
                          </td>
                          <td className={`${CELL} text-right`}>
                            <span
                              className="whitespace-nowrap font-mono text-[12px] tabular-nums text-ink-2"
                              title={model.max_tokens.toLocaleString(localeTag())}
                            >
                              {formatK(model.max_tokens)}
                            </span>
                          </td>
                          <td className={CELL}>
                            {rate == null ? (
                              <span className="font-mono text-[12px] text-ink-3">—</span>
                            ) : (
                              <div className="flex items-center gap-2">
                                <span className="block h-1 min-w-[48px] flex-1 overflow-hidden rounded-[3px] bg-track">
                                  <span
                                    className={`block h-full rounded-[3px] ${isTop ? 'bg-warn' : 'bg-brand'}`}
                                    style={{ width: `${Math.min(100, Math.max(0, barWidth))}%` }}
                                  />
                                </span>
                                <span
                                  className={`flex-none font-mono text-[12px] font-semibold tabular-nums ${
                                    isTop ? 'text-warn' : 'text-ink'
                                  }`}
                                >
                                  {formatRate(rate)}
                                </span>
                              </div>
                            )}
                          </td>
                          <td className={CELL}>
                            <span className={`${TAG_BASE} ${visual.cls}`}>
                              <span className={`size-[5px] flex-none rounded-full ${visual.pip}`} aria-hidden="true" />
                              {t(visual.labelKey)}
                            </span>
                          </td>
                          <td className={CELL}>
                            <div className="flex justify-end">
                              <button
                                type="button"
                                className={ICON_BTN}
                                title={t('models.copyIdLabel')}
                                aria-label={t('models.copyIdLabel')}
                                onClick={() => handleCopyId(model.id)}
                              >
                                <Copy className="size-[13px]" strokeWidth={1.7} aria-hidden="true" />
                              </button>
                            </div>
                          </td>
                        </tr>
                      )
                    })}
                  </Fragment>
                ))
              )}
            </tbody>
          </table>
        </div>

        <div className={PANEL_FOOT}>
          <span>{t('models.footRateNote')}</span>
          <span className="ml-auto">
            {t('models.footTotals', { models: filtered.length, families: groups.length })}
          </span>
        </div>
      </section>
    </div>
  )
}
