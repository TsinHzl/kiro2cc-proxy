// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useRef, useEffect, useCallback, useMemo } from 'react'
import { useLogStream, type LogEntry } from '@/hooks/use-log-stream'
import { storage } from '@/lib/storage'
import { copyToClipboard } from '@/lib/clipboard'

type LevelFilter = 'ALL' | 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR'

const LEVEL_FILTERS: LevelFilter[] = ['ALL', 'DEBUG', 'INFO', 'WARN', 'ERROR']

function levelColor(level: string): string {
  switch (level) {
    case 'TRACE': return '#6e7681'
    case 'DEBUG': return '#6e7681'
    case 'INFO':  return '#58a6ff'
    case 'WARN':  return '#f0b429'
    case 'ERROR': return '#f85149'
    default:      return '#e6edf3'
  }
}

function rowBackground(level: string): string {
  if (level === 'WARN')  return 'rgba(240,180,41,0.08)'
  if (level === 'ERROR') return 'rgba(248,81,73,0.08)'
  return 'transparent'
}

function formatTimestamp(ts: string): string {
  // "2026-06-08T10:23:44.123Z" → "2026-06-08 10:23:44.123"
  return ts.replace('T', ' ').replace('Z', '')
}

interface LogViewerPageProps {
  embedded?: boolean
  initialLevelFilter?: LevelFilter
  initialKeyword?: string
}

