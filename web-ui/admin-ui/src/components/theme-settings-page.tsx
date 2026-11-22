// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 主题设置页（移植自 codex2api ThemeSettings.tsx + ThemePreview.tsx，uiStyle/CinematicThemeSwitcher 不移植）。
// 样式全部使用 Tailwind 原子类；预览色块用内联 style 消费 ThemePreviewSwatch 的实际色值。
import { useMemo, useState, type CSSProperties } from 'react'
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

/* ---------- 缩略图与实时预览渐变辅助（色值来自 ThemePreviewSwatch） ---------- */

function hexToHsl(hex: string): [number, number, number] | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim())
  if (!m) return null
  const raw = m[1].length === 3 ? m[1].replace(/./g, (c) => c + c) : m[1]
  const n = parseInt(raw, 16)
  const r = ((n >> 16) & 255) / 255
  const g = ((n >> 8) & 255) / 255
  const b = (n & 255) / 255
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const l = (max + min) / 2
  if (max === min) return [0, 0, l * 100]
  const d = max - min
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min)
  let h = 0
  if (max === r) h = (g - b) / d + (g < b ? 6 : 0)
  else if (max === g) h = (b - r) / d + 2
  else h = (r - g) / d + 4
  return [(h / 6) * 360, s * 100, l * 100]
}

interface PreviewVisuals {
  windowStyle: CSSProperties
  chromeStyle: CSSProperties
  sidebarStyle: CSSProperties
  cardStyle: CSSProperties
  pillGradient: string
}

function buildPreviewVisuals(
  swatch: ThemePreviewSwatch,
  mode: ThemeMode,
  canvasGradient: boolean,
): PreviewVisuals {
  const sidebarBg = swatch.sidebar ?? swatch.surface
  if (!canvasGradient) {
    return {
      windowStyle: { background: swatch.bg },
      chromeStyle: { background: swatch.surface, borderColor: swatch.muted },
      sidebarStyle: { background: sidebarBg, borderColor: swatch.muted },
      cardStyle: { background: swatch.surface, borderColor: swatch.muted },
      pillGradient: swatch.primary,
    }
  }
  const isDark = mode === 'dark'
  const hsl = hexToHsl(swatch.primary) ?? [174, 60, isDark ? 50 : 35]
  const [h, s] = hsl
  const sat = s < 3 ? s : isDark ? Math.min(s * 1.15, 92) : Math.min(s * 0.78, 72)
  const glowL = isDark ? 50 : 88
  const g1 = `hsla(${Math.round(h)}, ${Math.round(sat)}%, ${glowL}%, ${isDark ? 0.34 : 0.72})`
  const g2 = `hsla(${Math.round((h + (isDark ? 58 : 44)) % 360)}, ${Math.round(sat * 0.92)}%, ${glowL}%, ${isDark ? 0.32 : 0.66})`
  const g3 = `hsla(${Math.round((h + (isDark ? 318 : 328)) % 360)}, ${Math.round(sat * 0.84)}%, ${isDark ? 45 : 90}%, ${isDark ? 0.26 : 0.56})`
  const edge = isDark ? 'rgba(255,255,255,0.22)' : 'rgba(255,255,255,0.88)'
  const rimBorder = isDark
    ? `color-mix(in srgb, ${swatch.primary} 32%, rgba(255,255,255,0.16))`
    : `color-mix(in srgb, ${swatch.primary} 24%, ${swatch.muted})`
  return {
    windowStyle: {
      backgroundColor: swatch.bg,
      backgroundImage: [
        `radial-gradient(75% 65% at ${isDark ? '22% 8%' : '10% 6%'}, ${g1} 0%, transparent 74%)`,
        `radial-gradient(68% 60% at 88% 12%, ${g2} 0%, transparent 72%)`,
        `radial-gradient(72% 64% at 54% 96%, ${g3} 0%, transparent 76%)`,
      ].join(', '),
    },
    chromeStyle: {
      backgroundColor: 'transparent',
      borderColor: rimBorder,
    },
    sidebarStyle: {
      backgroundColor: 'transparent',
      borderColor: `color-mix(in srgb, ${swatch.primary} 20%, ${swatch.muted})`,
    },
    cardStyle: isDark
      ? {
          backgroundColor: `color-mix(in srgb, ${swatch.surface} 42%, transparent)`,
          backgroundImage: 'linear-gradient(155deg, rgba(255,255,255,0.055) 0%, rgba(255,255,255,0.014) 32%, transparent 65%)',
          borderColor: rimBorder,
          boxShadow: `inset 0 1px 0 ${edge}, inset 0 0 0 0.5px rgba(255,255,255,0.065), 0 6px 14px -6px rgba(0,0,0,0.65)`,
        }
      : {
          backgroundColor: 'transparent',
          borderColor: rimBorder,
          boxShadow: `inset 0 1px 0 ${edge}, 0 4px 12px -6px rgba(16,18,20,0.08)`,
        },
    pillGradient: `linear-gradient(135deg, ${swatch.primary}, color-mix(in srgb, ${swatch.primary} 78%, ${isDark ? '#ffffff' : swatch.surface}))`,
  }
}

