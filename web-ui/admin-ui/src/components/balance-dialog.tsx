// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Progress, QuotaPercentBadge, quotaTone } from '@/components/ui/progress'
import { useCredentialBalance } from '@/hooks/use-credentials'
import { ACCOUNT_STATE_VISUAL, deriveAccountState, loginSource } from '@/lib/account-state'
import { parseError, getSubscriptionColor } from '@/lib/utils'
import { copyToClipboard } from '@/lib/clipboard'
import { localeTag } from '@/lib/locale'
import type { CredentialStatusItem } from '@/types/api'

interface BalanceDialogProps {
  credentialId: number | null
  /** 当前账号状态项；列表未加载完 / 账号已删除时为 null，此时账号信息区块渲染占位态 */
  credential: CredentialStatusItem | null
  open: boolean
  onOpenChange: (open: boolean) => void
}

/** 卡片内单行字段：小号灰标签在上、值在下 */
function InfoRow({ label, value, title }: { label: string; value: string; title?: string }) {
  return (
    <div>
      <div className="text-[10.5px] text-ink-3">{label}</div>
      <div className="truncate text-[12px] font-medium text-ink" title={title ?? value}>
        {value}
      </div>
    </div>
  )
}

/** 卡片头部：全大写小标题 + 装饰图标 */
function CardHeader({ label, icon }: { label: string; icon: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between text-[11px] font-semibold uppercase tracking-wider text-ink-3">
      <span>{label}</span>
      <span className="opacity-60">{icon}</span>
    </div>
  )
}

/** 一键复制按钮：成功后 1.5s 内显示对勾，失败静默降级 */
function CopyButton({ text, title }: { text: string; title: string }) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  // 卸载时清理定时器，避免组件已卸载后仍触发 setState
  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [])

  const handleCopy = async (e: React.MouseEvent) => {
    e.stopPropagation()
    try {
      await copyToClipboard(text)
      setCopied(true)
      if (timerRef.current) clearTimeout(timerRef.current)
      timerRef.current = setTimeout(() => setCopied(false), 1500)
    } catch {
      // 复制失败静默降级，不打断用户
    }
  }

  return (
    <button
      type="button"
      onClick={handleCopy}
      title={copied ? t('credentials.copied') : title}
      className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded text-ink-3 transition-colors hover:bg-surface-3 hover:text-brand"
    >
      {copied ? (
        <svg className="h-3 w-3 text-ok" viewBox="0 0 20 20" fill="currentColor">
          <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
        </svg>
      ) : (
        <svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
        </svg>
      )}
    </button>
  )
}

