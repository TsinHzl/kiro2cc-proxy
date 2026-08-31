// Copyright (c) 2026 Harllan He. Licensed under MIT.

/**
 * 复制文本到剪贴板
 * 安全上下文下使用 Clipboard API，否则降级为 execCommand
 */
export async function copyToClipboard(text: string): Promise<void> {
  if (navigator.clipboard) {
    await navigator.clipboard.writeText(text)
    return
  }

  const textarea = document.createElement('textarea')
  textarea.value = text
  textarea.readOnly = true
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.focus()
  textarea.select()
  try {
    const ok = document.execCommand('copy')
    if (!ok) throw new Error('复制失败：浏览器不支持自动复制')
  } finally {
    document.body.removeChild(textarea)
  }
}
