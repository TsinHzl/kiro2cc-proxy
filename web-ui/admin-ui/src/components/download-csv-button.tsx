// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Download, Loader2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { toast } from '@/lib/toast'
import { extractErrorMessage } from '@/lib/utils'

/**
 * 使用日志页的「下载 CSV」按钮。
 *
 * 各页只负责给出「下载什么」（onDownload），忙碌态、成功 / 失败提示与重入保护统一在这里。
 */
export function DownloadCsvButton({ onDownload }: { onDownload: () => Promise<void> }) {
  const { t } = useTranslation()
  const [busy, setBusy] = useState(false)

  const handleClick = async () => {
    setBusy(true)
    try {
      await onDownload()
      toast.success(t('common.downloadCsvSuccess'))
    } catch (error) {
      toast.error(t('common.downloadCsvFailed', { message: extractErrorMessage(error) }))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Button variant="outline" onClick={handleClick} disabled={busy}>
      {busy ? <Loader2 className="animate-spin" aria-hidden="true" /> : <Download aria-hidden="true" />}
      {t('common.downloadCsv')}
    </Button>
  )
}
