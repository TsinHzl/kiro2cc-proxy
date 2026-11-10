// Copyright (c) 2026 Harllan He. Licensed under MIT.
import assert from 'node:assert/strict'
import { registerHooks } from 'node:module'
import test from 'node:test'

const playground = new URL('../src/components/playground/', import.meta.url).href
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (context.parentURL?.startsWith(playground) && /^\.\/[^.]+$/.test(specifier)) {
      return nextResolve(`${specifier}.ts`, context)
    }
    return nextResolve(specifier, context)
  },
})
const { streamMessage } = await import('../src/components/playground/stream.ts')
const { normalizeMessages } = await import('../src/components/playground/history.ts')

async function receive(t, parts, fragmented = false, truncated = false, effort = 'high') {
  const events = []
  for (const [index, [type, value]] of parts.entries()) {
    events.push({ type: 'content_block_start', index, content_block: { type, [type]: '' } })
    for (const chunk of fragmented ? Array.from(value) : [value]) {
      events.push({
        type: 'content_block_delta',
        index,
        delta: { type: `${type}_delta`, [type]: chunk },
      })
    }
    if (!truncated) events.push({ type: 'content_block_stop', index })
  }
  if (!truncated) events.push({ type: 'message_stop' })
  const wire = new TextEncoder().encode(events.map((event) => `data: ${JSON.stringify(event)}\n\n`).join(''))
  let offset = 0
  const fetchMock = t.mock.method(globalThis, 'fetch', async () => new Response(new ReadableStream({
    pull(controller) {
      if (offset < wire.length) {
        controller.enqueue(wire.slice(offset, offset + 7))
        offset += 7
      } else if (truncated === 'abort') {
        controller.error(new DOMException('测试中止', 'AbortError'))
      } else {
        controller.close()
      }
    },
  })))
  const result = { reasoning: '', content: '', snapshots: [], error: undefined }
  const record = (field, value) => {
    result[field] += value
    result.snapshots.push(result.reasoning + result.content)
  }
  try {
    await streamMessage({
      apiKey: 'test-only',
      model: 'claude-haiku-4.5',
      effort,
      webSearch: true,
      messages: [{ role: 'user', content: '测试' }],
      signal: new AbortController().signal,
    }, {
      onThinking: (value) => record('reasoning', value),
      onText: (value) => record('content', value),
      onUsage() {},
      onSearchCall() {},
      onSearchResult() {},
    })
  } catch (error) {
    result.error = error
  } finally {
    fetchMock.mock.restore()
  }
  return result
}

test('会话页实时分离多段思考，标签分片期间也不闪现', async (t) => {
  for (const fragmented of [false, true]) {
    const result = await receive(t, [
      ['thinking', '搜索前\n</thinking><thinking>\n搜索后'],
      ['text', '正文前<thinking>\n补充一\n</thinking>\n\n正文中<thinking>\n补充二\n</thinking>\n\n正文后'],
    ], fragmented)
    assert.equal(result.error, undefined)
    assert.equal(result.reasoning, '搜索前\n\n搜索后\n补充一\n\n补充二\n')
    assert.equal(result.content, '正文前\n\n正文中\n\n正文后')
    assert.ok(result.snapshots.every((text) => !text.includes('<')))
  }
})

test('流式解析保留代码、引用和普通标签说明', async (t) => {
  const content = [
    '`<thinking>示例</thinking>`',
    '``<thinking>示例</thinking>``',
    '```xml\n<thinking>\n示例\n</thinking>\n```',
    '~~~xml\n<thinking>\n示例\n</thinking>\n~~~',
    '"<thinking>\n示例\n</thinking>"',
    '说明 <thinking> 与 </thinking> 标签；a < b',
    '```xml\n<thinking>\n尚未闭合的代码块',
  ].join('\n\n')
  const result = await receive(t, [['text', content]], true)
  assert.equal(result.error, undefined)
  assert.equal(result.content, content)
  assert.equal(result.reasoning, '')
})

test('流截断仍报告错误，同时保留已到达的思考内容', async (t) => {
  const result = await receive(t, [['text', '正文<thinking>\n未完成思考']], true, true)
  assert.match(result.error?.message ?? '', /ended unexpectedly/)
  assert.equal(result.content, '正文')
  assert.equal(result.reasoning, '\n未完成思考')
})

test('关闭思考时不改写普通正文', async (t) => {
  const content = '<thinking>\n正文示例\n</thinking>'
  const result = await receive(t, [['text', content]], true, false, 'off')
  assert.equal(result.error, undefined)
  assert.equal(result.content, content)
  assert.equal(result.reasoning, '')
  const saved = {
    id: 'reply',
    role: 'assistant',
    status: 'done',
    content: result.content,
    reasoning: result.reasoning,
    searches: [{ id: 'search', query: 'test', status: 'done', results: [] }],
  }
  assert.deepEqual(normalizeMessages([saved]), [saved])
})

