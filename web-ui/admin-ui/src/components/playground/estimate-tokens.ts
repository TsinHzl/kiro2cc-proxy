// Copyright (c) 2026 Harllan He. Licensed under MIT.

// 中日韩文字、全角标点与假名、韩文音节
const CJK = /[\u2e80-\u9fff\uac00-\ud7af\uff00-\uffef]/g

/**
 * 粗略估算一段文字的 token 数（只用于深度思考内容的展示）。
 *
 * 接口不单独返回深度思考的 token 数，这里按字符类别估算：
 * 中日韩字符约 1.3 token/字，其余字符约 4 字符 1 token（cl100k 一类分词器的常见比例）。
 * 误差可能有两成左右，所以界面上会带「约」字。
 */
export function estimateTokens(text: string): number {
  if (text === '') return 0
  const cjk = text.match(CJK)?.length ?? 0
  const rest = text.length - cjk
  return Math.max(1, Math.round(cjk * 1.3 + rest / 4))
}
