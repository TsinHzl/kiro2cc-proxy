// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useCallback, useMemo, useState } from 'react'
import { Loader2, PanelLeft } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { PageHead } from '@/components/page-head'
import { confirmDialog } from '@/components/ui/confirm-dialog'
import { useApiKeys, useChatSessions, useModels } from '@/hooks/use-credentials'
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
  const { data: modelsData } = useModels()
  const { data: apiKeys } = useApiKeys()
  const { data: sessions, isLoading: sessionsLoading } = useChatSessions()
  const [sidebarOpen, setSidebarOpen] = useState(() => window.matchMedia(DESKTOP_QUERY).matches)

  const modelIds = useMemo(() => (modelsData?.data ?? []).map((m) => m.id), [modelsData])
  // 已保存的模型不在当前列表（被下线 / 首次使用）时，回退到列表第一项；只做派生，不回写配置
  const model = modelIds.includes(config.model) ? config.model : (modelIds[0] ?? '')

  const enabledKeys = useMemo(() => (apiKeys ?? []).filter((k) => k.enabled), [apiKeys])
  const activeKey = enabledKeys.find((k) => k.id === config.keyId) ?? enabledKeys[0] ?? null

  const effort: Effort = normalizeEffort(model, config.effort)
  const webSearch = config.webSearch
  const runOptions = { apiKey: activeKey?.key ?? null, model, effort, webSearch }
  const canChat = activeKey !== null && model !== '' && !loading

  const activeTitle = sessions?.find((s) => s.id === activeId)?.title

  const handleRegenerate = useCallback(
    () => regenerate({ apiKey: activeKey?.key ?? null, model, effort, webSearch }),
    [regenerate, activeKey?.key, model, effort, webSearch],
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
          始终挂载，用样式做动画：桌面端宽度 16rem ↔ 0（内层固定宽度，内容不会随之重排），
          窄屏端整体平移滑入 / 滑出。invisible 的 visibility 会在动画结束时才生效，
          收起后既不可见也不会被 Tab 聚焦到。
        */}
        <div
          className={`shrink-0 overflow-hidden transition-[width,transform,visibility] duration-300 ease-out motion-reduce:transition-none max-md:absolute max-md:inset-y-0 max-md:left-0 max-md:z-20 max-md:w-64 ${
            sidebarOpen
              ? 'visible md:w-64 max-md:shadow-panel'
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
              onClick={() => setSidebarOpen((open) => !open)}
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
            <Composer
              keys={enabledKeys}
              keyId={activeKey?.id ?? null}
              models={modelIds}
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
