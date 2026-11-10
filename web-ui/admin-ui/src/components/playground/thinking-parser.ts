// Copyright (c) 2026 Harllan He. Licensed under MIT.
type Channel = 'text' | 'thinking'
type Handlers = { onText: (text: string) => void; onThinking: (text: string) => void }
const TAGS = ['<thinking>', '</thinking>']

/** 实时和历史共用的代码、引用保护；引用的无前缀续行保留到空行。 */
class MarkdownContext {
  private indent = 0
  private marker = ''
  private atStart = true
  private blank = true
  private previous = ''
  private containerIndent = 0
  private fenceIndent = 0
  private fencePending = false
  private fence = ''
  private inline = ''
  private quoted = false
  private indented = false
  private canIndent = true
  private listIndent = 0

  get protectsTags(): boolean {
    return Boolean(this.fence || this.inline || this.quoted || this.indentedLine)
  }

  private get contentIndent(): number {
    const base = this.containerIndent || (this.indent >= this.listIndent ? this.listIndent : 0)
    return this.containerIndent + this.indent - base
  }

  private get indentedLine(): boolean {
    return this.contentIndent >= 4 && (this.canIndent || this.indented)
  }

  get allowsThinkingClose(): boolean {
    return this.quoted && this.atStart && this.marker === '' &&
      this.contentIndent < 4 && !this.fence && !this.inline
  }

  clearQuote(): void {
    this.quoted = false
  }

  advance(text: string): void {
    for (const char of text) {
      if (char === '\n') {
        const indented = this.indentedLine
        this.indented = indented || (this.blank && this.indented)
        this.canIndent = this.blank || indented
        if (this.blank) this.quoted = false
        this.indent = 0
        this.marker = ''
        this.atStart = true
        this.blank = true
        this.previous = ''
        this.containerIndent = 0
        this.fencePending = false
      } else {
        if (char !== ' ' && char !== '\t' && char !== '\r') this.blank = false
        this.advancePrefix(char)
        this.previous = char
      }
    }
  }

  /** 只跟踪缩进和最多九位数字的列表标记，不累积正文。 */
  private advancePrefix(char: string): void {
    if (!this.atStart) return
    if (char !== ' ' && char !== '\t' && this.containerIndent + this.indent < this.listIndent) {
      this.listIndent = 0
    }
    if (this.marker === '') {
      if (char === ' ' || char === '\t') {
        this.indent += char === ' ' ? 1 : 4 - ((this.containerIndent + this.indent) % 4)
        return
      }
      if (this.indent <= 3 && !this.fence && !this.inline) {
        if (char === '>') this.quoted = true
        else if (/^[-+*\d]$/.test(char)) {
          this.marker = char
          return
        }
      }
    } else if (!this.fence && !this.inline) {
      if (/^\d{1,8}$/.test(this.marker) && /^\d$/.test(char)) {
        this.marker += char
        return
      }
      if (/^\d{1,9}$/.test(this.marker) && /^[.)]$/.test(char)) {
        this.marker += char
        return
      }
      if (/^(?:[-+*]|\d{1,9}[.)])$/.test(this.marker) && (char === ' ' || char === '\t')) {
        this.containerIndent += this.indent + this.marker.length
        this.containerIndent += char === ' ' ? 1 : 4 - (this.containerIndent % 4)
        this.listIndent = this.containerIndent
        this.canIndent = true
        this.indent = 0
        this.marker = ''
        return
      }
    }
    this.atStart = false
  }

  /** 返回 0 表示还需分片确定分隔符长度或围栏结束行。 */
  readCode(buffer: string, final: boolean): number {
    const run = /^(`+|~+)/.exec(buffer)![0]
    if (!final && run.length === buffer.length) return 0
    if (this.quoted || this.indentedLine) return run.length
    const lineStart = this.atStart && this.marker === ''
    if (this.fencePending && run[0] === '`') {
      // 起始行再次出现反引号，不是合法围栏；按原长度的行内代码处理。
      this.inline = run.length === this.fence.length ? '' : this.fence
      this.fence = ''
      this.fencePending = false
      return run.length
    }
    if (this.fence) {
      const indent = this.containerIndent + this.indent - this.fenceIndent
      if (lineStart && indent >= 0 && indent <= 3 && run[0] === this.fence[0] && run.length >= this.fence.length) {
        const rest = buffer.slice(run.length)
        const newline = rest.indexOf('\n')
        const suffix = newline < 0 ? rest : rest.slice(0, newline)
        if (/^[ \t\r]*$/.test(suffix)) {
          if (newline < 0 && !final) return 0
          this.fence = ''
        }
      }
    } else if (this.inline) {
      if (run[0] === '`' && run.length === this.inline.length) this.inline = ''
    } else if (lineStart && this.indent <= 3 && run.length >= 3) {
      this.fence = run
      this.fenceIndent = this.containerIndent
      this.fencePending = run[0] === '`'
    } else if (run[0] === '`' && this.previous !== '\\') {
      this.inline = run
    }
    return run.length
  }
}

