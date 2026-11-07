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

/**
 * 深度思考的两种实现，由后端能否接受 additionalModelRequestFields 决定：
 * - effort：Claude 4.6+ / GPT 系，请求带 thinking: adaptive + output_config.effort，上游原生推理；
 * - budget：Claude 4.5 代际与国产模型（kimi / glm / deepseek / qwen / minimax 等），上游拒绝上述字段，
 *   改发 thinking: enabled + budget_tokens，代理把它转成提示词里的思考标签，再从输出里拆出思考内容。
 */
export type ThinkingMode = 'effort' | 'budget'

/** Claude 4.5 代际（含 claude-opus-4-5-20251101 这类带日期的连字符别名） */
const CLAUDE_45_RE = /^claude-.*-4[.-]5($|-)/

export function thinkingMode(modelId: string): ThinkingMode {
  const id = modelId.toLowerCase()
  if (CLAUDE_45_RE.test(id)) return 'budget'
  return id.startsWith('claude-') || id.startsWith('gpt-') ? 'effort' : 'budget'
}

/** budget 模式只提供三档，对应思考预算 */
const BUDGET_EFFORT_OPTIONS: readonly Effort[] = ['off', 'low', 'medium', 'high']

/** 各档位的 thinking.budget_tokens（后端上限 24576） */
const BUDGET_TOKENS: Partial<Record<Effort, number>> = { low: 4096, medium: 10240, high: 20000 }

export function effortOptionsFor(modelId: string): readonly Effort[] {
  return thinkingMode(modelId) === 'budget' ? BUDGET_EFFORT_OPTIONS : EFFORT_OPTIONS
}

/** 把已保存的档位收敛到当前模型可选的范围内（xhigh / max 在 budget 模式下按 high 处理） */
export function normalizeEffort(modelId: string, effort: Effort): Effort {
  if (effortOptionsFor(modelId).includes(effort)) return effort
  return 'high'
}

/** 请求体里的 thinking / output_config；off 时两者都不带 */
export function thinkingRequestFields(modelId: string, effort: Effort): Record<string, unknown> {
  if (effort === 'off') return {}
  if (thinkingMode(modelId) === 'budget') {
    return { thinking: { type: 'enabled', budget_tokens: BUDGET_TOKENS[effort] ?? BUDGET_TOKENS.high } }
  }
  return { thinking: { type: 'adaptive' }, output_config: { effort } }
}
