// Copyright (c) 2026 Harllan He. Licensed under MIT.

/**
 * 打字机式的匀速输出。
 *
 * 上游（Kiro）并不总是逐 token 下发：思考内容和正文经常攒成一大块、隔很久才到一次。
 * 直接把到达的片段渲染出来，就是「等一阵，然后一下子全出来」。这里把到达的文本先排队，
 * 再按固定节拍逐步放出：
 * - 积压很少时每拍放出少量字符，呈现逐字输出的效果，且不增加延迟；
 * - 积压很多时按比例加快（时间常数约 1.5 秒（90 拍），积压越大放得越快），长文不会越拖越慢；
 * - 页面在后台时浏览器会把定时器降频，此时直接全部放出，避免后台标签页内容迟迟不落地。
 */

export type TypewriterKind = 'reasoning' | 'text'

/** 每拍至少放出的字符数（约 60 拍/秒，即 120 字符/秒起步） */
const MIN_CHARS_PER_TICK = 2
/** 积压按此拍数追平：每拍放出 积压/CATCH_UP_TICKS 个字符 */
const CATCH_UP_TICKS = 90
const TICK_MS = 16

type Segment = { kind: TypewriterKind; text: string } | { kind: 'mark'; run: () => void }

export interface Typewriter {
  push: (kind: TypewriterKind, text: string) => void
  /** 排在已入队内容之后执行：前面的字符全部显示出来才会调用（没有积压时下一拍调用） */
  after: (run: () => void) => void
  /** 立刻放出所有积压（用户点了停止 / 请求出错时用） */
  flush: () => void
  /** 积压全部放出后 resolve；没有积压时立即 resolve */
  idle: () => Promise<void>
}

export function createTypewriter(apply: (reasoning: string, text: string) => void): Typewriter {
  // 思考内容与正文按到达顺序排队：思考没打完之前，正文不会提前冒出来
  const queue: Segment[] = []
  let backlog = 0
  let timer: ReturnType<typeof setTimeout> | null = null
  let waiters: (() => void)[] = []

  /**
   * 取出至多 budget 个字符；排在队首的 mark 不占额度。
   * stopAtMark 为真时取到 mark 就停，让「前文显示完 → 执行 mark → 再显示后文」分在不同的拍里。
   */
  const take = (
    budget: number,
    stopAtMark: boolean,
  ): { reasoning: string; text: string; marks: (() => void)[] } => {
    let reasoning = ''
    let text = ''
    const marks: (() => void)[] = []
    while (queue.length > 0) {
      const head = queue[0]
      if (head.kind === 'mark') {
        queue.shift()
        marks.push(head.run)
        if (stopAtMark) break
        continue
      }
      if (budget <= 0) break
      let n = Math.min(budget, head.text.length)
      // 不把代理对（emoji 等）从中间劈开
      if (n < head.text.length) {
        const code = head.text.charCodeAt(n - 1)
        if (code >= 0xd800 && code <= 0xdbff) n++
      }
      const piece = head.text.slice(0, n)
      if (n >= head.text.length) queue.shift()
      else head.text = head.text.slice(n)
      backlog -= piece.length
      budget -= piece.length
      if (head.kind === 'reasoning') reasoning += piece
      else text += piece
    }
    return { reasoning, text, marks }
  }

  const settle = () => {
    const pending = waiters
    waiters = []
    for (const resolve of pending) resolve()
  }

  const tick = () => {
    timer = null
    if (queue.length === 0) {
      settle()
      return
    }
    const budget =
      typeof document !== 'undefined' && document.hidden
        ? backlog
        : Math.max(MIN_CHARS_PER_TICK, Math.ceil(backlog / CATCH_UP_TICKS))
    const { reasoning, text, marks } = take(budget, true)
    if (reasoning !== '' || text !== '') apply(reasoning, text)
    for (const run of marks) run()
    if (queue.length > 0) timer = setTimeout(tick, TICK_MS)
    else settle()
  }

  return {
    after(run) {
      queue.push({ kind: 'mark', run })
      if (timer === null) timer = setTimeout(tick, TICK_MS)
    },
    push(kind, text) {
      if (text === '') return
      const last = queue[queue.length - 1]
      if (last && last.kind === kind) last.text += text
      else queue.push({ kind, text })
      backlog += text.length
      if (timer === null) timer = setTimeout(tick, TICK_MS)
    },
    flush() {
      if (timer !== null) {
        clearTimeout(timer)
        timer = null
      }
      const { reasoning, text, marks } = take(Infinity, false)
      if (reasoning !== '' || text !== '') apply(reasoning, text)
      for (const run of marks) run()
      settle()
    },
    idle() {
      if (queue.length === 0) return Promise.resolve()
      return new Promise<void>((resolve) => waiters.push(resolve))
    },
  }
}
