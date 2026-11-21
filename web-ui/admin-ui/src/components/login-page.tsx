// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { storage } from '@/lib/storage'
import { Input } from '@/components/ui/input'
import { Button } from '@/components/ui/button'

interface LoginPageProps {
  onLogin: (apiKey: string) => void
}

/** 字段标签：与 `.panel-title` / `Metric` 标签同款小型大写字 */
const LABEL = 'block text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3'

export function LoginPage({ onLogin }: LoginPageProps) {
  const { t } = useTranslation()
  const [apiKey, setApiKey] = useState('')

  useEffect(() => {
    // 从 storage 读取保存的 API Key
    const savedKey = storage.getApiKey()
    if (savedKey) {
      setApiKey(savedKey)
    }
  }, [])

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (apiKey.trim()) {
      storage.setApiKey(apiKey.trim())
      onLogin(apiKey.trim())
    }
  }

  return (
    // 登录页无设计稿，按已建立的令牌与原语推导：`bg` 页面底 + 氛围极光柔光 + `.panel` 卡片 + `.field` + `.btn-primary`
    <div className="relative grid min-h-screen place-items-center overflow-hidden bg-app-canvas p-4">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute left-1/2 top-1/2 h-[420px] w-[560px] -translate-x-1/2 -translate-y-1/2 rounded-full opacity-25 blur-3xl"
        style={{ background: 'radial-gradient(circle, hsl(var(--primary) / 0.36) 0%, transparent 70%)' }}
      />
      <div className="relative w-full max-w-[368px] rounded-[16px] border border-transparent bg-chat-canvas p-7 shadow-pop">
        {/* 品牌区：徽标与文案沿用侧栏同款 Aurora 徽标与字号比例，放大一档（30→42px） */}
        <div className="flex items-center gap-3.5">
          {/* 须走 BASE_URL 拼接（vite base 只重写 index.html 的 src，不重写 TS 字符串），详见 sidebar.tsx 注释 */}
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
            <div className="text-[18px] font-semibold leading-[1.2] tracking-[-.025em] text-ink">Kiro2CCProxy</div>
            <div className="mt-0.5 text-[11.5px] tracking-[.02em] text-ink-3">{t('dashboard.consoleSubtitle')}</div>
          </div>
        </div>

        <p className="mt-5 text-[12px] leading-[1.6] text-ink-2">{t('login.description')}</p>

        <form onSubmit={handleSubmit} className="mt-4 flex flex-col gap-2">
          <label htmlFor="admin-psw" className={LABEL}>
            {t('settings.adminPassword')}
          </label>
          <Input
            id="admin-psw"
            type="password"
            autoComplete="current-password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            className="h-9 rounded-[8px] text-[12.5px]"
          />
          <Button type="submit" className="mt-2.5 h-9 w-full rounded-[8px]" disabled={!apiKey.trim()}>
            {t('login.loginButton')}
          </Button>
        </form>
      </div>
    </div>
  )
}
