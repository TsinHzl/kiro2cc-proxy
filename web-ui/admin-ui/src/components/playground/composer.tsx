// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react'
import { ArrowUp, Brain, ChevronDown, Plus, Square } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { EFFORT_OPTIONS, modelSupportsEffort, type Effort } from './types'

const TEXTAREA_MAX_HEIGHT_PX = 220

/** 底栏胶囊按钮（模型 / 思考力度选择器共用） */
const PILL =
  'inline-flex h-9 min-w-0 items-center gap-1.5 rounded-full px-3 text-[12.5px] font-medium text-ink-2 transition-colors ' +
  'hover:bg-surface-3 hover:text-ink focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand ' +
  'disabled:pointer-events-none disabled:opacity-50 data-[state=open]:bg-surface-3 data-[state=open]:text-ink'

/** 可选 API Key（仅展示用字段） */
export interface KeyOption {
  id: number
  name: string
}

interface ComposerProps {
  keys: KeyOption[]
  keyId: number | null
  models: string[]
  model: string
  effort: Effort
  isGenerating: boolean
  /** 缺少可用 API Key 等导致无法发送时为 true */
  sendDisabled: boolean
  hasMessages: boolean
  onKeyChange: (keyId: number) => void
  onModelChange: (model: string) => void
  onEffortChange: (effort: Effort) => void
  onSend: (text: string) => void
  onStop: () => void
  onNewChat: () => void
}

/** 对话输入框：圆角大容器，底栏左侧 新对话 / 思考力度，右侧 模型 / 发送 */
export function Composer({
  keys,
  keyId,
  models,
  model,
  effort,
  isGenerating,
  sendDisabled,
  hasMessages,
  onKeyChange,
  onModelChange,
  onEffortChange,
  onSend,
  onStop,
  onNewChat,
}: ComposerProps) {
  const { t } = useTranslation()
  const [text, setText] = useState('')
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const effortSupported = modelSupportsEffort(model)
  const canSend = text.trim() !== '' && !sendDisabled && !isGenerating

  // 随内容自动增高，超过上限后改为内部滚动
  useLayoutEffect(() => {
    const el = textareaRef.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, TEXTAREA_MAX_HEIGHT_PX)}px`
  }, [text])

  const submit = () => {
    if (!canSend) return
    onSend(text)
    setText('')
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // isComposing：中文输入法选词时的回车不能当成发送
    if (e.key !== 'Enter' || e.shiftKey || e.nativeEvent.isComposing) return
    e.preventDefault()
    submit()
  }

  // 思考力度档位名在所有语言下都直接显示英文原值（off / low / medium / high / xhigh / max）
  const pillLabel = t('playground.effortPill', { level: effortSupported ? effort : 'off' })
  const hasKeys = keys.length > 0

  return (
    <div className="rounded-[24px] border-[0.5px] border-hairline-2 bg-surface px-4 pb-2.5 pt-3.5 shadow-panel transition-colors focus-within:border-brand">
      <textarea
        ref={textareaRef}
        value={text}
        rows={2}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={t('playground.placeholder')}
        autoComplete="off"
        spellCheck={false}
        className="block max-h-[220px] min-h-[52px] w-full resize-none bg-transparent px-1 text-[14px] leading-7 text-ink outline-none placeholder:text-ink-3"
      />

      <div className="mt-2 flex items-center gap-1.5">
        <button
          type="button"
          onClick={onNewChat}
          disabled={!hasMessages || isGenerating}
          title={t('playground.newChat')}
          aria-label={t('playground.newChat')}
          className="grid size-9 shrink-0 place-items-center rounded-full border-[0.5px] border-hairline-2 text-ink-2 transition-colors hover:bg-surface-3 hover:text-ink disabled:pointer-events-none disabled:opacity-50"
        >
          <Plus className="size-[17px]" />
        </button>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              className={PILL}
              disabled={!effortSupported}
              title={effortSupported ? t('playground.effortTitle') : t('playground.effortUnsupported')}
            >
              <Brain className="size-4 shrink-0" />
              <span className="truncate">{pillLabel}</span>
              <ChevronDown className="size-3.5 shrink-0 text-ink-3" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent side="top" align="start" className="min-w-[200px]">
            <div className="px-2 pb-1 pt-1.5 text-[10.5px] font-semibold tracking-[.04em] text-ink-3">
              {t('playground.effortTitle')}
            </div>
            <DropdownMenuRadioGroup value={effort} onValueChange={(v) => onEffortChange(v as Effort)}>
              {EFFORT_OPTIONS.map((value) => (
                <DropdownMenuRadioItem key={value} value={value}>
                  {value}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>

        <div className="ml-auto flex min-w-0 items-center gap-1.5">
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                className={`${PILL} max-w-[260px]`}
                // 没有可用 API Key 时无法选择模型
                disabled={!hasKeys}
                title={hasKeys ? undefined : t('playground.noKeyHint')}
              >
                <span className="truncate">
                  {!hasKeys ? t('playground.noKey') : model || t('playground.noModel')}
                </span>
                <ChevronDown className="size-3.5 shrink-0 text-ink-3" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent side="top" align="end" className="max-h-[min(420px,60vh)] min-w-[220px] overflow-y-auto">
              <div className="px-2 pb-1 pt-1.5 text-[10.5px] font-semibold tracking-[.04em] text-ink-3">
                {t('playground.keyTitle')}
              </div>
              <DropdownMenuRadioGroup
                value={keyId === null ? '' : String(keyId)}
                onValueChange={(v) => onKeyChange(Number(v))}
              >
                {keys.map((k) => (
                  <DropdownMenuRadioItem key={k.id} value={String(k.id)}>
                    <span className="truncate">{k.name}</span>
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
              <DropdownMenuSeparator />
              <div className="px-2 pb-1 pt-1.5 text-[10.5px] font-semibold tracking-[.04em] text-ink-3">
                {t('playground.modelTitle')}
              </div>
              {models.length === 0 && (
                <div className="px-2 py-1.5 text-[12.5px] text-ink-3">{t('playground.noModel')}</div>
              )}
              <DropdownMenuRadioGroup value={model} onValueChange={onModelChange}>
                {models.map((id) => (
                  <DropdownMenuRadioItem key={id} value={id}>
                    <span className="truncate">{id}</span>
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>

          {isGenerating ? (
            <button
              type="button"
              onClick={onStop}
              title={t('playground.stop')}
              aria-label={t('playground.stop')}
              className="grid size-9 shrink-0 place-items-center rounded-full bg-ink text-surface transition-opacity hover:opacity-85"
            >
              <Square className="size-3.5 fill-current" />
            </button>
          ) : (
            <button
              type="button"
              onClick={submit}
              disabled={!canSend}
              title={t('playground.send')}
              aria-label={t('playground.send')}
              className="grid size-9 shrink-0 place-items-center rounded-full bg-brand text-brand-fg transition-colors hover:bg-brand-hover disabled:cursor-not-allowed disabled:bg-surface-3 disabled:text-ink-3"
            >
              <ArrowUp className="size-[18px]" />
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