export function BalanceDialog({ credentialId, credential, open, onOpenChange }: BalanceDialogProps) {
  const { t } = useTranslation()
  const { data: balance, isLoading, error } = useCredentialBalance(credentialId)

  const formatTimestamp = (timestamp: number | null) => {
    if (!timestamp) return t('credentials.unknown')
    return new Date(timestamp * 1000).toLocaleString(localeTag())
  }

  const formatNumber = (num: number) => {
    return num.toLocaleString(localeTag(), { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  }

  /** RFC3339 → 本地时间；解析失败原样返回，避免出现 Invalid Date */
  const formatIso = (iso: string | null | undefined) => {
    if (!iso) return t('credentials.unknown')
    const d = new Date(iso)
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleString(localeTag())
  }

  // 登录来源：Google / GitHub 由 provider 声明，BuilderId 由 authMethod 推断，企业 IdP 展示 provider 原值
  const source = credential ? loginSource(credential) : null
  const loginSourceText = source === 'Idp'
    ? credential?.provider || t('credentials.unknown')
    : source
      ? t(`credentials.loginSource_${source}`, { defaultValue: source })
      : t('credentials.loginSourceUnknown')

  // 认证方式：后端已归一化为 social / idc / external_idp 三值
  const authMethodText = credential?.authMethod
    ? t(`credentials.authMethod_${credential.authMethod}`, { defaultValue: credential.authMethod })
    : t('credentials.unknown')

  const stateVisual = credential ? ACCOUNT_STATE_VISUAL[deriveAccountState(credential, balance != null)] : null

  // 百分比做一次有限性 + 区间收敛，避免后端异常值让进度条越界
  const safeUsagePercentage = balance && Number.isFinite(balance.usagePercentage) ? balance.usagePercentage : 0
  const clampedProgressValue = Math.min(100, Math.max(0, safeUsagePercentage))
  // 剩余额度色阶复用项目统一的 quotaTone（按剩余百分比 60/30/15 分四档），与表格进度条同源
  const remainingTone = quotaTone(Math.max(0, 100 - clampedProgressValue))

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-[536px] p-0 gap-0 sm:p-0 rounded-2xl border-hairline bg-surface">
        {/* 阶梯一：顶栏资产标识与全局状态 */}
        <DialogHeader className="flex flex-col gap-1 border-b border-hairline px-[22px] pt-5 pb-4 text-left">
          {/* pr-6 为右上角关闭按钮让位，避免标题与其重叠 */}
          <div className="flex flex-wrap items-center gap-2 pr-6">
            <DialogTitle className="text-[17px] font-semibold tracking-tight text-ink">
              {t('credentials.balanceDialogTitle', { id: credentialId })}
            </DialogTitle>
            {stateVisual && (
              <span className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-[11px] font-medium ${stateVisual.tagClass}`}>
                <span className={`h-1.5 w-1.5 rounded-full ${stateVisual.pipClass.split(' ')[0]}`} />
                {t(stateVisual.labelKey)}
              </span>
            )}
          </div>
          {credential && (
            <div className="truncate text-[12px] text-ink-3">
              {credential.email || t('credentials.unknown')}
              <span className="mx-1.5 opacity-40">·</span>
              {t('credentials.priorityFieldLabel')} {credential.priority}
            </div>
          )}
        </DialogHeader>

        <div className="space-y-4 px-[22px] pt-[18px] pb-[22px]">
          {/* 加载态：余额查询中，Hero 与错误卡片均不渲染，但账号信息卡片照常显示 */}
          {isLoading && (
            <div className="flex items-center justify-center py-10">
              <div className="h-8 w-8 animate-spin rounded-full border-2 border-brand border-t-transparent" />
            </div>
          )}

          {/* 错误态 */}
          {error && (() => {
            const parsed = parseError(error)
            return (
              <div className="space-y-1.5 rounded-xl border border-danger-line bg-danger-soft p-4 text-center">
                <div className="flex items-center justify-center gap-1.5 text-[13px] font-medium text-danger">
                  <svg className="h-4 w-4" viewBox="0 0 20 20" fill="currentColor">
                    <path fillRule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clipRule="evenodd" />
                  </svg>
                  <span>{parsed.title}</span>
                </div>
                {parsed.detail && (
                  <p className="text-[11.5px] leading-relaxed text-ink-3">
                    {parsed.detail}
                  </p>
                )}
              </div>
            )
          })()}

          {/* 阶梯二：核心额度看板 */}
          {balance && (
            <div className="space-y-3.5 rounded-xl border border-hairline bg-surface-2 p-4 px-[18px]">
              <div className="flex items-start justify-between gap-2">
                <div className="flex min-w-0 items-center gap-1.5">
                  <svg className="h-4 w-4 shrink-0 text-brand" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
                  </svg>
                  <span className={`truncate text-[14px] font-bold tracking-wide ${balance.subscriptionTitle ? getSubscriptionColor(balance.subscriptionTitle) : 'text-brand'}`}>
                    {balance.subscriptionTitle || t('credentials.unknownSubscription')}
                  </span>
                </div>

                {/* 剩余额度为最关注指标：标签置于金额上方，色阶取 quotaTone（CSS var 需走 style） */}
                <div className="shrink-0 max-w-[55%] text-right">
                  <div className="mb-0.5 truncate text-[11px] text-ink-3">
                    {t('credentials.remainingQuotaLabel')}
                  </div>
                  <div
                    className="font-mono text-[20px] font-bold leading-none tracking-tight"
                    style={{ color: remainingTone.badgeText }}
                  >
                    ${formatNumber(balance.remaining)}
                  </div>
                </div>
              </div>

              {/* 进度条 + 用量行：百分比只在此处出现一次，避免与上方文案重复 */}
              <div className="space-y-1.5">
                <Progress value={clampedProgressValue} className="h-2 rounded-full" />
                <div className="flex items-center justify-between text-[11.5px]">
                  <div className="flex items-center gap-1.5 text-ink-2">
                    <span>{t('credentials.usedLabel', { amount: formatNumber(balance.currentUsage) })}</span>
                    <QuotaPercentBadge percent={safeUsagePercentage} />
                  </div>
                  <div className="font-mono text-ink-3">
                    {t('credentials.limitLabel', { amount: formatNumber(balance.usageLimit) })}
                  </div>
                </div>
              </div>

              {/* 重置周期：虚线分隔行，标签与时间紧挨并整体左对齐 */}
              <div className="flex items-center gap-1 border-t border-dashed border-hairline pt-2.5 text-[11px] text-ink-3">
                <span className="flex items-center gap-1">
                  <svg className="h-3.5 w-3.5 opacity-70" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <rect x="3" y="4" width="18" height="18" rx="2" ry="2" />
                    <line x1="16" y1="2" x2="16" y2="6" />
                    <line x1="8" y1="2" x2="8" y2="6" />
                    <line x1="3" y1="10" x2="21" y2="10" />
                  </svg>
                  {t('credentials.nextResetLabel')}
                </span>
                <span className="font-mono font-medium text-ink-2">
                  {formatTimestamp(balance.nextResetAt)}
                </span>
              </div>
            </div>
          )}

          {/* 阶梯三：模块化双栏卡片网格 */}
          {credential && (
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              {/* 卡片 A：核心身份标识 */}
              <div className="space-y-2.5 rounded-lg border border-hairline bg-surface-2 p-3 px-3.5">
                <CardHeader
                  label={t('credentials.infoGroupIdentity')}
                  icon={
                    <svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
                      <circle cx="12" cy="7" r="4" />
                    </svg>
                  }
                />

                <div className="space-y-2">
                  <div>
                    <div className="text-[10.5px] text-ink-3">{t('credentials.emailLabel')}</div>
                    <div className="flex items-center gap-1.5">
                      <span className="truncate text-[12px] font-medium text-ink" title={credential.email || ''}>
                        {credential.email || t('credentials.unknown')}
                      </span>
                      {credential.email && (
                        <CopyButton text={credential.email} title={t('credentials.copyEmail')} />
                      )}
                    </div>
                  </div>

                  <InfoRow label={t('credentials.nicknameLabel')} value={credential.nickname || t('credentials.unknown')} />

                  <div>
                    <div className="text-[10.5px] text-ink-3">{t('credentials.priorityFieldLabel')}</div>
                    <div className="font-mono text-[12px] font-medium text-ink">{credential.priority}</div>
                  </div>
                </div>
              </div>

              {/* 卡片 B：认证与路由（保持三行结构） */}
              <div className="space-y-2.5 rounded-lg border border-hairline bg-surface-2 p-3 px-3.5">
                <CardHeader
                  label={t('credentials.infoGroupAuth')}
                  icon={
                    <svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <circle cx="12" cy="12" r="10" />
                      <line x1="2" y1="12" x2="22" y2="12" />
                      <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
                    </svg>
                  }
                />

                <div className="space-y-2">
                  <InfoRow
                    label={t('credentials.loginSourceLabel')}
                    value={loginSourceText}
                    title={source ? loginSourceText : t('credentials.loginSourceUnknownHint')}
                  />

                  <InfoRow label={t('credentials.authMethodLabel')} value={authMethodText} />

                  <div>
                    <div className="text-[10.5px] text-ink-3">{t('credentials.regionCombinedLabel')}</div>
                    <div className="truncate font-mono text-[12px] font-medium text-ink">
                      {credential.authRegion || t('credentials.regionFallbackGlobal')}
                      <span className="mx-1 text-ink-3">/</span>
                      {credential.apiRegion || t('credentials.regionFallbackGlobal')}
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* 阶梯四：运行健康与监控 */}
          {credential && (
            <div className="space-y-3 rounded-lg border border-hairline bg-surface-2 p-3.5">
              <CardHeader
                label={`${t('credentials.infoGroupHealth')} & ${t('credentials.infoGroupRuntime')}`}
                icon={
                  <svg className="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
                  </svg>
                }
              />

              {/* 状态徽章群 */}
              <div className="flex flex-wrap items-center gap-2">
                <span className={`inline-flex items-center gap-1 rounded border px-2 py-0.5 text-[11px] font-medium ${
                  credential.disabled
                    ? 'border-danger-line bg-danger-soft text-danger'
                    : 'border-ok-line bg-ok-soft text-ok'
                }`}>
                  <span className={`h-1.5 w-1.5 rounded-full ${credential.disabled ? 'bg-danger' : 'bg-ok'}`} />
                  {credential.disabled
                    ? (credential.disabledReason === 'quota_exceeded'
                        ? t('credentials.stateQuotaExceeded')
                        : t('credentials.stateDisabled'))
                    : t('credentials.stateEnabled')}
                </span>

                {/* 代理徽章三态：优先展示地址；仅有代理配置（地址缺失）显示「已配置」，否则「未配置」 */}
                <span className="inline-flex max-w-full items-center rounded border border-hairline-2 bg-surface-3 px-2 py-0.5 text-[11px] font-medium text-ink-2">
                  <span className="truncate">
                    {credential.proxyUrl
                      ? `${t('credentials.proxyBadgeLabel')}: ${credential.proxyUrl}`
                      : credential.hasProxy
                        ? t('credentials.proxyConfigured')
                        : t('credentials.proxyNone')}
                  </span>
                </span>

                <span className="inline-flex items-center rounded border border-hairline-2 bg-surface-3 px-2 py-0.5 text-[11px] font-medium text-ink-2">
                  {t('credentials.adaptiveLabel')}: {credential.thinkingAdaptive ? t('credentials.switchOn') : t('credentials.switchOff')}
                </span>
              </div>

              {/* 关键调用指标矩阵 */}
              <div className="grid grid-cols-3 divide-x divide-hairline rounded-lg border border-hairline bg-surface p-2 px-2.5 text-center">
                <div>
                  <div className="text-[10px] text-ink-3">{t('credentials.successCountLabel')}</div>
                  <div className="font-mono text-[13px] font-semibold text-ok">{credential.successCount}</div>
                </div>
                <div>
                  <div className="text-[10px] text-ink-3">{t('credentials.failureCountLabel')}</div>
                  <div className={`font-mono text-[13px] font-semibold ${credential.failureCount > 0 ? 'text-danger' : 'text-ink'}`}>
                    {credential.failureCount}
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-ink-3">{t('credentials.throttleCountLabel')}</div>
                  <div className={`font-mono text-[13px] font-semibold ${credential.throttleCount > 0 ? 'text-warn' : 'text-ink'}`}>
                    {credential.throttleCount}
                  </div>
                </div>
              </div>

              {/* 时间戳页脚 */}
              <div className="truncate pt-1 text-[11px] text-ink-3">
                <span>{t('credentials.lastUsedAtLabel')}: </span>
                <span className="font-mono text-ink-2">{formatIso(credential.lastUsedAt)}</span>
              </div>
            </div>
          )}

          {/* 账号列表尚未加载完成时的占位态 */}
          {!credential && (
            <div className="py-6 text-center text-[12px] text-ink-3">
              {t('credentials.accountInfoLoading')}
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
