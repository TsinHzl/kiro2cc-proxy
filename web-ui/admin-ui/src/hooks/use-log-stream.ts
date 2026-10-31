// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useEffect, useCallback, useRef } from 'react'
import { storage } from '@/lib/storage'

/** log 事件的合批窗口（ms）：窗口内的日志只触发一次 setState / re-render */
const FLUSH_INTERVAL_MS = 150

export interface LogEntry {
  timestamp: string
  level: 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR'
  target: string
  message: string
}

/** 前端环形缓冲上限；页面侧的容量指标卡直接复用此常量 */
export const MAX_FRONT_LOGS = 2000

const logKey = (e: LogEntry) => `${e.timestamp}|${e.level}|${e.target}|${e.message}`

/**
 * 上次离开日志页时的显示内容，模块级保留到浏览器刷新为止。日志页每次进入都会重新挂载并
 * 重新取快照，远程部署时这一个往返约 1 秒；先展示上次内容，首份快照 / history 到达后整体切换。
 * 不与缓存合并：缓存可能早于快照窗口，mergeHistory 会把旧条目排到新条目之后
 */
let lastLogs: LogEntry[] = []

/**
 * 把历史快照并入已有日志：快照在前，已有但快照里没有的条目（更新的实时日志）追加在后。
 * 快照 / SSE history / 实时推送三路来源互相重叠，按内容去重，谁先到都不丢不重。
 */
function mergeHistory(prev: LogEntry[], entries: LogEntry[]): LogEntry[] {
  if (prev.length === 0) return entries.slice(-MAX_FRONT_LOGS)
  const seen = new Set(entries.map(logKey))
  const extras = prev.filter((e) => !seen.has(logKey(e)))
  return [...entries, ...extras].slice(-MAX_FRONT_LOGS)
}

export function useLogStream(enabled: boolean): {
  logs: LogEntry[]
  connected: boolean
  /** 清空缓冲：必须由 hook 内部清，否则下一条日志到达时旧内容会整体回填 */
  clear: () => void
} {
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [initialLogs] = useState(() => lastLogs)
  /** 是否已收到首份快照 / history；收到前对外展示 initialLogs */
  const [synced, setSynced] = useState(false)
  const [connected, setConnected] = useState(false)
  const esRef = useRef<EventSource | null>(null)
  const mountedRef = useRef(true)
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const reconnectDelay = useRef(1000)

  const pendingRef = useRef<LogEntry[]>([])
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  /** 把合批缓冲一次性并入 logs；clear/断连时也要调用以避免旧内容回填 */
  const flush = useCallback(() => {
    flushTimerRef.current = null
    if (pendingRef.current.length === 0) return
    const incoming = pendingRef.current
    pendingRef.current = []
    setLogs((prev) => {
      // 与快照 / history 可能重叠（订阅与取快照之间产生的日志），按内容去重
      const seen = new Set(prev.map(logKey))
      const fresh = incoming.filter((e) => !seen.has(logKey(e)))
      if (fresh.length === 0) return prev
      return prev.length + fresh.length > MAX_FRONT_LOGS
        ? [...prev, ...fresh].slice(-MAX_FRONT_LOGS)
        : [...prev, ...fresh]
    })
  }, [])

  const clear = useCallback(() => {
    pendingRef.current = []
    if (flushTimerRef.current) {
      clearTimeout(flushTimerRef.current)
      flushTimerRef.current = null
    }
    setLogs([])
    setSynced(true)
  }, [])

  const connect = useCallback(() => {
    if (esRef.current) return
    const apiKey = storage.getApiKey()
    if (!apiKey) return

    // 立即 REST 取快照，不等 SSE 建立：反向代理 / CDN 缓冲 SSE 时，EventSource 的
    // onopen 与 history 事件可能很久才到，等它们再取快照会导致日志页长时间空白
    fetch(`/api/admin/logs/snapshot?api_key=${encodeURIComponent(apiKey)}&limit=${MAX_FRONT_LOGS}`)
      .then((r) => (r.ok ? r.json() : []))
      .then((entries: LogEntry[]) => {
        if (mountedRef.current && Array.isArray(entries)) {
          setLogs((prev) => mergeHistory(prev, entries))
          setSynced(true)
        }
      })
      .catch(() => {})

    const es = new EventSource(
      `/api/admin/logs/stream?api_key=${encodeURIComponent(apiKey)}&limit=${MAX_FRONT_LOGS}`
    )
    esRef.current = es

    es.onopen = () => {
      setConnected(true)
      reconnectDelay.current = 1000
    }

    es.addEventListener('history', (e: MessageEvent) => {
      try {
        const entries: LogEntry[] = JSON.parse(e.data)
        if (Array.isArray(entries)) {
          setLogs((prev) => mergeHistory(prev, entries))
          setSynced(true)
        }
      } catch {
        // ignore malformed history payload
      }
    })

    es.addEventListener('log', (e: MessageEvent) => {
      try {
        const entry: LogEntry = JSON.parse(e.data)
        // 合批缓冲：SSE 逐条推送时每条一次 setState + O(n) 复制会让页面在
        // 日志多时整段卡顿；先攒入 pending 再由定时器一次性 flush。
        pendingRef.current.push(entry)
        // 缓冲加上限：上游日志洪峰时避免 pending 无界增长
        if (pendingRef.current.length > MAX_FRONT_LOGS) pendingRef.current.shift()
        if (!flushTimerRef.current) {
          flushTimerRef.current = setTimeout(flush, FLUSH_INTERVAL_MS)
        }
      } catch {
        // ignore malformed log entry
      }
    })

    es.onerror = () => {
      if (!mountedRef.current) return
      setConnected(false)
      es.close()
      esRef.current = null
      const delay = reconnectDelay.current
      reconnectDelay.current = Math.min(delay * 2, 30000)
      reconnectTimer.current = setTimeout(connect, delay)
    }
  }, [flush])

  useEffect(() => {
    if (!enabled) {
      esRef.current?.close()
      esRef.current = null
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current)
      setConnected(false)
      pendingRef.current = []
      if (flushTimerRef.current) {
        clearTimeout(flushTimerRef.current)
        flushTimerRef.current = null
      }
      setLogs([])
      return
    }

    mountedRef.current = true
    connect()

    return () => {
      mountedRef.current = false
      esRef.current?.close()
      esRef.current = null
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current)
      if (flushTimerRef.current) {
        clearTimeout(flushTimerRef.current)
        flushTimerRef.current = null
      }
      pendingRef.current = []
    }
  }, [enabled, connect])

  useEffect(() => {
    if (synced) lastLogs = logs
  }, [logs, synced])

  return { logs: synced ? logs : initialLogs, connected, clear }
}
