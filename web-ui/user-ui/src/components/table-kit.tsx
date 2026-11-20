// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { ChevronLeft, ChevronRight } from 'lucide-react'

/** 面板外壳（设计稿 .panel） */
export const PANEL = 'overflow-hidden rounded-[12px] border border-transparent bg-chat-canvas shadow-panel'

/** 面板上方小标题（设计稿 .panel 前的段标题） */
export const PANEL_TITLE = 'mb-2 text-[10.5px] font-semibold uppercase tracking-[.08em] text-ink-3'

/** 面板脚（设计稿 .panel-foot） */
export const PANEL_FOOT =
  'flex flex-wrap items-center gap-x-2.5 gap-y-1.5 border-t-[0.5px] border-hairline bg-[color-mix(in_srgb,var(--surface-2)_45%,transparent)] px-3.5 py-2.5 text-[11.5px] text-ink-3'

/** 设计稿 thead th：10.5px 大写字距 .07em + 磨砂底 + sticky */
export const TH_BASE =
  'sticky top-0 z-[2] whitespace-nowrap border-b-[0.5px] border-hairline bg-[color-mix(in_srgb,var(--surface-2)_72%,transparent)] px-3.5 py-[9px] text-left text-[10.5px] font-semibold uppercase tracking-[0.07em] text-ink-3 backdrop-blur-md'

/** 表格行悬停高亮 */
export const ROW_HOVER = 'transition-colors duration-150 hover:bg-[color-mix(in_srgb,var(--surface-2)_55%,transparent)]'

/** 单元格基线（设计稿 tbody td）：11px/14px 内边距 + 分隔线 + 垂直居中 */
export const CELL = 'border-b-[0.5px] border-hairline px-3.5 py-[11px] align-middle'

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
