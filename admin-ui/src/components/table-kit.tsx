// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import * as CheckboxPrimitive from '@radix-ui/react-checkbox'
import { Check, ChevronLeft, ChevronRight, Minus } from 'lucide-react'

/** 面板外壳（设计稿 .panel） */
export const PANEL = 'overflow-hidden rounded-[11px] border border-hairline bg-surface shadow-panel'

/** 面板上方小标题（设计稿 .panel 前的段标题） */
export const PANEL_TITLE = 'mb-[7px] text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3'

/** 面板脚（设计稿 .panel-foot） */
export const PANEL_FOOT =
  'flex flex-wrap items-center gap-x-2.5 gap-y-1.5 border-t border-hairline bg-surface-2 px-[13px] py-[9px] text-[11.5px] text-ink-3'

/** 设计稿 thead th：10.5px 大写字距 .07em + surface-2 底 + sticky */
export const TH_BASE =
  'sticky top-0 z-[2] whitespace-nowrap border-b border-hairline bg-surface-2 px-3 py-[9px] text-left text-[10.5px] font-semibold uppercase tracking-[0.07em] text-ink-3'

/** 单元格基线（设计稿 tbody td）：11px/12px 内边距 + 分隔线 + 垂直居中 */
export const CELL = 'border-b border-hairline px-3 py-[11px] align-middle'

/** 26px 图标钮（设计稿 .iconbtn） */
export const ICON_BTN =
  'grid size-[26px] flex-none place-items-center rounded-[6px] text-ink-3 transition-colors hover:bg-surface-3 hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand'

/** 设计稿 .panel-foot 内的 btn-ghost：24px 高 / 11.5px 字，与主操作条的 31px 按钮分级 */
export const FOOT_BTN =
  'inline-flex h-6 flex-none items-center rounded-[6px] px-[7px] text-[11.5px] font-medium text-ink-2 transition-colors hover:bg-surface-3 hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent'

/** 设计稿 .pager button：24px 方钮，当前页 surface-3 底 + 加粗 */
export const PAGER_BTN =
  'grid h-6 min-w-6 flex-none place-items-center rounded-[5px] px-1 text-[11.5px] text-ink-2 transition-colors hover:bg-surface-3 hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent'

/** 页码窗口：最多 5 个数字钮，居中当前页并在两端收敛 */
const PAGE_WINDOW = 5
export function pageWindow(page: number, totalPages: number): number[] {
  const size = Math.min(PAGE_WINDOW, totalPages)
  const start = Math.min(Math.max(1, page - Math.floor(size / 2)), totalPages - size + 1)
  return Array.from({ length: size }, (_, i) => start + i)
}

/** 页码组（设计稿 .pager）：前后翻页钮 + 最多 5 个数字钮 */
export function Pager({
  page,
  totalPages,
  onPage,
}: {
  page: number
  totalPages: number
  onPage: (page: number) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex gap-[2px]">
      <button
        type="button"
        className={PAGER_BTN}
        onClick={() => onPage(Math.max(1, page - 1))}
        disabled={page <= 1}
        aria-label={t('common.prevPage')}
      >
        <ChevronLeft className="size-[13px]" strokeWidth={1.8} />
      </button>
      {pageWindow(page, totalPages).map((n) => (
        <button
          key={n}
          type="button"
          onClick={() => onPage(n)}
          aria-current={n === page ? 'page' : undefined}
          className={`${PAGER_BTN} ${n === page ? 'bg-surface-3 font-semibold text-ink' : ''}`}
        >
          {n}
        </button>
      ))}
      <button
        type="button"
        className={PAGER_BTN}
        onClick={() => onPage(Math.min(totalPages, page + 1))}
        disabled={page >= totalPages}
        aria-label={t('common.nextPage')}
      >
        <ChevronRight className="size-[13px]" strokeWidth={1.8} />
      </button>
    </div>
  )
}

/**
 * 表格复选框（设计稿 .cb，14px / 4px 圆角）：表头需要 indeterminate 半选态且半选要显示横线
 * 而非对勾，shadcn 的 Checkbox 封装把指示器图标固定为 Check，故直接用 Radix Primitive。
 * 行内复选框复用同一规格，只是不传 indeterminate。
 */
export function DataCheckbox({
  checked,
  indeterminate = false,
  disabled = false,
  onToggle,
  label,
}: {
  checked: boolean
  indeterminate?: boolean
  disabled?: boolean
  onToggle: () => void
  label: string
}) {
  return (
    <CheckboxPrimitive.Root
      checked={checked ? true : indeterminate ? 'indeterminate' : false}
      onCheckedChange={onToggle}
      disabled={disabled}
      aria-label={label}
      className="grid h-[14px] w-[14px] place-items-center rounded-[4px] border-[1.5px] border-hairline-2 bg-surface text-brand-fg transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:border-brand data-[state=checked]:bg-brand data-[state=indeterminate]:border-brand data-[state=indeterminate]:bg-brand"
    >
      <CheckboxPrimitive.Indicator className="grid place-items-center text-current">
        {checked ? <Check className="size-[10px]" /> : <Minus className="size-[10px]" />}
      </CheckboxPrimitive.Indicator>
    </CheckboxPrimitive.Root>
  )
}
