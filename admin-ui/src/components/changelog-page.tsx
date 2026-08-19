// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Check, Dot, Plus, RefreshCw, RotateCw, type LucideIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { PageHead } from '@/components/page-head'
import { PANEL } from '@/components/table-kit'
import { TAG_BASE } from '@/components/api-key-row'
import { extractErrorMessage } from '@/lib/utils'
import { useChangelog } from '@/hooks/use-credentials'
import type { ReleaseNote, ReleaseNoteGroup } from '@/types/api'

/** 分区语义档：与后端 changelog_data.rs 固定的三类分组一一对应，未知标题落 other */
type SectionKind = 'add' | 'fix' | 'chg' | 'other'

/**
 * 后端分组标题（src/admin/changelog_data.rs）；zh/en 两侧都登记，避免语言切换后失配。
 * 用 Map 而非对象字面量：标题是后端下发的任意字符串，裸下标查找会命中
 * Object.prototype 上的同名属性（如标题恰为 constructor），返回真值非法档。
 */
const SECTION_KIND = new Map<string, SectionKind>([
  ['新功能', 'add'],
  ['New Features', 'add'],
  ['修复', 'fix'],
  ['Fixes', 'fix'],
  ['优化', 'chg'],
  ['Improvements', 'chg'],
])
const SECTION_STYLE: Readonly<Record<SectionKind, string>> = {
  add: 'text-ok',
  fix: 'text-brand',
  chg: 'text-warn',
  other: 'text-ink-3',
}
const SECTION_ICON: Readonly<Record<SectionKind, LucideIcon>> = {
  add: Plus,
  fix: Check,
  chg: RefreshCw,
  other: Dot,
}
const TAG_CURRENT = `${TAG_BASE} border-brand-line bg-brand-soft text-brand`
const TL_IDX_LAB = 'px-0.5 pb-2 text-[10px] font-semibold uppercase tracking-[.08em] text-ink-3'
const TL_LINK =
  'flex h-[27px] w-full items-center gap-[7px] rounded-[6px] px-2 font-mono text-[11.5px] text-ink-3 transition-colors hover:bg-surface-3 hover:text-ink-2 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand'
const TL_LINK_ON = 'bg-brand-soft font-semibold text-brand'
/** 索引区「版本」档展示的最近版本数，其余进「归档」档 */
const RECENT_COUNT = 6
/** notes 引用稳定化，避免 data 未就绪时每次渲染新建数组打断 useMemo / useEffect */
const NO_NOTES: ReleaseNote[] = []

function kindOf(group: ReleaseNoteGroup): SectionKind {
  return SECTION_KIND.get(group.title_zh) ?? SECTION_KIND.get(group.title_en) ?? 'other'
}

/** 「2026-08-13」→「08-13」；格式不符时回退原串 */
function shortDate(date: string): string {
  return /^\d{4}-\d{2}-\d{2}$/.test(date) ? date.slice(5) : date
}

/** 「2.8.25」→「2.8」；段数不足时回退原串 */
function minorOf(version: string): string {
  const parts = version.split('.')
  return parts.length >= 2 ? `${parts[0]}.${parts[1]}` : version
}

function changeCount(note: ReleaseNote): number {
  return note.groups.reduce((sum, group) => sum + group.items.length, 0)
}

/** 反引号包裹段渲染为内联 code；反引号数为奇数（不成对）时整串按纯文本渲染 */
function renderInline(text: string): ReactNode {
  const parts = text.split('`')
  if (parts.length < 3 || parts.length % 2 === 0) return text
  return parts.map((part, i) =>
    i % 2 === 1 ? (
      <code
        key={i}
        className="rounded-[4px] border border-hairline bg-surface-3 px-1 font-mono text-[11.5px] text-ink"
      >
        {part}
      </code>
    ) : (
      part
    )
  )
}

