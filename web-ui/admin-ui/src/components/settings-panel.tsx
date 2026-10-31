// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useRef, useState, type ReactNode } from 'react'
import { toast } from '@/lib/toast'
import { useTranslation } from 'react-i18next'
import { Bug, Download, History, Info, Monitor, Pencil, Palette, Server, ShieldCheck, type LucideIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { PageHead } from '@/components/page-head'
import { DumpFilesDialog } from '@/components/dump-files-dialog'
import {
  useLoadBalancingMode, useSetLoadBalancingMode,
  useSuggestionMode, useSetSuggestionMode,
  useClientTokenPassthrough, useSetClientTokenPassthrough,
  useThinkingAsText, useSetThinkingAsText,
  useShowSessionCredits, useSetShowSessionCredits,
  useDumpKiroBody, useSetDumpKiroBody,
  useDebugLogs, useSetDebugLogs,
  useRuntimeConfig, useSetRuntimeConfig,
  useAuthKeys, useSetAuthKeys,
} from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import { LANG_STORAGE_KEY } from '@/i18n'

/** 分区标题（设计稿 .set-cap） */
const SET_CAP =
  'flex items-center gap-1.5 px-0.5 pb-2 text-[10.5px] font-semibold uppercase tracking-[.08em] text-ink-3'
/** 分区卡片（设计稿 .set-card） */
const SET_CARD = 'overflow-hidden rounded-lg border border-transparent bg-surface shadow-hair'
/** 配置行（设计稿 .set-row） */
const SET_ROW = 'flex items-center gap-4 border-b-[0.5px] border-hairline px-4 py-[13px] last:border-b-0'
/** 行键名（设计稿 .set-k） */
const SET_K = 'flex items-center gap-[7px] text-[12.5px] font-semibold'
/** 行说明（设计稿 .set-d） */
const SET_D = 'mt-0.5 max-w-[760px] text-[11px] leading-[1.5] text-ink-3'
/** 行控件区（设计稿 .set-c） */
const SET_C = 'ml-auto flex flex-none items-center gap-2'
/** 只读值框（设计稿 .field.w-s） */
const FIELD_S =
  'flex h-[31px] min-w-[96px] items-center rounded-[7px] border border-transparent bg-surface-2 px-2.5 font-mono text-[12px] text-ink-2'
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
  onOpenThemeSettings: () => void
  onOpenChangelog: () => void
}

