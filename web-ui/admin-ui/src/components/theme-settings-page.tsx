// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 主题设置页（移植自 codex2api ThemeSettings.tsx + ThemePreview.tsx，uiStyle/CinematicThemeSwitcher 不移植）。
// 样式全部使用 Tailwind 原子类；预览色块用内联 style 消费 ThemePreviewSwatch 的实际色值。
import { useMemo, useState } from 'react'
import {
  Check,
  CheckCheck,
  Globe,
  LayoutDashboard,
  Moon,
  Palette,
  RotateCcw,
  Search,
  Settings2,
  SlidersHorizontal,
  Sparkles,
  Sun,
  X,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useTheme, type ThemeMode } from '@/hooks/use-theme'
import { readableText, secondaryText } from '@/lib/preview-contrast'
import {
  COLOR_THEMES,
  THEME_GROUP_ORDER,
  getThemePreviewSwatch,
  type ColorThemeDef,
  type ThemeGroup,
  type ThemePreviewSwatch,
} from '@/theme-definitions'
import { PageHead } from './page-head'

const MODE_OPTIONS: { id: ThemeMode; icon: typeof Sun; labelKey: string; descKey: string }[] = [
  { id: 'light', icon: Sun, labelKey: 'themeSettings.modeLight', descKey: 'themeSettings.modeLightDesc' },
  { id: 'dark', icon: Moon, labelKey: 'themeSettings.modeDark', descKey: 'themeSettings.modeDarkDesc' },
]

const GROUP_FILTERS: { id: 'all' | ThemeGroup; labelKey: string; icon?: typeof Sparkles }[] = [
  { id: 'all', labelKey: 'themeSettings.filterAll' },
  { id: 'recommended', labelKey: 'themeSettings.filterRecommended', icon: Sparkles },
  { id: 'website', labelKey: 'themeSettings.filterWebsite', icon: Globe },
  { id: 'light', labelKey: 'themeSettings.filterLight' },
  { id: 'dark', labelKey: 'themeSettings.filterDark' },
  { id: 'editor', labelKey: 'themeSettings.filterEditor' },
]

function matchesGroup(item: ColorThemeDef, group: 'all' | ThemeGroup) {
  return group === 'all' || (group === 'recommended' ? !!item.recommended : item.group === group)
}

/* ---------- 缩略图（移植自 codex2api ThemePreview.tsx，色值直接内联） ---------- */

const BARS = [28, 43, 35, 55, 42, 66, 52, 70, 57, 76, 65, 90, 73, 86, 68, 96, 82, 92]

function AppearanceThumbnail({ swatch }: { swatch: ThemePreviewSwatch }) {
  return (
    <div
      aria-hidden
      className="flex h-14 overflow-hidden rounded-md"
      style={{ background: swatch.bg }}
    >
      <div className="flex w-6 flex-col gap-1 p-1.5" style={{ background: swatch.sidebar ?? swatch.surface }}>
        {[0, 1, 2].map((i) => (
          <span key={i} className="block h-1 rounded-full" style={{ background: i === 0 ? swatch.primary : swatch.muted }} />
        ))}
      </div>
      <div className="flex-1 p-2">
        <span className="mb-1.5 block h-1.5 w-3/4 rounded-full" style={{ background: swatch.primary }} />
        <span className="block h-1 w-1/2 rounded-full" style={{ background: swatch.muted }} />
      </div>
    </div>
  )
}

function ThemeThumbnail({ swatch }: { swatch: ThemePreviewSwatch }) {
  return (
    <div
      aria-hidden
      className="flex h-24 overflow-hidden rounded-md text-left"
      style={{ background: swatch.bg }}
    >
      <div className="flex w-12 shrink-0 flex-col gap-1.5 p-2" style={{ background: swatch.sidebar ?? swatch.surface }}>
        <span className="block h-2 w-6 rounded-sm" style={{ background: swatch.primary }} />
        {[0, 1, 2].map((i) => (
          <span key={i} className="block h-1 rounded-full" style={{ background: i === 0 ? swatch.primary : swatch.muted, opacity: i === 0 ? 1 : 0.55 }} />
        ))}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-1.5 p-2">
        <div className="space-y-1">
          <span className="block h-1.5 w-2/3 rounded-full" style={{ background: swatch.primary }} />
          <span className="block h-1 w-1/3 rounded-full" style={{ background: swatch.muted, opacity: 0.7 }} />
        </div>
        <div className="grid grid-cols-3 gap-1">
          {[0, 1, 2].map((i) => (
            <span key={i} className="block h-3 rounded-sm" style={{ background: swatch.surface }} />
          ))}
        </div>
        <div className="flex h-8 items-end gap-[2px]">
          {BARS.filter((_, i) => i % 2 === 0).map((h, i) => (
            <span key={i} className="flex-1 rounded-t-[1px]" style={{ height: `${h}%`, background: swatch.primary, opacity: 0.75 }} />
          ))}
        </div>
      </div>
    </div>
  )
}

