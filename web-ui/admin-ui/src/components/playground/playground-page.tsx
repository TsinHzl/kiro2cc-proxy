// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useCallback, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { PageHead } from '@/components/page-head'
import { confirmDialog } from '@/components/ui/confirm-dialog'
import { useApiKeys, useModels } from '@/hooks/use-credentials'
import { Composer } from './composer'
import { MessageList } from './message-list'
import { modelSupportsEffort, type Effort } from './types'
import { usePlayground } from './use-playground'

/** 对话页：用所选子 API Key 调用本代理的 /v1/messages，在线试用各模型与思考力度 */
export function PlaygroundPage() {
  const { t } = useTranslation()
  const { messages, config, isGenerating, send, regenerate, stop, clear, updateConfig } = usePlayground()
  const { data: modelsData } = useModels()
  const { data: apiKeys } = useApiKeys()

  const modelIds = useMemo(() => (modelsData?.data ?? []).map((m) => m.id), [modelsData])
  // 已保存的模型不在当前列表（被下线 / 首次使用）时，回退到列表第一项；只做派生，不回写配置
  const model = modelIds.includes(config.model) ? config.model : (modelIds[0] ?? '')

  const enabledKeys = useMemo(() => (apiKeys ?? []).filter((k) => k.enabled), [apiKeys])
  const activeKey = enabledKeys.find((k) => k.id === config.keyId) ?? enabledKeys[0] ?? null

  const effort: Effort = modelSupportsEffort(model) ? config.effort : 'off'
  const runOptions = { apiKey: activeKey?.key ?? null, model, effort }
  const canChat = activeKey !== null && model !== ''

  const handleRegenerate = useCallback(
    () => regenerate({ apiKey: activeKey?.key ?? null, model, effort }),
    [regenerate, activeKey?.key, model, effort],
  )

  const handleNewChat = async () => {
    if (messages.length === 0) return
    if (await confirmDialog(t('playground.confirmNewChat'))) clear()
  }

  return (
    <div className="flex h-[calc(100dvh-56px)] min-h-[420px] flex-col max-md:h-[calc(100dvh-82px)]">
      <PageHead
        crumb={[t('dashboard.navMain'), t('playground.title')]}
        title={t('playground.title')}
        note={t('playground.headNote')}
      />

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-[14px] border-[0.5px] border-hairline bg-surface shadow-hair">
        <MessageList
          messages={messages}
          isGenerating={isGenerating}
          hasKey={activeKey !== null}
          onRegenerate={handleRegenerate}
        />
        <div className="mx-auto w-full max-w-4xl shrink-0 px-4 pb-4">
          <Composer
            keys={enabledKeys}
            keyId={activeKey?.id ?? null}
            models={modelIds}
            model={model}
            effort={config.effort}
            isGenerating={isGenerating}
            sendDisabled={!canChat}
            hasMessages={messages.length > 0}
            onKeyChange={(value) => updateConfig('keyId', value)}
            onModelChange={(value) => updateConfig('model', value)}
            onEffortChange={(value) => updateConfig('effort', value)}
            onSend={(text) => send(text, runOptions)}
            onStop={stop}
            onNewChat={handleNewChat}
          />
        </div>
      </div>
    </div>
  )
}
