// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useCallback, useEffect, useMemo, useState } from 'react'
import { Loader2, PanelLeft } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { readStoredChatSidebarOpen, writeStoredChatSidebarOpen } from '@/components/dashboard/panel-constants'
import { PageHead } from '@/components/page-head'
import { confirmDialog } from '@/components/ui/confirm-dialog'
import { useApiKeys, useChatSessions, useCredentials, usePlaygroundModels, useUiPreferences, useUpdateUiPreferences } from '@/hooks/use-credentials'
import { toast } from '@/lib/toast'
import { extractErrorMessage } from '@/lib/utils'
import type { ChatSessionMeta } from '@/types/api'
import { Composer } from './composer'
import { MessageList } from './message-list'
import { SessionsSidebar } from './sessions-sidebar'
import { normalizeEffort, type Effort } from './types'
import { usePlayground } from './use-playground'

/** 与 Tailwind 的 md 断点一致：窄屏下历史列表以浮层形式出现，默认收起 */
const DESKTOP_QUERY = '(min-width: 768px)'

/** 对话页：用所选子 API Key 调用本代理的 /v1/messages，在线试用各模型与思考力度；会话保存在后端 */
export function PlaygroundPage() {
  const { t } = useTranslation()
  const {
    messages,
    config,
    isGenerating,
    loading,
    activeId,
    viewKey,
    send,
    regenerate,
    stop,
    startNew,
    selectSession,
    deleteSession,
    clearSessions,
    updateConfig,
  } = usePlayground()
  const { data: apiKeys } = useApiKeys()
  const { data: credentials } = useCredentials()
  const { data: sessions, isLoading: sessionsLoading } = useChatSessions()
  const { data: uiPreferences } = useUiPreferences()
  const { mutate: saveUiPreferences } = useUpdateUiPreferences()
  const [sidebarOpen, setSidebarOpen] = useState(() =>
    window.matchMedia(DESKTOP_QUERY).matches
      ? (uiPreferences?.chatSidebarOpen ?? readStoredChatSidebarOpen())
      : false,
  )

  // 桌面端同步服务端持久化的对话侧边栏状态；窄屏始终作为浮层默认收起，不覆盖服务端值
  useEffect(() => {
    if (!uiPreferences) return
    writeStoredChatSidebarOpen(uiPreferences.chatSidebarOpen)
    if (window.matchMedia(DESKTOP_QUERY).matches) {
      setSidebarOpen(uiPreferences.chatSidebarOpen)
    }
  }, [uiPreferences?.chatSidebarOpen])

  useEffect(() => {
    const mql = window.matchMedia(DESKTOP_QUERY)
    const onChange = (e: MediaQueryListEvent) => {
      setSidebarOpen(e.matches ? readStoredChatSidebarOpen() : false)
    }
    mql.addEventListener('change', onChange)
    return () => mql.removeEventListener('change', onChange)
  }, [])

  /**
   * 对话页展示的 API Key：全部保留（含绑定账号已全部禁用的），后者置灰不可选。
   * 未绑定账号的 Key 路由到全部账号；绑定的 Key 只在绑定账号中选择。
   * 账号数据尚未加载时先视为可用，避免启动瞬间把所有 Key 置灰。
   */
  const keyOptions = useMemo(() => {
    const keys = (apiKeys ?? []).filter((k) => k.enabled)
    if (!credentials) return keys.map((k) => ({ ...k, usable: true }))
    const byId = new Map(credentials.credentials.map((c) => [c.id, c]))
    const anyEnabled = credentials.credentials.some((c) => !c.disabled)
    return keys.map((k) => {
      const bound = k.boundCredentialIds ?? []
      const usable =
        bound.length === 0
          ? anyEnabled
          : bound.some((id) => {
              const credential = byId.get(id)
              return credential !== undefined && !credential.disabled
            })
      return { ...k, usable }
    })
  }, [apiKeys, credentials])
  const activeKey =
    keyOptions.find((k) => k.id === config.keyId && k.usable) ??
    keyOptions.find((k) => k.usable) ??
    null
  const {
    models: modelItems,
    isPending: modelsLoading,
    isError: modelsError,
    refetch: refetchModels,
  } = usePlaygroundModels(activeKey)
  const modelIds = modelItems.map((m) => m.id)
  // 已保存的模型不在当前 Key 的列表时回退；只做派生，不回写配置
  const model = modelIds.includes(config.model) ? config.model : (modelIds[0] ?? '')

  const effort: Effort = normalizeEffort(model, config.effort)
  const webSearch = config.webSearch
  const runOptions = { apiKey: activeKey?.key ?? null, model, effort, webSearch }
  const canChat = activeKey !== null && model !== '' && !loading

  const activeTitle = sessions?.find((s) => s.id === activeId)?.title

  const handleRegenerate = useCallback(
    () => {
      if (canChat) regenerate({ apiKey: activeKey?.key ?? null, model, effort, webSearch })
    },
    [canChat, regenerate, activeKey?.key, model, effort, webSearch],
  )

  /** 窄屏下选择完成后收起浮层，露出对话 */
  const closeOverlay = () => {
    if (!window.matchMedia(DESKTOP_QUERY).matches) setSidebarOpen(false)
  }

  const handleSelect = (id: string) => {
    selectSession(id)
    closeOverlay()
  }

  const handleNew = () => {
    startNew()
    closeOverlay()
  }

  const handleDelete = async (session: ChatSessionMeta) => {
    const name = session.title || t('playground.untitled')
    if (await confirmDialog(t('playground.confirmDelete', { title: name }))) await deleteSession(session.id)
  }

  const handleClearAll = async () => {
    if (await confirmDialog(t('playground.confirmClearAll'))) await clearSessions()
  }

  return (
    <div className="flex h-[calc(100dvh-56px)] min-h-[420px] flex-col max-md:h-[calc(100dvh-82px)]">
      <PageHead
        crumb={[t('dashboard.navMain'), t('playground.title')]}
        title={t('playground.title')}
        note={t('playground.headNote')}
      />

      <div className="relative flex min-h-0 flex-1 overflow-hidden rounded-[14px] border-[0.5px] border-hairline bg-surface shadow-hair">
        {/* 窄屏浮层的点击遮罩：淡入淡出，收起时不拦截点击 */}
        <div
          aria-hidden
          onClick={() => setSidebarOpen(false)}
          className={`absolute inset-0 z-10 bg-black/30 transition-opacity duration-300 motion-reduce:transition-none md:hidden ${
            sidebarOpen ? 'opacity-100' : 'pointer-events-none opacity-0'
          }`}
        />
        {/*
          始终挂载，用样式做动画：桌面端宽度 14rem ↔ 0（内层固定宽度，内容不会随之重排），
          窄屏端整体平移滑入 / 滑出。invisible 的 visibility 会在动画结束时才生效，
          收起后既不可见也不会被 Tab 聚焦到。
        */}
        <div
          className={`shrink-0 overflow-hidden transition-[width,transform,visibility] duration-300 ease-out motion-reduce:transition-none max-md:absolute max-md:inset-y-0 max-md:left-0 max-md:z-20 max-md:w-64 ${
            sidebarOpen
              ? 'visible md:w-56 max-md:shadow-panel'
              : 'invisible md:w-0 max-md:-translate-x-full'
          }`}
        >
          <SessionsSidebar
            sessions={sessions}
            isLoading={sessionsLoading}
            activeId={activeId}
            locked={isGenerating}
            onNew={handleNew}
            onSelect={handleSelect}
            onDelete={handleDelete}
            onClearAll={handleClearAll}
          />
        </div>

        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-11 shrink-0 items-center gap-2 border-b-[0.5px] border-hairline px-3">
            <button
              type="button"
              onClick={() => {
                const prev = sidebarOpen
                const next = !prev
                setSidebarOpen(next)
                if (window.matchMedia(DESKTOP_QUERY).matches) {
                  writeStoredChatSidebarOpen(next)
                  saveUiPreferences(
                    { chatSidebarOpen: next },
                    {
                      onError: (err) => {
                        setSidebarOpen(prev)
                        writeStoredChatSidebarOpen(prev)
                        toast.error(t('credentials.toastOpFailed', { message: extractErrorMessage(err) }))
                      },
                    },
                  )
                }
              }}
              title={t('playground.toggleHistory')}
              aria-label={t('playground.toggleHistory')}
              aria-pressed={sidebarOpen}
              className="grid size-8 shrink-0 place-items-center rounded-md text-ink-3 transition-colors hover:bg-surface-3 hover:text-ink aria-pressed:text-ink"
            >
              <PanelLeft className="size-4" />
            </button>
            <span className="min-w-0 flex-1 truncate text-[13px] text-ink-2">
              {activeId ? activeTitle || t('playground.untitled') : t('playground.newChat')}
            </span>
          </div>

          {loading ? (
            <div className="grid min-h-0 flex-1 place-items-center text-ink-3">
              <Loader2 className="size-5 animate-spin" />
            </div>
          ) : (
            <MessageList
              key={viewKey}
              messages={messages}
              isGenerating={isGenerating}
              hasKey={activeKey !== null}
              onRegenerate={handleRegenerate}
            />
          )}

          <div className="mx-auto w-full shrink-0 px-4 pb-4 md:w-[95%] md:px-0">
            {(modelsLoading || modelsError) && (
              <div role="status" className="mb-2 flex items-center gap-2 text-xs text-ink-3">
                <span>{t(modelsLoading ? 'common.loading' : 'common.loadFailed')}</span>
                {modelsError && !modelsLoading && (
                  <button type="button" className="underline" onClick={() => void refetchModels()}>
                    {t('common.retry')}
                  </button>
                )}
              </div>
            )}
            <Composer
              keys={keyOptions}
              keyId={activeKey?.id ?? null}
              models={modelItems}
              modelsLoading={modelsLoading}
              model={model}
              effort={effort}
              webSearch={webSearch}
              isGenerating={isGenerating}
              sendDisabled={!canChat}
              onKeyChange={(value) => updateConfig('keyId', value)}
              onModelChange={(value) => updateConfig('model', value)}
              onEffortChange={(value) => updateConfig('effort', value)}
              onWebSearchChange={(value) => updateConfig('webSearch', value)}
              onSend={(text, attachments) => send(text, runOptions, attachments)}
              onStop={stop}
            />
          </div>
        </div>
      </div>
    </div>
  )
}
