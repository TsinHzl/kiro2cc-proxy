// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { storage } from '@/lib/storage'

/** 下载超时：单 Key / 账号最多 10 万条明细（约数十 MB），慢链路留足时间 */
const DOWNLOAD_TIMEOUT_MS = 120_000
/** 对象 URL 延迟释放时长：click 后浏览器才异步读取 blob，立即撤销可能中断下载 */
const OBJECT_URL_RELEASE_MS = 60_000

/** 从 Content-Disposition 提取文件名；服务端已给名则优先采用，解析失败返回 null */
function filenameFromDisposition(header: string | null): string | null {
  if (!header) return null
  const match = header.match(/filename="([^"]+)"/)
  return match ? match[1] : null
}

/** 读取非 2xx 响应体并提取后端错误信息；解析失败回退为 `HTTP <status>` */
async function errorMessageOf(res: Response): Promise<string> {
  try {
    const body = (await res.json()) as { error?: { message?: unknown } }
    const message = body?.error?.message
    if (typeof message === 'string' && message) return message
  } catch {
    // 非 JSON（如反代返回 HTML）：回退通用文案
  }
  return `HTTP ${res.status}`
}

/**
 * 经 fetch + Blob 触发浏览器下载（鉴权走 `x-api-key` 头，不把凭据放进 URL）。
 *
 * 不走 axios：其默认 responseType 为 json，且错误响应体会变成 Blob，取不到后端错误信息。
 */
export async function downloadFile(
  url: string,
  params: Record<string, string> | undefined,
  fallbackName: string,
): Promise<void> {
  const query = params ? `?${new URLSearchParams(params).toString()}` : ''
  const res = await fetch(`${url}${query}`, {
    headers: { 'x-api-key': storage.getApiKey() ?? '' },
    signal: AbortSignal.timeout(DOWNLOAD_TIMEOUT_MS),
  })
  if (!res.ok) throw new Error(await errorMessageOf(res))

  const blob = await res.blob()
  const name = filenameFromDisposition(res.headers.get('Content-Disposition')) ?? fallbackName
  const objectUrl = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = objectUrl
  a.download = name
  a.rel = 'noopener'
  // 部分浏览器要求锚点已挂载才触发下载
  document.body.appendChild(a)
  a.click()
  a.remove()
  window.setTimeout(() => URL.revokeObjectURL(objectUrl), OBJECT_URL_RELEASE_MS)
}
