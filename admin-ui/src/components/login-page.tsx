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
    // 登录页无设计稿，按已建立的令牌与原语推导：`bg` 页面底 + `.panel` 卡片 + `.field` + `.btn-primary`
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
            <div className="text-[17px] font-semibold leading-[1.2] tracking-[-.02em]">Kiro2CCProxy</div>
            <div className="text-[11px] tracking-[.02em] text-ink-3">{t('dashboard.consoleSubtitle')}</div>
          </div>
        </div>

        <p className="mt-[18px] text-[11.5px] leading-[1.55] text-ink-3">{t('login.description')}</p>

        <form onSubmit={handleSubmit} className="mt-4 flex flex-col gap-2">
          <label htmlFor="admin-psw" className={LABEL}>
            {t('settings.adminPassword')}
          </label>
          <Input
            id="admin-psw"
            type="password"
            autoFocus
            autoComplete="current-password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            className="h-[34px] text-[12.5px]"
          />
          <Button type="submit" className="mt-2 h-[34px] w-full" disabled={!apiKey.trim()}>
            {t('login.loginButton')}
          </Button>
        </form>
      </div>
    </div>
  )
}
