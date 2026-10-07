// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useRef, useState, type ReactNode } from 'react'
import { toast } from 'sonner'
import { useTranslation } from 'react-i18next'
import { History, Info, Monitor, Moon, Pencil, Server, ShieldCheck, Sun, type LucideIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { PageHead } from '@/components/page-head'
import {
  useLoadBalancingMode, useSetLoadBalancingMode,
  useSuggestionMode, useSetSuggestionMode,
  useClientTokenPassthrough, useSetClientTokenPassthrough,
  useThinkingAsText, useSetThinkingAsText,
  useRuntimeConfig, useSetRuntimeConfig,
  useAuthKeys, useSetAuthKeys,
} from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import { LANG_STORAGE_KEY } from '@/i18n'
import type { Theme } from '@/hooks/use-theme'

/** 分区标题（设计稿 .set-cap） */
const SET_CAP =
  'flex items-center gap-1.5 px-0.5 pb-2 text-[10.5px] font-semibold uppercase tracking-[.08em] text-ink-3'
/** 分区卡片（设计稿 .set-card） */
const SET_CARD = 'overflow-hidden rounded-lg border border-hairline bg-surface shadow-hair'
/** 配置行（设计稿 .set-row） */
const SET_ROW = 'flex items-center gap-4 border-b border-hairline px-4 py-[13px] last:border-b-0'
/** 行键名（设计稿 .set-k） */
const SET_K = 'flex items-center gap-[7px] text-[12.5px] font-semibold'
/** 行说明（设计稿 .set-d） */
const SET_D = 'mt-0.5 max-w-[760px] text-[11px] leading-[1.5] text-ink-3'
/** 行控件区（设计稿 .set-c） */
const SET_C = 'ml-auto flex flex-none items-center gap-2'
/** 只读值框（设计稿 .field.w-s） */
const FIELD_S =
  'flex h-[31px] min-w-[96px] items-center rounded-[7px] border border-hairline-2 bg-surface-2 px-2.5 font-mono text-[12px] text-ink-2'
/** 分段器容器（设计稿 .seg，8px 圆角与 borderRadius.lg 的 11px 不同，故用字面量） */
const SEG = 'flex flex-none gap-0.5 rounded-[8px] bg-surface-3 p-0.5'
/** 分段器档位（设计稿 .seg button） */
const SEG_BTN =
  'flex h-[27px] items-center gap-[5px] whitespace-nowrap rounded-[6px] px-2.5 text-[12px] font-medium text-ink-2 transition-colors hover:text-ink focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand disabled:pointer-events-none disabled:opacity-50'
/** 分段器激活档（设计稿 .seg button.on） */
const SEG_BTN_ON = 'bg-surface font-semibold text-ink shadow-hair'
/** 开关（设计稿 .sw） */
const SW =
  'relative h-[18px] w-8 flex-none rounded-[10px] border border-transparent transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand'
/** 开关滑块（设计稿 .sw i）：白色不随主题反转，与设计稿一致 */
const SW_DOT = 'absolute top-0.5 size-3 rounded-full bg-white shadow-[0_1px_2px_rgba(0,0,0,.28)]'
/** 管理密码掩码：固定 12 位，不随真实长度变化以免泄露长度信息 */
const PSW_MASK = '∗'.repeat(12)

/** 配置分区：`.set-cap` 标题 + `.set-card` 卡片 */
function Section({ icon: Icon, title, children }: { icon: LucideIcon; title: string; children: ReactNode }) {
  return (
    <section>
      <div className={SET_CAP}>
        <Icon className="size-3.5" aria-hidden="true" />
        {title}
      </div>
      <div className={SET_CARD}>{children}</div>
    </section>
  )
}

/** 配置行：左列键名 + 说明，右列控件 */
function Row({ label, desc, children }: { label: string; desc: string; children: ReactNode }) {
  return (
    <div className={SET_ROW}>
      <div className="min-w-0">
        <div className={SET_K}>{label}</div>
        <div className={SET_D}>{desc}</div>
      </div>
      <div className={SET_C}>{children}</div>
    </div>
  )
}

interface SegOption<T extends string> {
  value: T
  label: string
  icon?: LucideIcon
}

/**
 * 两档互斥选择（设计稿 .seg）。点击已激活档不回调，
 * 使 onSelect 可安全绑定 toggle 式（无参）的状态切换函数 —— 该等价性仅在两档下成立，
 * 故 options 类型收紧为二元组：新增第三档会在此处直接编译报错，
 * 迫使调用方改用显式设值而非 toggle。
 */
function Seg<T extends string>({
  groupLabel,
  value,
  options,
  onSelect,
  disabled,
}: {
  groupLabel: string
  value: T | undefined
  options: readonly [SegOption<T>, SegOption<T>]
  onSelect: (next: T, x?: number, y?: number) => void
  disabled?: boolean
}) {
  const rootRef = useRef<HTMLDivElement>(null)
  const selectedIdx = options.findIndex((o) => o.value === value)

  /** APG radiogroup：方向键从当前焦点档位移并同步选中，焦点跟随目标档 */
  const move = (from: number, delta: number) => {
    const to = (from + delta + options.length) % options.length
    rootRef.current?.querySelectorAll('button')[to]?.focus()
    const next = options[to]
    if (next && next.value !== value) onSelect(next.value)
  }

  return (
    <div ref={rootRef} className={SEG} role="radiogroup" aria-label={groupLabel}>
      {options.map((opt, i) => {
        const on = opt.value === value
        const Icon = opt.icon
        return (
          <button
            key={opt.value}
            type="button"
            role="radio"
            aria-checked={on}
            disabled={disabled}
            // roving tabindex：整组只占一个 Tab 位；value 未加载时回退到首档以免整组无法聚焦
            tabIndex={on || (selectedIdx < 0 && i === 0) ? 0 : -1}
            onKeyDown={(e) => {
              if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
                e.preventDefault()
                move(i, -1)
              } else if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
                e.preventDefault()
                move(i, 1)
              }
            }}
            onClick={(e) => {
              if (!on) onSelect(opt.value, e.clientX, e.clientY)
            }}
            className={`${SEG_BTN} ${on ? SEG_BTN_ON : ''}`}
          >
            {Icon && <Icon className="size-3.5" aria-hidden="true" />}
            {opt.label}
          </button>
        )
      })}
    </div>
  )
}