interface IndexEntry {
  key: string
  label: string
  /** 跳转目标版本号（聚合档指向该 minor 下最新的一个版本） */
  target: string
  /** 右侧副文本：逐条档显示发布日期，聚合档显示该 minor 落在归档区的版本数 */
  meta: { kind: 'date'; value: string } | { kind: 'count'; value: number }
}

/**
 * 索引条目：前 RECENT_COUNT 个逐条列出，其余进归档档。
 * 归档档仅聚合「整个 minor 都落在归档区」的版本 —— 若某 minor 横跨两区，
 * 聚合出的「N 条」会被读成该 minor 只有 N 个版本，故这类版本仍逐条列出。
 */
function buildIndex(notes: ReleaseNote[]): { recent: IndexEntry[]; archive: IndexEntry[] } {
  const recentNotes = notes.slice(0, RECENT_COUNT)
  const archiveNotes = notes.slice(RECENT_COUNT)
  const recentMinors = new Set(recentNotes.map((note) => minorOf(note.version)))
  const minorCounts = new Map<string, number>()
  archiveNotes.forEach((note) => {
    const minor = minorOf(note.version)
    minorCounts.set(minor, (minorCounts.get(minor) ?? 0) + 1)
  })

  const seen = new Set<string>()
  const archive: IndexEntry[] = []
  archiveNotes.forEach((note) => {
    const minor = minorOf(note.version)
    if (recentMinors.has(minor)) {
      archive.push({
        key: note.version,
        label: `v${note.version}`,
        target: note.version,
        meta: { kind: 'date', value: note.date },
      })
      return
    }
    if (seen.has(minor)) return
    seen.add(minor)
    archive.push({
      key: `minor-${minor}`,
      label: `v${minor}.x`,
      target: note.version,
      meta: { kind: 'count', value: minorCounts.get(minor) ?? 1 },
    })
  })

  return {
    recent: recentNotes.map((note) => ({
      key: note.version,
      label: `v${note.version}`,
      target: note.version,
      meta: { kind: 'date' as const, value: note.date },
    })),
    archive,
  }
}

