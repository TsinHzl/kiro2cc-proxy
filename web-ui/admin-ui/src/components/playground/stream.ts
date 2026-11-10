// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { thinkingRequestFields, type ApiMessage, type Effort, type WebSearchResult } from './types'
import { ThinkingParser } from './thinking-parser'

/** 单次回复的输出上限；后端会再按模型能力裁剪 */
const MAX_OUTPUT_TOKENS = 16384
/** 单次回复内模型最多可发起的搜索次数（后端另有 5 次的硬上限） */
const WEB_SEARCH_MAX_USES = 5
/** 让代理走「模型自主决定是否搜索」的桥接路径，见后端 websearch::wants_model_driven_search */
const WEB_SEARCH_MODE_HEADER = 'x-web-search-mode'
const STREAM_TRUNCATED_MESSAGE = 'Response stream ended unexpectedly, the reply may be incomplete. Please retry'

export interface StreamParams {
  apiKey: string
  model: string
  effort: Effort
  /** 允许模型联网搜索 */
  webSearch: boolean
  messages: ApiMessage[]
  signal: AbortSignal
}

export interface StreamHandlers {
  onThinking: (delta: string) => void
  onText: (delta: string) => void
  onUsage: (usage: { input?: number; output?: number; cached?: number }) => void
  /** 模型发起了一次搜索（查询词已完整） */
  onSearchCall: (id: string, query: string) => void
  /** 搜索返回结果；error 表示上游报告了搜索错误 */
  onSearchResult: (id: string, results: WebSearchResult[], error: boolean) => void
}

/** 流式解析过程中的状态：按 content block 下标跟踪 server_tool_use 的 input_json 分片 */
interface ParseState {
  searchBlocks: Map<number, { id: string; json: string }>
  contentBlocks: Map<number, ThinkingParser> | null
  /** 是否收到 message_stop；传输层正常结束但没收到它，说明回复被截断 */
  messageStopped: boolean
}

function toSearchResults(content: unknown): WebSearchResult[] {
  if (!Array.isArray(content)) return []
  return content.flatMap((item): WebSearchResult[] => {
    if (!item || typeof item !== 'object') return []
    const r = item as Record<string, unknown>
    if (typeof r.url !== 'string') return []
    return [
      {
        url: r.url,
        title: typeof r.title === 'string' && r.title !== '' ? r.title : r.url,
        snippet: typeof r.encrypted_content === 'string' ? r.encrypted_content : '',
      },
    ]
  })
}

function parseSearchQuery(json: string): string {
  try {
    const query = (JSON.parse(json) as { query?: unknown }).query
    return typeof query === 'string' ? query : ''
  } catch {
    return ''
  }
}

interface AnthropicErrorBody {
  error?: { message?: string }
  message?: string
}

/** 把非 2xx 响应转成可读错误：优先取 JSON 里的 error.message，否则退回状态码 */
async function readHttpError(res: Response): Promise<string> {
  const raw = await res.text().catch(() => '')
  try {
    const body = JSON.parse(raw) as AnthropicErrorBody
    const message = body.error?.message ?? body.message
    if (message) return `${message} (HTTP ${res.status})`
  } catch {
    // 非 JSON 响应，走下面的兜底
  }
  return raw.trim() !== '' ? `${raw.trim().slice(0, 300)} (HTTP ${res.status})` : `HTTP ${res.status}`
}

function num(v: unknown): number | undefined {
  return typeof v === 'number' && Number.isFinite(v) ? v : undefined
}

/**
 * Anthropic 口径的 input_tokens 只含未命中缓存的部分，
 * 真正的输入总量要再加上缓存读取与缓存写入。
 */
function toUsage(
  usage: Record<string, unknown> | undefined,
  withOutput: boolean
): { input?: number; output?: number; cached?: number } {
  const fresh = num(usage?.input_tokens)
  const cached = num(usage?.cache_read_input_tokens)
  const created = num(usage?.cache_creation_input_tokens)
  const input = fresh === undefined ? undefined : fresh + (cached ?? 0) + (created ?? 0)
  return { input, cached, output: withOutput ? num(usage?.output_tokens) : undefined }
}

function dispatchContent(
  index: number,
  type: 'text' | 'thinking',
  text: string,
  handlers: StreamHandlers,
  state: ParseState
): void {
  if (!state.contentBlocks) {
    if (type === 'thinking') handlers.onThinking(text)
    else handlers.onText(text)
    return
  }
  let parser = state.contentBlocks.get(index)
  if (!parser) {
    parser = new ThinkingParser(type, handlers)
    state.contentBlocks.set(index, parser)
  }
  parser.push(text)
}

