// Copyright (c) 2026 Harllan He. Licensed under MIT.
import type { ReactNode } from 'react'
import { ArrowLeft } from 'lucide-react'
import { useTranslation } from 'react-i18next'

interface PageHeadProps {
  /** 面包屑层级，末级与 title 同文案 */
  crumb: string[]
  title: string
  /** 与标题同基线的副标题 */
  note?: ReactNode
  /** 右侧动作区 */
  actions?: ReactNode
  /** 传入时在标题左侧渲染返回按钮 */
  onBack?: () => void
}

/** 页头（设计稿 .head）：面包屑 + 19px 标题 + 同基线副标题 + 右侧动作区 */
export function PageHead({ crumb, title, note, actions, onBack }: PageHeadProps) {
  const { t } = useTranslation()

  return (
    <div className="mb-5 flex flex-wrap items-end gap-4">
      <div>
        {/* 仅当面包屑末级与 h1 同文案时才对读屏隐藏（避免连读两次）；
            末级文案与标题不一致的页面必须保留导航层级可读 */}
        {crumb.length > 0 && (
          <div
            aria-hidden={crumb[crumb.length - 1] === title || undefined}
            className="mb-1.5 flex items-center gap-1.5 text-[11px] font-medium tracking-[.02em] text-ink-3"
          >
            {crumb.map((seg, i) => (
              <span key={i} className="inline-flex items-center gap-1.5">
                {i > 0 && <span className="text-hairline-2">/</span>}
                {i === crumb.length - 1 ? <b className="font-semibold text-ink-2">{seg}</b> : seg}
              </span>
            ))}
          </div>
        )}
        <div className="flex items-center gap-2.5">
          {onBack && (
            <button
              type="button"
              onClick={onBack}
              aria-label={t('common.back')}
              className="grid h-7 w-7 shrink-0 place-items-center rounded-[7px] border border-transparent bg-surface bg-control-glass text-ink-2 shadow-hair transition-all hover:bg-surface-2 hover:text-ink active:scale-95"
            >
              <ArrowLeft className="h-3.5 w-3.5" />
            </button>
          )}
          <h1 className="text-[20px] font-semibold leading-[1.2] tracking-[-.025em] text-ink">{title}</h1>
        </div>
      </div>
      {note && <p className="ml-0.5 min-w-0 truncate pb-0.5 text-[12px] text-ink-3">{note}</p>}
      {actions && <div className="ml-auto flex flex-wrap items-center justify-end gap-2">{actions}</div>}
    </div>
  )
}
