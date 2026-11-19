// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 使用文档页代码块与端点卡片（自 codex2cc EndpointDoc 适配，零依赖纯文本高亮）
import { memo, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Check, ChevronRight, Copy } from 'lucide-react'
import { copyToClipboard } from '@/lib/clipboard'
import type { EndpointSpec } from './docsContent'

const CODE_PANEL =
  'overflow-hidden rounded-[10px] border-[0.5px] border-hairline bg-[color-mix(in_srgb,var(--surface-2)_55%,transparent)] shadow-panel'
const CODE_LABEL = 'flex h-[30px] items-center justify-between border-b-[0.5px] border-hairline px-3 text-[10.5px] font-semibold uppercase tracking-[.06em] text-ink-3'
const CODE_PRE = 'overflow-x-auto p-3 font-mono text-[12px] leading-[1.65] text-ink-2'

/** 代码块：label（文件路径/语言）+ 右上角复制按钮；纯 pre/code 无语法高亮 */
export const CodeBlock = memo(function CodeBlock({
  label,
  content,
  showCopy = true,
}: {
  label?: string
  content: string
  /** false 时隐藏右上复制按钮（外层窗体已提供统一复制入口） */
  showCopy?: boolean
}) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  const timerRef = useRef<number | null>(null)

  useEffect(
    () => () => {
      if (timerRef.current) window.clearTimeout(timerRef.current)
    },
    [],
  )

  const handleCopy = async () => {
    try {
      await copyToClipboard(content)
      setCopied(true)
      if (timerRef.current) window.clearTimeout(timerRef.current)
      timerRef.current = window.setTimeout(() => setCopied(false), 2000)
    } catch {
      // copyToClipboard 内部已做降级；仍失败时静默（复制按钮瞬时反馈不可用时无破坏性后果）
    }
  }

  return (
    <div className={CODE_PANEL}>
      <div className={label ? CODE_LABEL : 'relative h-0 border-b-0'}>
        {label ? (
          <span className="truncate">{label}</span>
        ) : null}
        {showCopy && (
          <button
            type="button"
            onClick={() => void handleCopy()}
            aria-label={copied ? t('common.copied') : t('common.copy')}
            className={`inline-flex items-center gap-1 rounded-[6px] px-1.5 py-1 text-[10.5px] font-semibold transition-colors ${
              copied
                ? 'bg-ok-soft text-ok'
                : 'text-ink-3 hover:bg-surface-3 hover:text-ink'
            } ${label ? '' : 'absolute right-2 top-2 z-10'}`}
          >
            {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
          </button>
        )}
      </div>
      <pre className={CODE_PRE}>
        <code>{content}</code>
      </pre>
    </div>
  )
})

const METHOD_COLORS: Record<string, string> = {
  GET: 'bg-ok-soft text-ok',
  POST: 'bg-brand-soft text-brand',
  PUT: 'bg-warn-soft text-warn',
  DELETE: 'bg-danger-soft text-danger',
}

/** HTTP 方法徽章 */
export function MethodBadge({ method }: { method: string }) {
  return (
    <span
      className={`inline-flex h-[22px] items-center rounded-[6px] px-2.5 text-[11px] font-bold ${
        METHOD_COLORS[method] || 'bg-surface-3 text-ink-2'
      }`}
    >
      {method}
    </span>
  )
}

/** 端点类型徽章：数据源为 EndpointSpec.kind */
export function TypeBadge({ kind }: { kind: string }) {
  return (
    <span className="inline-flex h-[22px] items-center rounded-[6px] border-[0.5px] border-hairline-2 bg-surface-2 px-2.5 text-[11px] font-medium text-ink-2">
      {kind}
    </span>
  )
}

const TD_BASE = 'border-b-[0.5px] border-hairline px-2.5 py-1.5 align-top text-[12px]'

/** 折叠端点卡片：方法徽章 + 路径 + 描述 + 参数表 + 请求/响应示例 + cURL */
export function EndpointDoc({ endpoint, activeId }: { endpoint: EndpointSpec; activeId: string }) {
  const [expanded, setExpanded] = useState(activeId === endpoint.id)
  const [prevActiveId, setPrevActiveId] = useState(activeId)

  // hash 导航切入该端点的瞬间展开一次；之后允许手动折叠（再次导航到本端点会重新展开）
  if (activeId !== prevActiveId) {
    setPrevActiveId(activeId)
    if (activeId === endpoint.id) setExpanded(true)
  }

  return (
    <article id={endpoint.id} className="scroll-mt-24">
      <div className="overflow-hidden rounded-[11px] border-[0.5px] border-hairline bg-chat-canvas shadow-panel">
        <button
          type="button"
          aria-expanded={expanded}
          onClick={() => setExpanded(!expanded)}
          className="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-[color-mix(in_srgb,var(--surface-3)_45%,transparent)]"
        >
          <MethodBadge method={endpoint.method} />
          <TypeBadge kind={endpoint.kind} />
          <code className="min-w-0 flex-1 truncate font-mono text-[12.5px] font-semibold text-ink">
            {endpoint.path}
          </code>
          <span className="hidden min-w-0 max-w-[46%] truncate text-[12px] text-ink-3 md:block">
            {endpoint.title}
          </span>
          <ChevronRight
            className={`size-4 shrink-0 text-ink-3 transition-transform ${expanded ? 'rotate-90' : ''}`}
          />
        </button>
        {expanded && (
          <div className="space-y-4 border-t-[0.5px] border-hairline px-4 py-4">
            <p className="text-[12.5px] leading-relaxed text-ink-2">{endpoint.description}</p>
            {endpoint.parameters && endpoint.parameters.length > 0 && (
              <div className="overflow-x-auto rounded-[10px] border-[0.5px] border-hairline">
                <table className="w-full border-collapse">
                  <thead>
                    <tr className="bg-[color-mix(in_srgb,var(--surface-2)_50%,transparent)] text-left text-[11px] font-semibold uppercase tracking-[.04em] text-ink-3">
                      <th className="px-2.5 py-2">{endpoint.paramHeaderName}</th>
                      <th className="px-2.5 py-2">{endpoint.paramHeaderType}</th>
                      <th className="px-2.5 py-2">{endpoint.paramHeaderRequired}</th>
                      <th className="px-2.5 py-2">{endpoint.paramHeaderDesc}</th>
                    </tr>
                  </thead>
                  <tbody className="text-ink-2">
                    {endpoint.parameters.map((param) => (
                      <tr key={param.name} className="border-t-[0.5px] border-hairline">
                        <td className={`${TD_BASE} font-mono font-semibold text-ink`}>{param.name}</td>
                        <td className={`${TD_BASE} font-mono text-ink-3`}>{param.type}</td>
                        <td className={TD_BASE}>
                          {param.required ? (
                            <span className="font-semibold text-danger">{endpoint.requiredLabel}</span>
                          ) : (
                            <span className="text-ink-3">{endpoint.optionalLabel}</span>
                          )}
                        </td>
                        <td className={TD_BASE}>{param.description}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            {endpoint.defaultBody && (
              <CodeBlock label={endpoint.requestLabel} content={endpoint.defaultBody} />
            )}
            <CodeBlock label="cURL" content={endpoint.curl} />
            {endpoint.responses.map((response) => (
              <CodeBlock
                key={`${response.code}-${response.label ?? ''}`}
                label={response.label || `${endpoint.responseLabel} ${response.code}`}
                content={response.body}
              />
            ))}
          </div>
        )}
      </div>
    </article>
  )
}
