// Copyright (c) 2026 Harllan He. Licensed under MIT.

/** 深度思考力度：off = 不带 thinking；其余取值原样写入 output_config.effort */
export type Effort = 'off' | 'low' | 'medium' | 'high' | 'xhigh' | 'max'

export const EFFORT_OPTIONS: readonly Effort[] = ['off', 'low', 'medium', 'high', 'xhigh', 'max']

export type MessageStatus = 'streaming' | 'done' | 'stopped' | 'error'

/** 联网搜索结果条目（来自 web_search_tool_result；encrypted_content 在本代理里实为摘要文本） */
export interface WebSearchResult {
  title: string
  url: string
  snippet: string
}

/** 模型发起的一次联网搜索：searching = 已发出等待结果；stopped = 生成被中断时仍未拿到结果 */
export interface WebSearchCall {
  id: string
  query: string
  status: 'searching' | 'done' | 'error' | 'stopped'
  results: WebSearchResult[]
}

/** 用户随消息发送的附件：图片以 base64 保存，文本文件保存解码后的文本 */
export interface Attachment {
  id: string
  kind: 'image' | 'text'
  name: string
  mediaType: string
  /** 原文件大小（字节），仅用于展示 */
  size: number
  data: string
}

export interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  /** 用户消息携带的图片 / 文本文件 */
  attachments?: Attachment[]
  /** 模型的思考内容（thinking_delta 累积） */
  reasoning?: string
  /** 从首个思考片段到思考结束的耗时 */
  reasoningMs?: number
  /** 本条回复中模型发起的联网搜索（仅用于展示，不会回传给上游） */
  searches?: WebSearchCall[]
  status: MessageStatus
  error?: string
  /** 产出该回复所用的模型 */
  model?: string
  usage?: { input: number; output: number }
}

export interface PlaygroundConfig {
  model: string
  effort: Effort
  /** 用于请求 /v1/messages 的子 API Key ID */
  keyId: number | null
  /** 是否允许模型联网搜索 */
  webSearch: boolean
}

export type ApiContentBlock =
  | { type: 'text'; text: string }
  | { type: 'image'; source: { type: 'base64'; media_type: string; data: string } }

export interface ApiMessage {
  role: 'user' | 'assistant'
  /** 纯文本消息用字符串；带图片 / 文件附件时用内容块数组 */
  content: string | ApiContentBlock[]
}

/** 当前模型是否会真正应用 thinking / effort（与后端 additional_fields_skipped 的口径一致） */
export function modelSupportsEffort(modelId: string): boolean {
  const id = modelId.toLowerCase()
  if (id.endsWith('4.5')) return false
  return id.startsWith('claude-') || id.startsWith('gpt-')
}
