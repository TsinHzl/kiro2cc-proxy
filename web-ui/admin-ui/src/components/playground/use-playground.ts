// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useCallback, useEffect, useRef, useState } from 'react'
import { streamMessage } from './stream'
import {
  EFFORT_OPTIONS,
  type ApiMessage,
  type ChatMessage,
  type Effort,
  type PlaygroundConfig,
} from './types'

const MESSAGES_KEY = 'playground:v1:messages'
const CONFIG_KEY = 'playground:v1:config'
/** 本地仅保留最近 N 条，避免 localStorage 被长对话撑爆 */
const MAX_PERSISTED_MESSAGES = 200

const DEFAULT_CONFIG: PlaygroundConfig = { model: '', effort: 'off', keyId: null }

function newId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

function loadMessages(): ChatMessage[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(MESSAGES_KEY) ?? '[]')
    if (!Array.isArray(parsed)) return []
    // 页面在流式中途被刷新时，遗留的 streaming 状态不会再有后续数据，改记为已中断
    return (parsed as ChatMessage[]).map((m) => (m.status === 'streaming' ? { ...m, status: 'stopped' } : m))
  } catch {
    return []
  }
}

function loadConfig(): PlaygroundConfig {
  try {
    const parsed = JSON.parse(localStorage.getItem(CONFIG_KEY) ?? '{}') as Partial<PlaygroundConfig>
    return {
      model: typeof parsed.model === 'string' ? parsed.model : '',
      effort: EFFORT_OPTIONS.includes(parsed.effort as Effort) ? (parsed.effort as Effort) : 'off',
      keyId: typeof parsed.keyId === 'number' ? parsed.keyId : null,
    }
  } catch {
    return DEFAULT_CONFIG
  }
}

function persist(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // 配额已满或隐私模式：本地持久化只是便利功能，静默放弃
  }
}

/**
 * 转成 Anthropic messages：丢弃没有正文的回复（失败 / 中断且无输出），
 * 并合并相邻同角色消息 —— 上游要求 user / assistant 交替出现。
 */
function toApiMessages(messages: ChatMessage[]): ApiMessage[] {
  const out: ApiMessage[] = []
  for (const m of messages) {
    if (m.content.trim() === '') continue
    const last = out[out.length - 1]
    if (last && last.role === m.role) last.content += `\n\n${m.content}`
    else out.push({ role: m.role, content: m.content })
  }
  return out
}

export interface RunOptions {
  apiKey: string | null
  model: string
  /** 已按模型能力折算后的实际力度（模型不支持时应传 off） */
  effort: Effort
}

export function usePlayground() {
  const [messages, setMessages] = useState<ChatMessage[]>(loadMessages)
  const [config, setConfig] = useState<PlaygroundConfig>(loadConfig)
  const [isGenerating, setIsGenerating] = useState(false)
  const abortRef = useRef<AbortController | null>(null)
  const messagesRef = useRef(messages)
  messagesRef.current = messages

  useEffect(() => persist(CONFIG_KEY, config), [config])
  // 流式期间每个 token 都会改 messages，整段序列化代价高；只在空闲时落盘
  useEffect(() => {
    if (!isGenerating) persist(MESSAGES_KEY, messages.slice(-MAX_PERSISTED_MESSAGES))
  }, [messages, isGenerating])
  useEffect(() => () => abortRef.current?.abort(), [])

  const patchMessage = useCallback((id: string, patch: (m: ChatMessage) => ChatMessage) => {
    setMessages((prev) => prev.map((m) => (m.id === id ? patch(m) : m)))
  }, [])

  /** base 必须以 user 消息结尾；追加一条空的 assistant 消息并开始流式填充 */
  const run = useCallback(
    async (base: ChatMessage[], { apiKey, model, effort }: RunOptions) => {
      if (!apiKey) return
      const assistant: ChatMessage = { id: newId(), role: 'assistant', content: '', status: 'streaming', model }
      setMessages([...base, assistant])

      const controller = new AbortController()
      abortRef.current = controller
      setIsGenerating(true)

      let thinkStartedAt: number | null = null
      const closeThinking = () => {
        if (thinkStartedAt === null) return
        const elapsed = Date.now() - thinkStartedAt
        thinkStartedAt = null
        patchMessage(assistant.id, (m) => ({ ...m, reasoningMs: elapsed }))
      }

      try {
        await streamMessage(
          {
            apiKey,
            model,
            effort,
            messages: toApiMessages(base),
            signal: controller.signal,
          },
          {
            onThinking: (delta) => {
              thinkStartedAt ??= Date.now()
              patchMessage(assistant.id, (m) => ({ ...m, reasoning: (m.reasoning ?? '') + delta }))
            },
            onText: (delta) => {
              closeThinking()
              patchMessage(assistant.id, (m) => ({ ...m, content: m.content + delta }))
            },
            onUsage: ({ input, output }) =>
              patchMessage(assistant.id, (m) => ({
                ...m,
                usage: { input: input ?? m.usage?.input ?? 0, output: output ?? m.usage?.output ?? 0 },
              })),
          },
        )
        closeThinking()
        patchMessage(assistant.id, (m) => ({ ...m, status: 'done' }))
      } catch (error) {
        closeThinking()
        const aborted = error instanceof DOMException && error.name === 'AbortError'
        patchMessage(assistant.id, (m) => ({
          ...m,
          status: aborted ? 'stopped' : 'error',
          error: aborted ? undefined : error instanceof Error ? error.message : String(error),
        }))
      } finally {
        if (abortRef.current === controller) {
          abortRef.current = null
          setIsGenerating(false)
        }
      }
    },
    [patchMessage],
  )

  const send = useCallback(
    (text: string, opts: RunOptions) => {
      const content = text.trim()
      if (content === '' || abortRef.current) return
      const user: ChatMessage = { id: newId(), role: 'user', content, status: 'done' }
      void run([...messagesRef.current, user], opts)
    },
    [run],
  )

  /** 丢弃最后一条用户消息之后的所有内容并重新生成 */
  const regenerate = useCallback(
    (opts: RunOptions) => {
      if (abortRef.current) return
      const current = messagesRef.current
      let lastUser = -1
      for (let i = current.length - 1; i >= 0; i--) {
        if (current[i].role === 'user') {
          lastUser = i
          break
        }
      }
      if (lastUser === -1) return
      void run(current.slice(0, lastUser + 1), opts)
    },
    [run],
  )

  const stop = useCallback(() => abortRef.current?.abort(), [])

  const clear = useCallback(() => {
    abortRef.current?.abort()
    abortRef.current = null
    setIsGenerating(false)
    setMessages([])
  }, [])

  const updateConfig = useCallback(<K extends keyof PlaygroundConfig>(key: K, value: PlaygroundConfig[K]) => {
    setConfig((prev) => ({ ...prev, [key]: value }))
  }, [])

  return { messages, config, isGenerating, send, regenerate, stop, clear, updateConfig }
}
