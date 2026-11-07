// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useCallback, useEffect, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  clearChatSessions,
  createChatSession,
  deleteChatSession,
  getChatSession,
  updateChatSession,
} from '@/api/credentials'
import { CHAT_SESSIONS_KEY } from '@/hooks/use-credentials'
import { newId } from '@/lib/id'
import { toast } from '@/lib/toast'
import { extractErrorMessage } from '@/lib/utils'
import { streamMessage } from './stream'
import { createTypewriter, type Typewriter } from './typewriter'
import {
  EFFORT_OPTIONS,
  type ApiContentBlock,
  type ApiMessage,
  type Attachment,
  type ChatMessage,
  type Effort,
  type PlaygroundConfig,
  type WebSearchCall,
} from './types'

/** 模型 / 思考力度 / Key 的选择仍存浏览器本地；会话本身存后端 */
const CONFIG_KEY = 'playground:v1:config'
/** 当前打开的会话 ID，刷新后据此恢复 */
const ACTIVE_KEY = 'playground:v2:active'
/** 旧版本把单个对话存在 localStorage，首次加载时迁移成一个后端会话 */
const LEGACY_MESSAGES_KEY = 'playground:v1:messages'
const TITLE_MAX_CHARS = 40

const DEFAULT_CONFIG: PlaygroundConfig = { model: '', effort: 'off', keyId: null, webSearch: true }

/** 仍在等待结果的搜索不会再收到结果（流已结束或被中断），改记为已中断 */
function settleSearches(m: ChatMessage): ChatMessage {
  if (!m.searches?.some((s) => s.status === 'searching')) return m
  return { ...m, searches: m.searches.map((s) => (s.status === 'searching' ? { ...s, status: 'stopped' } : s)) }
}

/** 流式中途被中断（刷新 / 离开页面）而遗留的 streaming 状态不会再有后续数据，改记为已中断 */
function normalizeMessages(raw: unknown): ChatMessage[] {
  if (!Array.isArray(raw)) return []
  return (raw as ChatMessage[]).map((m) => settleSearches(m.status === 'streaming' ? { ...m, status: 'stopped' } : m))
}

function readLegacyMessages(): ChatMessage[] {
  try {
    return normalizeMessages(JSON.parse(localStorage.getItem(LEGACY_MESSAGES_KEY) ?? '[]'))
  } catch {
    return []
  }
}

function readActiveId(): string | null {
  try {
    return localStorage.getItem(ACTIVE_KEY)
  } catch {
    return null
  }
}

function writeActiveId(id: string | null): void {
  try {
    if (id) localStorage.setItem(ACTIVE_KEY, id)
    else localStorage.removeItem(ACTIVE_KEY)
  } catch {
    // 隐私模式等：恢复上次会话只是便利功能，静默放弃
  }
}

/** 会话标题取第一条用户消息的开头；只发了附件时用第一个附件的文件名 */
function deriveTitle(messages: ChatMessage[]): string {
  const firstUser = messages.find((m) => m.role === 'user')
  const first = firstUser?.content.trim() ? firstUser.content : (firstUser?.attachments?.[0]?.name ?? '')
  return Array.from(first.replace(/\s+/g, ' ').trim()).slice(0, TITLE_MAX_CHARS).join('')
}

function loadConfig(): PlaygroundConfig {
  try {
    const parsed = JSON.parse(localStorage.getItem(CONFIG_KEY) ?? '{}') as Partial<PlaygroundConfig>
    return {
      model: typeof parsed.model === 'string' ? parsed.model : '',
      effort: EFFORT_OPTIONS.includes(parsed.effort as Effort) ? (parsed.effort as Effort) : 'off',
      keyId: typeof parsed.keyId === 'number' ? parsed.keyId : null,
      // 默认开启；只有用户在本地明确关闭过才保持关闭
      webSearch: typeof parsed.webSearch === 'boolean' ? parsed.webSearch : DEFAULT_CONFIG.webSearch,
    }
  } catch {
    return DEFAULT_CONFIG
  }
}

function persistConfig(config: PlaygroundConfig): void {
  try {
    localStorage.setItem(CONFIG_KEY, JSON.stringify(config))
  } catch {
    // 配额已满或隐私模式：本地持久化只是便利功能，静默放弃
  }
}