const BARS = [28, 43, 35, 55, 42, 66, 52, 70, 57, 76, 65, 90, 73, 86, 68, 96, 82, 92]

function AppearanceThumbnail({
  swatch,
  mode,
  canvasGradient,
}: {
  swatch: ThemePreviewSwatch
  mode: ThemeMode
  canvasGradient: boolean
}) {
  const visuals = buildPreviewVisuals(swatch, mode, canvasGradient)
  return (
    <div
      aria-hidden
      className="flex h-14 overflow-hidden rounded-md"
      style={visuals.windowStyle}
    >
      <div className={`flex w-6 flex-col gap-1 p-1.5 ${canvasGradient ? 'border-r-[0.5px]' : ''}`} style={visuals.sidebarStyle}>
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="block h-1 rounded-full"
            style={{ background: i === 0 ? visuals.pillGradient : swatch.muted }}
          />
        ))}
      </div>
      <div className="flex-1 p-2">
        <span className="mb-1.5 block h-1.5 w-3/4 rounded-full" style={{ background: visuals.pillGradient }} />
        <span className="block h-1 w-1/2 rounded-full" style={{ background: swatch.muted }} />
      </div>
    </div>
  )
}

function ThemeThumbnail({
  swatch,
  mode,
  canvasGradient,
}: {
  swatch: ThemePreviewSwatch
  mode: ThemeMode
  canvasGradient: boolean
}) {
  const visuals = buildPreviewVisuals(swatch, mode, canvasGradient)
  return (
    <div
      aria-hidden
      className="flex h-24 overflow-hidden rounded-md text-left"
      style={visuals.windowStyle}
    >
      <div className={`flex w-12 shrink-0 flex-col gap-1.5 p-2 ${canvasGradient ? 'border-r-[0.5px]' : ''}`} style={visuals.sidebarStyle}>
        <span className="block h-2 w-6 rounded-sm" style={{ background: visuals.pillGradient }} />
        {[0, 1, 2].map((i) => (
          <span
            key={i}
            className="block h-1 rounded-full"
            style={{ background: i === 0 ? visuals.pillGradient : swatch.muted, opacity: i === 0 ? 1 : 0.55 }}
          />
        ))}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-1.5 p-2">
        <div className="space-y-1">
          <span className="block h-1.5 w-2/3 rounded-full" style={{ background: visuals.pillGradient }} />
          <span className="block h-1 w-1/3 rounded-full" style={{ background: swatch.muted, opacity: 0.7 }} />
        </div>
        <div className="grid grid-cols-3 gap-1">
          {[0, 1, 2].map((i) => (
            <span key={i} className={`block h-3 rounded-sm ${canvasGradient ? 'border-[0.5px]' : ''}`} style={visuals.cardStyle} />
          ))}
        </div>
        <div className="flex h-8 items-end gap-[2px]">
          {BARS.filter((_, i) => i % 2 === 0).map((h, i) => (
            <span key={i} className="flex-1 rounded-t-[1px]" style={{ height: `${h}%`, background: visuals.pillGradient, opacity: 0.78 }} />
          ))}
        </div>
      </div>
    </div>
  )
}

/* ---------- 主题卡片 ---------- */

/** 选择卡片未选中态：透明底，悬停时透出半透明 surface-2（主题卡片与外观模式卡片共用） */
const CHOICE_CARD_IDLE =
  'border-hairline/70 bg-transparent hover:border-hairline-2 hover:bg-[color-mix(in_srgb,var(--surface-2)_65%,transparent)] hover:shadow-hair'

