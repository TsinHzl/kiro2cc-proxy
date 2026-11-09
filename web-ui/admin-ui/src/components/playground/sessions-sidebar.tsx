// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { Loader2, MessageSquarePlus, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { ChatSessionMeta } from '@/types/api'

interface SessionsSidebarProps {
  sessions: ChatSessionMeta[] | undefined
  isLoading: boolean
  activeId: string | null
  /** 当前正在生成：禁止切换 / 新建 / 删除当前会话，避免生成结果落到别的会话里 */
  locked: boolean
  className?: string
  onNew: () => void
  onSelect: (id: string) => void
  onDelete: (session: ChatSessionMeta) => void
  onClearAll: () => void
}

/** 当天只显示时分，其他日期显示月-日 */
function formatTime(ms: number): string {
  const date = new Date(ms)
  const sameDay = date.toDateString() === new Date().toDateString()
  return date.toLocaleString(
    undefined,
    sameDay ? { hour: '2-digit', minute: '2-digit' } : { month: '2-digit', day: '2-digit' },
  )
}

/** 历史对话列表：切换、删除单个会话、清空全部 */
export function SessionsSidebar({
  sessions,
  isLoading,
  activeId,
  locked,
  className = '',
  onNew,
  onSelect,
  onDelete,
  onClearAll,
}: SessionsSidebarProps) {
  const { t } = useTranslation()
  const list = sessions ?? []

  return (
    <aside className={`flex h-full min-h-0 w-64 shrink-0 flex-col border-r-[0.5px] border-hairline bg-surface-2 md:w-56 ${className}`}>
      <div className="flex shrink-0 items-center gap-2 p-3">
        <button
          type="button"
          onClick={onNew}
          disabled={locked}
          className="inline-flex h-9 min-w-0 flex-1 items-center justify-center gap-1.5 rounded-lg border-[0.5px] border-hairline-2 bg-surface text-[12.5px] font-medium text-ink-2 transition-colors hover:bg-surface-3 hover:text-ink disabled:pointer-events-none disabled:opacity-50"
        >
          <MessageSquarePlus className="size-4 shrink-0" />
          <span className="truncate">{t('playground.newChat')}</span>
        </button>
        <button
          type="button"
          onClick={onClearAll}
          disabled={locked || list.length === 0}
          title={t('playground.clearAll')}
          aria-label={t('playground.clearAll')}
          className="grid size-9 shrink-0 place-items-center rounded-lg text-ink-3 transition-colors hover:bg-surface-3 hover:text-danger disabled:pointer-events-none disabled:opacity-40"
        >
          <Trash2 className="size-4" />
        </button>
      </div>

      <div className="px-4 pb-1.5 text-[10.5px] font-semibold tracking-[.04em] text-ink-3">
        {t('playground.history')}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
        {isLoading && list.length === 0 ? (
          <div className="grid place-items-center py-8 text-ink-3">
            <Loader2 className="size-4 animate-spin" />
          </div>
        ) : list.length === 0 ? (
          <p className="px-2 py-6 text-center text-[12px] leading-relaxed text-ink-3">{t('playground.historyEmpty')}</p>
        ) : (
          <ul className="space-y-0.5">
            {list.map((session) => {
              const active = session.id === activeId
              // 生成中只允许删除其他会话；当前会话要先停止生成
              const deleteDisabled = locked && active
              return (
                <li key={session.id} className="group relative">
                  <button
                    type="button"
                    onClick={() => onSelect(session.id)}
                    disabled={locked && !active}
                    aria-current={active || undefined}
                    className={`block w-full rounded-lg py-2 pl-3 pr-9 text-left transition-colors disabled:pointer-events-none disabled:opacity-60 ${
                      active ? 'bg-brand-soft' : 'hover:bg-surface-3'
                    }`}
                  >
                    <div className={`truncate text-[13px] ${active ? 'font-semibold text-ink' : 'text-ink-2'}`}>
                      {session.title || t('playground.untitled')}
                    </div>
                    <div className="mt-0.5 truncate text-[11px] text-ink-3">
                      {formatTime(session.updatedAt)} · {t('playground.messageCount', { count: session.messageCount })}
                    </div>
                  </button>
                  <button
                    type="button"
                    onClick={() => onDelete(session)}
                    disabled={deleteDisabled}
                    title={t('playground.deleteSession')}
                    aria-label={t('playground.deleteSession')}
                    className="absolute right-1.5 top-1/2 grid size-7 -translate-y-1/2 place-items-center rounded-md text-ink-3 opacity-0 transition-opacity hover:bg-surface hover:text-danger focus-visible:opacity-100 disabled:pointer-events-none disabled:opacity-0 group-hover:opacity-100 max-md:opacity-100"
                  >
                    <Trash2 className="size-3.5" />
                  </button>
                </li>
              )
            })}
          </ul>
        )}
      </div>
    </aside>
  )
}
