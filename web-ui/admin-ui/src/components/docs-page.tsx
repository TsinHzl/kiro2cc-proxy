// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 使用文档页（自 codex2cc Docs.tsx 适配）：sticky hero + 计数 tabs + 三步流转卡 +
// Mac 窗体工作台 + 左内容/右 TOC + hash 路由 + API Key 填充 + 客户端配置生成器
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from '@/lib/toast'
import {
  ArrowLeft,
  BookOpen,
  Check,
  Copy,
  ExternalLink,
  KeyRound,
  LifeBuoy,
  Server,
  Sparkles,
  Terminal,
  Wrench,
} from 'lucide-react'
import { useApiKeys, useServerInfo } from '@/hooks/use-credentials'
import { copyToClipboard } from '@/lib/clipboard'
import { buildCcSwitchDeeplink, importToCcSwitch } from '@/lib/ccswitch'
import { resolveDocsTarget, type DocEntry } from '@/components/docs/docsNavigation'
import { buildEndpointSpecs } from '@/components/docs/docsContent'
import { buildGuides, type DocsSection } from '@/components/docs/docsGuides'
import { CodeBlock, EndpointDoc } from '@/components/docs/EndpointDoc'
import DocsTOC, { type DocsTOCItem } from '@/components/docs/DocsTOC'
import '@/components/docs/docs.css'

type QuickToolTab = 'claude-code' | 'codex-cli' | 'openai-sdk'
type OsTab = 'unix' | 'windows'

/** 解析服务地址：非空时必须是合法的 http(s) 绝对地址且不带 query/fragment，返回 URL 或 null */
function parseBaseUrl(input: string): URL | null {
  const trimmed = input.trim()
  if (!trimmed) return null
  try {
    const url = new URL(trimmed)
    if (url.protocol !== 'http:' && url.protocol !== 'https:') return null
    if (url.search || url.hash) return null
    return url
  } catch {
    return null
  }
}

/** 静态兜底模型名（用户已确认剔除模型下拉；示例统一用此值） */
const FALLBACK_MODEL = 'claude-sonnet-4-5'

const TOOL_TABS: { value: QuickToolTab; label: string }[] = [
  { value: 'claude-code', label: 'Claude Code' },
  { value: 'codex-cli', label: 'Codex CLI' },
  { value: 'openai-sdk', label: 'OpenAI SDK' },
]

const QUICK_STEP_KEYS = ['step1', 'step2', 'step3'] as const
const QUICK_STEP_ANCHORS = ['qs-step-1', 'qs-step-2', 'qs-step-3'] as const
const QUICK_STEP_ICONS = [Server, KeyRound, Terminal] as const

/** macOS 交通灯三色：用户已确认照搬设计稿固定值，不随主题变化（本页唯一硬编码色值） */
const TRAFFIC_LIGHTS = ['#FF5F57', '#FEBC2E', '#28C840'] as const

/** 状态徽章：ok=已就绪 / brand=已选 / muted=等待配置 */
function StatusBadge({ label, tone }: { label: string; tone: 'ok' | 'brand' | 'muted' }) {
  const toneClass =
    tone === 'ok' ? 'bg-ok-soft text-ok' : tone === 'brand' ? 'bg-brand-soft text-brand' : 'bg-surface-3 text-ink-3'
  return (
    <span className={`inline-flex h-[20px] items-center rounded-full px-2 text-[10.5px] font-semibold ${toneClass}`}>
      {label}
    </span>
  )
}

/** 紧凑操作系统切换（Step 03 内嵌；Claude Code 与 Codex CLI 各自独立状态） */
function OsToggle({
  active,
  onChange,
  label,
}: {
  active: OsTab
  onChange: (v: OsTab) => void
  label: string
}) {
  return (
    <div className="inline-flex items-center gap-0.5 rounded-[6px] bg-surface-3 p-0.5" role="group" aria-label={label}>
      {(['unix', 'windows'] as const).map((value) => (
        <button
          key={value}
          type="button"
          onClick={() => onChange(value)}
          aria-pressed={active === value}
          className={`rounded-[5px] px-2 py-[3px] text-[10.5px] font-semibold transition-colors ${
            active === value ? 'bg-surface text-ink shadow-sm' : 'text-ink-3 hover:text-ink-2'
          }`}
        >
          {value === 'unix' ? 'macOS / Linux' : 'Windows'}
        </button>
      ))}
    </div>
  )
}

