// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { Loader2 } from 'lucide-react'
import { storage } from '@/lib/storage'
import { login } from '@/api/user'
import { Button } from '@/components/ui/button'
import { toast } from 'sonner'
import type { LoginResponse } from '@/types/api'

interface LoginPageProps {
  onLogin: (data: LoginResponse) => void
}

/** 字段标签：与 `.panel-title` / `Metric` 标签同款小型大写字 */
const LABEL = 'block text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3'

export function LoginPage({ onLogin }: LoginPageProps) {
  const [apiKey, setApiKey] = useState('')
  const [loading, setLoading] = useState(false)

  // 从输入中提取 API Key（支持粘贴整段发货信息）
  const extractApiKey = (input: string): string => {
    const match = input.match(/sk-[a-zA-Z0-9]+/)
    return match ? match[0] : input.trim()
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const key = extractApiKey(apiKey)
    if (!key) return

    setLoading(true)
    try {
      const data = await login(key)
      storage.setApiKey(key)
      onLogin(data)
    } catch (err: unknown) {
      const axiosErr = err as { response?: { data?: { error?: string } } }
      const msg = axiosErr.response?.data?.error || '登录失败，请检查 API Key'
      toast.error(msg)
    } finally {
      setLoading(false)
    }
  }

  return (
    // 登录页按已建立的令牌与原语推导：`bg` 页面底 + `.panel` 卡片 + `.field` + `.btn-primary`
    <div className="grid min-h-screen place-items-center bg-background p-4">
      <div className="w-full max-w-[352px] rounded-[11px] border border-hairline bg-surface p-[26px] shadow-pop">
        {/* 品牌区：徽标与文案沿用侧栏同一套渐变与字号比例，放大一档 */}
        <div className="flex items-center gap-3">
          <span
            className="grid size-10 shrink-0 place-items-center rounded-[12px] text-brand-fg shadow-hair"
            style={{ backgroundImage: 'linear-gradient(150deg, var(--brand), var(--brand-deep))' }}
          >
            <svg
              viewBox="0 0 24 24"
              className="size-[22px]"
              fill="none"
              stroke="currentColor"
              strokeWidth={1.9}
              strokeLinecap="round"
              aria-hidden="true"
            >
              <path d="M5 20V7a7 7 0 0 1 14 0v13l-2.3-2-2.4 2-2.3-2-2.3 2-2.4-2z" />
              <path d="M9.5 10h.01M14.5 10h.01" />
            </svg>
          </span>
          <div className="min-w-0">
            <div className="text-[17px] font-semibold leading-[1.2] tracking-[-.02em]">额度用量监控</div>
            <div className="text-[11px] tracking-[.02em] text-ink-3">额度用量监控控制台</div>
          </div>
        </div>

        <p className="mt-[18px] text-[11.5px] leading-[1.55] text-ink-3">
          请输入您的 API Key 或粘贴发货信息查看用量数据
        </p>

        <form onSubmit={handleSubmit} className="mt-4 flex flex-col gap-2">
          <label htmlFor="api-key" className={LABEL}>
            API Key
          </label>
          {/* textarea 支持粘贴整段发货信息，外观对齐 Input 原语规格 */}
          <textarea
            id="api-key"
            placeholder="sk-... 或粘贴发货信息"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            className="flex h-[68px] w-full rounded-[7px] border border-hairline-2 bg-surface-2 px-3 py-2 text-[12.5px] text-ink placeholder:text-ink-3 focus-visible:border-brand focus-visible:outline-none focus-visible:ring-0 disabled:cursor-not-allowed disabled:opacity-50 font-mono resize-none"
            rows={3}
          />
          <Button type="submit" className="mt-2 h-[34px] w-full" disabled={!apiKey.trim() || loading}>
            {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {loading ? '验证中...' : '查看用量'}
          </Button>
        </form>
      </div>
    </div>
  )
}
