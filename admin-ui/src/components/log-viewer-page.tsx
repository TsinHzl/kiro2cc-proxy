// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useRef, useEffect, useCallback, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Copy, Download, Pause, Play, Trash2 } from 'lucide-react'
import { useLogStream, MAX_FRONT_LOGS, type LogEntry } from '@/hooks/use-log-stream'
import { copyToClipboard } from '@/lib/clipboard'
import { extractErrorMessage } from '@/lib/utils'
import { formatTokenCount } from '@/lib/locale'
import { Button } from '@/components/ui/button'
import { PageHead } from '@/components/page-head'
import { TAG_BASE } from '@/components/api-key-row'
import { PANEL, PANEL_FOOT, FOOT_BTN, ICON_BTN } from '@/components/table-kit'
import { MetricsBar, Metric, MetricValue, MetricFoot, MetricAside, FootSep, Ring } from '@/components/metrics'
import { Toolbar, SearchBox, Segmented, Toggle } from '@/components/toolbar'

/** 严重度降序；与 LogEntry['level'] 同集合 */
const LEVELS = ['ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE'] as const
type LogLevel = (typeof LEVELS)[number]
type LevelFilter = 'ALL' | LogLevel

/** logbar 分段只列 4 档，TRACE 仅在「全部」下可见 —— 与改版前的筛选档位一致 */
const SEG_LEVELS: LogLevel[] = ['ERROR', 'WARN', 'INFO', 'DEBUG']

const LEVEL_PIP: Readonly<Record<LogLevel, string>> = {
  ERROR: 'bg-danger',
  WARN: 'bg-warn',
  INFO: 'bg-brand',
  DEBUG: 'bg-ink-3',
  TRACE: 'bg-hairline-2',
}

/** index.css 白名单类 .lv.<档> —— 必须是静态字符串，动态拼接会被 Tailwind purge 连带清掉 */
const LV_CLASS: Readonly<Record<LogLevel, string>> = {
  ERROR: 'lv error',
  WARN: 'lv warn',
  INFO: 'lv info',
  DEBUG: 'lv debug',
  TRACE: 'lv trace',
}

const HOUR_MS = 3_600_000

function pad(n: number, width = 2): string {
  return String(n).padStart(width, '0')
}

/** 「UTC+8」/「UTC-3:30」；会话内时区不变，故取模块级常量 */
const TZ_LABEL = (() => {
  const offset = -new Date().getTimezoneOffset()
  const abs = Math.abs(offset)
  const minutes = abs % 60
  return `UTC${offset < 0 ? '-' : '+'}${Math.floor(abs / 60)}${minutes > 0 ? `:${pad(minutes)}` : ''}`
})()

/**
 * 后端时间戳为 UTC（`src/log_capture.rs` 的 `chrono::Utc::now()`），改版前只做
 * `replace('T',' ').replace('Z','')`，等于把 UTC 时钟当本地时间显示。此处按浏览器
 * 时区渲染，并由页脚标注实际时区。固定 24 小时制 + 毫秒，故不走 toLocaleTimeString。
 */
function localClock(timestamp: string, withMs = false): string {
  const ms = Date.parse(timestamp)
  if (Number.isNaN(ms)) return timestamp
  const d = new Date(ms)
  const clock = `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
  return withMs ? `${clock}.${pad(d.getMilliseconds(), 3)}` : clock
}

/** 复制/导出用的完整本地时间戳「YYYY-MM-DD HH:MM:SS.mmm」 */
function localStamp(timestamp: string): string {
  const ms = Date.parse(timestamp)
  if (Number.isNaN(ms)) return timestamp
  const d = new Date(ms)
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${localClock(timestamp, true)}`
}

interface CollapsedRow {
  entry: LogEntry
  /** 该组合并的条数，1 表示未折叠 */
  repeat: number
  key: string
}

/**
 * 连续同 level + target + message 合并为一行。累加计数直接写在本函数新建的 row 上 ——
 * 逐次复制数组会退化为 O(n²)，而 rows 与其元素均未逃逸出构建过程。时间戳取该组首条。
 */
function collapseConsecutive(entries: LogEntry[]): CollapsedRow[] {
  const rows: CollapsedRow[] = []
  entries.forEach((entry, i) => {
    const last = rows[rows.length - 1]
    if (
      last &&
      last.entry.level === entry.level &&
      last.entry.target === entry.target &&
      last.entry.message === entry.message
    ) {
      last.repeat += 1
      return
    }
    rows.push({ entry, repeat: 1, key: `${entry.timestamp}-${i}` })
  })
  return rows
}

type LevelCounts = Record<LogLevel, number>