export function ChangelogPage() {
  const { t, i18n } = useTranslation()
  const { data, isLoading, isError, error, refetch } = useChangelog()
  const notes = data?.data ?? NO_NOTES
  const lang = i18n.language === 'en' ? 'en' : 'zh'
  const latest = notes.find((note) => note.is_latest) ?? notes[0]

  const { recent, archive } = useMemo(() => buildIndex(notes), [notes])
  const [activeVersion, setActiveVersion] = useState<string | null>(null)
  const mainRef = useRef<HTMLDivElement>(null)

  // 视口内最靠前的版本高亮索引；notes 按版本降序，nodes 顺序即展示顺序
  useEffect(() => {
    const root = mainRef.current
    if (!root) return
    const nodes = Array.from(root.querySelectorAll<HTMLElement>('[data-version]'))
    if (nodes.length === 0) return
    setActiveVersion(nodes[0].dataset.version ?? null)
    const visible = new Set<string>()
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          const version = (entry.target as HTMLElement).dataset.version
          if (!version) return
          if (entry.isIntersecting) visible.add(version)
          else visible.delete(version)
        })
        const first = nodes.find((node) => node.dataset.version && visible.has(node.dataset.version))
        if (first?.dataset.version) setActiveVersion(first.dataset.version)
      },
      { rootMargin: '-8% 0px -70% 0px' }
    )
    nodes.forEach((node) => observer.observe(node))
    return () => observer.disconnect()
  }, [notes])

  const jumpTo = (version: string) => {
    document.getElementById(`rel-${version}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  const renderLink = (entry: IndexEntry) => (
    <button
      key={entry.key}
      type="button"
      onClick={() => jumpTo(entry.target)}
      aria-label={t('changelog.jumpToVersion', { version: entry.label })}
      className={`${TL_LINK} ${activeVersion === entry.target ? TL_LINK_ON : ''}`}
    >
      <span className="truncate">{entry.label}</span>
      <span className="ml-auto flex-none text-[10px] text-ink-3">
        {entry.meta.kind === 'date'
          ? shortDate(entry.meta.value)
          : t('changelog.archiveCount', { count: entry.meta.value })}
      </span>
    </button>
  )

  return (
    <div>
      <PageHead
        crumb={[t('dashboard.navSystem'), t('changelog.pageTitle')]}
        title={t('changelog.pageTitle')}
        note={t('changelog.headNote')}
        actions={
          <>
            {latest && (
              <span className={TAG_CURRENT}>
                <span className="size-[5px] flex-none rounded-full bg-brand" aria-hidden="true" />
                {t('changelog.tagCurrent', { version: latest.version })}
              </span>
            )}
            <Button variant="outline" onClick={() => refetch()} disabled={isLoading}>
              <RotateCw className={isLoading ? 'animate-spin' : ''} />
              {t('common.refresh')}
            </Button>
          </>
        }
      />

      {isLoading ? (
        <div className={`${PANEL} px-4 py-8 text-center text-[12.5px] text-ink-3`}>{t('common.loading')}</div>
      ) : isError ? (
        <div className={`${PANEL} px-4 py-8 text-center text-[12.5px] text-danger`}>
          {t('changelog.loadFailed', { message: extractErrorMessage(error) })}
        </div>
      ) : notes.length === 0 ? (
        <div className={`${PANEL} px-4 py-8 text-center text-[12.5px] text-ink-3`}>
          {t('changelog.emptyNoChangelog')}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-0 pb-6 lg:grid-cols-[150px_1fr]">
          <nav className="hidden self-start pt-0.5 lg:sticky lg:top-7 lg:block">
            <div className={TL_IDX_LAB}>{t('changelog.idxLabelVersions')}</div>
            {recent.map(renderLink)}
            {archive.length > 0 && (
              <>
                <div className={`${TL_IDX_LAB} pt-[14px]`}>{t('changelog.idxLabelArchive')}</div>
                {archive.map(renderLink)}
              </>
            )}
          </nav>

          <div ref={mainRef} className="tl-main min-w-0">
            {notes.map((note) => (
              <article
                key={note.version}
                id={`rel-${note.version}`}
                data-version={note.version}
                className={`rel ${note.is_latest ? 'is-new' : ''}`}
              >
                <div className="mb-[3px] flex flex-wrap items-center gap-[9px]">
                  <span className="font-mono text-[15px] font-semibold tracking-[-.02em]">v{note.version}</span>
                  {note.is_latest && (
                    <span className={TAG_CURRENT}>
                      <span className="size-[5px] flex-none rounded-full bg-brand" aria-hidden="true" />
                      {t('changelog.tagCurrentVersion')}
                    </span>
                  )}
                  <span className="text-[11px] text-ink-3">
                    {note.date} · {t('changelog.changeCount', { count: changeCount(note) })}
                  </span>
                </div>

                {note.groups.length > 0 && (
                  <div className="mt-[9px] overflow-hidden rounded-[10px] border border-hairline bg-surface shadow-hair">
                    {note.groups.map((group, groupIdx) => {
                      const kind = kindOf(group)
                      const Icon = SECTION_ICON[kind]
                      return (
                        <div key={groupIdx} className="border-b border-hairline px-[14px] py-[11px] last:border-b-0">
                          <div
                            className={`mb-[7px] flex items-center gap-1.5 text-[10px] font-bold uppercase tracking-[.07em] ${SECTION_STYLE[kind]}`}
                          >
                            <Icon className="size-3" aria-hidden="true" />
                            {lang === 'zh' ? group.title_zh : group.title_en}
                          </div>
                          <ul>
                            {group.items.map((item, itemIdx) => (
                              <li
                                key={itemIdx}
                                className="rel-li flex gap-[9px] py-[2.5px] text-[12.5px] leading-[1.5] text-ink-2"
                              >
                                <span className="min-w-0">{renderInline(lang === 'zh' ? item.zh : item.en)}</span>
                              </li>
                            ))}
                          </ul>
                        </div>
                      )
                    })}
                  </div>
                )}
              </article>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