export function SettingsPanel({
  onOpenThemeSettings,
  onOpenChangelog,
}: SettingsPanelProps) {
  const { t, i18n } = useTranslation()
  const { data: loadBalancingData, isLoading: isLoadingMode } = useLoadBalancingMode()
  const { mutate: setLoadBalancingMode, isPending: isSettingMode } = useSetLoadBalancingMode()
  const { data: suggestionModeData, isError: suggestionModeError, isLoading: isLoadingSuggestionMode } = useSuggestionMode()
  const { mutate: setSuggestionMode, isPending: isSettingSuggestionMode } = useSetSuggestionMode()
  const { data: tokenPassthroughData, isError: tokenPassthroughError, isLoading: isLoadingTokenPassthrough } = useClientTokenPassthrough()
  const { mutate: setTokenPassthrough, isPending: isSettingTokenPassthrough } = useSetClientTokenPassthrough()
  const { data: thinkingAsTextData, isError: thinkingAsTextError, isLoading: isLoadingThinkingAsText } = useThinkingAsText()
  const { mutate: setThinkingAsText, isPending: isSettingThinkingAsText } = useSetThinkingAsText()
  const { data: showSessionCreditsData, isError: showSessionCreditsError, isLoading: isLoadingShowSessionCredits } = useShowSessionCredits()
  const { mutate: setShowSessionCredits, isPending: isSettingShowSessionCredits } = useSetShowSessionCredits()
  const { data: dumpKiroBodyData, isError: dumpKiroBodyError, isLoading: isLoadingDumpKiroBody } = useDumpKiroBody()
  const { mutate: setDumpKiroBody, isPending: isSettingDumpKiroBody } = useSetDumpKiroBody()
  const { data: debugLogsData, isError: debugLogsError, isLoading: isLoadingDebugLogs } = useDebugLogs()
  const { mutate: setDebugLogsMut, isPending: isSettingDebugLogs } = useSetDebugLogs()
  const { data: authKeysData, isLoading: isLoadingAuthKeys } = useAuthKeys()
  const { mutate: setAuthKeysMut, isPending: isSettingAuthKeys } = useSetAuthKeys()
  const [adminPswDraft, setAdminPswDraft] = useState('')
  const [editingAdminPsw, setEditingAdminPsw] = useState(false)
  const { data: runtimeConfigData, isError: isRuntimeConfigError, isLoading: isLoadingRuntimeConfig } = useRuntimeConfig()
  const { mutate: setRuntimeConfigMut, isPending: isSettingRuntimeConfig } = useSetRuntimeConfig()
  const [runtimeDraft, setRuntimeDraft] = useState<{ maxRpm: string; chunkLines: string; port: string; proxyUrl: string }>({
    maxRpm: '',
    chunkLines: '',
    port: '',
    proxyUrl: '',
  })
  const [editingRuntime, setEditingRuntime] = useState<'maxRpm' | 'chunkLines' | 'port' | 'proxyUrl' | null>(null)
  // dump 文件浏览弹窗开关；弹窗关闭时列表查询不下发请求
  const [dumpFilesOpen, setDumpFilesOpen] = useState(false)

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

  const saveRuntimeField = (field: 'maxRpm' | 'chunkLines' | 'port' | 'proxyUrl') => {
    const payload: {
      maxRpmPerCredential?: number
      writeChunkingMaxLines?: number
      port?: number
      proxyUrl?: string
    } = {}
    if (field === 'maxRpm') payload.maxRpmPerCredential = Number(runtimeDraft.maxRpm)
    else if (field === 'chunkLines') payload.writeChunkingMaxLines = Number(runtimeDraft.chunkLines)
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

  const startEditRuntime = (field: 'maxRpm' | 'chunkLines' | 'port' | 'proxyUrl') => {
    if (field === 'maxRpm') setRuntimeDraft((d) => ({ ...d, maxRpm: String(runtimeConfigData?.maxRpmPerCredential ?? '') }))
    else if (field === 'chunkLines') setRuntimeDraft((d) => ({ ...d, chunkLines: String(runtimeConfigData?.writeChunkingMaxLines ?? '') }))
    else if (field === 'port') setRuntimeDraft((d) => ({ ...d, port: String(runtimeConfigData?.port ?? '') }))
    else setRuntimeDraft((d) => ({ ...d, proxyUrl: runtimeConfigData?.proxyUrl ?? '' }))
    setEditingRuntime(field)
  }

  const cancelEditRuntime = () => {
    setEditingRuntime(null)
    setRuntimeDraft({ maxRpm: '', chunkLines: '', port: '', proxyUrl: '' })
  }

  const runtimeCanSave = (field: 'maxRpm' | 'chunkLines' | 'port' | 'proxyUrl') => {
    if (field === 'maxRpm') {
      // 与后端 Option<u32> 对齐：非负整数且 ≤ 4294967295
      return /^\d+$/.test(runtimeDraft.maxRpm) && Number(runtimeDraft.maxRpm) <= 4294967295
    }
    if (field === 'chunkLines') {
      // 与后端 Option<u32> 对齐：非负整数且 ≤ 4294967295（0 = 不追加约束）
      return /^\d+$/.test(runtimeDraft.chunkLines) && Number(runtimeDraft.chunkLines) <= 4294967295
    }
    if (field === 'port') {
      return /^\d+$/.test(runtimeDraft.port) && Number(runtimeDraft.port) >= 1 && Number(runtimeDraft.port) <= 65535
    }
    return true
  }

  /** 运行时配置行共用的编辑态控件（Input + 保存/取消按钮） */
  const runtimeEditor = (
    field: 'maxRpm' | 'chunkLines' | 'port' | 'proxyUrl',
    value: string,
    onChange: (v: string) => void,
    inputType: 'text' | 'number',
    canSave: boolean
  ) => (
    <>
      <Input
        type={inputType}
        aria-label={t(
          field === 'maxRpm'
            ? 'settings.maxRpmPerCredential'
            : field === 'chunkLines'
              ? 'settings.writeChunkingMaxLines'
              : field === 'port'
                ? 'settings.port'
                : 'settings.proxyUrl'
        )}
        autoFocus
        disabled={isSettingRuntimeConfig}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        // 数值类（maxRpm/chunkLines/port）用短输入框，URL 类（proxyUrl）保留宽输入框
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
          <Row label={t('settings.suggestionMode')} desc={suggestionModeError ? t('common.loadFailed') : t('settings.suggestionModeDesc')}>
            <Sw
              label={t('settings.suggestionMode')}
              on={!!suggestionModeData?.enabled}
              disabled={suggestionModeError || isLoadingSuggestionMode || isSettingSuggestionMode}
              onToggle={() =>
                setSuggestionMode(!suggestionModeData?.enabled, {
                  onSuccess: (d) => toast.success(d.message),
                  onError: (e) => toast.error(extractErrorMessage(e)),
                })
              }
            />
          </Row>
          <Row label={t('settings.clientTokenPassthrough')} desc={tokenPassthroughError ? t('common.loadFailed') : t('settings.clientTokenPassthroughDesc')}>
            <Sw
              label={t('settings.clientTokenPassthrough')}
              on={!!tokenPassthroughData?.enabled}
              disabled={tokenPassthroughError || isLoadingTokenPassthrough || isSettingTokenPassthrough}
              onToggle={() =>
                setTokenPassthrough(!tokenPassthroughData?.enabled, {
                  onSuccess: (d) => toast.success(d.message),
                  onError: (e) => toast.error(extractErrorMessage(e)),
                })
              }
            />
          </Row>
          <Row label={t('settings.thinkingAsText')} desc={thinkingAsTextError ? t('common.loadFailed') : t('settings.thinkingAsTextDesc')}>
            <Sw
              label={t('settings.thinkingAsText')}
              on={!!thinkingAsTextData?.enabled}
              disabled={thinkingAsTextError || isLoadingThinkingAsText || isSettingThinkingAsText}
              onToggle={() =>
                setThinkingAsText(!thinkingAsTextData?.enabled, {
                  onSuccess: (d) => toast.success(d.message),
                  onError: (e) => toast.error(extractErrorMessage(e)),
                })
              }
            />
          </Row>
          <Row label={t('settings.showSessionCredits')} desc={showSessionCreditsError ? t('common.loadFailed') : t('settings.showSessionCreditsDesc')}>
            <Sw
              label={t('settings.showSessionCredits')}
              on={!!showSessionCreditsData?.enabled}
              disabled={showSessionCreditsError || isLoadingShowSessionCredits || isSettingShowSessionCredits}
              onToggle={() =>
                setShowSessionCredits(!showSessionCreditsData?.enabled, {
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
          <Row label={t('settings.writeChunkingMaxLines')} desc={t('settings.writeChunkingMaxLinesDesc')}>
            {editingRuntime === 'chunkLines' ? (
              runtimeEditor(
                'chunkLines',
                runtimeDraft.chunkLines,
                (v) => setRuntimeDraft((d) => ({ ...d, chunkLines: v })),
                'number',
                runtimeCanSave('chunkLines')
              )
            ) : (
              <>
                <div className={FIELD_S}>{isLoadingRuntimeConfig ? t('common.loading') : (runtimeConfigData?.writeChunkingMaxLines ?? '—')}</div>
                <Button variant="ghost" size="sm" disabled={isLoadingRuntimeConfig || isRuntimeConfigError || isSettingRuntimeConfig} onClick={() => startEditRuntime('chunkLines')}>
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
            <Button type="button" variant="ghost" onClick={onOpenThemeSettings}>
              <Palette aria-hidden="true" />
              {t('dashboard.navTheme')}
            </Button>
          </Row>
        </Section>

        <Section icon={Bug} title={t('settings.capDebug')}>
          <Row label={t('settings.dumpKiroBody')} desc={dumpKiroBodyError ? t('common.loadFailed') : t('settings.dumpKiroBodyDesc')}>
            {/* 不随开关置灰：开关读取失败（dumpKiroBodyError）时落盘历史仍可下载 */}
            <Button
              type="button"
              variant="outline"
              aria-label={t('settings.downloadDumpFiles')}
              onClick={() => setDumpFilesOpen(true)}
            >
              <Download aria-hidden="true" />
              <span className="hidden sm:inline">{t('settings.downloadDumpFiles')}</span>
            </Button>
            <Sw
              label={t('settings.dumpKiroBody')}
              on={!!dumpKiroBodyData?.enabled}
              disabled={dumpKiroBodyError || isLoadingDumpKiroBody || isSettingDumpKiroBody}
              onToggle={() =>
                setDumpKiroBody(!dumpKiroBodyData?.enabled, {
                  onSuccess: (d) => toast.success(d.message),
                  onError: (e) => toast.error(extractErrorMessage(e)),
                })
              }
            />
          </Row>
          <Row label={t('settings.debugLogs')} desc={debugLogsError ? t('common.loadFailed') : t('settings.debugLogsDesc')}>
            <Sw
              label={t('settings.debugLogs')}
              on={!!debugLogsData?.enabled}
              disabled={debugLogsError || isLoadingDebugLogs || isSettingDebugLogs}
              onToggle={() =>
                setDebugLogsMut(!debugLogsData?.enabled, {
                  onSuccess: (d) => toast.success(d.message),
                  onError: (e) => toast.error(extractErrorMessage(e)),
                })
              }
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

      <DumpFilesDialog open={dumpFilesOpen} onOpenChange={setDumpFilesOpen} />
    </div>
  )
}