/** Mac 交通灯窗体：三色圆点 + 等宽标题 + 一键复制；子内容自行渲染代码块 */
function TerminalWindow({
  title,
  copyText,
  children,
}: {
  title: string
  copyText: string
  children: ReactNode
}) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  const timerRef = useRef<number | null>(null)

  useEffect(
    () => () => {
      if (timerRef.current) window.clearTimeout(timerRef.current)
    },
    [],
  )

  const handleCopy = async () => {
    try {
      await copyToClipboard(copyText)
      setCopied(true)
      if (timerRef.current) window.clearTimeout(timerRef.current)
      timerRef.current = window.setTimeout(() => setCopied(false), 2000)
    } catch {
      toast.error(t('common.copyFailed'))
    }
  }

  return (
    <div className="overflow-hidden rounded-[11px] border-[0.5px] border-hairline bg-surface shadow-panel">
      <div className="flex h-10 items-center gap-3 border-b-[0.5px] border-hairline bg-surface-2 px-3.5">
        <span className="flex items-center gap-1.5" aria-hidden="true">
          {TRAFFIC_LIGHTS.map((color) => (
            <span key={color} className="size-[10px] rounded-full" style={{ backgroundColor: color }} />
          ))}
        </span>
        <span className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-ink-3">{title}</span>
        <button
          type="button"
          onClick={() => void handleCopy()}
          aria-label={copied ? t('common.copied') : t('docs.windowCopy')}
          className={`inline-flex items-center gap-1 rounded-[6px] px-1.5 py-1 text-[10.5px] font-semibold transition-colors ${
            copied ? 'bg-ok-soft text-ok' : 'text-ink-3 hover:bg-surface-3 hover:text-ink'
          }`}
        >
          {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
          {copied ? t('common.copied') : t('common.copy')}
        </button>
      </div>
      <div className="space-y-3 p-3">{children}</div>
    </div>
  )
}

