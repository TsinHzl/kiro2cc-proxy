// Copyright (c) 2026 Harllan He. Licensed under MIT.
import type { ApiMessage, Effort } from './types'

/** 单次回复的输出上限；后端会再按模型能力裁剪 */
const MAX_OUTPUT_TOKENS = 16384

export interface StreamParams {
  apiKey: string
  model: string
  effort: Effort
  messages: ApiMessage[]
  signal: AbortSignal
}

export interface StreamHandlers {
  onThinking: (delta: string) => void
  onText: (delta: string) => void
  onUsage: (usage: { input?: number; output?: number }) => void
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

/** 处理一条 SSE 事件的 data 负载；遇到上游 error 事件时抛出 */
function dispatchEvent(data: string, handlers: StreamHandlers): void {
  let event: Record<string, any>
  try {
    event = JSON.parse(data)
  } catch {
    return
  }

  switch (event.type) {
    case 'message_start':
      handlers.onUsage({ input: event.message?.usage?.input_tokens })
      break
    case 'content_block_delta': {
      const delta = event.delta ?? {}
      if (delta.type === 'thinking_delta' && typeof delta.thinking === 'string') handlers.onThinking(delta.thinking)
      else if (delta.type === 'text_delta' && typeof delta.text === 'string') handlers.onText(delta.text)
      break
    }
    case 'message_delta':
      handlers.onUsage({ input: event.usage?.input_tokens, output: event.usage?.output_tokens })
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
  if (params.effort !== 'off') {
    body.thinking = { type: 'adaptive' }
    body.output_config = { effort: params.effort }
  }

  const res = await fetch('/v1/messages', {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-api-key': params.apiKey,
      'anthropic-version': '2023-06-01',
    },
    body: JSON.stringify(body),
    signal: params.signal,
  })
  if (!res.ok) throw new Error(await readHttpError(res))
  if (!res.body) throw new Error('Empty response body')

  const reader = res.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  const flushBlock = (block: string) => {
    const data = block
      .split('\n')
      .filter((line) => line.startsWith('data:'))
      .map((line) => line.slice(5).trimStart())
      .join('\n')
    if (data !== '') dispatchEvent(data, handlers)
  }

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
}
