// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useEffect, useCallback, useRef } from 'react'
import { storage } from '@/lib/storage'

export interface LogEntry {
  timestamp: string
  level: 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR'
  target: string
  message: string
}

const MAX_FRONT_LOGS = 2000

export function useLogStream(enabled: boolean): {
  logs: LogEntry[]
  connected: boolean
} {
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [connected, setConnected] = useState(false)
  const esRef = useRef<EventSource | null>(null)
  const mountedRef = useRef(true)
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const reconnectDelay = useRef(1000)

  const connect = useCallback(() => {
    if (esRef.current) return
    const apiKey = storage.getApiKey()
    if (!apiKey) return

    const es = new EventSource(
      `/api/admin/logs/stream?api_key=${encodeURIComponent(apiKey)}`
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
          setLogs(entries)
        }
      } catch {
        // ignore malformed history payload
      }
    })

    es.addEventListener('log', (e: MessageEvent) => {
      try {
        const entry: LogEntry = JSON.parse(e.data)
        setLogs((prev) => {
          const next = [...prev, entry]
          return next.length > MAX_FRONT_LOGS
            ? next.slice(next.length - MAX_FRONT_LOGS)
            : next
        })
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
  }, [])

  useEffect(() => {
    if (!enabled) {
      esRef.current?.close()
      esRef.current = null
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current)
      setConnected(false)
      setLogs([])
      return
    }

    connect()

    return () => {
      mountedRef.current = false
      esRef.current?.close()
      esRef.current = null
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current)
    }
  }, [enabled, connect])

  return { logs, connected }
}