/* ---------- 主题卡片 ---------- */

function ThemeStyleCard({
  item,
  swatch,
  active,
  onSelect,
}: {
  item: ColorThemeDef
  swatch: ThemePreviewSwatch
  active: boolean
  onSelect: () => void
}) {
  const { t } = useTranslation()
  const name = t(`common.theme.${item.nameKey}`)

  return (
    <button
      type="button"
      onClick={onSelect}
      aria-pressed={active}
      aria-label={t('themeSettings.selectTheme', { theme: name })}
      aria-describedby={`theme-description-${item.id}`}
      className={`relative rounded-lg border p-2 text-left transition-colors ${
        active ? 'border-brand bg-brand-soft' : 'border-transparent bg-surface hover:bg-surface-2'
      }`}
    >
      {item.recommended && (
        <span className="absolute right-2 top-2 z-10 inline-flex items-center gap-0.5 rounded-full bg-brand px-1.5 py-0.5 text-[9px] font-semibold text-brand-fg">
          <Sparkles className="h-2.5 w-2.5" />
          {t('themeSettings.recommendedBadge')}
        </span>
      )}
      {active && (
        <span className="absolute right-2 bottom-2 z-10 grid h-5 w-5 place-items-center rounded-full bg-brand text-brand-fg">
          <Check className="h-3 w-3" />
        </span>
      )}
      <ThemeThumbnail swatch={swatch} />
      <div className="mt-2 flex items-center gap-1.5">
        <span className="text-[12.5px] font-medium text-ink">{name}</span>
        <span className="flex gap-0.5">
          {[swatch.primary, swatch.bg, swatch.muted].map((c, i) => (
            <span key={i} className="h-2 w-2 rounded-full border border-transparent" style={{ background: c }} />
          ))}
        </span>
        {active && <span className="ml-auto text-[10px] text-brand">{t('themeSettings.applied')}</span>}
      </div>
      <p id={`theme-description-${item.id}`} className="mt-1 line-clamp-2 text-[10.5px] leading-snug text-ink-2">
        {t(`themeSettings.themeDesc.${item.descriptionKey}`)}
      </p>
      <p className="mt-1 text-[9.5px] text-ink-3">
        {t(`themeSettings.group.${item.group}`)}
        <span className="sr-only"> · {t('themeSettings.bothModes')}</span>
      </p>
    </button>
  )
}

/* ---------- 实时预览（工作台 / 基础组件双场景） ---------- */

type PreviewScene = 'workbench' | 'components'