export function DocsPage({ onBack }: { onBack?: () => void }) {
  const { t, i18n } = useTranslation()
  const locale = (i18n.language === 'en' ? 'en' : 'zh') as 'zh' | 'en'
  const { data: apiKeys } = useApiKeys()
  const { data: serverInfo } = useServerInfo()

  const [baseUrlInput, setBaseUrlInput] = useState(window.location.origin)
  const [selectedKeyId, setSelectedKeyId] = useState<number | null>(null)
  const [activeToolTab, setActiveToolTab] = useState<QuickToolTab>('claude-code')
  const [claudeOs, setClaudeOs] = useState<OsTab>('unix')
  const [codexOs, setCodexOs] = useState<OsTab>('unix')
  const [hash, setHash] = useState(window.location.hash)
  const [scrollId, setScrollId] = useState<string | null>(null)
  const [hashLock, setHashLock] = useState(false)
  const contentRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const onHashChange = () => setHash(window.location.hash)
    window.addEventListener('hashchange', onHashChange)
    return () => window.removeEventListener('hashchange', onHashChange)
  }, [])

  // 默认选中第一个启用的 Key；被删的 Key 回落并自动选中下一个可用 Key
  useEffect(() => {
    if (selectedKeyId === null && apiKeys && apiKeys.length > 0) {
      const enabled = apiKeys.find((k) => k.enabled) ?? apiKeys[0]
      setSelectedKeyId(enabled.id)
    }
    if (selectedKeyId !== null && apiKeys && !apiKeys.some((k) => k.id === selectedKeyId)) {
      setSelectedKeyId(null)
    }
  }, [apiKeys, selectedKeyId])

  const selectedKey = apiKeys?.find((k) => k.id === selectedKeyId) ?? null
  const apiKey = selectedKey?.key ?? '<API_KEY>'

  // 生成器统一用派生地址：经 URL 规范化（空格/控制字符已编码）+ 去尾斜杠；
  // 空值/非法输入回退页面 origin，保证示例永远可复制
  const parsedBaseUrl = parseBaseUrl(baseUrlInput)
  const baseUrlError = parsedBaseUrl === null && baseUrlInput.trim() !== ''
  const baseUrl = parsedBaseUrl
    ? `${parsedBaseUrl.origin}${parsedBaseUrl.pathname}`.replace(/\/+$/, '')
    : window.location.origin

  const guides = useMemo(() => buildGuides(baseUrl, locale), [baseUrl, locale])
  const endpoints = useMemo(() => buildEndpointSpecs(baseUrl, locale, apiKey), [baseUrl, locale, apiKey])

  const sections = useMemo(
    () => [
      { id: 'quick-start', label: t('docs.sectionQuickStart'), icon: Sparkles },
      { id: 'development', label: t('docs.sectionDevelopment'), icon: Wrench },
      { id: 'model-api', label: t('docs.sectionModelApi'), icon: Server },
      { id: 'troubleshooting', label: t('docs.sectionTroubleshooting'), icon: LifeBuoy },
    ],
    [t],
  )

  const entries = useMemo<DocEntry[]>(
    () => [
      ...sections.map((s) => ({ id: s.id, section: s.id, title: s.label })),
      { id: 'qs-step-1', section: 'quick-start', title: t('docs.tocQuick.accounts') },
      { id: 'qs-step-2', section: 'quick-start', title: t('docs.tocQuick.key') },
      { id: 'qs-step-3', section: 'quick-start', title: t('docs.tocQuick.service') },
      { id: 'qs-workbench', section: 'quick-start', title: t('docs.tocQuick.workbench') },
      { id: 'qs-curl', section: 'quick-start', title: t('docs.tocQuick.curl') },
      ...guides.map((g) => ({ id: g.id, section: g.section, title: g.title })),
      ...endpoints.map((e) => ({
        id: e.id,
        section: 'model-api',
        title: e.path,
        method: e.method,
      })),
    ],
    [sections, guides, endpoints, t],
  )

  const target = useMemo(() => resolveDocsTarget(hash, entries), [hash, entries])
  const activeSection = target.section as DocsSection
  const section = sections.find((s) => s.id === activeSection) ?? sections[0]

  // hash 直达滚动补偿：mount 前浏览器 fragment 滚动已发生（元素尚未渲染），由 effect 补一次；
  // 无 fragment 的普通入口不滚动，避免进入文档页即跳过 hero
  useEffect(() => {
    if (!hash) {
      // 后退/前进清空 fragment 时复位锁定，否则快速接入区的滚动跟随高亮会永久失效
      setHashLock(false)
      return
    }
    // hash 跳转后 800ms 内锁定 hash 目标：滚动观察不覆盖，避免同行卡片抢走高亮
    setHashLock(true)
    const timer = window.setTimeout(() => setHashLock(false), 800)
    // 分区级目标（tab 点击 / #分区 直达）回到页面顶部：hero + tabs + 分区标题整体可见，
    // 按锚点偏移滚动会把 hero 截掉；指南/接口等条目级目标仍按锚点定位
    if (target.id === target.section) {
      window.scrollTo({ top: 0 })
    } else {
      document.getElementById(target.id)?.scrollIntoView({ block: 'start' })
    }
    return () => window.clearTimeout(timer)
  }, [target.id, target.section, hash])

  // 快速接入分区：滚动跟随当前 TOC 子项（复用 changelog 页的 rootMargin 取值）
  useEffect(() => {
    if (activeSection !== 'quick-start') {
      setScrollId(null)
      return
    }
    const root = contentRef.current
    if (!root) return
    const nodes = Array.from(root.querySelectorAll<HTMLElement>('[data-toc-id]'))
    if (nodes.length === 0) return
    const visible = new Set<string>()
    const observer = new IntersectionObserver(
      (items) => {
        items.forEach((item) => {
          const id = (item.target as HTMLElement).dataset.tocId
          if (!id) return
          if (item.isIntersecting) visible.add(id)
          else visible.delete(id)
        })
        const first = nodes.find((node) => node.dataset.tocId && visible.has(node.dataset.tocId))
        if (first?.dataset.tocId) setScrollId(first.dataset.tocId)
      },
      { rootMargin: '-8% 0px -70% 0px' },
    )
    nodes.forEach((node) => observer.observe(node))
    return () => observer.disconnect()
  }, [activeSection])

  /** 滚动观察结果优先于 hash 目标（仅在快速接入分区内生效；hash 跳转后 800ms 锁定期内以 hash 为准） */
  const activeTocId = activeSection === 'quick-start' && scrollId && !hashLock ? scrollId : target.id

  const sectionCounts = useMemo(() => {
    const counts: Record<string, number> = {}
    sections.forEach((s) => {
      counts[s.id] = entries.filter((e) => e.section === s.id && e.id !== s.id).length
    })
    return counts
  }, [sections, entries])

  const tocItems = useMemo<DocsTOCItem[]>(
    () => [
      {
        id: section.id,
        label: section.label,
        children:
          activeSection === 'quick-start'
            ? [
                { id: 'qs-step-1', label: t('docs.tocQuick.accounts') },
                { id: 'qs-step-2', label: t('docs.tocQuick.key') },
                { id: 'qs-step-3', label: t('docs.tocQuick.service') },
                { id: 'qs-workbench', label: t('docs.tocQuick.workbench') },
                { id: 'qs-curl', label: t('docs.tocQuick.curl') },
              ]
            : entries
                .filter((e) => e.section === activeSection && e.id !== activeSection)
                .map((e) => ({ id: e.id, label: e.title, method: e.method })),
      },
    ],
    [section, entries, activeSection, t],
  )

  // —— 客户端配置生成（Claude Code / Codex CLI / OpenAI SDK）——
  const claudeEnvUnix = `export ANTHROPIC_BASE_URL="${baseUrl}"
export ANTHROPIC_AUTH_TOKEN="${apiKey}"
export CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`
  const claudeEnvWindows = `setx ANTHROPIC_BASE_URL "${baseUrl}"
setx ANTHROPIC_AUTH_TOKEN "${apiKey}"
setx CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC 1`
  const claudeSettings = `{
  "env": {
    "ANTHROPIC_BASE_URL": "${baseUrl}",
    "ANTHROPIC_AUTH_TOKEN": "${apiKey}"
  }
}`

  const codexConfig = `model = "${FALLBACK_MODEL}"
model_provider = "kiro2cc"

[model_providers.kiro2cc]
name = "kiro2cc"
base_url = "${baseUrl}/v1"
wire_api = "responses"
env_key = "KIRO_API_KEY"`
  const codexAuth = `KIRO_API_KEY="${apiKey}"`

  const openaiSdk = `from openai import OpenAI

client = OpenAI(
    base_url="${baseUrl}/v1",
    api_key="${apiKey}",
)

resp = client.chat.completions.create(
    model="${FALLBACK_MODEL}",
    messages=[{"role": "user", "content": "Hello!"}],
)
print(resp.choices[0].message.content)`

  const ccSwitchLink = selectedKey
    ? buildCcSwitchDeeplink('claude', { apiKey, keyName: selectedKey.name, baseUrl })
    : ''

  const curlExample = `curl --request POST \\
  --url ${baseUrl}/v1/messages \\
  --header 'Authorization: Bearer ${apiKey}' \\
  --header 'content-type: application/json' \\
  --data '{"model":"${FALLBACK_MODEL}","max_tokens":1024,"messages":[{"role":"user","content":"ping"}]}'`

  const handleCcSwitchImport = () => {
    if (!selectedKey) return
    importToCcSwitch(
      'claude',
      { apiKey, keyName: selectedKey.name, baseUrl },
      { onNotInstalled: () => toast.error(t('docs.ccSwitchNotInstalled')) },
    )
  }

  /** 显式端口才展示；默认端口（80/443）由浏览器从 URL 省略，拼接反而误导 */
  const port = window.location.port

  const windowCopyText =
    activeToolTab === 'claude-code'
      ? claudeOs === 'unix'
        ? claudeEnvUnix
        : claudeEnvWindows
      : activeToolTab === 'codex-cli'
        ? codexConfig
        : openaiSdk

  const windowTitle =
    activeToolTab === 'claude-code'
      ? claudeOs === 'unix'
        ? t('docs.unixTerminal')
        : t('docs.windowsTerminal')
      : activeToolTab === 'codex-cli'
        ? '~/.codex/config.toml'
        : 'Python'

  /** 三步状态按 key 具名，避免下标数组与语义跳跃（step3 由 Base URL 合法性代表端口可监听） */
  const stepStatus: Record<
    (typeof QUICK_STEP_KEYS)[number],
    { label: string; tone: 'ok' | 'brand' | 'muted' }
  > = {
    step1: serverInfo
      ? { label: t('docs.statusReady'), tone: 'ok' }
      : { label: t('docs.statusPending'), tone: 'muted' },
    step2: selectedKey
      ? { label: t('docs.statusKeySelected'), tone: 'brand' }
      : { label: t('docs.statusPending'), tone: 'muted' },
    step3: baseUrlError
      ? { label: t('docs.statusPending'), tone: 'muted' }
      : { label: t('docs.statusPortListening'), tone: 'ok' },
  }

  return (
    <div>
      <header className="sticky top-0 z-20 -mx-9 -mt-7 border-b-[0.5px] border-hairline bg-background px-9 pb-3 pt-7">
        <div className="flex items-end justify-between gap-4">
          <div className="min-w-0">
            <div className="docs-eyebrow">
              <BookOpen className="size-3.5" />
              KIRO2CC / {t('docs.eyebrow')}
              <span className="docs-ready">
                <span className="docs-ready-dot" aria-hidden="true" />
                {t('docs.readyLabel')}
                {port ? `:${port}` : ''}
              </span>
            </div>
            <div className="flex items-center gap-2">
              {onBack && (
                <button
                  type="button"
                  onClick={onBack}
                  aria-label={t('common.back')}
                  className="grid h-[26px] w-[26px] shrink-0 place-items-center rounded-md text-ink-3 transition-colors hover:bg-surface-3 hover:text-ink"
                >
                  <ArrowLeft className="h-4 w-4" />
                </button>
              )}
              <h2 className="docs-hero-title">{t('docs.heroTitle')}</h2>
            </div>
            <p className="docs-hero-sub">{t('docs.heroSubtitle')}</p>
          </div>
          <a
            href="https://github.com/TsinHzl/kiro2cc-proxy#readme"
            target="_blank"
            rel="noopener noreferrer"
            className="group inline-flex h-[31px] shrink-0 items-center gap-1.5 rounded-[7px] px-[11px] text-[12.5px] font-medium text-ink-2 transition-colors hover:bg-surface-3 hover:text-ink"
          >
            <ExternalLink className="h-3.5 w-3.5 text-ink-3 transition-colors group-hover:text-ink-2" />
            GitHub README
          </a>
        </div>
      </header>

      <nav className="docs-tabs mt-4" aria-label={t('docs.tabsLabel')}>
        {sections.map((s) => (
          <a
            key={s.id}
            href={`#${s.id}`}
            aria-current={activeSection === s.id ? 'page' : undefined}
            onClick={(e) => {
              // 同 hash 重复点击不触发 hashchange，原生 fragment 滚动会截掉 hero；手动回顶部
              if (activeSection === s.id) {
                e.preventDefault()
                window.scrollTo({ top: 0 })
              }
            }}
          >
            <s.icon className="size-4" />
            {s.label}
            <span
              aria-label={t('docs.sectionCount', { count: sectionCounts[s.id] ?? 0 })}
              className="ml-0.5 rounded-full bg-surface px-1.5 text-[10px] font-semibold tabular-nums text-ink-3"
            >
              {sectionCounts[s.id] ?? 0}
            </span>
          </a>
        ))}
      </nav>

      <div className="mt-5 grid grid-cols-1 gap-6 pb-6 lg:grid-cols-[minmax(0,1fr)_220px]">
        <div ref={contentRef} className="min-w-0">
          <div id={section.id} className="docs-section-heading scroll-mt-32">
            {activeSection !== 'quick-start' && (
              <>
                <h3>{section.label}</h3>
                <p>{t(`docs.sectionSummaries.${section.id}`)}</p>
              </>
            )}
          </div>

          {activeSection === 'quick-start' && (
            <>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
                {QUICK_STEP_KEYS.map((key, i) => {
                  const Icon = QUICK_STEP_ICONS[i]
                  const status = stepStatus[key]
                  return (
                    <article
                      key={key}
                      id={QUICK_STEP_ANCHORS[i]}
                      data-toc-id={QUICK_STEP_ANCHORS[i]}
                      className="flex scroll-mt-32 flex-col rounded-[11px] border-[0.5px] border-hairline bg-surface p-4 shadow-panel"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-mono text-[10px] font-bold tracking-[.14em] text-ink-3">
                          {t('docs.stepLabel')} 0{i + 1}
                        </span>
                        <StatusBadge label={status.label} tone={status.tone} />
                      </div>
                      <div className="mt-3 flex items-start gap-2.5">
                        <span className="grid size-7 shrink-0 place-items-center rounded-[7px] bg-brand-soft text-brand">
                          <Icon className="size-4" />
                        </span>
                        <div className="min-w-0">
                          <h3 className="text-[13px] font-semibold text-ink">
                            {t(`docs.steps.${key}.title`)}
                          </h3>
                          <p className="mt-1 text-[11.5px] leading-relaxed text-ink-2">
                            {t(`docs.steps.${key}.detail`)}
                          </p>
                        </div>
                      </div>

                      {i === 1 && (
                        <div className="mt-3 border-t-[0.5px] border-hairline pt-3">
                          <label className="block">
                            <span className="mb-1 block text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3">
                              {t('docs.selectKey')}
                            </span>
                            <select
                              value={selectedKeyId ?? ''}
                              onChange={(e) => setSelectedKeyId(e.target.value ? Number(e.target.value) : null)}
                              className="h-8 w-full rounded-[6px] border-[0.5px] border-hairline-2 bg-surface-2 px-2 text-[11.5px] text-ink"
                              aria-label={t('docs.selectKey')}
                            >
                              {apiKeys && apiKeys.length > 0 ? (
                                apiKeys.map((k) => (
                                  <option key={k.id} value={k.id}>
                                    {k.name} · {k.key.slice(0, 7)}...{k.key.slice(-4)}
                                  </option>
                                ))
                              ) : (
                                <option value="">{t('docs.noKeys')}</option>
                              )}
                            </select>
                          </label>
                        </div>
                      )}

                      {i === 2 && (
                        <div className="mt-3 border-t-[0.5px] border-hairline pt-3">
                          <label className="block">
                            <span className="mb-1 block text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3">
                              {t('docs.baseUrlLabel')}
                            </span>
                            <input
                              value={baseUrlInput}
                              onChange={(e) => setBaseUrlInput(e.target.value)}
                              aria-invalid={!!baseUrlError}
                              aria-describedby={baseUrlError ? 'docs-baseurl-error' : undefined}
                              className={`h-8 w-full rounded-[6px] border-[0.5px] px-2 font-mono text-[11.5px] ${
                                baseUrlError
                                  ? 'border-danger bg-danger-soft text-ink'
                                  : 'border-hairline-2 bg-surface-2 text-ink'
                              }`}
                              aria-label={t('docs.baseUrlLabel')}
                            />
                          </label>
                          {baseUrlError && (
                            <p id="docs-baseurl-error" role="alert" className="mt-1 text-[10.5px] text-danger">
                              {t('docs.baseUrlInvalid')}
                            </p>
                          )}
                          <div className="mt-2.5 space-y-1.5">
                            <div className="flex items-center justify-between gap-2">
                              <span className="text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3">
                                {t('docs.osForClaude')}
                              </span>
                              <OsToggle
                                active={claudeOs}
                                onChange={setClaudeOs}
                                label={`${t('docs.osTabsLabel')} · ${t('docs.osForClaude')}`}
                              />
                            </div>
                            <div className="flex items-center justify-between gap-2">
                              <span className="text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3">
                                {t('docs.osForCodex')}
                              </span>
                              <OsToggle
                                active={codexOs}
                                onChange={setCodexOs}
                                label={`${t('docs.osTabsLabel')} · ${t('docs.osForCodex')}`}
                              />
                            </div>
                          </div>
                        </div>
                      )}
                    </article>
                  )
                })}
              </div>

              <div id="qs-workbench" data-toc-id="qs-workbench" className="mt-6 scroll-mt-32">
                <h4 className="docs-group-title">{t('docs.workbenchTitle')}</h4>
                <TerminalWindow title={windowTitle} copyText={windowCopyText}>
                  <div className="docs-tabs" role="group" aria-label={t('docs.toolsLabel')}>
                    {TOOL_TABS.map((tab) => (
                      <button
                        key={tab.value}
                        type="button"
                        aria-pressed={activeToolTab === tab.value}
                        onClick={() => setActiveToolTab(tab.value)}
                      >
                        {tab.label}
                      </button>
                    ))}
                  </div>

                  {activeToolTab === 'claude-code' && (
                    <>
                      <CodeBlock
                        label={claudeOs === 'unix' ? t('docs.unixTerminal') : t('docs.windowsTerminal')}
                        content={claudeOs === 'unix' ? claudeEnvUnix : claudeEnvWindows}
                        showCopy={false}
                      />
                      <CodeBlock label="~/.claude/settings.json" content={claudeSettings} />
                    </>
                  )}
                  {activeToolTab === 'codex-cli' && (
                    <>
                      <CodeBlock label="~/.codex/config.toml" content={codexConfig} showCopy={false} />
                      <CodeBlock
                        label={codexOs === 'unix' ? '~/.codex/.env' : '%USERPROFILE%\\.codex\\.env'}
                        content={codexAuth}
                      />
                    </>
                  )}
                  {activeToolTab === 'openai-sdk' && <CodeBlock label="Python" content={openaiSdk} showCopy={false} />}
                </TerminalWindow>
              </div>

              <div className="mt-4 rounded-[11px] border-[0.5px] border-hairline bg-surface p-4 shadow-panel">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <h4 className="text-[13px] font-semibold text-ink">{t('docs.ccSwitchTitle')}</h4>
                    <p className="mt-1 text-[11.5px] leading-relaxed text-ink-2">{t('docs.ccSwitchDesc')}</p>
                  </div>
                  <button
                    type="button"
                    disabled={!selectedKey}
                    onClick={handleCcSwitchImport}
                    className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-[8px] bg-brand px-3 text-[12.5px] font-semibold text-brand-fg transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    <ExternalLink className="size-3.5" />
                    {t('docs.ccSwitchLaunch')}
                  </button>
                </div>
                <div className="mt-3">
                  {ccSwitchLink ? (
                    <CodeBlock label="ccswitch://" content={ccSwitchLink} />
                  ) : (
                    <p className="text-[11.5px] text-ink-3">{t('docs.selectKeyFirst')}</p>
                  )}
                </div>
              </div>

              <div id="qs-curl" data-toc-id="qs-curl" className="mt-6 scroll-mt-32">
                <h4 className="docs-group-title">{t('docs.curlTitle')}</h4>
                <CodeBlock content={curlExample} />
              </div>

              <p className="docs-callout">
                {t('docs.verifyNote')}{' '}
                <a href="#connection-check">{t('docs.verifyLink')}</a>
              </p>
            </>
          )}

          {guides
            .filter((g) => g.section === activeSection)
            .map((g) => (
              <article key={g.id} id={g.id} className="docs-guide scroll-mt-32">
                <div className="docs-guide-heading">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      {g.statusCode !== undefined && (
                        <span className="inline-flex h-[20px] items-center rounded-[5px] bg-danger-soft px-2 font-mono text-[11px] font-bold text-danger">
                          {g.statusCode}
                        </span>
                      )}
                      <h3>
                        <a href={`#${g.id}`}>{g.title}</a>
                      </h3>
                    </div>
                    <p>{g.summary}</p>
                  </div>
                </div>
                <div className="docs-guide-body">
                  {g.paragraphs?.map((paragraph, i) => (
                    <p key={i}>{paragraph}</p>
                  ))}
                  {g.table && (
                    <div className="docs-table-wrap">
                      <table>
                        <thead>
                          <tr>
                            {g.table.headers.map((h) => (
                              <th key={h} scope="col">
                                {h}
                              </th>
                            ))}
                          </tr>
                        </thead>
                        <tbody>
                          {g.table.rows.map((row, i) => (
                            <tr key={i}>
                              {row.map((cell, j) => (
                                <td key={j}>
                                  {j === 0 ? (
                                    <code className="font-mono text-[11.5px] text-ink">{cell}</code>
                                  ) : (
                                    cell
                                  )}
                                </td>
                              ))}
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                  {g.examples?.map((example) => (
                    <CodeBlock key={example.label} label={example.label} content={example.content} />
                  ))}
                </div>
              </article>
            ))}

          {activeSection === 'model-api' && (
            <section>
              <h4 className="docs-group-title">
                {t('docs.endpointsGroup')}
                <span>{endpoints.length}</span>
              </h4>
              <div className="space-y-3">
                {endpoints.map((endpoint) => (
                  <EndpointDoc key={endpoint.id} endpoint={endpoint} activeId={target.id} />
                ))}
              </div>
            </section>
          )}
        </div>

        <aside className="hidden self-start lg:sticky lg:top-[120px] lg:block">
          <DocsTOC items={tocItems} title={t('docs.tocTitle')} activeId={activeTocId} />
        </aside>
      </div>
    </div>
  )
}
