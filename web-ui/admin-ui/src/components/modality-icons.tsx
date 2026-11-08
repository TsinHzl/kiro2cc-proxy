// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import { FileText, Image as ImageIcon } from 'lucide-react'
import { inputModalities, modalityEmptyState } from '@/lib/model-family'

/**
 * 模型输入模态的图标徽章（文本 / 图片），供「支持模型」弹窗与对话页模型选择器共用。
 *
 * 空态与列表页同口径：上游未声明显示 `—`、声明了但全不可识别显示 `?`，均不猜测能力。
 */
export function ModalityIcons({ inputTypes }: { inputTypes?: string[] | null }) {
  const { t } = useTranslation()
  const modalities = inputModalities(inputTypes)

  if (modalities.length === 0) {
    const empty = modalityEmptyState(inputTypes)
    const hint = t(empty.kind === 'unrecognized' ? 'models.inputUnrecognized' : 'models.inputNone', {
      values: empty.kind === 'unrecognized' ? empty.values : undefined,
    })
    return (
      <span role="img" aria-label={hint} title={hint} className="text-ink-3">
        {empty.kind === 'unrecognized' ? '?' : '—'}
      </span>
    )
  }

  return (
    <span className="flex items-center gap-1">
      {modalities.map((modality) => {
        const label = t(modality === 'image' ? 'models.inputImage' : 'models.inputText')
        return (
          <span
            key={modality}
            role="img"
            aria-label={label}
            title={label}
            className={`inline-flex size-[18px] items-center justify-center rounded-[4px] ${
              modality === 'image' ? 'bg-brand-soft text-brand-ink' : 'bg-surface-3 text-ink-2'
            }`}
          >
            {modality === 'image' ? (
              <ImageIcon className="size-[11px]" aria-hidden="true" />
            ) : (
              <FileText className="size-[11px]" aria-hidden="true" />
            )}
          </span>
        )
      })}
    </span>
  )
}