function ThemeLivePreview({ item, mode }: { item: ColorThemeDef; mode: ThemeMode }) {
  const { t } = useTranslation()
  const [scene, setScene] = useState<PreviewScene>('workbench')
  const swatch = getThemePreviewSwatch(item, mode)
  const sidebarBg = swatch.sidebar ?? swatch.surface
  const primaryOnBg = readableText(swatch.primary, swatch.bg)
  const primaryOnSurface = readableText(swatch.primary, swatch.surface)
  const primaryOnSidebar = readableText(swatch.primary, sidebarBg)
  const primaryOnMuted = readableText(swatch.primary, swatch.muted)
  const onPrimary = readableText(swatch.bg, swatch.primary)
  const mutedOnBg = secondaryText(swatch.bg)
  const mutedOnSurface = secondaryText(swatch.surface)
  const mutedOnSidebar = secondaryText(sidebarBg)

  const scenes: { id: PreviewScene; label: string; icon: typeof LayoutDashboard }[] = [
    { id: 'workbench', label: t('themeSettings.sceneWorkbench'), icon: LayoutDashboard },
    { id: 'components', label: t('themeSettings.sceneComponents'), icon: Settings2 },
  ]

  return (
    <div className="rounded-xl border border-transparent bg-surface-2 p-3">
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div>
          <p className="text-[12px] font-medium text-ink">{t('themeSettings.previewTitle')}</p>
          <p className="text-[10.5px] text-ink-3">{t('themeSettings.previewDesc')}</p>
        </div>
        <div role="tablist" aria-label={t('themeSettings.previewScene')} className="flex gap-1 rounded-md border border-transparent bg-surface p-0.5">
          {scenes.map((s) => (
            <button
              key={s.id}
              type="button"
              role="tab"
              aria-selected={scene === s.id}
              onClick={() => setScene(s.id)}
              className={`inline-flex items-center gap-1 rounded-[5px] px-2 py-1 text-[11px] transition-colors ${
                scene === s.id ? 'bg-brand-soft text-brand' : 'text-ink-2 hover:bg-surface-3'
              }`}
            >
              <s.icon className="h-3 w-3" />
              {s.label}
            </button>
          ))}
        </div>
      </div>

      {/* 窗口 chrome */}
      <div
        role="tabpanel"
        aria-label={t('themeSettings.previewAccessible', {
          scene: scenes.find((s) => s.id === scene)?.label,
          theme: t(`common.theme.${item.nameKey}`),
          mode: mode === 'dark' ? t('settings.themeDark') : t('settings.themeLight'),
        })}
        className="overflow-hidden rounded-lg border border-transparent"
        style={{ background: swatch.bg }}
      >
        <div className="flex items-center gap-1.5 border-b px-3 py-2" style={{ background: swatch.surface, borderColor: swatch.muted }}>
          <span className="flex gap-1">
            {['#EE6A5F', '#F5BD4F', '#61C454'].map((c) => (
              <span key={c} className="h-2.5 w-2.5 rounded-full" style={{ background: c }} />
            ))}
          </span>
          <span className="ml-1.5 text-[10px] font-medium" style={{ color: primaryOnSurface }}>
            kiro2cc
          </span>
          <span className="text-[10px]" style={{ color: mutedOnSurface }}>
            · {scenes.find((s) => s.id === scene)?.label} · {t('themeSettings.demoLabel')}
          </span>
        </div>

        {scene === 'workbench' ? (
          <div className="flex min-h-[168px]">
            <div className="w-28 shrink-0 space-y-1.5 border-r p-2.5" style={{ background: sidebarBg, borderColor: swatch.muted }}>
              <p className="text-[10px] font-semibold" style={{ color: primaryOnSidebar }}>
                kiro2cc
              </p>
              {['overview', 'accounts', 'keys', 'usage'].map((k, i) => (
                <p
                  key={k}
                  className="rounded px-1.5 py-0.5 text-[9.5px]"
                  style={{
                    background: i === 0 ? swatch.primary : 'transparent',
                    color: i === 0 ? onPrimary : mutedOnSidebar,
                  }}
                >
                  {t(`themeSettings.demo.${k}`)}
                </p>
              ))}
            </div>
            <div className="flex-1 space-y-2 p-3">
              <div className="grid grid-cols-3 gap-2">
                <div className="rounded-md border p-2" style={{ background: swatch.surface, borderColor: swatch.muted }}>
                  <p className="text-[8.5px]" style={{ color: mutedOnSurface }}>{t('themeSettings.demo.requests')}</p>
                  <p className="text-[12px] font-semibold" style={{ color: primaryOnSurface }}>24,680</p>
                </div>
                <div className="rounded-md border p-2" style={{ background: swatch.surface, borderColor: swatch.muted }}>
                  <p className="text-[8.5px]" style={{ color: mutedOnSurface }}>{t('themeSettings.demo.successRate')}</p>
                  <p className="text-[12px] font-semibold" style={{ color: primaryOnSurface }}>99.9%</p>
                </div>
                <div className="rounded-md border p-2" style={{ background: swatch.surface, borderColor: swatch.muted }}>
                  <p className="text-[8.5px]" style={{ color: mutedOnSurface }}>{t('themeSettings.demo.latency')}</p>
                  <p className="text-[12px] font-semibold" style={{ color: primaryOnSurface }}>128ms</p>
                </div>
              </div>
              <div className="rounded-md border p-2" style={{ background: swatch.surface, borderColor: swatch.muted }}>
                <div className="mb-1 flex items-center justify-between">
                  <p className="text-[9px]" style={{ color: mutedOnSurface }}>{t('themeSettings.demo.traffic')} · {t('themeSettings.demo.week')}</p>
                  <p className="text-[8.5px]" style={{ color: mutedOnSurface }}>00:00 – 24:00</p>
                </div>
                <div className="flex h-12 items-end gap-[2px]">
                  {BARS.map((h, i) => (
                    <span key={i} className="flex-1 rounded-t-[1px]" style={{ height: `${h}%`, background: swatch.primary, opacity: 0.75 }} />
                  ))}
                </div>
              </div>
              <p className="text-[9px]" style={{ color: mutedOnBg }}>
                POST /v1/messages · 200 OK
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-2.5 p-3">
            <p className="text-[10px] font-medium" style={{ color: primaryOnBg }}>{t('themeSettings.demo.componentsTitle')}</p>
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="rounded-md px-2 py-1 text-[9px] font-medium" style={{ background: swatch.primary, color: onPrimary }}>
                {t('themeSettings.demo.primary')}
              </span>
              <span className="rounded-md border px-2 py-1 text-[9px]" style={{ borderColor: swatch.muted, color: primaryOnBg }}>
                {t('themeSettings.demo.secondary')}
              </span>
            </div>
            <div className="flex flex-wrap gap-1.5">
              <span className="rounded-md border px-2 py-1 text-[9px]" style={{ background: swatch.surface, borderColor: swatch.muted, color: mutedOnSurface }}>
                {t('themeSettings.demo.input')}
              </span>
              <span className="rounded-md border px-2 py-1 text-[9px]" style={{ background: swatch.surface, borderColor: swatch.muted, color: mutedOnSurface }}>
                {t('themeSettings.demo.selection')}
              </span>
            </div>
            <div className="flex flex-wrap gap-1.5">
              <span className="rounded-full px-1.5 py-0.5 text-[8.5px]" style={{ background: swatch.primary, color: onPrimary }}>
                {t('themeSettings.demo.enabled')}
              </span>
              <span className="rounded-full px-1.5 py-0.5 text-[8.5px]" style={{ background: swatch.muted, color: primaryOnMuted }}>
                {t('themeSettings.demo.pending')}
              </span>
              <span className="rounded-full border px-1.5 py-0.5 text-[8.5px]" style={{ borderColor: swatch.primary, color: primaryOnBg }}>
                {t('themeSettings.demo.error')}
              </span>
            </div>
            <div className="flex items-center gap-2">
              <div className="h-1.5 flex-1 overflow-hidden rounded-full" style={{ background: swatch.muted }}>
                <div className="h-full rounded-full" style={{ width: '68%', background: swatch.primary }} />
              </div>
              <span className="text-[8.5px]" style={{ color: mutedOnBg }}>68%</span>
            </div>
            <p className="rounded-md px-2 py-1.5 text-[9px]" style={{ background: swatch.muted, color: primaryOnMuted }}>
              {t('themeSettings.demo.notice')}
            </p>
          </div>
        )}
      </div>

      {/* 色板 token 行 */}
      <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1" aria-label={t('themeSettings.paletteLabel')}>
        {([
          ['primary', swatch.primary],
          ['background', swatch.bg],
          ['surface', swatch.surface],
          ['muted', swatch.muted],
        ] as const).map(([token, color]) => (
          <span key={token} className="inline-flex items-center gap-1 text-[9.5px] text-ink-3">
            <span className="h-2.5 w-2.5 rounded-full border border-transparent" style={{ background: color }} />
            {t(`themeSettings.token.${token}`)}
          </span>
        ))}
      </div>
    </div>
  )
}

