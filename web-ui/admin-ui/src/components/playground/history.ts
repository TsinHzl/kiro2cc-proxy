// Copyright (c) 2026 Harllan He. Licensed under MIT.
import type { ChatMessage } from './types'
import { thinkingTags } from './thinking-parser'

function repairReasoningTransitions(text: string): string {
  const tags = thinkingTags(text)
  let result = text
  for (let i = tags.length - 2; i >= 0; i--) {
    const close = tags[i]
    const open = tags[i + 1]
    if (close.opening || !open.opening || text.slice(close.end, open.index).trim() !== '') continue
    result = `${result.slice(0, close.index).trimEnd()}\n\n${result.slice(open.end).trimStart()}`
  }
  return result
}

/** 仅清理旧搜索回复思考区的分界标签；正文来源不明，保持原样。 */
function repairSearchMessage(message: ChatMessage): ChatMessage {
  if (message.role !== 'assistant' || !message.searches?.length) return message
  const reasoning = repairReasoningTransitions(message.reasoning ?? '')
  if (reasoning === (message.reasoning ?? '')) return message
  return { ...message, reasoning }
}

/** 仍在等待结果的搜索不会再收到结果（流已结束或被中断），改记为已中断 */
export function settleSearches(m: ChatMessage): ChatMessage {
  if (!m.searches?.some((s) => s.status === 'searching')) return m
  return { ...m, searches: m.searches.map((s) => (s.status === 'searching' ? { ...s, status: 'stopped' } : s)) }
}

/** 流式中途被中断（刷新 / 离开页面）而遗留的 streaming 状态不会再有后续数据，改记为已中断 */
export function normalizeMessages(raw: unknown): ChatMessage[] {
  if (!Array.isArray(raw)) return []
  return (raw as ChatMessage[]).map((m) =>
    repairSearchMessage(settleSearches(m.status === 'streaming' ? { ...m, status: 'stopped' } : m))
  )
}
