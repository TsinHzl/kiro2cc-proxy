// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { memo, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import {
  AlertCircle,
  ArrowDown,
  ArrowUp,
  Brain,
  Check,
  ChevronRight,
  Coins,
  Copy,
  Globe,
  Loader2,
  MessageSquarePlus,
  RotateCw,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { copyToClipboard } from '@/lib/clipboard'
import { toast } from '@/lib/toast'
import { AttachmentView } from './attachment-view'
import { estimateTokens } from './estimate-tokens'
import { Markdown } from './markdown'
import type { ChatMessage, WebSearchCall } from './types'

/** 距底部小于该值视为「正在跟随最新消息」，新内容到达时自动滚到底 */
const STICK_THRESHOLD_PX = 80

function formatSeconds(ms: number): string {
  return `${(ms / 1000).toFixed(1)}s`
}

function ActionButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      onClick={onClick}
      className="grid size-7 place-items-center rounded-md text-ink-3 transition-colors hover:bg-surface-3 hover:text-ink"
    >
      {children}
    </button>
  )
}

function ReasoningBlock({ message }: { message: ChatMessage }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState<boolean | null>(null)
  const boxRef = useRef<HTMLDivElement>(null)
  const streaming = message.status === 'streaming'
  // reasoningMs 在思考文字全部显示完之后才会写入，所以思考期间展开、打完自动折叠；
  // 用户手动切换后以其选择为准
  const thinking = streaming && message.reasoningMs === undefined
  const isOpen = open ?? thinking

  // 思考文字逐字增长，超出框高后让框内容跟着滚到最新一行
  useLayoutEffect(() => {
    const el = boxRef.current
    if (thinking && el) el.scrollTop = el.scrollHeight
  }, [message.reasoning, thinking, isOpen])

  return (
    <div className="mb-2">
      <button
        type="button"
        onClick={() => setOpen(!isOpen)}
        aria-expanded={isOpen}
        className="inline-flex items-center gap-1.5 rounded-md py-1 text-[12px] text-ink-3 transition-colors hover:text-ink-2"
      >
        {thinking ? <Loader2 className="size-3.5 animate-spin" /> : <Brain className="size-3.5" />}
        <span>
          {thinking
            ? t('playground.thinking')
            : message.reasoningMs !== undefined
              ? t('playground.thought', { seconds: formatSeconds(message.reasoningMs) })
              : t('playground.thoughtNoTime')}
        </span>
        <ChevronRight className={`size-3 transition-transform ${isOpen ? 'rotate-90' : ''}`} />
      </button>
      {isOpen && (
        <div
          ref={boxRef}
          className="mt-1 max-h-72 overflow-y-auto whitespace-pre-wrap break-words border-l-2 border-hairline-2 pl-3 text-[12.5px] leading-[1.7] text-ink-3"
        >
          {message.reasoning}
        </div>
      )}
    </div>
  )
}

/** 只允许 http(s) 链接可点击；搜索结果来自外部网页，不能把 javascript: 之类的地址放进 href */
function isHttpUrl(url: string): boolean {
  return /^https?:\/\//i.test(url)
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname
  } catch {
    return url
  }
}

