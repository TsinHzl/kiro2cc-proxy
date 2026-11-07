// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { memo, useMemo, useState, type ReactNode } from 'react'
import { Check, Copy } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { copyToClipboard } from '@/lib/clipboard'
import { toast } from '@/lib/toast'

/**
 * 轻量 Markdown 渲染：只覆盖对话里常见的语法（标题 / 列表 / 引用 / 表格 / 代码块 /
 * 行内代码 / 粗斜体 / 链接），直接产出 React 元素，不使用 dangerouslySetInnerHTML。
 * 模型输出属于不可信内容，所以不引入原始 HTML 支持，链接也只放行 http(s)。
 */

type Block =
  | { type: 'code'; lang: string; body: string }
  | { type: 'heading'; level: number; text: string }
  | { type: 'hr' }
  | { type: 'quote'; text: string }
  | { type: 'list'; ordered: boolean; start: number; items: { indent: number; text: string }[] }
  | { type: 'table'; header: string[]; rows: string[][] }
  | { type: 'paragraph'; text: string }

const FENCE = /^\s*```\s*([\w+#.-]*)\s*$/
// 闭合井号必须与正文之间有空白，否则 "# C#" 里的 # 会被误当作闭合标记吞掉
const HEADING = /^(#{1,6})\s+(.*?)(?:\s+#+)?\s*$/
const HR = /^\s*([-*_])(\s*\1){2,}\s*$/
const QUOTE = /^\s*>\s?(.*)$/
const UL = /^(\s*)[-*+]\s+(.*)$/
const OL = /^(\s*)(\d+)[.)]\s+(.*)$/
const TABLE_SEP = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/

function splitRow(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((cell) => cell.trim())
}

function isBlockStart(line: string, next: string | undefined): boolean {
  return (
    FENCE.test(line) ||
    HEADING.test(line) ||
    HR.test(line) ||
    QUOTE.test(line) ||
    UL.test(line) ||
    OL.test(line) ||
    (line.includes('|') && next !== undefined && TABLE_SEP.test(next))
  )
}

function parseBlocks(text: string): Block[] {
  const lines = text.replace(/\r\n/g, '\n').split('\n')
  const blocks: Block[] = []
  let i = 0

  while (i < lines.length) {
    const line = lines[i]
    if (line.trim() === '') {
      i++
      continue
    }

    const fence = FENCE.exec(line)
    if (fence) {
      // 流式输出时围栏可能尚未闭合：把剩余内容都当作代码
      const body: string[] = []
      i++
      while (i < lines.length && !/^\s*```\s*$/.test(lines[i])) body.push(lines[i++])
      i++
      blocks.push({ type: 'code', lang: fence[1], body: body.join('\n') })
      continue
    }

    const heading = HEADING.exec(line)
    if (heading) {
      blocks.push({ type: 'heading', level: heading[1].length, text: heading[2] })
      i++
      continue
    }

    if (HR.test(line)) {
      blocks.push({ type: 'hr' })
      i++
      continue
    }

    if (QUOTE.test(line)) {
      const quoted: string[] = []
      while (i < lines.length && QUOTE.test(lines[i])) quoted.push(QUOTE.exec(lines[i++])![1])
      blocks.push({ type: 'quote', text: quoted.join('\n') })
      continue
    }

    if (line.includes('|') && i + 1 < lines.length && TABLE_SEP.test(lines[i + 1])) {
      const header = splitRow(line)
      const rows: string[][] = []
      i += 2
      while (i < lines.length && lines[i].trim() !== '' && lines[i].includes('|')) rows.push(splitRow(lines[i++]))
      blocks.push({ type: 'table', header, rows })
      continue
    }

    const ul = UL.exec(line)
    const ol = OL.exec(line)
    if (ul || ol) {
      const ordered = !ul
      const start = ol ? Number(ol[2]) : 1
      const items: { indent: number; text: string }[] = []
      while (i < lines.length) {
        const m = (ordered ? OL : UL).exec(lines[i])
        if (!m) break
        items.push({ indent: m[1].length, text: ordered ? m[3] : m[2] })
        i++
      }
      blocks.push({ type: 'list', ordered, start, items })
      continue
    }

    const para: string[] = [line]
    i++
    while (i < lines.length && lines[i].trim() !== '' && !isBlockStart(lines[i], lines[i + 1])) para.push(lines[i++])
    blocks.push({ type: 'paragraph', text: para.join('\n') })
  }

  return blocks
}

