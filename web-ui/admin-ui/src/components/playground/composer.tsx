// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useLayoutEffect, useRef, useState, type ChangeEvent, type KeyboardEvent } from 'react'
import { ArrowUp, Brain, ChevronDown, Globe, Loader2, Plus, Square } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { ModalityIcons } from '@/components/modality-icons'
import { toast } from '@/lib/toast'
import type { ModelItem } from '@/types/api'
import { AttachmentView } from './attachment-view'
import {
  AttachmentError,
  MAX_ATTACHMENTS,
  MAX_TEXT_BYTES,
  formatFileSize,
  processFile,
} from './attachments'
import { effortOptionsFor, thinkingMode, type Attachment, type Effort } from './types'

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
  models: ModelItem[]
  model: string
  effort: Effort
  webSearch: boolean
  isGenerating: boolean
  /** 缺少可用 API Key 等导致无法发送时为 true */
  sendDisabled: boolean
  onKeyChange: (keyId: number) => void
  onModelChange: (model: string) => void
  onEffortChange: (effort: Effort) => void
  onWebSearchChange: (enabled: boolean) => void
  onSend: (text: string, attachments: Attachment[]) => void
  onStop: () => void
}

/** 对话输入框：圆角大容器，底栏左侧 添加附件 / 思考力度 / 联网搜索，右侧 模型 / 发送 */
export function Composer({
  keys,
  keyId,
  models,
  model,
  effort,
  webSearch,
  isGenerating,
  sendDisabled,
  onKeyChange,
  onModelChange,
  onEffortChange,
  onWebSearchChange,
  onSend,
  onStop,
}: ComposerProps) {
  const { t } = useTranslation()
  const [text, setText] = useState('')
  const [attachments, setAttachments] = useState<Attachment[]>([])
  /** 正在读取 / 压缩的文件数；处理完成前不允许发送 */
  const [processing, setProcessing] = useState(0)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const effortOptions = effortOptionsFor(model)
  const budgetMode = thinkingMode(model) === 'budget'
  const canSend =
    (text.trim() !== '' || attachments.length > 0) && processing === 0 && !sendDisabled && !isGenerating
  const attachFull = attachments.length + processing >= MAX_ATTACHMENTS

  // 随内容自动增高，超过上限后改为内部滚动
  useLayoutEffect(() => {
    const el = textareaRef.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, TEXTAREA_MAX_HEIGHT_PX)}px`
  }, [text])

  const submit = () => {
    if (!canSend) return
    onSend(text, attachments)
    setText('')
    setAttachments([])
  }

  const reportAttachmentError = (error: unknown) => {
    if (!(error instanceof AttachmentError)) {
      toast.error(t('playground.attachFailed', { name: '' }))
      return
    }
    if (error.code === 'tooLarge') {
      toast.error(t('playground.attachTooLarge', { name: error.fileName, limit: formatFileSize(MAX_TEXT_BYTES) }))
    } else if (error.code === 'unsupported') {
      toast.error(t('playground.attachUnsupported', { name: error.fileName }))
    } else {
      toast.error(t('playground.attachFailed', { name: error.fileName }))
    }
  }

  const handleFiles = async (event: ChangeEvent<HTMLInputElement>) => {
    const picked = Array.from(event.target.files ?? [])
    event.target.value = '' // 清空后再次选择同一个文件也会触发 change
    const room = Math.max(0, MAX_ATTACHMENTS - attachments.length - processing)
    if (picked.length > room) toast.error(t('playground.attachTooMany', { max: MAX_ATTACHMENTS }))
    const files = picked.slice(0, room)
    if (files.length === 0) return

    setProcessing((n) => n + files.length)
    const results = await Promise.all(
      files.map((file) =>
        processFile(file).catch((error: unknown) => {
          reportAttachmentError(error)
          return null
        }),
      ),
    )
    setAttachments((prev) => [...prev, ...results.filter((a): a is Attachment => a !== null)])
    setProcessing((n) => n - files.length)
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    // isComposing：中文输入法选词时的回车不能当成发送
    if (e.key !== 'Enter' || e.shiftKey || e.nativeEvent.isComposing) return
    e.preventDefault()
    submit()
  }

  // 思考力度档位名在所有语言下都直接显示英文原值（off / low / medium / high / xhigh / max）
  const pillLabel = t('playground.effortPill', { level: effort })
  const hasKeys = keys.length > 0

  return (
    <div className="rounded-[24px] border-[0.5px] border-hairline-2 bg-surface px-4 pb-2.5 pt-3.5 shadow-panel transition-colors focus-within:border-brand">
      {(attachments.length > 0 || processing > 0) && (
        <div className="mb-2 flex flex-wrap gap-2 px-1 pt-1">
          {attachments.map((a) => (
            <AttachmentView
              key={a.id}
              attachment={a}
              onRemove={() => setAttachments((prev) => prev.filter((x) => x.id !== a.id))}
            />
          ))}
          {Array.from({ length: processing }, (_, i) => (
            <div
              key={`pending-${i}`}
              className="grid size-14 place-items-center rounded-lg border-[0.5px] border-hairline-2 bg-surface-2 text-ink-3"
            >
              <Loader2 className="size-4 animate-spin" />
            </div>
          ))}
        </div>
      )}
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
        {/* 不限制 accept：图片之外的文本 / 代码文件扩展名太多，选择后再校验 */}
        <input ref={fileInputRef} type="file" multiple hidden onChange={handleFiles} />
        <button
          type="button"
          onClick={() => fileInputRef.current?.click()}
          disabled={isGenerating || attachFull}
          title={attachFull ? t('playground.attachTooMany', { max: MAX_ATTACHMENTS }) : t('playground.attach')}
          aria-label={t('playground.attach')}
          className="grid size-9 shrink-0 place-items-center rounded-full border-[0.5px] border-hairline-2 text-ink-2 transition-colors hover:bg-surface-3 hover:text-ink disabled:pointer-events-none disabled:opacity-50"
        >
          <Plus className="size-[17px]" />
        </button>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              className={PILL}
              title={budgetMode ? t('playground.effortBudgetTitle') : t('playground.effortTitle')}
            >
              <Brain className="size-4 shrink-0" />
              <span className="truncate">{pillLabel}</span>
              <ChevronDown className="size-3.5 shrink-0 text-ink-3" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent side="top" align="start" className="min-w-[200px]">
            <div className="px-2 pb-1 pt-1.5 text-[10.5px] font-semibold tracking-[.04em] text-ink-3">
              {budgetMode ? t('playground.effortBudgetTitle') : t('playground.effortTitle')}
            </div>
            <DropdownMenuRadioGroup value={effort} onValueChange={(v) => onEffortChange(v as Effort)}>
              {effortOptions.map((value) => (
                <DropdownMenuRadioItem key={value} value={value}>
                  {value}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>

        <button
          type="button"
          onClick={() => onWebSearchChange(!webSearch)}
          aria-pressed={webSearch}
          title={t('playground.webSearchTitle')}
          className={`${PILL} ${webSearch ? 'bg-brand-soft text-ink' : ''}`}
        >
          <Globe className="size-4 shrink-0" />
          {/* 窄屏只留图标，避免底栏挤不下 */}
          <span className="truncate max-sm:hidden">{t('playground.webSearch')}</span>
        </button>

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
                {models.map((m) => (
                  <DropdownMenuRadioItem key={m.id} value={m.id}>
                    <span className="min-w-0 flex-1 truncate">{m.id}</span>
                    {/* 与「支持模型」弹窗同口径：展示是否支持识别图片 */}
                    <span className="ml-2 flex shrink-0 items-center">
                      <ModalityIcons inputTypes={m.input_types} />
                    </span>
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
