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
    // 登录页按已建立的令牌与原语推导：`bg` 页面底 + 氛围极光柔光 + `.panel` 卡片 + `.field` + `.btn-primary`
    <div className="relative grid min-h-screen place-items-center overflow-hidden bg-background p-4">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute left-1/2 top-1/2 h-[420px] w-[560px] -translate-x-1/2 -translate-y-1/2 rounded-full opacity-25 blur-3xl"
        style={{ background: 'radial-gradient(circle, hsl(var(--primary) / 0.36) 0%, transparent 70%)' }}
      />
      <div className="relative w-full max-w-[380px] rounded-[16px] border border-transparent bg-chat-canvas p-7 shadow-pop">
        {/* 品牌区：与 admin-ui 统一的 Aurora 徽标 */}
        <div className="flex items-center gap-3.5">
          <img
            src={`${import.meta.env.BASE_URL}logo-aurora-dark.svg`}
            alt="Kiro2CCProxy"
            className="hidden size-[42px] shrink-0 rounded-[10px] shadow-hair dark:block"
          />
          <img
            src={`${import.meta.env.BASE_URL}logo-aurora-light.svg`}
            alt="Kiro2CCProxy"
            className="size-[42px] shrink-0 rounded-[10px] shadow-hair dark:hidden"
          />
          <div className="min-w-0">
            <div className="text-[18px] font-semibold leading-[1.2] tracking-[-.025em] text-ink">额度用量监控</div>
            <div className="mt-0.5 text-[11.5px] tracking-[.02em] text-ink-3">额度用量监控控制台</div>
          </div>
        </div>

        <p className="mt-5 text-[12px] leading-[1.6] text-ink-2">
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
            className="flex h-[74px] w-full resize-none rounded-[8px] border-[0.5px] border-hairline-2 bg-surface-2 px-3 py-2.5 font-mono text-[12.5px] text-ink transition-shadow placeholder:text-ink-3 focus-visible:border-brand focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-soft disabled:cursor-not-allowed disabled:opacity-50"
            rows={3}
          />
          <Button type="submit" className="mt-2.5 h-9 w-full rounded-[8px]" disabled={!apiKey.trim() || loading}>
            {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
            {loading ? '验证中...' : '查看用量'}
          </Button>
        </form>
      </div>
    </div>
  )
}