// 依次匹配：行内代码 / 粗体 / 删除线 / 斜体 / 链接。粗体必须排在斜体前，否则 ** 会被拆成两个 *。
const INLINE = /(`[^`\n]+`)|(\*\*[^*\n]+?\*\*)|(~~[^~\n]+?~~)|(\*(?!\s)[^*\n]+?\*)|(\[[^\]\n]+\]\(https?:\/\/[^)\s]+\))/g

function renderInline(text: string): ReactNode[] {
  const nodes: ReactNode[] = []
  let last = 0
  let key = 0
  for (const m of text.matchAll(INLINE)) {
    const index = m.index ?? 0
    if (index > last) nodes.push(text.slice(last, index))
    const token = m[0]
    if (m[1]) {
      nodes.push(
        <code key={key++} className="rounded-[4px] bg-surface-3 px-1 py-px font-mono text-[0.9em] text-ink">
          {token.slice(1, -1)}
        </code>,
      )
    } else if (m[2]) {
      nodes.push(<strong key={key++} className="font-semibold">{renderInline(token.slice(2, -2))}</strong>)
    } else if (m[3]) {
      nodes.push(<del key={key++}>{renderInline(token.slice(2, -2))}</del>)
    } else if (m[4]) {
      nodes.push(<em key={key++}>{renderInline(token.slice(1, -1))}</em>)
    } else if (m[5]) {
      const sep = token.indexOf('](')
      nodes.push(
        <a
          key={key++}
          href={token.slice(sep + 2, -1)}
          target="_blank"
          rel="noopener noreferrer"
          className="text-brand underline underline-offset-2 hover:text-brand-hover"
        >
          {token.slice(1, sep)}
        </a>,
      )
    }
    last = index + token.length
  }
  if (last < text.length) nodes.push(text.slice(last))
  return nodes
}

function CodeBlock({ lang, body }: { lang: string; body: string }) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)

  const handleCopy = async () => {
    try {
      await copyToClipboard(body)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      toast.error(t('common.copyFailed'))
    }
  }

  return (
    <div className="my-3 overflow-hidden rounded-[10px] border-[0.5px] border-hairline-2 bg-surface-2">
      <div className="flex h-8 items-center justify-between border-b-[0.5px] border-hairline px-3 text-[11px] text-ink-3">
        <span className="font-mono">{lang || 'text'}</span>
        <button
          type="button"
          onClick={handleCopy}
          className="inline-flex items-center gap-1 rounded-[5px] px-1.5 py-0.5 transition-colors hover:bg-surface-3 hover:text-ink"
        >
          {copied ? <Check className="size-3" /> : <Copy className="size-3" />}
          {copied ? t('common.copied') : t('common.copy')}
        </button>
      </div>
      <pre className="overflow-x-auto px-3 py-2.5 font-mono text-[12.5px] leading-[1.65] text-ink">
        <code>{body}</code>
      </pre>
    </div>
  )
}

const HEADING_CLASS: Record<number, string> = {
  1: 'mt-5 mb-2 text-[19px] font-semibold tracking-[-.01em]',
  2: 'mt-4 mb-2 text-[16.5px] font-semibold tracking-[-.01em]',
  3: 'mt-4 mb-1.5 text-[14.5px] font-semibold',
  4: 'mt-3 mb-1 text-[13.5px] font-semibold',
  5: 'mt-3 mb-1 text-[13px] font-semibold',
  6: 'mt-3 mb-1 text-[13px] font-semibold text-ink-2',
}

function renderBlock(block: Block, key: number): ReactNode {
  switch (block.type) {
    case 'code':
      return <CodeBlock key={key} lang={block.lang} body={block.body} />
    case 'heading': {
      const Tag = `h${Math.min(block.level, 6)}` as 'h1'
      return <Tag key={key} className={HEADING_CLASS[block.level]}>{renderInline(block.text)}</Tag>
    }
    case 'hr':
      return <hr key={key} className="my-4 border-0 border-t-[0.5px] border-hairline-2" />
    case 'quote':
      return (
        <blockquote key={key} className="my-2.5 border-l-2 border-hairline-2 pl-3 text-ink-2">
          <Markdown text={block.text} />
        </blockquote>
      )
    case 'list': {
      const Tag = block.ordered ? 'ol' : 'ul'
      return (
        <Tag
          key={key}
          start={block.ordered ? block.start : undefined}
          className={`my-2 space-y-1 pl-5 ${block.ordered ? 'list-decimal' : 'list-disc'}`}
        >
          {block.items.map((item, idx) => (
            <li key={idx} style={item.indent > 0 ? { marginLeft: item.indent * 6 } : undefined}>
              {renderInline(item.text)}
            </li>
          ))}
        </Tag>
      )
    }
    case 'table':
      return (
        <div key={key} className="my-3 overflow-x-auto rounded-[10px] border-[0.5px] border-hairline-2">
          <table className="w-full border-collapse text-[12.5px]">
            <thead className="bg-surface-2">
              <tr>
                {block.header.map((cell, idx) => (
                  <th key={idx} className="border-b-[0.5px] border-hairline-2 px-3 py-1.5 text-left font-semibold">
                    {renderInline(cell)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {block.rows.map((row, rIdx) => (
                <tr key={rIdx} className="border-t-[0.5px] border-hairline first:border-t-0">
                  {row.map((cell, cIdx) => (
                    <td key={cIdx} className="px-3 py-1.5 align-top">{renderInline(cell)}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )
    case 'paragraph':
      return <p key={key} className="my-2 whitespace-pre-wrap break-words">{renderInline(block.text)}</p>
  }
}

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  const blocks = useMemo(() => parseBlocks(text), [text])
  return (
    <div className="text-[13.5px] leading-[1.75] text-ink [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
      {blocks.map((block, idx) => renderBlock(block, idx))}
    </div>
  )
})