function SearchBlock({ search }: { search: WebSearchCall }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const searching = search.status === 'searching'
  const query = search.query || '…'
  const label = searching
    ? t('playground.searching', { query })
    : t('playground.searched', { query })
  const detail =
    search.status === 'error'
      ? t('playground.searchFailed')
      : search.status === 'stopped'
        ? t('playground.searchStopped')
        : searching
          ? null
          : search.results.length === 0
            ? t('playground.searchNoResults')
            : t('playground.searchResultCount', { count: search.results.length })
  const expandable = search.results.length > 0

  return (
    <div className="mb-2">
      <button
        type="button"
        onClick={() => expandable && setOpen(!open)}
        aria-expanded={expandable ? open : undefined}
        className={`inline-flex max-w-full items-center gap-1.5 rounded-md py-1 text-[12px] text-ink-3 transition-colors ${
          expandable ? 'hover:text-ink-2' : 'cursor-default'
        }`}
      >
        {searching ? <Loader2 className="size-3.5 shrink-0 animate-spin" /> : <Globe className="size-3.5 shrink-0" />}
        <span className="min-w-0 truncate">{label}</span>
        {detail && <span className="shrink-0 text-ink-3/80">· {detail}</span>}
        {expandable && (
          <ChevronRight className={`size-3 shrink-0 transition-transform ${open ? 'rotate-90' : ''}`} />
        )}
      </button>
      {open && expandable && (
        <ul className="mt-1 max-h-72 space-y-2 overflow-y-auto border-l-2 border-hairline-2 pl-3">
          {search.results.map((r, i) => (
            <li key={`${r.url}-${i}`} className="min-w-0 text-[12.5px] leading-[1.6]">
              {isHttpUrl(r.url) ? (
                <a
                  href={r.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="block truncate font-medium text-ink-2 hover:text-brand hover:underline"
                >
                  {r.title}
                </a>
              ) : (
                <span className="block truncate font-medium text-ink-2">{r.title}</span>
              )}
              <div className="truncate text-[11px] text-ink-3">{hostOf(r.url)}</div>
              {r.snippet && <p className="line-clamp-2 break-words text-ink-3">{r.snippet}</p>}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** 用量：↓ 输入 tokens、↑ 输出 tokens、思考 tokens 估算、本条回复的 credits；含义放在悬停提示里 */
function UsageStats({ usage, reasoning }: { usage: NonNullable<ChatMessage['usage']>; reasoning?: string }) {
  const { t } = useTranslation()
  const n = (v: number) => v.toLocaleString()
  const inputTip = usage.cached
    ? t('playground.usageInputCachedTip', { total: n(usage.input), cached: n(usage.cached) })
    : t('playground.usageInputTip', { total: n(usage.input) })
  const outputTip = t('playground.usageOutputTip', { total: n(usage.output) })
  const thinkingTokens = reasoning ? estimateTokens(reasoning) : 0
  const thinkingTip = t('playground.usageThinkingTip', { total: n(thinkingTokens) })
  const creditsText = (usage.credits ?? 0).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 4,
  })
  const creditsTip = t('playground.usageCreditsTip', { total: creditsText })
  return (
    <>
      <span className="inline-flex shrink-0 items-center gap-0.5" title={inputTip} aria-label={inputTip}>
        <ArrowDown className="size-3" aria-hidden="true" />
        {n(usage.input)} tokens
      </span>
      <span className="inline-flex shrink-0 items-center gap-0.5" title={outputTip} aria-label={outputTip}>
        <ArrowUp className="size-3" aria-hidden="true" />
        {n(usage.output)} tokens
      </span>
      {thinkingTokens > 0 && (
        <span className="inline-flex shrink-0 items-center gap-0.5" title={thinkingTip} aria-label={thinkingTip}>
          <Brain className="size-3" aria-hidden="true" />≈{n(thinkingTokens)} tokens
        </span>
      )}
      {usage.credits !== undefined && (
        <span className="inline-flex shrink-0 items-center gap-0.5" title={creditsTip} aria-label={creditsTip}>
          <Coins className="size-3" aria-hidden="true" />
          {creditsText} credits
        </span>
      )}
    </>
  )
}

interface MessageItemProps {
  message: ChatMessage
  canRegenerate: boolean
  onRegenerate: () => void
}

const MessageItem = memo(function MessageItem({ message, canRegenerate, onRegenerate }: MessageItemProps) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)

  const handleCopy = async () => {
    try {
      await copyToClipboard(message.content)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      toast.error(t('common.copyFailed'))
    }
  }

  if (message.role === 'user') {
    return (
      <div className="group flex flex-col items-end gap-2 py-2.5">
        {message.attachments && message.attachments.length > 0 && (
          <div className="flex max-w-[85%] flex-wrap justify-end gap-2">
            {message.attachments.map((a) => (
              <AttachmentView key={a.id} attachment={a} large />
            ))}
          </div>
        )}
        {message.content !== '' && (
          <>
            <div className="max-w-[85%] whitespace-pre-wrap break-words rounded-[16px] bg-surface-3 px-3.5 py-2.5 text-[13.5px] leading-[1.7] text-ink">
              {message.content}
            </div>
            {/* 悬停或键盘聚焦时显示；触屏设备没有悬停，始终显示 */}
            <div className="-mt-1 flex opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100 [@media(hover:none)]:opacity-100">
              <ActionButton label={copied ? t('common.copied') : t('common.copy')} onClick={handleCopy}>
                {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
              </ActionButton>
            </div>
          </>
        )}
      </div>
    )
  }

  const streaming = message.status === 'streaming'
  // 搜索进行中时搜索块自带转圈，不再叠加跳动的圆点
  const searchPending = message.searches?.some((s) => s.status === 'searching') ?? false
  const waiting = streaming && message.content === '' && !message.reasoning && !searchPending

  return (
    <div className="group py-2.5">
      {message.reasoning && <ReasoningBlock message={message} />}
      {message.searches?.map((s) => <SearchBlock key={s.id} search={s} />)}
      {message.content !== '' && <Markdown text={message.content} />}
      {waiting && (
        <div className="flex items-center gap-1 py-2" aria-label={t('playground.generating')}>
          {[0, 150, 300].map((delay) => (
            <span
              key={delay}
              className="size-1.5 animate-pulse rounded-full bg-ink-3"
              style={{ animationDelay: `${delay}ms` }}
            />
          ))}
        </div>
      )}
      {message.status === 'stopped' && (
        <p className="mt-1 text-[11.5px] text-ink-3">{t('playground.stopped')}</p>
      )}
      {message.status === 'error' && (
        <div className="mt-2 flex items-start gap-2 rounded-[10px] bg-danger-soft px-3 py-2.5 text-[12.5px] text-danger">
          <AlertCircle className="mt-px size-4 shrink-0" />
          <span className="min-w-0 break-words">{message.error ?? t('playground.requestFailed')}</span>
        </div>
      )}
      {!streaming && (
        <div className="mt-1.5 flex items-center gap-0.5">
          {message.content !== '' && (
            <ActionButton label={copied ? t('common.copied') : t('common.copy')} onClick={handleCopy}>
              {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
            </ActionButton>
          )}
          {canRegenerate && (
            <ActionButton label={t('playground.regenerate')} onClick={onRegenerate}>
              <RotateCw className="size-3.5" />
            </ActionButton>
          )}
          {(message.model || message.usage) && (
            <span className="ml-2 flex min-w-0 items-center gap-2.5 text-[11px] text-ink-3">
              {message.model && <span className="truncate">{message.model}</span>}
              {message.usage && <UsageStats usage={message.usage} reasoning={message.reasoning} />}
            </span>
          )}
        </div>
      )}
    </div>
  )
})

function EmptyState({ hasKey }: { hasKey: boolean }) {
  const { t } = useTranslation()
  return (
    <div className="flex h-full min-h-[240px] flex-col items-center justify-center px-4 text-center">
      <div className="mb-5 grid size-11 place-items-center rounded-xl border-[0.5px] border-hairline-2 bg-surface-2 text-ink-2">
        <MessageSquarePlus className="size-5" aria-hidden="true" />
      </div>
      <h2 className="text-[22px] font-semibold tracking-[-.02em]">{t('playground.emptyTitle')}</h2>
      <p className="mt-2 max-w-md text-[13px] leading-6 text-ink-3">
        {hasKey ? t('playground.emptyHint') : t('playground.noKeyHint')}
      </p>
    </div>
  )
}

interface MessageListProps {
  messages: ChatMessage[]
  isGenerating: boolean
  hasKey: boolean
  onRegenerate: () => void
}

export function MessageList({ messages, isGenerating, hasKey, onRegenerate }: MessageListProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const stickRef = useRef(true)
  const prevLengthRef = useRef(messages.length)

  useLayoutEffect(() => {
    const el = containerRef.current
    if (!el) return
    // 新增消息（用户刚发送 / 重新生成）时无条件回到底部；流式追加只在用户没有上滚时跟随
    if (messages.length > prevLengthRef.current) stickRef.current = true
    prevLengthRef.current = messages.length
    if (stickRef.current) el.scrollTop = el.scrollHeight
  }, [messages])

  const handleScroll = () => {
    const el = containerRef.current
    if (!el) return
    stickRef.current = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_THRESHOLD_PX
  }

  const lastAssistantId = [...messages].reverse().find((m) => m.role === 'assistant')?.id

  return (
    <div ref={containerRef} onScroll={handleScroll} className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto h-full w-full px-4 py-4 md:w-[95%] md:px-0">
        {messages.length === 0 ? (
          <EmptyState hasKey={hasKey} />
        ) : (
          messages.map((m) => (
            <MessageItem
              key={m.id}
              message={m}
              canRegenerate={!isGenerating && m.id === lastAssistantId}
              onRegenerate={onRegenerate}
            />
          ))
        )}
      </div>
    </div>
  )
}