/** 处理一条 SSE 事件的 data 负载；遇到上游 error 事件时抛出 */
function dispatchEvent(data: string, handlers: StreamHandlers, state: ParseState): void {
  let event: Record<string, any>
  try {
    event = JSON.parse(data)
  } catch {
    return
  }

  switch (event.type) {
    case 'message_start':
      handlers.onUsage(toUsage(event.message?.usage, false))
      break
    case 'content_block_start': {
      const block = event.content_block ?? {}
      if (block.type === 'server_tool_use' && block.name === 'web_search' && typeof block.id === 'string') {
        state.searchBlocks.set(event.index, { id: block.id, json: '' })
      } else if (block.type === 'web_search_tool_result' && typeof block.tool_use_id === 'string') {
        // 成功时 content 是结果数组；搜索出错时是 { type: 'web_search_tool_result_error' } 对象
        handlers.onSearchResult(block.tool_use_id, toSearchResults(block.content), !Array.isArray(block.content))
      }
      break
    }
    case 'content_block_delta': {
      const delta = event.delta ?? {}
      if (delta.type === 'thinking_delta' && typeof delta.thinking === 'string') {
        dispatchContent(event.index, 'thinking', delta.thinking, handlers, state)
      } else if (delta.type === 'text_delta' && typeof delta.text === 'string') {
        dispatchContent(event.index, 'text', delta.text, handlers, state)
      }
      else if (delta.type === 'input_json_delta' && typeof delta.partial_json === 'string') {
        const search = state.searchBlocks.get(event.index)
        if (search) search.json += delta.partial_json
      }
      break
    }
    case 'content_block_stop': {
      state.contentBlocks?.get(event.index)?.finish()
      state.contentBlocks?.delete(event.index)
      const search = state.searchBlocks.get(event.index)
      if (search) {
        state.searchBlocks.delete(event.index)
        handlers.onSearchCall(search.id, parseSearchQuery(search.json))
      }
      break
    }
    case 'message_delta':
      handlers.onUsage(toUsage(event.usage, true))
      break
    case 'message_stop':
      state.messageStopped = true
      break
    case 'error':
      throw new Error(event.error?.message ?? 'Upstream stream error')
  }
}

/**
 * 以流式方式调用本代理的 /v1/messages（Anthropic 协议）。
 *
 * 浏览器原生 EventSource 只支持 GET 且不能带自定义头，所以用 fetch + ReadableStream 手动解析 SSE。
 * 正常结束时 resolve；被 signal 中止时 fetch / reader 会抛 AbortError，由调用方识别。
 */
export async function streamMessage(params: StreamParams, handlers: StreamHandlers): Promise<void> {
  const body: Record<string, unknown> = {
    model: params.model,
    max_tokens: MAX_OUTPUT_TOKENS,
    stream: true,
    messages: params.messages,
  }
  Object.assign(body, thinkingRequestFields(params.model, params.effort))
  const headers: Record<string, string> = {
    'content-type': 'application/json',
    'x-api-key': params.apiKey,
    'anthropic-version': '2023-06-01',
  }
  if (params.webSearch) {
    body.tools = [{ type: 'web_search_20250305', name: 'web_search', max_uses: WEB_SEARCH_MAX_USES }]
    headers[WEB_SEARCH_MODE_HEADER] = 'model'
  }

  const res = await fetch('/v1/messages', {
    method: 'POST',
    headers,
    body: JSON.stringify(body),
    signal: params.signal,
  })
  if (!res.ok) throw new Error(await readHttpError(res))
  if (!res.body) throw new Error('Empty response body')

  const reader = res.body.getReader()
  const decoder = new TextDecoder()
  const state: ParseState = {
    searchBlocks: new Map(),
    contentBlocks: params.effort === 'off' ? null : new Map(),
    messageStopped: false,
  }
  let buffer = ''

  const flushBlock = (block: string) => {
    const data = block
      .split('\n')
      .filter((line) => line.startsWith('data:'))
      .map((line) => line.slice(5).trimStart())
      .join('\n')
    if (data !== '') dispatchEvent(data, handlers, state)
  }

  try {
    for (;;) {
      const { done, value } = await reader.read()
      // 对拼接后的整段缓冲做换行归一化，避免 "\r" 与 "\n" 被切到两个 chunk 时漏替换
      buffer = (buffer + decoder.decode(value, { stream: !done })).replace(/\r\n/g, '\n')
      let sep = buffer.indexOf('\n\n')
      while (sep !== -1) {
        flushBlock(buffer.slice(0, sep))
        buffer = buffer.slice(sep + 2)
        sep = buffer.indexOf('\n\n')
      }
      if (done) break
    }
    if (buffer.trim() !== '') flushBlock(buffer)
    // 代理在所有正常收尾路径都会发 message_stop；没收到说明连接被中途干净地关掉了（如反向代理超时）
    if (!state.messageStopped) throw new Error(STREAM_TRUNCATED_MESSAGE)
  } finally {
    for (const parser of state.contentBlocks?.values() ?? []) parser.finish()
  }
}
