// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { sha256 } from 'js-sha256'

/**
 * 计算字符串的 SHA-256 十六进制摘要
 * 安全上下文（HTTPS/localhost）下使用 Web Crypto API，否则降级为纯 JS 实现
 */
export async function sha256Hex(value: string): Promise<string> {
  if (crypto.subtle) {
    const encoded = new TextEncoder().encode(value)
    const digest = await crypto.subtle.digest('SHA-256', encoded)
    const bytes = new Uint8Array(digest)
    return Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')
  }
  return sha256(value)
}