function countByLevel(entries: LogEntry[]): LevelCounts {
  const counts: LevelCounts = { ERROR: 0, WARN: 0, INFO: 0, DEBUG: 0, TRACE: 0 }
  for (const entry of entries) counts[entry.level] += 1
  return counts
}

/** 近 1 小时的 ERROR / WARN 条数；时间戳不可解析的条目跳过而非计入 */
function countRecent(entries: LogEntry[], now: number): { errors: number; warns: number } {
  const since = now - HOUR_MS
  let errors = 0
  let warns = 0
  for (const entry of entries) {
    const ts = Date.parse(entry.timestamp)
    if (Number.isNaN(ts) || ts < since) continue
    if (entry.level === 'ERROR') errors += 1
    else if (entry.level === 'WARN') warns += 1
  }
  return { errors, warns }
}

interface LogViewerPageProps {
  embedded?: boolean
  initialLevelFilter?: LevelFilter
  initialKeyword?: string
}

export function LogViewerPage({ embedded, initialLevelFilter = 'ALL', initialKeyword = '' }: LogViewerPageProps = {}) {
  const { t } = useTranslation()
  const [levelFilter, setLevelFilter] = useState<LevelFilter>(initialLevelFilter)
  const [keyword, setKeyword] = useState(initialKeyword)
  const [autoScroll, setAutoScroll] = useState(true)
  const [paused, setPaused] = useState(false)
  const [collapseRepeats, setCollapseRepeats] = useState(true)
  const [localLogs, setLocalLogs] = useState<LogEntry[]>([])

  const logEndRef = useRef<HTMLDivElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const autoScrollRef = useRef(true)

  const { logs, connected, clear } = useLogStream(true)

  // 暂停只冻结显示，不断开 SSE；恢复时一次性追上期间积压的日志
  useEffect(() => {
    if (paused) return
    setLocalLogs(logs)
  }, [logs, paused])

  const counts = useMemo(() => countByLevel(localLogs), [localLogs])
  // 依赖 localLogs 重算即可：日志流入本身就是心跳；暂停时数字随画面一同冻结
  const recent = useMemo(() => countRecent(localLogs, Date.now()), [localLogs])
  // 计数并列时 reduce 初值 'INFO' 使其胜出 —— 确定性平局规则，非偏好
  const { topLevel, topShare, observedLevels } = useMemo(() => {
    const top = LEVELS.reduce((acc, level) => (counts[level] > counts[acc] ? level : acc), 'INFO' as LogLevel)
    return {
      topLevel: top,
      topShare: localLogs.length > 0 ? (counts[top] / localLogs.length) * 100 : 0,
      observedLevels: LEVELS.filter((level) => counts[level] > 0),
    }
  }, [counts, localLogs.length])

  const filteredLogs = useMemo(
    () =>
      localLogs.filter((entry) => {
        if (levelFilter !== 'ALL' && entry.level !== levelFilter) return false
        if (keyword) {
          const lower = keyword.toLowerCase()
          return (
            entry.message.toLowerCase().includes(lower) ||
            entry.target.toLowerCase().includes(lower)
          )
        }
        return true
      }),
    [localLogs, levelFilter, keyword]
  )

  // 折叠开启时才合并；关闭时也走同一 row 结构，日志流渲染无需分支
  const rows = useMemo<CollapsedRow[]>(
    () =>
      collapseRepeats
        ? collapseConsecutive(filteredLogs)
        : filteredLogs.map((entry, i) => ({ entry, repeat: 1, key: `${entry.timestamp}-${i}` })),
    [filteredLogs, collapseRepeats]
  )
  const collapsedCount = filteredLogs.length - rows.length

  // Auto-scroll to bottom when rendered rows change
  useEffect(() => {
    if (autoScrollRef.current && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: 'auto' })
    }
  }, [rows])

  // Keep ref in sync so scroll handler doesn't close over stale state
  useEffect(() => {
    autoScrollRef.current = autoScroll
  }, [autoScroll])

  const handleScroll = useCallback(() => {
    const el = containerRef.current
    if (!el) return
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 50
    if (atBottom !== autoScrollRef.current) {
      setAutoScroll(atBottom)
    }
  }, [])

  /** 一行一条；「下载」与「复制」共用同一序列化，避免两处漂移 */
  const serializeLogs = () =>
    filteredLogs
      .map((e) => `${localStamp(e.timestamp)} [${e.level}] ${e.target} ${e.message}`)
      .join('\n')

  // 取客户端缓冲区走 Blob，不请求后端：/api/admin/logs/download 仅支持 query 鉴权，
  // 走它会把 adminApiKey 留在浏览器历史与反代 access log 中
  const handleDownload = () => {
    const url = URL.createObjectURL(
      new Blob([serializeLogs()], { type: 'text/plain;charset=utf-8' })
    )
    try {
      const a = document.createElement('a')
      a.href = url
      a.download = `kiro2cc-logs-${new Date().toISOString().replace(/[:.]/g, '-')}.log`
      a.click()
    } finally {
      URL.revokeObjectURL(url)
    }
  }

  const handleCopy = async () => {
    const text = serializeLogs()
    try {
      await copyToClipboard(text)
      toast.success(t('logs.copiedLogsCount', { count: filteredLogs.length }))
    } catch (error) {
      toast.error(extractErrorMessage(error))
    }
  }

  // 两处都清：hook 内缓冲是数据源，本地镜像在暂停态下不会被 effect 同步
  const handleClear = () => {
    clear()
    setLocalLogs([])
  }

  // Scroll to bottom immediately when auto-scroll is turned on
  useEffect(() => {
    if (autoScroll && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: 'auto' })
    }
  }, [autoScroll])

  return (
    // 高度显式给定：日志流需内部滚动，而 dashboard 的 <main> 不是 flex 容器，flex-1 在此无效。
    // 56px 补偿 <main> 的 py-7（28px × 2）
    <div className={`flex flex-col ${embedded ? 'h-full' : 'h-[calc(100vh-56px)]'}`}>
      {!embedded && (
        <PageHead
          crumb={[t('dashboard.navSystem'), t('dashboard.navLogs')]}
          title={t('logs.realtimeLogsTitle')}
          note={t('logs.realtimeLogsSubtitle')}
          actions={
            <>
              <span
                className={`${TAG_BASE} ${
                  paused
                    ? 'border-hairline-2 bg-surface-3 text-ink-2'
                    : connected
                      ? 'border-ok-line bg-ok-soft text-ok'
                      : 'border-warn-line bg-warn-soft text-warn'
                }`}
              >
                <span
                  aria-hidden="true"
                  className={`size-[5px] flex-none rounded-full ${
                    paused ? 'bg-ink-3' : connected ? 'bg-ok' : 'animate-pulse bg-warn'
                  }`}
                />
                {paused
                  ? t('logs.statusPaused')
                  : connected
                    ? t('logs.connectedLabel')
                    : t('logs.reconnectingLabel')}
              </span>
              <Button variant="outline" onClick={() => setPaused((prev) => !prev)}>
                {paused ? <Play aria-hidden="true" /> : <Pause aria-hidden="true" />}
                {paused ? t('logs.resumeButton') : t('logs.pauseButton')}
              </Button>
            </>
          }
        />
      )}

      <MetricsBar>
        <Metric label={t('logs.metricBufferLabel')}>
          <MetricValue value={formatTokenCount(localLogs.length)} unit={`/ ${formatTokenCount(MAX_FRONT_LOGS)}`} />
          <MetricFoot className="truncate pr-12">
            <span className="shrink-0">{t('logs.metricRingBuffer')}</span>
            {localLogs.length > 0 && (
              <>
                <FootSep />
                <span className="truncate">{t('logs.metricEarliest', { time: localClock(localLogs[0].timestamp) })}</span>
              </>
            )}
          </MetricFoot>
          <MetricAside>
            <Ring
              percent={(localLogs.length / MAX_FRONT_LOGS) * 100}
              tone={localLogs.length >= MAX_FRONT_LOGS ? 'stroke-warn' : 'stroke-brand'}
              size={42}
            />
          </MetricAside>
        </Metric>

        <Metric label={t('logs.metricErrorsLabel')}>
          <MetricValue
            value={String(recent.errors)}
            unit={t('logs.unitEntries')}
            valueClass={recent.errors > 0 ? 'text-danger' : ''}
          />
          <MetricFoot className="truncate">
            <span className="truncate">{t('logs.metricLevelBuffered', { n: counts.ERROR })}</span>
          </MetricFoot>
        </Metric>

        <Metric label={t('logs.metricWarnsLabel')}>
          <MetricValue
            value={String(recent.warns)}
            unit={t('logs.unitEntries')}
            valueClass={recent.warns > 0 ? 'text-warn' : ''}
          />
          <MetricFoot className="truncate">
            <span className="truncate">{t('logs.metricLevelBuffered', { n: counts.WARN })}</span>
          </MetricFoot>
        </Metric>

        <Metric label={t('logs.metricLevelsLabel')}>
          <MetricValue
            value={localLogs.length > 0 ? topLevel : '—'}
            unit={localLogs.length > 0 ? t('logs.metricLevelShare', { percent: topShare.toFixed(0) }) : undefined}
          />
          <MetricFoot className="flex-wrap">
            {observedLevels.length === 0 ? (
              <span>{t('logs.metricNoLevels')}</span>
            ) : (
              observedLevels.map((level) => (
                <span key={level} className="inline-flex items-center gap-1">
                  <span aria-hidden="true" className={`size-[5px] flex-none rounded-full ${LEVEL_PIP[level]}`} />
                  {level}
                  <span className="font-mono tabular-nums text-ink-2">{counts[level]}</span>
                </span>
              ))
            )}
          </MetricFoot>
        </Metric>
      </MetricsBar>

      <Toolbar>
        <SearchBox
          value={keyword}
          onChange={setKeyword}
          placeholder={t('logs.keywordFilterPlaceholder')}
          clearLabel={t('logs.searchClear')}
        />
        <Segmented
          value={levelFilter}
          onChange={setLevelFilter}
          groupLabel={t('logs.filterGroupLabel')}
          options={[
            { key: 'ALL' as LevelFilter, label: t('logs.filterAll'), count: localLogs.length },
            ...SEG_LEVELS.map((level) => ({
              key: level as LevelFilter,
              label: level,
              count: counts[level],
              pipClass: LEVEL_PIP[level],
            })),
          ]}
        />
        <span aria-hidden="true" className="h-[18px] w-px shrink-0 bg-hairline-2" />
        <Button variant="outline" onClick={handleDownload}>
          <Download aria-hidden="true" />
          {t('logs.downloadLogsButton')}
        </Button>
        <button
          type="button"
          onClick={handleCopy}
          aria-label={t('logs.copyLogsButton')}
          title={t('logs.copyLogsButton')}
          className={ICON_BTN}
        >
          <Copy className="size-[14px]" aria-hidden="true" />
        </button>
        <Button variant="destructive" onClick={handleClear} disabled={localLogs.length === 0}>
          <Trash2 aria-hidden="true" />
          {t('logs.clearBufferButton')}
        </Button>
        <div className="ml-auto flex shrink-0 items-center gap-3.5">
          <Toggle checked={collapseRepeats} onChange={setCollapseRepeats} label={t('logs.toggleCollapse')} />
          <Toggle checked={autoScroll} onChange={setAutoScroll} label={t('logs.autoScrollLabel')} />
        </div>
      </Toolbar>

      {/* 设计稿 .logpanel：与 PANEL 同壳，另加 flex 列与 min-h-0 供内部滚动 */}
      <section className={`${PANEL} mb-[22px] flex min-h-0 flex-1 flex-col`}>
        <div ref={containerRef} onScroll={handleScroll} className="logstream">
          {rows.length === 0 ? (
            <div className="px-[14px] py-8 text-center text-[11.5px] text-ink-3">
              {localLogs.length === 0 ? t('logs.emptyStreamHint') : t('logs.emptyNoMatch')}
            </div>
          ) : (
            rows.map(({ entry, repeat, key }) => (
              <div key={key} className={`logrow${entry.level === 'ERROR' ? ' is-err' : ''}`}>
                <span className="ts">{localClock(entry.timestamp, true)}</span>
                <span className={LV_CLASS[entry.level]}>{entry.level}</span>
                {/* .tgt 已有 min-width:250px，配 max-w 后为定宽并截断 */}
                <span className="tgt max-w-[250px] truncate">{entry.target}</span>
                {/* 后端 message 为纯文本，不做 .msg b 富文本注入（避免 XSS） */}
                <span className="msg break-all">{entry.message}</span>
                {repeat > 1 && (
                  <span
                    title={t('logs.repeatTimes', { n: repeat })}
                    className="flex-none rounded-[4px] border border-hairline-2 bg-surface-3 px-1 text-[9.5px] font-semibold text-ink-3"
                  >
                    ×{repeat}
                  </span>
                )}
              </div>
            ))
          )}
          <div ref={logEndRef} />
        </div>

        <div className={PANEL_FOOT}>
          {collapsedCount > 0 && <span>{t('logs.footCollapsedLabel')}</span>}
          <span>
            {t('logs.displayedBufferedCount', {
              shown: formatTokenCount(rows.length),
              total: formatTokenCount(localLogs.length),
            })}
          </span>
          {collapsedCount > 0 && (
            <button type="button" onClick={() => setCollapseRepeats(false)} className={FOOT_BTN}>
              {t('logs.expandAll')}
            </button>
          )}
          <div className="ml-auto flex flex-wrap items-center gap-2">
            {keyword && <span>{t('logs.filterKeywordPrefix', { keyword })}</span>}
            <span>
              {t('logs.levelLabel', { level: levelFilter === 'ALL' ? t('logs.filterAll') : levelFilter })}
            </span>
            <span aria-hidden="true">|</span>
            <span>{t('logs.footTimezone', { tz: TZ_LABEL })}</span>
          </div>
        </div>
      </section>
    </div>
  )
}