/** 返回代码、引用之外的完整标记位置，供历史修复使用。 */
export function thinkingTags(text: string) {
  const markdown = new MarkdownContext()
  const tags: { index: number; end: number; opening: boolean }[] = []
  let index = 0
  while (index < text.length) {
    let length = 1
    const first = text[index]
    if (first === '`' || first === '~') {
      length = markdown.readCode(text.slice(index), true)
    } else if (!markdown.protectsTags && first === '<') {
      const tag = TAGS.find((value) => text.startsWith(value, index))
      if (tag && !/[`"'\\]/.test((text[index - 1] ?? '') + (text[index + tag.length] ?? ''))) {
        length = tag.length
        tags.push({ index, end: index + length, opening: tag === '<thinking>' })
      }
    }
    markdown.advance(text.slice(index, index + length))
    index += length
  }
  return tags
}

/** 页面专用：只识别换行形式的思考标记，不改变代理返回的数据。 */
export class ThinkingParser {
  private buffer = ''
  private previous = ''
  private readonly markdown = new MarkdownContext()
  private pending = ''
  private channel: Channel
  private pendingChannel: Channel
  private readonly source: Channel
  private readonly handlers: Handlers

  constructor(source: Channel, handlers: Handlers) {
    this.source = source
    this.channel = source
    this.pendingChannel = source
    this.handlers = handlers
  }

  push(text: string): void {
    this.buffer += text
    this.drain(false)
  }

  finish(): void {
    this.drain(true)
  }

  private flush(): void {
    if (!this.pending) return
    const text = this.pending
    this.pending = ''
    if (this.pendingChannel === 'thinking') this.handlers.onThinking(text)
    else this.handlers.onText(text)
  }

  private emit(text: string): void {
    if (this.pendingChannel !== this.channel) this.flush()
    this.pendingChannel = this.channel
    this.pending += text
  }

  private readCode(final: boolean): number {
    const length = this.markdown.readCode(this.buffer, final)
    if (length > 0) this.emit(this.buffer.slice(0, length))
    return length
  }

  /** 返回 0 表示等待分片，-1 表示普通文本，正数表示消费了标记。 */
  private readTag(final: boolean): number {
    const tag = TAGS.find((value) => this.buffer.startsWith(value))
    if (!tag) {
      if (!TAGS.some((value) => value.startsWith(this.buffer))) return -1
      if (!final) return 0
      if (/[`"'\\]/.test(this.previous)) return -1
      // 单独的 "<"、"</" 仍是普通文本；只丢弃可识别的控制标签前缀。
      const opening = this.buffer.startsWith('<t')
      const closing = this.channel === 'thinking' && this.buffer.startsWith('</t')
      return opening || closing ? this.buffer.length : -1
    }
    if (!final && this.buffer.length === tag.length) return 0
    const after = this.buffer[tag.length] ?? ''
    if (/[`"'\\]/.test(this.previous + after)) return -1
    if (tag === '<thinking>') {
      if (after !== '\n' && after !== '\r' && !(final && after === '')) return -1
      this.channel = 'thinking'
    } else {
      if (this.channel !== 'thinking' || (after !== '' && !'\r\n<'.includes(after))) return -1
      if (this.source === 'text') this.markdown.clearQuote()
      this.channel = this.source
    }
    return tag.length
  }

  private drain(final: boolean): void {
    while (this.buffer) {
      let consumed = -1
      const first = this.buffer[0]
      const outerClose = first === '<' && this.source === 'text' && this.channel === 'thinking' &&
        this.markdown.allowsThinkingClose &&
        ('</thinking>'.startsWith(this.buffer) || this.buffer.startsWith('</thinking>'))
      if (first === '`' || first === '~') consumed = this.readCode(final)
      else if ((!this.markdown.protectsTags || outerClose) && first === '<') consumed = this.readTag(final)
      if (consumed === 0) break
      if (consumed < 0) {
        this.emit(first)
        consumed = 1
      }
      this.markdown.advance(this.buffer.slice(0, consumed))
      this.previous = this.buffer[consumed - 1]
      this.buffer = this.buffer.slice(consumed)
    }
    this.flush()
  }
}
