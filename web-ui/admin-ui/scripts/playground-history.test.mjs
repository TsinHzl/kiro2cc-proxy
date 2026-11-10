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
const { normalizeMessages } = await import('../src/components/playground/history.ts')

function message(fields = {}) {
  return {
    id: 'reply',
    role: 'assistant',
    status: 'done',
    content: '',
    searches: [{ id: 'search', query: 'rust', status: 'done', results: [] }],
    ...fields,
  }
}

test('历史搜索消息只清理思考分界，不搬移正文', () => {
  const raw = message({
    reasoning: '搜索前\n</thinking><thinking>\n搜索后',
    content: '答案前<thinking>\n补充一\n</thinking>\n\n答案中<thinking>\n补充二\n</thinking>\n\n答案后',
  })
  const original = structuredClone(raw)
  const [result] = normalizeMessages([raw])
  assert.equal(result.reasoning, '搜索前\n\n搜索后')
  assert.equal(result.content, raw.content)
  assert.deepEqual(raw, original)
  assert.deepEqual(normalizeMessages([result]), [result])
})

test('代码和引用中的 thinking 标签不作为历史污染删除', () => {
  for (const content of [
    '`<thinking>示例</thinking>`',
    '``<thinking>示例</thinking>``',
    '```xml\n<thinking>\n示例\n</thinking>\n```',
    '~~~xml\n<thinking>\n示例\n</thinking>\n~~~',
    '"<thinking>示例</thinking>"',
    "'<thinking>示例</thinking>'",
    '说明 <thinking> 与 </thinking> 标签',
    '```xml\n<thinking>\n尚未闭合的代码块',
  ]) {
    const raw = message({ content, reasoning: '`</thinking><thinking>` 是标签示例' })
    assert.deepEqual(normalizeMessages([raw]), [raw])
  }
})

test('用户消息和没有搜索记录的消息保持原文', () => {
  for (const fields of [{ role: 'user' }, { searches: [] }]) {
    const raw = message({ content: '<thinking>\n原文\n</thinking>', ...fields })
    assert.deepEqual(normalizeMessages([raw]), [raw])
  }
})

test('历史修复保留围栏内部标记和 Markdown 引用', () => {
  for (const content of [
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
    const raw = message({ content, reasoning: content })
    const [result] = normalizeMessages([raw])
    assert.deepEqual(result, raw)
    assert.deepEqual(normalizeMessages([result]), [raw])
  }
})

test('历史思考分界清理不修改正文', () => {
  for (const prefix of [
    '```text\n内部包含 ``` 标记\n```',
    '~~~text\n内部包含 ~~~ 标记\n~~~~',
    '> 引用开始\n引用续行',
    '```const x = 1```',
    '- ~~~xml\n  示例代码\n  ~~~',
    '123. ~~~xml\n     示例代码\n     ~~~',
  ]) {
    const raw = message({
      content: `${prefix}\n\n<thinking>\n正文示例\n</thinking>\n\n正文`,
      reasoning: `${prefix}\n\n搜索前\n</thinking><thinking>\n搜索后`,
    })
    const [result] = normalizeMessages([raw])
    assert.equal(result.content, raw.content)
    assert.equal(result.reasoning, `${prefix}\n\n搜索前\n\n搜索后`)
    assert.deepEqual(normalizeMessages([result]), [result])
  }
})

test('历史长行正文保持原文', () => {
  const prefix = 'a'.repeat(256_000)
  const raw = message({ content: `${prefix}<thinking>\n正文示例\n</thinking>\n\n正文` })
  assert.deepEqual(normalizeMessages([raw]), [raw])
})

test('缩进代码中的思考分界保持原文', () => {
  for (const reasoning of [
    '示例：\n\n    </thinking><thinking>\n    示例代码',
    '示例：\n\n\t</thinking><thinking>\n\t示例代码',
    '- 示例：\n\n      </thinking><thinking>\n      示例代码',
    '-     </thinking><thinking>\n      示例代码',
  ]) {
    const raw = message({ reasoning, content: '正文不变' })
    assert.deepEqual(normalizeMessages([raw]), [raw])
  }
})

test('恢复历史仍会结束遗留的生成与搜索状态', () => {
  const raw = message({
    status: 'streaming',
    content: '未完成的正文',
    searches: [{ id: 'search', query: 'rust', status: 'searching', results: [] }],
  })
  const [result] = normalizeMessages([raw])
  assert.equal(result.status, 'stopped')
  assert.equal(result.searches[0].status, 'stopped')
  assert.deepEqual(normalizeMessages(null), [])
})
