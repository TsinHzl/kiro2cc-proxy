// Copyright (c) 2026 Harllan He. Licensed under MIT.
// Dashboard 常量与本地工具（自 dashboard.tsx 拆出，纯代码搬移）
// 侧栏身份区：后端仅有单一管理员口令，无用户名概念 → 名称固定，头像取首字母
export const ADMIN_NAME = 'admin'

/**
 * credits 环比的最小昨日基线。
 *
 * credits 是浮点量，可任意接近 0，若沿用调用次数那套「分母 > 0」守卫，昨日 0.001、
 * 今日 5 会算出 ↑499900%，而 Delta 徽标不做上限裁剪，会把这种无意义的值直接渲染出来。
 * 调用次数不存在该问题（整数分母最小为 1，放大倍数天然有上限）。
 */
export const CREDITS_DELTA_MIN_BASE = 0.5

/** 本地时区 YYYY-MM-DD（日用量接口按本地日期对齐） */
export function formatLocalDate(d: Date) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

/** 操作条按钮基类（设计稿 .btn）：31px 高 / 7px 圆角 / 15px 图标 */
const ACTION_BTN_BASE =
  'inline-flex h-[31px] items-center gap-1.5 whitespace-nowrap rounded-[7px] border px-[11px] text-[12.5px] font-medium shadow-hair transition-colors [&_svg]:size-[15px] [&_svg]:shrink-0 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand disabled:pointer-events-none disabled:opacity-50'
/** 常规态（设计稿 .btn） */
export const ACTION_BTN = `${ACTION_BTN_BASE} border-hairline-2 bg-surface text-ink-2 [&_svg]:text-ink-3 hover:border-ink-3 hover:bg-surface-2 hover:text-ink hover:[&_svg]:text-ink-2`
/** 危险态（设计稿 .btn-danger） */
export const ACTION_BTN_DANGER = `${ACTION_BTN_BASE} border-danger-line bg-surface text-danger [&_svg]:text-danger hover:border-danger hover:bg-danger-soft`
/** 主按钮（设计稿 .btn-primary） */
export const ACTION_BTN_PRIMARY = `${ACTION_BTN_BASE} border-transparent bg-brand font-semibold text-brand-fg [&_svg]:text-brand-fg [&_svg]:opacity-90 hover:bg-brand-hover`
/** 竖分隔线（设计稿 .actionbar .vdiv） */
export const ACTION_VDIV = 'mx-0.5 h-[19px] w-px shrink-0 bg-hairline-2'

export const SIDEBAR_COLLAPSED_STORAGE_KEY = 'sidebar-collapsed'
// 与 aside/main 的 Tailwind `duration-200` 宽度过渡保持一致，Logo 头部布局延迟这么久才跟随切换
export const SIDEBAR_TRANSITION_MS = 200

export function readStoredSidebarCollapsed(): boolean {
  return localStorage.getItem(SIDEBAR_COLLAPSED_STORAGE_KEY) === 'true'
}