test('流式解析保留代码块和 Markdown 引用', async (t) => {
  for (const content of [
    '示例：\n\n    <thinking>\n    示例代码\n    </thinking>',
    '示例：\n\n\t<thinking>\n\t示例代码\n\t</thinking>',
    '- 示例：\n\n      <thinking>\n      示例代码\n      </thinking>',
    '-     <thinking>\n      示例代码\n      </thinking>',
    '```text\n内部包含 ``` 三个反引号\n<thinking>\n代码示例\n</thinking>\n```',
    '~~~text\n内部包含 ~~~ 三个波浪线\n<thinking>\n代码示例\n</thinking>\n~~~',
    '````text\n```\n<thinking>\n代码示例\n</thinking>\n```\n````',
    '- ~~~xml\n  <thinking>\n  示例代码\n  </thinking>\n  ~~~',
    '123. ~~~xml\n     <thinking>\n     示例代码\n     </thinking>\n     ~~~',
    '- - ~~~xml\n    <thinking>\n    示例代码\n    </thinking>\n    ~~~',
    '> <thinking>\n> 引用内容\n> </thinking>',
    '> 引用开始\n<thinking>\n引用延续\n</thinking>',
    '  > <thinking>\n  > 引用内容\n  > </thinking>',
  ]) {
    for (const fragmented of [false, true]) {
      const result = await receive(t, [['text', content]], fragmented)
      assert.equal(result.error, undefined)
      assert.equal(result.content, content)
      assert.equal(result.reasoning, '')
    }
  }
})

test('流式代码和引用结束后仍能提取真实思考', async (t) => {
  for (const prefix of [
    '示例：\n\n    示例代码',
    '示例：\n\n\t示例代码',
    '- 示例：\n\n      示例代码',
    '```text\n内部包含 ``` 标记\n```',
    '~~~text\n内部包含 ~~~ 标记\n~~~~',
    '> 引用开始\n引用续行',
    '```const x = 1```',
    '- ~~~xml\n  示例代码\n  ~~~',
    '123. ~~~xml\n     示例代码\n     ~~~',
  ]) {
    for (const fragmented of [false, true]) {
      const result = await receive(t, [
        ['text', `${prefix}\n\n<thinking>\n真实思考\n</thinking>\n\n正文`],
      ], fragmented)
      assert.equal(result.error, undefined)
      assert.equal(result.content, `${prefix}\n\n\n\n正文`)
      assert.equal(result.reasoning, '\n真实思考\n')
    }
  }
})

test('段内引用不屏蔽外层结束标签，字面结束标签仍受保护', async (t) => {
  for (const reasoning of [
    '\n> 引用资料\n',
    '\n> </thinking>\n> 引用资料\n',
    '\n> 引用资料\n  ',
    '\n\n```xml\n</thinking>\n```\n',
    '\n\n    </thinking>\n\n',
  ]) {
    for (const fragmented of [false, true]) {
      const result = await receive(t, [
        ['text', `<thinking>${reasoning}</thinking>\n\n最终答案`],
      ], fragmented)
      assert.equal(result.error, undefined)
      assert.equal(result.content, '\n\n最终答案')
      assert.equal(result.reasoning, reasoning)
    }
  }
})

test('非外层结束标签不越过引用保护', async (t) => {
  for (const type of ['text', 'thinking']) {
    const content = '> 引用资料\n</thinking>\n\n后续内容'
    const result = await receive(t, [[type, content]], true)
    assert.equal(result.error, undefined)
    assert.equal(result.content, type === 'text' ? content : '')
    assert.equal(result.reasoning, type === 'thinking' ? content : '')
  }
})

test('段落续行和列表普通缩进不误判成代码块', async (t) => {
  for (const prefix of ['说明\n    ', '- 说明\n\n    ']) {
    const result = await receive(t, [
      ['text', `${prefix}<thinking>\n真实思考\n</thinking>\n\n正文`],
    ], true)
    assert.equal(result.error, undefined)
    assert.equal(result.content, `${prefix}\n\n正文`)
    assert.equal(result.reasoning, '\n真实思考\n')
  }
})

test('流式长行保持原文并提取后续思考', async (t) => {
  const prefix = 'a'.repeat(256_000)
  const result = await receive(t, [
    ['text', `${prefix}<thinking>\n真实思考\n</thinking>\n\n正文`],
  ])
  assert.equal(result.error, undefined)
  assert.equal(result.content, `${prefix}\n\n正文`)
  assert.equal(result.reasoning, '\n真实思考\n')
})

test('块结束、断流和中止时不输出半截控制标签', async (t) => {
  for (const truncated of [false, true, 'abort']) {
    for (const [tag, start, before, thinking] of [
      ['<thinking>', 2, '正文', ''],
      ['</thinking>', 3, '正文<thinking>\n思考\n', '\n思考\n'],
    ]) {
      for (let end = start; end < tag.length; end++) {
        const result = await receive(t, [['text', before + tag.slice(0, end)]], true, truncated)
        assert.equal(result.content, '正文')
        assert.equal(result.reasoning, thinking)
        assert.ok(result.snapshots.every((text) => !text.includes('<')))
        if (truncated === 'abort') assert.equal(result.error?.name, 'AbortError')
        else if (truncated) assert.match(result.error?.message ?? '', /ended unexpectedly/)
        else assert.equal(result.error, undefined)
      }
    }
  }
})

test('收尾保留比较符号及代码、引用中的标签前缀', async (t) => {
  for (const content of ['a < b', '正文<', '正文</', '`<think', '```xml\n<think', '> <think']) {
    const result = await receive(t, [['text', content]], true)
    assert.equal(result.error, undefined)
    assert.equal(result.content, content)
    assert.equal(result.reasoning, '')
  }
})
