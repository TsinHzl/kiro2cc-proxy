// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { ChevronLeft, ChevronRight } from 'lucide-react'

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
  if (totalPages <= 0) return []
  const size = Math.min(PAGE_WINDOW, totalPages)
  const start = Math.max(1, Math.min(page - Math.floor(size / 2), totalPages - size + 1))
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
  return (
    <div className="flex gap-[2px]">
      <button
        type="button"
        className={PAGER_BTN}
        onClick={() => onPage(Math.max(1, page - 1))}
        disabled={page <= 1}
        aria-label="上一页"
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
        aria-label="下一页"
      >
        <ChevronRight className="size-[13px]" strokeWidth={1.8} />
      </button>
    </div>
  )
}