function ThemeStyleCard({
  item,
  swatch,
  mode,
  canvasGradient,
  active,
  onSelect,
}: {
  item: ColorThemeDef
  swatch: ThemePreviewSwatch
  mode: ThemeMode
  canvasGradient: boolean
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
      className={`relative rounded-[11px] border p-2.5 text-left transition-all duration-150 ${
        active
          ? 'border-brand bg-brand-soft shadow-[0_0_0_1px_var(--brand)]'
          : CHOICE_CARD_IDLE
      }`}
    >
      {item.recommended && (
        <span className="absolute right-2 top-2 z-10 inline-flex items-center gap-0.5 rounded-full bg-brand btn-brand-gradient px-1.5 py-0.5 text-[9px] font-semibold text-brand-fg">
          <Sparkles className="h-2.5 w-2.5" />
          {t('themeSettings.recommendedBadge')}
        </span>
      )}
      {active && (
        <span className="absolute right-2 bottom-2 z-10 grid h-5 w-5 place-items-center rounded-full bg-brand btn-brand-gradient text-brand-fg">
          <Check className="h-3 w-3" />
        </span>
      )}
      <ThemeThumbnail swatch={swatch} mode={mode} canvasGradient={canvasGradient} />
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

function ThemeLivePreview({
  item,
  mode,
  canvasGradient,
}: {
  item: ColorThemeDef
  mode: ThemeMode
  canvasGradient: boolean
}) {
  const { t } = useTranslation()
  const [scene, setScene] = useState<PreviewScene>('workbench')
  const swatch = getThemePreviewSwatch(item, mode)
  const sidebarBg = swatch.sidebar ?? swatch.surface
  const visuals = buildPreviewVisuals(swatch, mode, canvasGradient)
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
    <div
      className={`rounded-xl border p-3 ${
        canvasGradient
          ? 'border-hairline/70 bg-transparent'
          : 'border-transparent bg-[color-mix(in_srgb,var(--surface-2)_50%,transparent)]'
      }`}
    >
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <div>
          <p className="text-[12px] font-medium text-ink">{t('themeSettings.previewTitle')}</p>
          <p className="text-[10.5px] text-ink-3">{t('themeSettings.previewDesc')}</p>
        </div>
        <div role="tablist" aria-label={t('themeSettings.previewScene')} className="flex gap-1 rounded-md border border-transparent bg-surface bg-control-glass p-0.5">
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
        className="overflow-hidden rounded-lg border border-transparent shadow-hair"
        style={visuals.windowStyle}
      >
        <div className="flex items-center gap-1.5 border-b px-3 py-2" style={visuals.chromeStyle}>
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
            <div className="w-28 shrink-0 space-y-1.5 border-r p-2.5" style={visuals.sidebarStyle}>
              <p className="text-[10px] font-semibold" style={{ color: primaryOnSidebar }}>
                kiro2cc
              </p>
              {['overview', 'accounts', 'keys', 'usage'].map((k, i) => (
                <p
                  key={k}
                  className="rounded px-1.5 py-0.5 text-[9.5px]"
                  style={{
                    background: i === 0 ? visuals.pillGradient : 'transparent',
                    color: i === 0 ? onPrimary : mutedOnSidebar,
                  }}
                >
                  {t(`themeSettings.demo.${k}`)}
                </p>
              ))}
            </div>
            <div className="flex-1 space-y-2 p-3">
              <div className="grid grid-cols-3 gap-2">
                <div className="rounded-md border p-2" style={visuals.cardStyle}>
                  <p className="text-[8.5px]" style={{ color: mutedOnSurface }}>{t('themeSettings.demo.requests')}</p>
                  <p className="text-[12px] font-semibold" style={{ color: primaryOnSurface }}>24,680</p>
                </div>
                <div className="rounded-md border p-2" style={visuals.cardStyle}>
                  <p className="text-[8.5px]" style={{ color: mutedOnSurface }}>{t('themeSettings.demo.successRate')}</p>
                  <p className="text-[12px] font-semibold" style={{ color: primaryOnSurface }}>99.9%</p>
                </div>
                <div className="rounded-md border p-2" style={visuals.cardStyle}>
                  <p className="text-[8.5px]" style={{ color: mutedOnSurface }}>{t('themeSettings.demo.latency')}</p>
                  <p className="text-[12px] font-semibold" style={{ color: primaryOnSurface }}>128ms</p>
                </div>
              </div>
              <div className="rounded-md border p-2" style={visuals.cardStyle}>
                <div className="mb-1 flex items-center justify-between">
                  <p className="text-[9px]" style={{ color: mutedOnSurface }}>{t('themeSettings.demo.traffic')} · {t('themeSettings.demo.week')}</p>
                  <p className="text-[8.5px]" style={{ color: mutedOnSurface }}>00:00 – 24:00</p>
                </div>
                <div className="flex h-12 items-end gap-[2px]">
                  {BARS.map((h, i) => (
                    <span key={i} className="flex-1 rounded-t-[1px]" style={{ height: `${h}%`, background: visuals.pillGradient, opacity: 0.78 }} />
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
              <span className="rounded-md px-2 py-1 text-[9px] font-medium" style={{ background: visuals.pillGradient, color: onPrimary }}>
                {t('themeSettings.demo.primary')}
              </span>
              <span className="rounded-md border px-2 py-1 text-[9px]" style={{ ...visuals.cardStyle, color: primaryOnBg }}>
                {t('themeSettings.demo.secondary')}
              </span>
            </div>
            <div className="flex flex-wrap gap-1.5">
              <span className="rounded-md border px-2 py-1 text-[9px]" style={{ ...visuals.cardStyle, color: mutedOnSurface }}>
                {t('themeSettings.demo.input')}
              </span>
              <span className="rounded-md border px-2 py-1 text-[9px]" style={{ ...visuals.cardStyle, color: mutedOnSurface }}>
                {t('themeSettings.demo.selection')}
              </span>
            </div>
            <div className="flex flex-wrap gap-1.5">
              <span className="rounded-full px-1.5 py-0.5 text-[8.5px]" style={{ background: visuals.pillGradient, color: onPrimary }}>
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
                <div className="h-full rounded-full" style={{ width: '68%', background: visuals.pillGradient }} />
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
  const { themeMode, colorTheme, theme, canvasGradient, setThemeMode, setColorTheme, setCanvasGradient, resetTheme } =
    useTheme()
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
              className="inline-flex items-center gap-1 rounded-md border border-transparent bg-surface bg-control-glass px-2.5 py-1.5 text-[11.5px] text-ink-2 transition-colors hover:text-ink"
            >
              <RotateCcw className="h-3.5 w-3.5" />
              {t('themeSettings.reset')}
            </button>
          </>
        }
      />

      <div className="space-y-6">
        {/* 外观模式三卡 */}
        <section aria-labelledby="appearance-modes-heading" className="rounded-[14px] border border-transparent bg-chat-canvas p-5 shadow-hair">
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
                    active
                      ? 'border-brand bg-brand-soft'
                      : CHOICE_CARD_IDLE
                  }`}
                >
                  <AppearanceThumbnail
                    swatch={getThemePreviewSwatch(activeItem, option.id)}
                    mode={option.id}
                    canvasGradient={canvasGradient}
                  />
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
          {/* 极光玻璃开关（默认开启；开启后与 <html class="canvas-gradient"> 联动） */}
          <div className="mt-4 flex items-center justify-between gap-4 border-t-[0.5px] border-hairline pt-3.5">
            <div className="min-w-0">
              <label
                htmlFor="canvas-gradient-switch"
                id="canvas-gradient-label"
                className="block cursor-pointer text-[12.5px] font-medium text-ink"
              >
                {t('themeSettings.canvasGradientTitle')}
              </label>
              <p id="canvas-gradient-desc" className="mt-0.5 text-[11px] leading-snug text-ink-3">
                {t('themeSettings.canvasGradientDesc')}
              </p>
            </div>
            <button
              id="canvas-gradient-switch"
              type="button"
              role="switch"
              aria-checked={canvasGradient}
              aria-labelledby="canvas-gradient-label"
              aria-describedby="canvas-gradient-desc"
              onClick={() => setCanvasGradient(!canvasGradient)}
              className={`relative h-[18px] w-8 flex-none rounded-[10px] border border-transparent transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand ${
                canvasGradient ? 'bg-brand switch-brand-track' : 'bg-track'
              }`}
            >
              <span
                className={`absolute left-0.5 top-0.5 size-3 rounded-full bg-white shadow-[0_1px_2px_rgba(0,0,0,.28)] transition-transform ${
                  canvasGradient ? 'translate-x-[14px]' : 'translate-x-0'
                }`}
              />
            </button>
          </div>
          <p className="mt-2.5 inline-flex items-center gap-1 text-[10.5px] text-ink-3">
            {t('themeSettings.localPreference')}
          </p>
        </section>

        {/* 当前主题 + 实时预览 */}
        <section aria-label={t('themeSettings.currentTheme', { theme: t(`common.theme.${activeItem.nameKey}`) })} className="rounded-[14px] border border-transparent bg-chat-canvas p-5 shadow-hair">
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
          <ThemeLivePreview item={activeItem} mode={themeMode} canvasGradient={canvasGradient} />
        </section>

        {/* 主题库：搜索 + 分组筛选 + 网格 */}
        <section aria-labelledby="styles-heading" className="rounded-[14px] border border-transparent bg-chat-canvas p-5 shadow-hair">
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
                className="w-full rounded-md border-[0.5px] border-hairline bg-[color-mix(in_srgb,var(--surface)_65%,transparent)] bg-control-glass py-1.5 pl-8 pr-8 text-[12px] text-ink placeholder:text-ink-3 focus:border-brand focus:outline-none"
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
                    active
                      ? 'border-brand bg-brand btn-brand-gradient text-brand-fg'
                      : 'border-transparent bg-transparent text-ink-2 hover:bg-[color-mix(in_srgb,var(--surface-3)_45%,transparent)]'
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
                  mode={theme}
                  canvasGradient={canvasGradient}
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
