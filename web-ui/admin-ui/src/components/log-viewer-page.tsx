// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useRef, useEffect, useCallback, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from '@/lib/toast'
import { Copy, Download, Pause, Play, Trash2 } from 'lucide-react'
import { useLogStream, MAX_FRONT_LOGS, type LogEntry } from '@/hooks/use-log-stream'
import { copyToClipboard } from '@/lib/clipboard'
import { storage } from '@/lib/storage'
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

/** 「下载」向服务端取回的最大条数，与后端 ring buffer 容量（LOG_CAPTURE_CAPACITY）一致 */
const DOWNLOAD_LIMIT = 100_000

/** 级别 + 关键字过滤；页面显示与下载共用，保证两者口径一致 */
function matchesFilter(entry: LogEntry, levelFilter: LogLevel | 'ALL', keyword: string): boolean {
  if (levelFilter !== 'ALL' && entry.level !== levelFilter) return false
  if (keyword) {
    const lower = keyword.toLowerCase()
    return entry.message.toLowerCase().includes(lower) || entry.target.toLowerCase().includes(lower)
  }
  return true
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

  const containerRef = useRef<HTMLDivElement>(null)
  const autoScrollRef = useRef(true)

  const { logs, connected, clear } = useLogStream(true)

  // 暂停只冻结显示，不断开 SSE；恢复时一次性追上期间积压的日志。
  // shownLogs 是「已冻结的显示帧」：暂停期间 hook 的 logs 继续累积但不入画，
  // 恢复后下一次 logs 变化把整段积压一次性带进来（天然合批，无逐条回放）。
  const [shownLogs, setShownLogs] = useState<LogEntry[]>([])
  useEffect(() => {
    if (paused) return
    setShownLogs(logs)
  }, [logs, paused])

  const counts = useMemo(() => countByLevel(shownLogs), [shownLogs])
  // 计数并列时 reduce 初值 'INFO' 使其胜出 —— 确定性平局规则，非偏好
  const { topLevel, topShare, observedLevels } = useMemo(() => {
    const top = LEVELS.reduce((acc, level) => (counts[level] > counts[acc] ? level : acc), 'INFO' as LogLevel)
    return {
      topLevel: top,
      topShare: shownLogs.length > 0 ? (counts[top] / shownLogs.length) * 100 : 0,
      observedLevels: LEVELS.filter((level) => counts[level] > 0),
    }
  }, [counts, shownLogs.length])

  // 近 1 小时计数跟随显示帧；日志流入本身就是心跳，暂停时数字随画面一同冻结
  const recent = useMemo(() => countRecent(shownLogs, Date.now()), [shownLogs])

  const filteredLogs = useMemo(
    () =>
      shownLogs.filter((entry) => matchesFilter(entry, levelFilter, keyword)),
    [shownLogs, levelFilter, keyword]
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

  // Auto-scroll to bottom when rendered rows change.
  // 不用 scrollIntoView：content-visibility 下视口外行未布局，浏览器定位可能不准。
  // 直接赋 scrollTop 后，滚动行为本身会让视口外的行实体化（行高从占位估算值变为
  // 实际值，scrollHeight 随之增长），因此用双 rAF 连续顶两次：首帧赋值触发实体化，
  // 次帧用更新后的 scrollHeight 再顶一次，确保真正落底。
  const scrollToBottom = useCallback(() => {
    const pin = () => {
      const el = containerRef.current
      if (el) el.scrollTop = el.scrollHeight
    }
    pin()
    requestAnimationFrame(() => requestAnimationFrame(pin))
  }, [])

  useEffect(() => {
    if (autoScrollRef.current) {
      scrollToBottom()
    }
  }, [rows, scrollToBottom])

  // Keep ref in sync so scroll handler doesn't close over stale state
  useEffect(() => {
    autoScrollRef.current = autoScroll
  }, [autoScroll])

  const handleScroll = useCallback(() => {
    const el = containerRef.current
    if (!el) return
    // 阈值放宽到 100：content-visibility 下行高为占位估算值，长消息行实际更高，
    // 过窄的判定带会在接近底部时误判为已离开底部而关闭自动滚动
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 100
    if (atBottom !== autoScrollRef.current) {
      setAutoScroll(atBottom)
    }
  }, [])

  /** 一行一条；「下载」与「复制」共用同一序列化，避免两处漂移 */
  const serializeLogs = (list: LogEntry[]) =>
    list
      .map((e) => `${localStamp(e.timestamp)} [${e.level}] ${e.target} ${e.message}`)
      .join('\n')

  // 页面只持有最新 2000 条；下载向服务端取回完整缓冲区（最多 DOWNLOAD_LIMIT 条），
  // 用 fetch + x-api-key 头鉴权，避免把 adminApiKey 放进 URL（浏览器历史 / 反代 access log）。
  // 过滤条件与页面一致；「清空」之后产生的日志才会被导出，与清空前的行为相同。
  const clearedAfterRef = useRef<string | null>(null)
  const [downloading, setDownloading] = useState(false)
  const handleDownload = async () => {
    if (downloading) return
    setDownloading(true)
    try {
      const res = await fetch(`/api/admin/logs/snapshot?limit=${DOWNLOAD_LIMIT}`, {
        headers: { 'x-api-key': storage.getApiKey() ?? '' },
        // 反代 / 网络挂起时不让按钮长时间停在禁用态；10 万条约 20MB，慢链路也留足时间
        signal: AbortSignal.timeout(120_000),
      })
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      const all = (await res.json()) as LogEntry[]
      const cutoff = clearedAfterRef.current
      const list = all.filter(
        (e) => (cutoff === null || e.timestamp > cutoff) && matchesFilter(e, levelFilter, keyword)
      )
      const url = URL.createObjectURL(
        new Blob([serializeLogs(list)], { type: 'text/plain;charset=utf-8' })
      )
      try {
        const a = document.createElement('a')
        a.href = url
        a.download = `kiro2cc-logs-${new Date().toISOString().replace(/[:.]/g, '-')}.log`
        a.click()
      } finally {
        URL.revokeObjectURL(url)
      }
    } catch (error) {
      toast.error(extractErrorMessage(error))
    } finally {
      setDownloading(false)
    }
  }

  const handleCopy = async () => {
    const text = serializeLogs(filteredLogs)
    try {
      await copyToClipboard(text)
      toast.success(t('logs.copiedLogsCount', { count: filteredLogs.length }))
    } catch (error) {
      toast.error(extractErrorMessage(error))
    }
  }

  const handleClear = () => {
    // 记下清空时最新一条的时间戳，之后的下载只导出更晚的日志
    clearedAfterRef.current = logs.length > 0 ? logs[logs.length - 1].timestamp : new Date().toISOString()
    clear()
    // shownLogs 是冻结显示帧，effect 在暂停态不会同步，需就地清空，
    // 否则计数卡与底部总条数在恢复播放前仍显示旧数据
    setShownLogs([])
  }

  // Scroll to bottom immediately when auto-scroll is turned on
  useEffect(() => {
    if (autoScroll) {
      scrollToBottom()
    }
  }, [autoScroll, scrollToBottom])

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
                    ? 'border-transparent bg-surface-3 text-ink-2'
                    : connected
                      ? 'border-transparent bg-ok-soft text-ok'
                      : 'border-transparent bg-warn-soft text-warn'
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
          <MetricValue value={formatTokenCount(shownLogs.length)} unit={`/ ${formatTokenCount(MAX_FRONT_LOGS)}`} />
          <MetricFoot className="truncate pr-12">
            <span className="shrink-0">{t('logs.metricRingBuffer')}</span>
            {shownLogs.length > 0 && (
              <>
                <FootSep />
                <span className="truncate">{t('logs.metricEarliest', { time: localClock(shownLogs[0].timestamp) })}</span>
              </>
            )}
          </MetricFoot>
          <MetricAside>
            <Ring
              percent={(shownLogs.length / MAX_FRONT_LOGS) * 100}
              tone={shownLogs.length >= MAX_FRONT_LOGS ? 'stroke-warn' : 'stroke-brand'}
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
            value={shownLogs.length > 0 ? topLevel : '—'}
            unit={shownLogs.length > 0 ? t('logs.metricLevelShare', { percent: topShare.toFixed(0) }) : undefined}
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
            { key: 'ALL' as LevelFilter, label: t('logs.filterAll'), count: shownLogs.length },
            ...SEG_LEVELS.map((level) => ({
              key: level as LevelFilter,
              label: level,
              count: counts[level],
              pipClass: LEVEL_PIP[level],
            })),
          ]}
        />
        <span aria-hidden="true" className="h-[18px] w-[0.5px] shrink-0 bg-hairline-2" />
        <Button variant="outline" onClick={handleDownload} disabled={downloading}>
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
        <Button variant="destructive" onClick={handleClear} disabled={shownLogs.length === 0}>
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
              {shownLogs.length === 0 ? t('logs.emptyStreamHint') : t('logs.emptyNoMatch')}
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
                    className="flex-none rounded-[4px] border border-transparent bg-surface-3 px-1 text-[9.5px] font-semibold text-ink-3"
                  >
                    ×{repeat}
                  </span>
                )}
              </div>
            ))
          )}
        </div>

        <div className={PANEL_FOOT}>
          {collapsedCount > 0 && <span>{t('logs.footCollapsedLabel')}</span>}
          <span>
            {t('logs.displayedBufferedCount', {
              shown: formatTokenCount(rows.length),
              total: formatTokenCount(shownLogs.length),
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