/* ---------- 主页面 ---------- */

export function ThemeSettingsPage() {
  const { t } = useTranslation()
  const { themeMode, colorTheme, theme, setThemeMode, setColorTheme, resetTheme } = useTheme()
  const [search, setSearch] = useState('')
  const [group, setGroup] = useState<'all' | ThemeGroup>('all')

  const activeItem = useMemo(
    () => COLOR_THEMES.find((item) => item.id === colorTheme) ?? COLOR_THEMES[0],
    [colorTheme],
  )

  const groupCounts = useMemo(() => {
    const counts = new Map<'all' | ThemeGroup, number>()
    counts.set('all', COLOR_THEMES.length)
    for (const g of THEME_GROUP_ORDER) {
      counts.set(g, COLOR_THEMES.filter((item) => matchesGroup(item, g)).length)
    }
    return counts
  }, [])

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    return COLOR_THEMES.filter((item) => {
      if (!matchesGroup(item, group)) return false
      if (!q) return true
      return [
        item.id,
        t(`common.theme.${item.nameKey}`),
        t(`themeSettings.themeDesc.${item.descriptionKey}`),
        t(`themeSettings.group.${item.group}`),
      ]
        .join(' ')
        .toLowerCase()
        .includes(q)
    })
  }, [search, group, t])

  const clearFilters = () => {
    setSearch('')
    setGroup('all')
  }

  return (
    <>
      <PageHead
        crumb={[t('dashboard.navSystem'), t('dashboard.navTheme')]}
        title={t('themeSettings.title')}
        note={t('themeSettings.description')}
        actions={
          <>
            <span className="inline-flex items-center gap-1 text-[11px] text-ink-3">
              <CheckCheck className="h-3.5 w-3.5" />
              {t('themeSettings.autoSave')}
            </span>
            <button
              type="button"
              onClick={() => {
                resetTheme()
                clearFilters()
              }}
              title={t('themeSettings.resetDesc')}
              className="inline-flex items-center gap-1 rounded-md border border-transparent bg-surface px-2.5 py-1.5 text-[11.5px] text-ink-2 transition-colors hover:text-ink"
            >
              <RotateCcw className="h-3.5 w-3.5" />
              {t('themeSettings.reset')}
            </button>
          </>
        }
      />

      <div className="space-y-6">
        {/* 外观模式三卡 */}
        <section aria-labelledby="appearance-modes-heading" className="rounded-xl border border-transparent bg-surface p-4">
          <h2 id="appearance-modes-heading" className="text-[13px] font-semibold text-ink">
            {t('themeSettings.modeTitle')}
          </h2>
          <p className="mt-0.5 text-[11px] text-ink-3">{t('themeSettings.modeDesc')}</p>
          <div className="mt-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
            {MODE_OPTIONS.map((option) => {
              const active = themeMode === option.id
              return (
                <button
                  key={option.id}
                  type="button"
                  aria-pressed={active}
                  onClick={(e) => setThemeMode(option.id, e.clientX, e.clientY)}
                  className={`rounded-lg border p-3 text-left transition-colors ${
                    active ? 'border-brand bg-brand-soft' : 'border-transparent bg-surface hover:bg-surface-2'
                  }`}
                >
                  <AppearanceThumbnail swatch={getThemePreviewSwatch(activeItem, option.id)} />
                  <div className="mt-2 flex items-center gap-1.5">
                    <option.icon className="h-3.5 w-3.5 text-ink-2" />
                    <span className="text-[12.5px] font-medium text-ink">{t(option.labelKey)}</span>
                    {active && <Check className="ml-auto h-3.5 w-3.5 text-brand" />}
                  </div>
                  <p className="mt-0.5 text-[10.5px] leading-snug text-ink-2">{t(option.descKey)}</p>
                </button>
              )
            })}
          </div>
          <p className="mt-2.5 inline-flex items-center gap-1 text-[10.5px] text-ink-3">
            {t('themeSettings.localPreference')}
          </p>
        </section>

        {/* 当前主题 + 实时预览 */}
        <section aria-label={t('themeSettings.currentTheme', { theme: t(`common.theme.${activeItem.nameKey}`) })} className="rounded-xl border border-transparent bg-surface p-4">
          <div className="mb-3 flex items-center gap-2.5">
            <span className="grid h-8 w-8 shrink-0 place-items-center rounded-md bg-brand-soft text-brand">
              <Palette className="h-4 w-4" />
            </span>
            <div>
              <p className="text-[10px] font-medium uppercase tracking-[.08em] text-ink-3">{t('themeSettings.currentAppearance')}</p>
              <p className="text-[13px] font-semibold text-ink">
                {t('themeSettings.currentTheme', { theme: t(`common.theme.${activeItem.nameKey}`) })}
                <span className="ml-1.5 font-normal text-ink-3">
                  {t('themeSettings.themeDesc.' + activeItem.descriptionKey)}
                </span>
              </p>
            </div>
          </div>
          <ThemeLivePreview item={activeItem} mode={themeMode} />
        </section>

        {/* 主题库：搜索 + 分组筛选 + 网格 */}
        <section aria-labelledby="styles-heading" className="rounded-xl border border-transparent bg-surface p-4">
          <div className="flex flex-wrap items-center gap-2">
            <h2 id="styles-heading" className="text-[13px] font-semibold text-ink">
              {t('themeSettings.stylesTitle')}
            </h2>
            <span className="rounded-full bg-surface-3 px-1.5 py-0.5 text-[10px] font-medium text-ink-2">{COLOR_THEMES.length}</span>
            <p className="min-w-0 flex-1 truncate text-[11px] text-ink-3">{t('themeSettings.stylesDesc')}</p>
          </div>

          <div className="mt-3 flex flex-wrap items-center gap-2">
            <div className="relative min-w-[220px] flex-1">
              <Search aria-hidden className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-ink-3" />
              <input
                type="search"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder={t('themeSettings.searchPlaceholder')}
                aria-label={t('themeSettings.searchLabel')}
                className="w-full rounded-md border-[0.5px] border-hairline bg-surface py-1.5 pl-8 pr-8 text-[12px] text-ink placeholder:text-ink-3 focus:border-brand focus:outline-none"
              />
              {search && (
                <button
                  type="button"
                  onClick={() => setSearch('')}
                  aria-label={t('themeSettings.clearSearch')}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-ink-3 hover:text-ink"
                >
                  <X className="h-3.5 w-3.5" />
                </button>
              )}
            </div>
          </div>

          <div role="group" aria-label={t('themeSettings.filterLabel')} className="mt-2 flex flex-wrap gap-1.5">
            {GROUP_FILTERS.map((f) => {
              const active = group === f.id
              return (
                <button
                  key={f.id}
                  type="button"
                  aria-pressed={active}
                  onClick={() => setGroup(f.id)}
                  className={`inline-flex items-center gap-1 rounded-full border px-2.5 py-1 text-[11px] transition-colors ${
                    active ? 'border-brand bg-brand text-brand-fg' : 'border-transparent bg-surface text-ink-2'
                  }`}
                >
                  {f.icon && <f.icon className="h-3 w-3" />}
                  {t(f.labelKey)}
                  <span className={active ? 'opacity-75' : 'text-ink-3'}>{groupCounts.get(f.id)}</span>
                </button>
              )
            })}
          </div>

          <p role="status" className="mt-2.5 text-[10.5px] text-ink-3">
            {t('themeSettings.resultsCount', { count: filtered.length })}
          </p>

          {filtered.length === 0 ? (
            <div className="py-10 text-center">
              <Palette className="mx-auto h-8 w-8 text-ink-3" />
              <p className="mt-2 text-[12.5px] font-medium text-ink">{t('themeSettings.emptyFilter')}</p>
              <p className="mt-1 text-[11px] text-ink-3">{t('themeSettings.emptyHint')}</p>
              <button
                type="button"
                onClick={clearFilters}
                className="mt-3 rounded-md border border-transparent px-3 py-1.5 text-[11.5px] text-ink-2 hover:text-ink"
              >
                {t('themeSettings.clearFilters')}
              </button>
            </div>
          ) : (
            <div className="mt-3 grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
              {filtered.map((item) => (
                <ThemeStyleCard
                  key={item.id}
                  item={item}
                  swatch={getThemePreviewSwatch(item, theme)}
                  active={item.id === colorTheme}
                  onSelect={() => setColorTheme(item.id)}
                />
              ))}
            </div>
          )}

          <p className="mt-3 inline-flex items-center gap-1 text-[10.5px] text-ink-3">
            <SlidersHorizontal className="h-3 w-3" />
            {t('themeSettings.libraryHint')}
          </p>
        </section>

        <span className="sr-only">
          {t('themeSettings.currentThemeFull', {
            theme: t(`common.theme.${activeItem.nameKey}`),
            mode: theme === 'dark' ? t('settings.themeDark') : t('settings.themeLight'),
          })}
        </span>
      </div>
    </>
  )
}
