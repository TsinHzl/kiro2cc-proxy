// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { FileText, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { formatFileSize, imageDataUrl } from './attachments'
import type { Attachment } from './types'

interface AttachmentViewProps {
  attachment: Attachment
  /** 传入后右上角显示移除按钮（输入框中的待发送附件） */
  onRemove?: () => void
  /** 消息气泡里的图片用较大的缩略图 */
  large?: boolean
}

/** 附件缩略展示：图片显示缩略图，文本文件显示文件名与大小 */
export function AttachmentView({ attachment, onRemove, large = false }: AttachmentViewProps) {
  const { t } = useTranslation()

  return (
    <div className="group/att relative">
      {attachment.kind === 'image' ? (
        <img
          src={imageDataUrl(attachment)}
          alt={attachment.name}
          title={attachment.name}
          className={`rounded-lg border-[0.5px] border-hairline-2 object-cover ${
            large ? 'max-h-40 max-w-[240px]' : 'size-14'
          }`}
        />
      ) : (
        <div
          title={attachment.name}
          className="flex h-14 max-w-[220px] items-center gap-2 rounded-lg border-[0.5px] border-hairline-2 bg-surface-2 px-2.5"
        >
          <FileText className="size-4 shrink-0 text-ink-3" />
          <div className="min-w-0">
            <div className="truncate text-[12px] font-medium text-ink-2">{attachment.name}</div>
            <div className="text-[10.5px] text-ink-3">{formatFileSize(attachment.size)}</div>
          </div>
        </div>
      )}
      {onRemove && (
        <button
          type="button"
          onClick={onRemove}
          title={t('playground.attachRemove')}
          aria-label={t('playground.attachRemove')}
          className="absolute -right-1.5 -top-1.5 grid size-5 place-items-center rounded-full border-[0.5px] border-hairline-2 bg-surface text-ink-3 shadow-hair transition-colors hover:text-danger"
        >
          <X className="size-3" />
        </button>
      )}
    </div>
  )
}