/** 单条消息的内容块：图片在前，文本文件随后，用户输入的文字放最后（Anthropic 推荐的顺序） */
function toContentBlocks(m: ChatMessage): ApiContentBlock[] {
  const blocks: ApiContentBlock[] = []
  const attachments = m.attachments ?? []
  for (const a of attachments) {
    if (a.kind === 'image') {
      blocks.push({ type: 'image', source: { type: 'base64', media_type: a.mediaType, data: a.data } })
    }
  }
  for (const a of attachments) {
    if (a.kind === 'text') {
      const name = a.name.replace(/"/g, "'")
      blocks.push({ type: 'text', text: `<attachment name="${name}">\n${a.data}\n</attachment>` })
    }
  }
  if (m.content.trim() !== '') blocks.push({ type: 'text', text: m.content })
  return blocks
}

/**
 * 转成 Anthropic messages：丢弃没有内容的回复（失败 / 中断且无输出），
 * 并合并相邻同角色消息 —— 上游要求 user / assistant 交替出现。
 * 只有文本的消息仍用字符串形式，与未支持附件时发出的请求一致。
 */
function toApiMessages(messages: ChatMessage[]): ApiMessage[] {
  const merged: { role: 'user' | 'assistant'; blocks: ApiContentBlock[] }[] = []
  for (const m of messages) {
    const blocks = toContentBlocks(m)
    if (blocks.length === 0) continue
    const last = merged[merged.length - 1]
    if (last && last.role === m.role) last.blocks.push(...blocks)
    else merged.push({ role: m.role, blocks })
  }
  return merged.map(({ role, blocks }) => {
    if (blocks.every((b) => b.type === 'text')) {
      return { role, content: blocks.map((b) => (b.type === 'text' ? b.text : '')).join('\n\n') }
    }
    return { role, content: blocks }
  })
}

export interface RunOptions {
  apiKey: string | null
  model: string
  /** 已按模型能力折算后的实际力度（模型不支持时应传 off） */
  effort: Effort
  /** 允许模型联网搜索 */
  webSearch: boolean
}

export function usePlayground() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [config, setConfig] = useState<PlaygroundConfig>(loadConfig)
  const [isGenerating, setIsGenerating] = useState(false)
  /** 切换 / 新建会话时自增，用作消息列表的 key 以重置滚动位置与折叠状态 */
  const [viewKey, setViewKey] = useState(0)
  const [activeId, setActiveIdState] = useState<string | null>(readActiveId)
  // 有待恢复的会话（上次打开的 / 旧版本本地对话）时，先显示加载态，避免闪一下空白页
  const [loading, setLoading] = useState(() => readActiveId() !== null || readLegacyMessages().length > 0)

  const abortRef = useRef<AbortController | null>(null)
  /** 当前这轮生成的打字机队列，停止按钮据此立即放出积压 */
  const typewriterRef = useRef<Typewriter | null>(null)
  const messagesRef = useRef(messages)
  messagesRef.current = messages
  const activeIdRef = useRef(activeId)
  /** 每次切换 / 新建 / 删除当前会话时自增，用于丢弃过期的异步结果 */
  const epochRef = useRef(0)
  /** 保存请求串行执行，避免同一会话的多次 PUT 乱序到达 */
  const saveChainRef = useRef<Promise<unknown>>(Promise.resolve())
  /** 本地改动尚未保存到后端 */
  const dirtyRef = useRef(false)
  const initRef = useRef(false)

  useEffect(() => persistConfig(config), [config])

  const setActive = useCallback((id: string | null) => {
    activeIdRef.current = id
    setActiveIdState(id)
    writeActiveId(id)
  }, [])

  const refreshList = useCallback(
    () => queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_KEY }),
    [queryClient],
  )

  /**
   * 保存消息到后端。id 为空则新建会话（成功后若用户没有切走，则把它设为当前会话）。
   * 返回会话 ID；失败时提示并返回传入的 id。
   */
  const saveSession = useCallback(
    (id: string | null, msgs: ChatMessage[]): Promise<string | null> => {
      const epoch = epochRef.current
      const task = async (): Promise<string | null> => {
        try {
          // 排队期间前一个任务可能刚创建好会话（如首次创建未返回就离开页面）：
          // 在任务真正执行时再取当前会话 ID，同一对话不会重复建出多条记录。
          // 用户中途切走（epoch 变了）时 activeIdRef 属于别的会话，不能复用
          const targetId = id ?? (epoch === epochRef.current ? activeIdRef.current : null)
          if (targetId) {
            await updateChatSession(targetId, msgs)
            void refreshList()
            return targetId
          }
          const meta = await createChatSession(deriveTitle(msgs), msgs)
          if (epoch === epochRef.current) setActive(meta.id)
          void refreshList()
          return meta.id
        } catch (error) {
          toast.error(t('playground.saveFailed', { message: extractErrorMessage(error) }))
          return id
        }
      }
      const next = saveChainRef.current.then(task, task)
      saveChainRef.current = next
      return next
    },
    [refreshList, setActive, t],
  )
  const saveSessionRef = useRef(saveSession)
  saveSessionRef.current = saveSession

  /** 载入会话消息（不检查是否已是当前会话）。失败时保留当前内容 */
  const openSession = useCallback(
    async (id: string) => {
      const epoch = ++epochRef.current
      setLoading(true)
      try {
        await saveChainRef.current // 等待上一次保存落地，避免读到旧数据
        const session = await getChatSession<ChatMessage>(id)
        if (epoch !== epochRef.current) return
        setMessages(normalizeMessages(session.messages))
        setActive(id)
        setViewKey((k) => k + 1)
      } catch (error) {
        if (epoch !== epochRef.current) return
        const notFound = (error as { response?: { status?: number } })?.response?.status === 404
        if (notFound) {
          // 会话已被删除（例如在另一个窗口）
          setMessages([])
          setActive(null)
          void refreshList()
        } else {
          toast.error(t('playground.loadFailed', { message: extractErrorMessage(error) }))
        }
      } finally {
        if (epoch === epochRef.current) setLoading(false)
      }
    },
    [refreshList, setActive, t],
  )

  // 首次挂载：迁移旧版本地对话，否则恢复上次打开的会话
  useEffect(() => {
    if (initRef.current) return
    initRef.current = true
    const legacy = readLegacyMessages()
    if (legacy.length > 0) {
      void (async () => {
        const id = await saveSession(null, legacy)
        // 迁移失败时本地数据保留不删，下次打开再试；本次仍然展示出来
        if (id) {
          try {
            localStorage.removeItem(LEGACY_MESSAGES_KEY)
          } catch {
            // 忽略：下次启动会再迁移一份，属于可接受的重复
          }
        }
        setMessages(legacy)
        setLoading(false)
      })()
    } else if (activeIdRef.current) {
      void openSession(activeIdRef.current)
    }
  }, [saveSession, openSession])

  // 离开页面时中止进行中的生成，并把已生成的部分保存下来
  useEffect(
    () => () => {
      if (!abortRef.current) return
      // 先放出打字机里已收到但还没显示的内容（会同步写入 messagesRef），再取快照保存
      typewriterRef.current?.flush()
      abortRef.current.abort()
      const msgs = normalizeMessages(messagesRef.current)
      void saveSessionRef.current(activeIdRef.current, msgs)
    },
    [],
  )

  // 生成结束后保存最终内容。流式期间每个 token 都会改 messages，这里只在空闲时保存
  useEffect(() => {
    if (isGenerating || !dirtyRef.current) return
    dirtyRef.current = false
    void saveSession(activeIdRef.current, messages)
  }, [messages, isGenerating, saveSession])

  /**
   * 修改一条消息。基于 messagesRef 计算并同步写回它：卸载清理函数需要立刻读到
   * 最新内容（含 flush 打字机放出的部分），不能等 React 下一次渲染
   */
  const patchMessage = useCallback((id: string, patch: (m: ChatMessage) => ChatMessage) => {
    const next = messagesRef.current.map((m) => (m.id === id ? patch(m) : m))
    messagesRef.current = next
    setMessages(next)
  }, [])

  /** base 必须以 user 消息结尾；追加一条空的 assistant 消息并开始流式填充 */
  const run = useCallback(
    async (base: ChatMessage[], { apiKey, model, effort, webSearch }: RunOptions) => {
      if (!apiKey) return
      const assistant: ChatMessage = { id: newId(), role: 'assistant', content: '', status: 'streaming', model }
      const initial = [...base, assistant]
      messagesRef.current = initial
      setMessages(initial)

      const controller = new AbortController()
      abortRef.current = controller
      setIsGenerating(true)
      dirtyRef.current = true

      // 先把用户消息存到后端（必要时新建会话），生成中途刷新也不会丢掉提问
      if (activeIdRef.current) void saveSession(activeIdRef.current, base)
      else await saveSession(null, base)

      // 思考与正文先排队再匀速放出，上游攒成一大块下发时也呈现逐字输出
      const typewriter = createTypewriter((reasoning, text) =>
        patchMessage(assistant.id, (m) => ({
          ...m,
          ...(reasoning !== '' ? { reasoning: (m.reasoning ?? '') + reasoning } : {}),
          ...(text !== '' ? { content: m.content + text } : {}),
        })),
      )
      typewriterRef.current = typewriter

      let thinkStartedAt: number | null = null
      const closeThinking = () => {
        if (thinkStartedAt === null) return
        const elapsed = Date.now() - thinkStartedAt
        thinkStartedAt = null
        // 耗时按上游实际思考时长计，但要等排队的思考文字全部显示完才写入：
        // 界面据此判断思考结束并自动折叠
        typewriter.after(() => patchMessage(assistant.id, (m) => ({ ...m, reasoningMs: elapsed })))
      }

      try {
        await streamMessage(
          {
            apiKey,
            model,
            effort,
            webSearch,
            messages: toApiMessages(base),
            signal: controller.signal,
          },
          {
            onThinking: (delta) => {
              thinkStartedAt ??= Date.now()
              typewriter.push('reasoning', delta)
            },
            onText: (delta) => {
              closeThinking()
              typewriter.push('text', delta)
            },
            onSearchCall: (id, query) => {
              closeThinking()
              const call: WebSearchCall = { id, query, status: 'searching', results: [] }
              patchMessage(assistant.id, (m) => ({ ...m, searches: [...(m.searches ?? []), call] }))
            },
            onSearchResult: (id, results, failed) =>
              patchMessage(assistant.id, (m) => ({
                ...m,
                searches: (m.searches ?? []).map((s) =>
                  s.id === id ? { ...s, status: failed ? 'error' : 'done', results } : s,
                ),
              })),
            onUsage: ({ input, output, cached }) =>
              patchMessage(assistant.id, (m) => ({
                ...m,
                usage: {
                  input: input ?? m.usage?.input ?? 0,
                  output: output ?? m.usage?.output ?? 0,
                  cached: cached ?? m.usage?.cached,
                },
              })),
          },
        )
        closeThinking()
        // 上游已发完，但排队的内容可能还没打完：等它放完再标记完成（点停止会立即放完）
        await typewriter.idle()
        patchMessage(assistant.id, (m) => settleSearches({ ...m, status: 'done' }))
      } catch (error) {
        closeThinking()
        typewriter.flush()
        const aborted = error instanceof DOMException && error.name === 'AbortError'
        patchMessage(assistant.id, (m) =>
          settleSearches({
            ...m,
            status: aborted ? 'stopped' : 'error',
            error: aborted ? undefined : error instanceof Error ? error.message : String(error),
          }),
        )
      } finally {
        if (typewriterRef.current === typewriter) typewriterRef.current = null
        if (abortRef.current === controller) {
          abortRef.current = null
          setIsGenerating(false)
        }
      }
    },
    [patchMessage, saveSession],
  )

  const send = useCallback(
    (text: string, opts: RunOptions, attachments: Attachment[] = []) => {
      const content = text.trim()
      if ((content === '' && attachments.length === 0) || abortRef.current) return
      const user: ChatMessage = {
        id: newId(),
        role: 'user',
        content,
        status: 'done',
        ...(attachments.length > 0 ? { attachments } : {}),
      }
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

  const stop = useCallback(() => {
    // 先放出已收到但还在排队的内容，再中止请求
    typewriterRef.current?.flush()
    abortRef.current?.abort()
  }, [])

  /** 回到空白的新对话；已有会话保留在历史中。生成中不可用 */
  const startNew = useCallback(() => {
    if (abortRef.current) return
    epochRef.current++
    dirtyRef.current = false
    setMessages([])
    setActive(null)
    setLoading(false)
    setViewKey((k) => k + 1)
  }, [setActive])

  const selectSession = useCallback(
    (id: string) => {
      if (abortRef.current || id === activeIdRef.current) return
      dirtyRef.current = false
      void openSession(id)
    },
    [openSession],
  )

  /** 删除一个会话；删除的是当前会话时回到新对话。当前会话生成中不可删除 */
  const deleteSession = useCallback(
    async (id: string) => {
      if (abortRef.current && id === activeIdRef.current) return
      try {
        await saveChainRef.current // 等待待落地的保存，避免随后到达的 PUT 命中已删除的会话
        await deleteChatSession(id)
        void refreshList()
        if (id === activeIdRef.current) startNew()
      } catch (error) {
        toast.error(t('playground.deleteFailed', { message: extractErrorMessage(error) }))
      }
    },
    [refreshList, startNew, t],
  )

  const clearSessions = useCallback(async () => {
    if (abortRef.current) return
    try {
      await saveChainRef.current
      await clearChatSessions()
      void refreshList()
      startNew()
    } catch (error) {
      toast.error(t('playground.deleteFailed', { message: extractErrorMessage(error) }))
    }
  }, [refreshList, startNew, t])

  const updateConfig = useCallback(<K extends keyof PlaygroundConfig>(key: K, value: PlaygroundConfig[K]) => {
    setConfig((prev) => ({ ...prev, [key]: value }))
  }, [])

  return {
    messages,
    config,
    isGenerating,
    loading,
    activeId,
    viewKey,
    send,
    regenerate,
    stop,
    startNew,
    selectSession,
    deleteSession,
    clearSessions,
    updateConfig,
  }
}
