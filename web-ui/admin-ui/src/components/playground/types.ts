// Copyright (c) 2026 Harllan He. Licensed under MIT.

/** 深度思考力度：off = 不带 thinking；其余取值原样写入 output_config.effort */
export type Effort = 'off' | 'low' | 'medium' | 'high' | 'xhigh' | 'max'

export const EFFORT_OPTIONS: readonly Effort[] = ['off', 'low', 'medium', 'high', 'xhigh', 'max']

export type MessageStatus = 'streaming' | 'done' | 'stopped' | 'error'

export interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  /** 模型的思考内容（thinking_delta 累积） */
  reasoning?: string
  /** 从首个思考片段到思考结束的耗时 */
  reasoningMs?: number
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
}

export interface ApiMessage {
  role: 'user' | 'assistant'
  content: string
}

/** 当前模型是否会真正应用 thinking / effort（与后端 additional_fields_skipped 的口径一致） */
export function modelSupportsEffort(modelId: string): boolean {
  const id = modelId.toLowerCase()
  if (id.endsWith('4.5')) return false
  return id.startsWith('claude-') || id.startsWith('gpt-')
}
