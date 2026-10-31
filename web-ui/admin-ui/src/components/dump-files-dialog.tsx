// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from '@/lib/toast'
import { Download, RefreshCw } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { useDumpFiles } from '@/hooks/use-credentials'
import type { DumpFileMeta } from '@/types/api'
import { extractErrorMessage, parseError } from '@/lib/utils'
import { formatDateTime } from '@/lib/locale'
import { storage } from '@/lib/storage'
import { CELL, TH_BASE } from '@/components/table-kit'

/** 单文件下载超时：单个 dump 文件约 170 KB，30s 足以覆盖慢链路 */
const FILE_TIMEOUT_MS = 30_000
/** 归档下载超时：生产上限 128 MiB，慢链路留足时间 */
const ARCHIVE_TIMEOUT_MS = 180_000
/** 对象 URL 延迟释放时长：click 后浏览器才异步读取 blob，过大归档（128 MiB）立即撤销会中断下载 */
const OBJECT_URL_RELEASE_MS = 60_000

/** 字节 → 人类可读（1 位小数，逐级降档至 GB） */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB'] as const
  let value = bytes / 1024
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value.toFixed(1)} ${units[unit]}`
}

/** 从 Content-Disposition 提取文件名；服务端已给名则优先采用，解析失败返回 null */
function filenameFromDisposition(header: string | null): string | null {
  if (!header) return null
  const match = header.match(/filename="([^"]+)"/)
  return match ? match[1] : null
}

/** 下载失败的结构化错误：保留状态码与响应体详情，调用方按 status 判定而非解析文案 */
class HttpError extends Error {
  readonly status: number
  readonly detail: string

  constructor(status: number, detail: string) {
    super(`HTTP ${status}`)
    this.name = 'HttpError'
    this.status = status
    this.detail = detail
  }
}

/** 读取非 2xx 响应体并提取后端错误详情；解析失败返回空串，由调用方回退通用文案 */
async function buildErrorDetail(res: Response): Promise<string> {
  try {
    const text = await res.text()
    if (!text) return ''
    try {
      const body = JSON.parse(text) as { error?: { message?: unknown } }
      const message = body?.error?.message
      return typeof message === 'string' && message ? message : text
    } catch {
      // 上游 502/504 等返回 HTML 时不是 JSON，原样展示文本
      return text
    }
  } catch {
    return ''
  }
}

/** 毫秒时间戳 → 本地时间文案；非法值降级为占位符（时间戳取自文件名，手工放入的超长数字名可超出 Date 上限） */
function formatDateMs(ms: number): string {
  if (!Number.isFinite(ms) || Math.abs(ms) > 8.64e15) return '—'
  return formatDateTime(new Date(ms).toISOString())
}

/**
 * 经 fetch + Blob 触发浏览器下载。
 *
 * 不走 axios：其默认 responseType 为 json，二进制响应会被解析破坏。
 * 鉴权用 `x-api-key` 头而非查询参数，避免凭据进入反向代理访问日志。
 */
async function triggerDownload(url: string, fallbackName: string, timeoutMs: number): Promise<void> {
  const res = await fetch(url, {
    headers: { 'x-api-key': storage.getApiKey() ?? '' },
    signal: AbortSignal.timeout(timeoutMs),
  })
  if (!res.ok) throw new HttpError(res.status, await buildErrorDetail(res))

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

interface DumpFilesDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function DumpFilesDialog({ open, onOpenChange }: DumpFilesDialogProps) {
  const { t } = useTranslation()
  const { data, isLoading, error, refetch } = useDumpFiles(open)
  const [busyName, setBusyName] = useState<string | null>(null)
  const [archiving, setArchiving] = useState(false)

  // 最新在前：createdAtMs 降序；时间戳相同时按名倒序，保证顺序稳定
  const files: DumpFileMeta[] = useMemo(
    () =>
      data
        ? [...data.files].sort(
            (a, b) => b.createdAtMs - a.createdAtMs || b.name.localeCompare(a.name)
          )
        : [],
    [data]
  )

  // 截断提示阈值取自响应，前后端不同步时不会误报（不在前端硬编码上限）
  const truncated = !!data && data.totalBytes > data.maxArchiveBytes
  const busy = busyName !== null || archiving

  /** 404 属常见竞态（列表展示后被滚动清理）：单独提示并刷新列表，不自动重试 */
  const reportFailure = (e: unknown, goneKey: string) => {
    if (e instanceof HttpError && e.status === 404) {
      toast.error(t(goneKey))
      void refetch()
      return
    }
    toast.error(e instanceof HttpError && e.detail ? e.detail : extractErrorMessage(e))
  }

  const handleDownloadOne = async (file: DumpFileMeta) => {
    if (busy) return
    setBusyName(file.name)
    try {
      await triggerDownload(
        `/api/admin/dump-kiro-body/files/${encodeURIComponent(file.name)}`,
        file.name,
        FILE_TIMEOUT_MS
      )
    } catch (e) {
      reportFailure(e, 'dumpFiles.goneHint')
    } finally {
      setBusyName(null)
    }
  }

  const handleDownloadArchive = async () => {
    if (busy) return
    setArchiving(true)
    try {
      await triggerDownload(
        '/api/admin/dump-kiro-body/archive',
        `kiro-body-dump-${new Date().toISOString().replace(/[:.]/g, '-')}.zip`,
        ARCHIVE_TIMEOUT_MS
      )
    } catch (e) {
      reportFailure(e, 'dumpFiles.archiveGoneHint')
    } finally {
      setArchiving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[90vh] w-[calc(100%-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[720px] sm:p-0">
        <DialogHeader className="flex shrink-0 flex-col gap-1 border-b-[0.5px] border-hairline px-[22px] pb-4 pt-5 text-left">
          <DialogTitle className="text-[17px] font-semibold tracking-tight text-ink">
            {t('dumpFiles.dialogTitle')}
          </DialogTitle>
          <DialogDescription>{t('dumpFiles.dialogDesc')}</DialogDescription>
        </DialogHeader>

        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-[22px] py-4">
          {isLoading && (
            <div className="flex items-center justify-center py-10">
              <div
                role="status"
                aria-label={t('common.loading')}
                className="h-8 w-8 animate-spin rounded-full border-b-2 border-brand"
              />
            </div>
          )}

          {error &&
            (() => {
              const parsed = parseError(error)
              return (
                <div className="space-y-3 py-8 text-center">
                  <div className="font-medium text-danger">{parsed.title}</div>
                  {parsed.detail && (
                    <div className="text-[11.5px] leading-[1.55] text-ink-3">{parsed.detail}</div>
                  )}
                  <div>
                    <Button variant="outline" onClick={() => void refetch()}>
                      <RefreshCw aria-hidden="true" />
                      {t('common.retry')}
                    </Button>
                  </div>
                </div>
              )
            })()}

          {data && files.length === 0 && (
            <div className="py-10 text-center text-[11.5px] text-ink-3">{t('dumpFiles.empty')}</div>
          )}

          {files.length > 0 && (
            <div className="overflow-x-auto">
              <table className="w-full border-collapse">
                <thead>
                  <tr>
                    <th scope="col" className={TH_BASE}>{t('dumpFiles.colName')}</th>
                    <th scope="col" className={TH_BASE}>{t('dumpFiles.colSize')}</th>
                    <th scope="col" className={TH_BASE}>{t('dumpFiles.colTime')}</th>
                    <th scope="col" className={`${TH_BASE} text-right`}>{t('dumpFiles.colActions')}</th>
                  </tr>
                </thead>
                <tbody>
                  {files.map((file) => (
                    <tr key={file.name}>
                      <td className={`${CELL} break-all font-mono text-[11.5px]`}>{file.name}</td>
                      <td className={`${CELL} whitespace-nowrap text-[11.5px] text-ink-2`}>
                        {formatBytes(file.sizeBytes)}
                      </td>
                      <td className={`${CELL} whitespace-nowrap text-[11.5px] text-ink-2`}>
                        {formatDateMs(file.createdAtMs)}
                      </td>
                      <td className={`${CELL} text-right`}>
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          disabled={busy}
                          onClick={() => void handleDownloadOne(file)}
                        >
                          <Download aria-hidden="true" />
                          {busyName === file.name
                            ? t('dumpFiles.downloading')
                            : t('dumpFiles.download')}
                        </Button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>

        {data && files.length > 0 && (
          <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-2 border-t-[0.5px] border-hairline bg-surface-2 px-[22px] py-3">
            <span className="text-[11.5px] text-ink-3">
              {t('dumpFiles.footSummary', {
                count: files.length,
                total: formatBytes(data.totalBytes),
                max: data.maxFiles,
              })}
            </span>
            <span className={`text-[11.5px] ${truncated ? 'text-danger' : 'text-ink-3'}`}>
              {truncated
                ? t('dumpFiles.truncatedHint', { limit: formatBytes(data.maxArchiveBytes) })
                : t('dumpFiles.archiveLimitHint', { limit: formatBytes(data.maxArchiveBytes) })}
            </span>
            <Button
              type="button"
              variant="outline"
              className="ml-auto"
              disabled={busy}
              onClick={() => void handleDownloadArchive()}
            >
              <Download aria-hidden="true" />
              {archiving ? t('dumpFiles.archiving') : t('dumpFiles.downloadArchive')}
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}