export function LogViewerPage({ embedded, initialLevelFilter = 'ALL', initialKeyword = '' }: LogViewerPageProps = {}) {
  const [levelFilter, setLevelFilter] = useState<LevelFilter>(initialLevelFilter)
  const [keyword, setKeyword] = useState(initialKeyword)
  const [autoScroll, setAutoScroll] = useState(true)
  const [localLogs, setLocalLogs] = useState<LogEntry[]>([])
  const [copyToast, setCopyToast] = useState(false)

  const logEndRef = useRef<HTMLDivElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const autoScrollRef = useRef(true)

  const { logs, connected } = useLogStream(true)

  // Sync hook logs into local state (allows "clear" to work independently)
  useEffect(() => {
    setLocalLogs(logs)
  }, [logs])

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

  // Auto-scroll to bottom when filtered logs change
  useEffect(() => {
    if (autoScrollRef.current && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: 'auto' })
    }
  }, [filteredLogs])

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

  const handleDownload = () => {
    const apiKey = storage.getApiKey()
    if (!apiKey) return
    window.open(
      `/api/admin/logs/download?api_key=${encodeURIComponent(apiKey)}`,
      '_blank'
    )
  }

  const handleCopy = async () => {
    const text = filteredLogs
      .map((e) => `${formatTimestamp(e.timestamp)} [${e.level}] ${e.target} ${e.message}`)
      .join('\n')
    await copyToClipboard(text)
    setCopyToast(true)
    setTimeout(() => setCopyToast(false), 2000)
  }

  const handleClear = () => setLocalLogs([])

  const toggleAutoScroll = () => setAutoScroll((prev) => !prev)

  // Scroll to bottom immediately when auto-scroll is turned on
  useEffect(() => {
    if (autoScroll && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: 'auto' })
    }
  }, [autoScroll])

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: embedded ? '100%' : 'calc(100vh - 56px)' }}>
      {/* Page Header */}
      {!embedded && (
        <div className="mb-4">
          <h1 className="text-[22px] font-bold tracking-[-0.02em]">实时日志</h1>
          <p className="text-[13px] text-muted-foreground mt-0.5">
            实时查看服务运行日志，最近 1000 条
          </p>
        </div>
      )}

      {/* Terminal area */}
      <div
        style={{
          flex: 1,
          background: '#0d1117',
          borderRadius: 8,
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
          border: '1px solid #21262d',
          minHeight: 0,
        }}
      >
        {/* Toolbar */}
        <div
          style={{
            padding: '10px 16px',
            background: '#161b22',
            borderBottom: '1px solid #21262d',
            display: 'flex',
            alignItems: 'center',
            gap: 8,
            flexWrap: 'wrap',
            flexShrink: 0,
          }}
        >
          {/* Level filter buttons */}
          <div style={{ display: 'flex', gap: 4 }}>
            {LEVEL_FILTERS.map((level) => (
              <button
                key={level}
                onClick={() => setLevelFilter(level)}
                style={{
                  padding: '3px 10px',
                  borderRadius: 4,
                  fontSize: 11,
                  border: '1px solid #30363d',
                  background: levelFilter === level ? '#388bfd20' : '#21262d',
                  color:
                    levelFilter === level
                      ? '#388bfd'
                      : levelColor(level === 'ALL' ? 'INFO' : level),
                  cursor: 'pointer',
                  fontWeight: levelFilter === level ? 600 : 400,
                }}
              >
                {level}
              </button>
            ))}
          </div>

          {/* Keyword filter */}
          <input
            type="text"
            placeholder="关键词过滤..."
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            style={{
              flex: 1,
              minWidth: 120,
              background: '#21262d',
              border: '1px solid #30363d',
              borderRadius: 4,
              padding: '4px 10px',
              fontSize: 11,
              color: '#e6edf3',
              outline: 'none',
            }}
          />

          {/* Auto-scroll toggle */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <span style={{ fontSize: 11, color: '#8b949e' }}>自动滚动</span>
            <div
              onClick={toggleAutoScroll}
              style={{
                width: 28,
                height: 16,
                borderRadius: 8,
                cursor: 'pointer',
                background: autoScroll ? '#388bfd' : '#30363d',
                position: 'relative',
                transition: 'background 0.2s',
              }}
            >
              <div
                style={{
                  width: 12,
                  height: 12,
                  background: 'white',
                  borderRadius: '50%',
                  position: 'absolute',
                  top: 2,
                  left: autoScroll ? 14 : 2,
                  transition: 'left 0.2s',
                }}
              />
            </div>
          </div>

          <button
            onClick={handleClear}
            style={{
              padding: '4px 10px',
              borderRadius: 4,
              fontSize: 11,
              border: '1px solid #30363d',
              background: '#21262d',
              color: '#8b949e',
              cursor: 'pointer',
            }}
          >
            清空
          </button>

          <button
            onClick={handleCopy}
            style={{
              padding: '4px 10px',
              borderRadius: 4,
              fontSize: 11,
              border: '1px solid #30363d',
              background: '#21262d',
              color: '#8b949e',
              cursor: 'pointer',
            }}
          >
            📋 复制日志
          </button>

          <button
            onClick={handleDownload}
            style={{
              padding: '4px 12px',
              borderRadius: 4,
              fontSize: 11,
              border: '1px solid #388bfd',
              background: '#388bfd20',
              color: '#388bfd',
              cursor: 'pointer',
              fontWeight: 600,
            }}
          >
            ⬇ 下载日志
          </button>

          {/* Connection status */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
            <div
              style={{
                width: 6,
                height: 6,
                borderRadius: '50%',
                background: connected ? '#3fb950' : '#f0b429',
              }}
            />
            <span
              style={{
                fontSize: 11,
                color: connected ? '#3fb950' : '#f0b429',
              }}
            >
              {connected ? '已连接' : '重连中...'}
            </span>
          </div>
        </div>

        {/* Log lines */}
        <div
          ref={containerRef}
          onScroll={handleScroll}
          style={{
            flex: 1,
            overflowY: 'auto',
            padding: '8px 0',
            fontFamily: "'SF Mono', 'Fira Code', 'Courier New', monospace",
            fontSize: 11,
            lineHeight: '1.7',
            minHeight: 0,
          }}
        >
          {filteredLogs.map((entry, i) => (
            <div
              key={`${entry.timestamp}-${i}`}
              style={{
                padding: '1px 16px',
                display: 'flex',
                gap: 10,
                alignItems: 'baseline',
                background: rowBackground(entry.level),
              }}
            >
              <span
                style={{ color: '#8b949e', minWidth: 170, flexShrink: 0, whiteSpace: 'nowrap' }}
              >
                {formatTimestamp(entry.timestamp)}
              </span>
              <span
                style={{
                  color: levelColor(entry.level),
                  background: `${levelColor(entry.level)}22`,
                  padding: '0 4px',
                  borderRadius: 2,
                  fontSize: 9,
                  minWidth: 40,
                  textAlign: 'center',
                  flexShrink: 0,
                }}
              >
                {entry.level}
              </span>
              <span
                style={{
                  color: '#6e7681',
                  flexShrink: 0,
                  fontSize: 9,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                  maxWidth: 280,
                }}
              >
                {entry.target}
              </span>
              <span
                style={{
                  color: entry.level === 'DEBUG' || entry.level === 'TRACE'
                    ? '#6e7681'
                    : '#e6edf3',
                  flex: 1,
                  wordBreak: 'break-all',
                }}
              >
                {entry.message}
              </span>
            </div>
          ))}
          <div ref={logEndRef} />
        </div>

        {/* Footer status bar */}
        <div
          style={{
            padding: '4px 16px',
            background: '#161b22',
            borderTop: '1px solid #21262d',
            display: 'flex',
            justifyContent: 'space-between',
            fontSize: 10,
            color: '#8b949e',
            flexShrink: 0,
          }}
        >
          <span>
            已显示 {filteredLogs.length} 条（缓冲 {localLogs.length} 条）
          </span>
          <span>
            {keyword ? `过滤: "${keyword}" | ` : ''}级别: {levelFilter}
          </span>
        </div>
      </div>
      {copyToast && (
        <div
          style={{
            position: 'fixed',
            bottom: 32,
            left: '50%',
            transform: 'translateX(-50%)',
            background: '#2ea043',
            color: 'white',
            padding: '8px 20px',
            borderRadius: 6,
            fontSize: 13,
            fontWeight: 500,
            boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
            zIndex: 9999,
          }}
        >
          已复制 {filteredLogs.length} 条日志
        </div>
      )}
    </div>
  )
}
