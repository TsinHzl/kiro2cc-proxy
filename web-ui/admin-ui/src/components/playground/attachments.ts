// Copyright (c) 2026 Harllan He. Licensed under MIT.
import type { Attachment } from './types'

/** 单条消息最多携带的附件数 */
export const MAX_ATTACHMENTS = 5
/** 文本文件上限；整份内容会作为提示词发给模型 */
export const MAX_TEXT_BYTES = 256 * 1024

/** 上游（Kiro）能识别的图片格式 */
const IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/gif', 'image/webp']
/** 长边超过该值的图片会缩小（Anthropic 建议的上限），会话里每轮都要重传历史，体积越小越好 */
const MAX_IMAGE_EDGE = 1568
/** 不超过该体积且尺寸合规的图片原样发送 */
const KEEP_ORIGINAL_BYTES = 1024 * 1024
const JPEG_QUALITY = 0.85

export type AttachmentErrorCode = 'unsupported' | 'tooLarge' | 'failed'

export class AttachmentError extends Error {
  constructor(
    readonly code: AttachmentErrorCode,
    readonly fileName: string,
  ) {
    super(`${code}: ${fileName}`)
  }
}

function newId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onerror = () => reject(reader.error)
    reader.onload = () => {
      const url = String(reader.result)
      resolve(url.slice(url.indexOf(',') + 1))
    }
    reader.readAsDataURL(blob)
  })
}

async function processImage(file: File): Promise<Attachment> {
  let bitmap: ImageBitmap
  try {
    bitmap = await createImageBitmap(file)
  } catch {
    throw new AttachmentError('failed', file.name)
  }
  try {
    const base = { id: newId(), kind: 'image' as const, name: file.name, size: file.size }
    const longEdge = Math.max(bitmap.width, bitmap.height)
    if (file.size <= KEEP_ORIGINAL_BYTES && longEdge <= MAX_IMAGE_EDGE) {
      return { ...base, mediaType: file.type, data: await blobToBase64(file) }
    }

    // 缩小并转成 JPEG；透明底先铺白色，否则会变成黑色
    const scale = Math.min(1, MAX_IMAGE_EDGE / longEdge)
    const canvas = document.createElement('canvas')
    canvas.width = Math.max(1, Math.round(bitmap.width * scale))
    canvas.height = Math.max(1, Math.round(bitmap.height * scale))
    const ctx = canvas.getContext('2d')
    if (!ctx) throw new AttachmentError('failed', file.name)
    ctx.fillStyle = '#fff'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/jpeg', JPEG_QUALITY))
    if (!blob) throw new AttachmentError('failed', file.name)
    return { ...base, mediaType: 'image/jpeg', data: await blobToBase64(blob) }
  } finally {
    bitmap.close()
  }
}

async function processText(file: File): Promise<Attachment> {
  if (file.size > MAX_TEXT_BYTES) throw new AttachmentError('tooLarge', file.name)
  let text: string
  try {
    text = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer())
  } catch {
    // 不是合法 UTF-8：二进制文件或其他编码
    throw new AttachmentError('unsupported', file.name)
  }
  if (text.includes('\u0000')) throw new AttachmentError('unsupported', file.name)
  return {
    id: newId(),
    kind: 'text',
    name: file.name,
    mediaType: file.type || 'text/plain',
    size: file.size,
    data: text,
  }
}

/** 把用户选择的文件转成附件：支持 PNG / JPEG / GIF / WebP 图片和 UTF-8 文本文件（代码、日志、Markdown 等） */
export function processFile(file: File): Promise<Attachment> {
  if (IMAGE_TYPES.includes(file.type)) return processImage(file)
  // 其他图片格式（SVG / HEIC / BMP 等）和 PDF 上游无法读取，不当作文本处理
  if (file.type.startsWith('image/') || file.type === 'application/pdf') {
    return Promise.reject(new AttachmentError('unsupported', file.name))
  }
  return processText(file)
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

export function imageDataUrl(a: Attachment): string {
  return `data:${a.mediaType};base64,${a.data}`
}