/** 布尔开关（设计稿 .sw） */
function Sw({ label, on, onToggle, disabled }: { label: string; on: boolean; onToggle: () => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      onClick={onToggle}
      className={`${SW} ${on ? 'bg-brand' : 'bg-track'} disabled:pointer-events-none disabled:opacity-50`}
    >
      <span className={`${SW_DOT} ${on ? 'right-0.5' : 'left-0.5'}`} />
    </button>
  )
}

interface SettingsPanelProps {
  /** 主题由 Dashboard 持有：useTheme() 每实例独立 state，
   *  本页自行调用会与侧栏页脚按钮各持一份而无法同步 */
  theme: Theme
  onToggleTheme: (x?: number, y?: number) => void
  onOpenChangelog: () => void
}

export function SettingsPanel({
  theme,
  onToggleTheme,
  onOpenChangelog,
}: SettingsPanelProps) {
  const { t, i18n } = useTranslation()
  const { data: loadBalancingData, isLoading: isLoadingMode } = useLoadBalancingMode()
  const { mutate: setLoadBalancingMode, isPending: isSettingMode } = useSetLoadBalancingMode()
  const { data: suggestionModeData, isLoading: isLoadingSuggestionMode } = useSuggestionMode()
  const { mutate: setSuggestionMode, isPending: isSettingSuggestionMode } = useSetSuggestionMode()
  const { data: tokenPassthroughData, isLoading: isLoadingTokenPassthrough } = useClientTokenPassthrough()
  const { mutate: setTokenPassthrough, isPending: isSettingTokenPassthrough } = useSetClientTokenPassthrough()
  const { data: thinkingAsTextData, isLoading: isLoadingThinkingAsText } = useThinkingAsText()
  const { mutate: setThinkingAsText, isPending: isSettingThinkingAsText } = useSetThinkingAsText()
  const { data: authKeysData, isLoading: isLoadingAuthKeys } = useAuthKeys()
  const { mutate: setAuthKeysMut, isPending: isSettingAuthKeys } = useSetAuthKeys()
  const [adminPswDraft, setAdminPswDraft] = useState('')
  const [editingAdminPsw, setEditingAdminPsw] = useState(false)
  const { data: runtimeConfigData, isError: isRuntimeConfigError, isLoading: isLoadingRuntimeConfig } = useRuntimeConfig()
  const { mutate: setRuntimeConfigMut, isPending: isSettingRuntimeConfig } = useSetRuntimeConfig()
  const [runtimeDraft, setRuntimeDraft] = useState<{ maxRpm: string; port: string; proxyUrl: string }>({
    maxRpm: '',
    port: '',
    proxyUrl: '',
  })
  const [editingRuntime, setEditingRuntime] = useState<'maxRpm' | 'port' | 'proxyUrl' | null>(null)

  const lang = i18n.language === 'en' ? 'en' : 'zh'

  const changeLanguage = (next: 'zh' | 'en') => {
    i18n.changeLanguage(next)
    localStorage.setItem(LANG_STORAGE_KEY, next)
  }

  const changeMode = (next: 'priority' | 'balanced') => {
    setLoadBalancingMode(next, {
      onSuccess: () =>
        toast.success(
          t('settings.switchedTo', {
            mode: next === 'priority' ? t('settings.priorityMode') : t('settings.balancedMode'),
          })
        ),
      onError: (e) => toast.error(extractErrorMessage(e)),
    })
  }

  const saveAdminPsw = () => {
    setAuthKeysMut(
      { adminPsw: adminPswDraft.trim() },
      {
        onSuccess: () => {
          toast.success(t('settings.adminPasswordUpdated'))
          setEditingAdminPsw(false)
          setAdminPswDraft('')
        },
        onError: (e) => toast.error(extractErrorMessage(e)),
      }
    )
  }

  const saveRuntimeField = (field: 'maxRpm' | 'port' | 'proxyUrl') => {
    const payload: { maxRpmPerCredential?: number; port?: number; proxyUrl?: string } = {}
    if (field === 'maxRpm') payload.maxRpmPerCredential = Number(runtimeDraft.maxRpm)
    else if (field === 'port') payload.port = Number(runtimeDraft.port)
    else payload.proxyUrl = runtimeDraft.proxyUrl.trim()

    setRuntimeConfigMut(payload, {
      onSuccess: (d) => {
        toast.success(d.message)
        setEditingRuntime(null)
      },
      onError: (e) => toast.error(extractErrorMessage(e)),
    })
  }

  const startEditRuntime = (field: 'maxRpm' | 'port' | 'proxyUrl') => {
    if (field === 'maxRpm') setRuntimeDraft((d) => ({ ...d, maxRpm: String(runtimeConfigData?.maxRpmPerCredential ?? '') }))
    else if (field === 'port') setRuntimeDraft((d) => ({ ...d, port: String(runtimeConfigData?.port ?? '') }))
    else setRuntimeDraft((d) => ({ ...d, proxyUrl: runtimeConfigData?.proxyUrl ?? '' }))
    setEditingRuntime(field)
  }

  const cancelEditRuntime = () => {
    setEditingRuntime(null)
    setRuntimeDraft({ maxRpm: '', port: '', proxyUrl: '' })
  }

  const runtimeCanSave = (field: 'maxRpm' | 'port' | 'proxyUrl') => {
    if (field === 'maxRpm') {
      // 与后端 Option<u32> 对齐：非负整数且 ≤ 4294967295
      return /^\d+$/.test(runtimeDraft.maxRpm) && Number(runtimeDraft.maxRpm) <= 4294967295
    }
    if (field === 'port') {
      return /^\d+$/.test(runtimeDraft.port) && Number(runtimeDraft.port) >= 1 && Number(runtimeDraft.port) <= 65535
    }
    return true
  }

  /** 运行时配置行共用的编辑态控件（Input + 保存/取消按钮） */
  const runtimeEditor = (
    field: 'maxRpm' | 'port' | 'proxyUrl',
    value: string,
    onChange: (v: string) => void,
    inputType: 'text' | 'number',
    canSave: boolean
  ) => (
    <>
      <Input
        type={inputType}
        aria-label={t(
          field === 'maxRpm' ? 'settings.maxRpmPerCredential' : field === 'port' ? 'settings.port' : 'settings.proxyUrl'
        )}
        autoFocus
        disabled={isSettingRuntimeConfig}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        // 数值类（maxRpm/port）用短输入框，URL 类（proxyUrl）保留宽输入框
        className={field === 'proxyUrl' ? 'w-[290px]' : 'w-[110px]'}
      />
      <Button size="sm" disabled={!canSave || isSettingRuntimeConfig} onClick={() => saveRuntimeField(field)}>
        {t('common.save')}
      </Button>
      <Button variant="ghost" size="sm" disabled={isSettingRuntimeConfig} onClick={cancelEditRuntime}>
        {t('common.cancel')}
      </Button>
    </>
  )

  return (
    <div>
      <PageHead
        crumb={[t('dashboard.navSystem'), t('settings.title')]}
        title={t('settings.title')}
        note={t('settings.headNote')}
      />

      <div className="flex flex-col gap-5 pb-[26px]">
        <Section icon={Server} title={t('settings.capService')}>
          <Row label={t('settings.loadBalancingMode')} desc={t('settings.selectStrategyDesc')}>
            <Seg
              groupLabel={t('settings.loadBalancingMode')}
              value={loadBalancingData?.mode}
              options={[
                { value: 'priority' as const, label: t('settings.priorityMode') },
                { value: 'balanced' as const, label: t('settings.balancedMode') },
              ]}
              onSelect={changeMode}
              disabled={isLoadingMode || isSettingMode}
            />
          </Row>
          <Row label={t('settings.suggestionMode')} desc={t('settings.suggestionModeDesc')}>
            <Sw
              label={t('settings.suggestionMode')}
              on={!!suggestionModeData?.enabled}
              disabled={isLoadingSuggestionMode || isSettingSuggestionMode}
              onToggle={() =>
                setSuggestionMode(!suggestionModeData?.enabled, {
                  onSuccess: (d) => toast.success(d.message),
                  onError: (e) => toast.error(extractErrorMessage(e)),
                })
              }
            />
          </Row>
          <Row label={t('settings.clientTokenPassthrough')} desc={t('settings.clientTokenPassthroughDesc')}>
            <Sw
              label={t('settings.clientTokenPassthrough')}
              on={!!tokenPassthroughData?.enabled}
              disabled={isLoadingTokenPassthrough || isSettingTokenPassthrough}
              onToggle={() =>
                setTokenPassthrough(!tokenPassthroughData?.enabled, {
                  onSuccess: (d) => toast.success(d.message),
                  onError: (e) => toast.error(extractErrorMessage(e)),
                })
              }
            />
          </Row>
          <Row label={t('settings.thinkingAsText')} desc={t('settings.thinkingAsTextDesc')}>
            <Sw
              label={t('settings.thinkingAsText')}
              on={!!thinkingAsTextData?.enabled}
              disabled={isLoadingThinkingAsText || isSettingThinkingAsText}
              onToggle={() =>
                setThinkingAsText(!thinkingAsTextData?.enabled, {
                  onSuccess: (d) => toast.success(d.message),
                  onError: (e) => toast.error(extractErrorMessage(e)),
                })
              }
            />
          </Row>
          <Row label={t('settings.maxRpmPerCredential')} desc={t('settings.maxRpmPerCredentialDesc')}>
            {editingRuntime === 'maxRpm' ? (
              runtimeEditor(
                'maxRpm',
                runtimeDraft.maxRpm,
                (v) => setRuntimeDraft((d) => ({ ...d, maxRpm: v })),
                'number',
                runtimeCanSave('maxRpm')
              )
            ) : (
              <>
                <div className={FIELD_S}>{isLoadingRuntimeConfig ? t('common.loading') : (runtimeConfigData?.maxRpmPerCredential ?? '—')}</div>
                <Button variant="ghost" size="sm" disabled={isLoadingRuntimeConfig || isRuntimeConfigError || isSettingRuntimeConfig} onClick={() => startEditRuntime('maxRpm')}>
                  <Pencil />
                  {t('common.edit')}
                </Button>
              </>
            )}
          </Row>
          <Row label={t('settings.port')} desc={t('settings.portDesc')}>
            {editingRuntime === 'port' ? (
              runtimeEditor(
                'port',
                runtimeDraft.port,
                (v) => setRuntimeDraft((d) => ({ ...d, port: v })),
                'number',
                runtimeCanSave('port')
              )
            ) : (
              <>
                <div className={FIELD_S}>{isLoadingRuntimeConfig ? t('common.loading') : (runtimeConfigData?.port ?? '—')}</div>
                <Button variant="ghost" size="sm" disabled={isLoadingRuntimeConfig || isRuntimeConfigError || isSettingRuntimeConfig} onClick={() => startEditRuntime('port')}>
                  <Pencil />
                  {t('common.edit')}
                </Button>
              </>
            )}
          </Row>
          <Row label={t('settings.proxyUrl')} desc={t('settings.proxyUrlDesc')}>
            {editingRuntime === 'proxyUrl' ? (
              runtimeEditor(
                'proxyUrl',
                runtimeDraft.proxyUrl,
                (v) => setRuntimeDraft((d) => ({ ...d, proxyUrl: v })),
                'text',
                true
              )
            ) : (
              <>
                <div className={`${FIELD_S} max-w-[280px] truncate`}>
                  {isLoadingRuntimeConfig ? t('common.loading') : runtimeConfigData?.proxyUrl || '—'}
                </div>
                <Button variant="ghost" size="sm" disabled={isLoadingRuntimeConfig || isRuntimeConfigError || isSettingRuntimeConfig} onClick={() => startEditRuntime('proxyUrl')}>
                  <Pencil />
                  {t('common.edit')}
                </Button>
              </>
            )}
          </Row>
          <div className="px-4 py-2 text-[11px] text-ink-3">{t('settings.runtimeConfigSaveHint')}</div>
        </Section>

        <Section icon={ShieldCheck} title={t('settings.capSecurity')}>
          <Row label={t('settings.adminPassword')} desc={t('settings.adminPasswordHint')}>
            {editingAdminPsw ? (
              <>
                <Input
                  type="text"
                  autoFocus
                  placeholder={t('settings.adminPasswordPlaceholder')}
                  value={adminPswDraft}
                  onChange={(e) => setAdminPswDraft(e.target.value)}
                  className="w-[290px]"
                />
                <Button size="sm" disabled={!adminPswDraft.trim() || isSettingAuthKeys} onClick={saveAdminPsw}>
                  {t('common.save')}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    setEditingAdminPsw(false)
                    setAdminPswDraft('')
                  }}
                >
                  {t('common.cancel')}
                </Button>
              </>
            ) : (
              <>
                <div className={FIELD_S}>
                  {isLoadingAuthKeys ? t('common.loading') : authKeysData?.adminPsw ? PSW_MASK : '—'}
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={isLoadingAuthKeys}
                  onClick={() => {
                    setAdminPswDraft('')
                    setEditingAdminPsw(true)
                  }}
                >
                  <Pencil />
                  {t('common.edit')}
                </Button>
              </>
            )}
          </Row>
        </Section>

        <Section icon={Monitor} title={t('settings.capUi')}>
          <Row label={t('settings.language')} desc={t('settings.languageDesc')}>
            <Seg
              groupLabel={t('settings.language')}
              value={lang}
              options={[
                { value: 'zh' as const, label: t('settings.languageZh') },
                { value: 'en' as const, label: t('settings.languageEn') },
              ]}
              onSelect={changeLanguage}
            />
          </Row>
          <Row label={t('settings.theme')} desc={t('settings.themeDesc')}>
            <Seg
              groupLabel={t('settings.theme')}
              value={theme}
              options={[
                { value: 'light' as const, label: t('settings.themeLight'), icon: Sun },
                { value: 'dark' as const, label: t('settings.themeDark'), icon: Moon },
              ]}
              onSelect={() => onToggleTheme()}
            />
          </Row>
        </Section>

        <Section icon={Info} title={t('settings.capAbout')}>
          <Row label={t('settings.changelog')} desc={t('settings.changelogDesc')}>
            <Button type="button" variant="ghost" onClick={onOpenChangelog}>
              <History aria-hidden="true" />
              {t('settings.openChangelog')}
            </Button>
          </Row>
        </Section>
      </div>
    </div>
  )
}
