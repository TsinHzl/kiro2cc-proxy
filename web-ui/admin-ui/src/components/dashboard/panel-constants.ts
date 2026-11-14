// Copyright (c) 2026 Harllan He. Licensed under MIT.
// Dashboard 常量与本地工具（自 dashboard.tsx 拆出，纯代码搬移）

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
export const ACTION_BTN = `${ACTION_BTN_BASE} border-transparent bg-surface text-ink-2 [&_svg]:text-ink-3 hover:bg-surface-2 hover:text-ink hover:[&_svg]:text-ink-2`
/** 主按钮（设计稿 .btn-primary） */
export const ACTION_BTN_PRIMARY = `${ACTION_BTN_BASE} border-transparent bg-brand font-semibold text-brand-fg [&_svg]:text-brand-fg [&_svg]:opacity-90 hover:bg-brand-hover`

export const SIDEBAR_COLLAPSED_STORAGE_KEY = 'sidebar-collapsed'
export const CHAT_SIDEBAR_OPEN_STORAGE_KEY = 'playground:v1:sidebar-open'
/** 账号表排序持久化：列 key（account/remaining），缺省即默认排序 */
export const ACCOUNT_SORT_STORAGE_KEY = 'account-sort-key'
/** 账号表排序持久化：方向（asc/desc），换列/恢复默认时移除 */
export const ACCOUNT_SORT_STORAGE_KEY_DIR = 'account-sort-dir'
// 与 aside/main 的 Tailwind `duration-200` 宽度过渡保持一致，Logo 头部布局延迟这么久才跟随切换
export const SIDEBAR_TRANSITION_MS = 200

export function readStoredSidebarCollapsed(): boolean {
  try {
    return localStorage.getItem(SIDEBAR_COLLAPSED_STORAGE_KEY) === 'true'
  } catch {
    return false
  }
}

export function writeStoredSidebarCollapsed(collapsed: boolean): void {
  try {
    localStorage.setItem(SIDEBAR_COLLAPSED_STORAGE_KEY, String(collapsed))
  } catch {
    /* localStorage 不可用时仅依赖服务端与内存状态 */
  }
}

export function readStoredChatSidebarOpen(): boolean {
  try {
    return localStorage.getItem(CHAT_SIDEBAR_OPEN_STORAGE_KEY) !== 'false'
  } catch {
    return true
  }
}

export function writeStoredChatSidebarOpen(open: boolean): void {
  try {
    localStorage.setItem(CHAT_SIDEBAR_OPEN_STORAGE_KEY, String(open))
  } catch {
    /* localStorage 不可用时仅依赖服务端与内存状态 */
  }
}
