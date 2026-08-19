// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from 'sonner'
import { useTranslation } from 'react-i18next'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useAddCredential } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'

interface AddCredentialDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

type AuthMethod = 'social' | 'idc'

/** 对话框字段标签：字号对齐设计稿体系；中文标签不套 uppercase */
const LABEL = 'text-[11.5px] font-medium text-ink-2'

export function AddCredentialDialog({ open, onOpenChange }: AddCredentialDialogProps) {
  const { t } = useTranslation()
  const [refreshToken, setRefreshToken] = useState('')
  const [email, setEmail] = useState('')
  const [authMethod, setAuthMethod] = useState<AuthMethod>('social')
  const [authRegion, setAuthRegion] = useState('')
  const [apiRegion, setApiRegion] = useState('')
  const [clientId, setClientId] = useState('')
  const [clientSecret, setClientSecret] = useState('')
  const [profileArn, setProfileArn] = useState('')
  const [priority, setPriority] = useState('0')
  const [machineId, setMachineId] = useState('')
  const [proxyUrl, setProxyUrl] = useState('')
  const [proxyUsername, setProxyUsername] = useState('')
  const [proxyPassword, setProxyPassword] = useState('')

  const { mutate, isPending } = useAddCredential()

  const resetForm = () => {
    setRefreshToken('')
    setEmail('')
    setAuthMethod('social')
    setAuthRegion('')
    setApiRegion('')
    setClientId('')
    setClientSecret('')
    setProfileArn('')
    setPriority('0')
    setMachineId('')
    setProxyUrl('')
    setProxyUsername('')
    setProxyPassword('')
  }

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()

    // 验证必填字段
    if (!refreshToken.trim()) {
      toast.error(t('credentials.toastRefreshTokenRequired'))
      return
    }

    // IdC/Builder-ID/IAM 需要额外字段
    if (authMethod === 'idc' && (!clientId.trim() || !clientSecret.trim())) {
      toast.error(t('credentials.toastIdcFieldsRequired'))
      return
    }

    mutate(
      {
        refreshToken: refreshToken.trim(),
        authMethod,
        email: email.trim() || undefined,
        authRegion: authRegion.trim() || undefined,
        apiRegion: apiRegion.trim() || undefined,
        clientId: clientId.trim() || undefined,
        clientSecret: clientSecret.trim() || undefined,
        profileArn: profileArn.trim() || undefined,
        priority: parseInt(priority) || 0,
        machineId: machineId.trim() || undefined,
        proxyUrl: proxyUrl.trim() || undefined,
        proxyUsername: proxyUsername.trim() || undefined,
        proxyPassword: proxyPassword.trim() || undefined,
      },
      {
        onSuccess: (data) => {
          toast.success(data.message)
          onOpenChange(false)
          resetForm()
        },
        onError: (error: unknown) => {
          toast.error(t('credentials.toastAddFailed', { message: extractErrorMessage(error) }))
        },
      }
    )
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('credentials.addDialogTitle')}</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex flex-col min-h-0 flex-1">
          <div className="space-y-4 py-4 overflow-y-auto flex-1 pr-1">
            {/* Refresh Token */}
            <div className="space-y-2">
              <label htmlFor="refreshToken" className={LABEL}>
                {t('credentials.refreshTokenLabel')} <span className="text-danger">*</span>
              </label>
              <Input
                id="refreshToken"
                type="password"
                placeholder={t('credentials.refreshTokenPlaceholder')}
                value={refreshToken}
                onChange={(e) => setRefreshToken(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* 用户名/邮箱 */}
            <div className="space-y-2">
              <label htmlFor="email" className={LABEL}>
                {t('credentials.emailLabel')}
              </label>
              <Input
                id="email"
                type="text"
                placeholder={t('credentials.emailPlaceholderAdd')}
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* 认证方式 */}
            <div className="space-y-2">
              <label htmlFor="authMethod" className={LABEL}>
                {t('credentials.authMethodLabel')}
              </label>
              <select
                id="authMethod"
                value={authMethod}
                onChange={(e) => setAuthMethod(e.target.value as AuthMethod)}
                disabled={isPending}
                className="h-[31px] w-full rounded-[7px] border border-hairline-2 bg-surface-2 px-2.5 text-[12px] text-ink outline-none transition-colors focus:border-brand disabled:cursor-not-allowed disabled:opacity-50"
              >
                <option value="social">Social</option>
                <option value="idc">IdC/Builder-ID/IAM</option>
              </select>
            </div>

            {/* Region 配置 */}
            <div className="space-y-2">
              <label className={LABEL}>{t('credentials.regionConfigLabel')}</label>
              <div className="grid grid-cols-2 gap-2">
                <div>
                  <Input
                    id="authRegion"
                    placeholder={t('credentials.authRegionPlaceholder')}
                    value={authRegion}
                    onChange={(e) => setAuthRegion(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div>
                  <Input
                    id="apiRegion"
                    placeholder={t('credentials.apiRegionPlaceholder')}
                    value={apiRegion}
                    onChange={(e) => setApiRegion(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </div>
              <p className="text-[11px] leading-[1.55] text-ink-3">
                {t('credentials.regionHintAdd')}
              </p>
            </div>

            {/* IdC/Builder-ID/IAM 额外字段 */}
            {authMethod === 'idc' && (
              <>
                <div className="space-y-2">
                  <label htmlFor="clientId" className={LABEL}>
                    {t('credentials.clientIdLabel')} <span className="text-danger">*</span>
                  </label>
                  <Input
                    id="clientId"
                    placeholder={t('credentials.clientIdPlaceholderAdd')}
                    value={clientId}
                    onChange={(e) => setClientId(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label htmlFor="clientSecret" className={LABEL}>
                    {t('credentials.clientSecretLabel')} <span className="text-danger">*</span>
                  </label>
                  <Input
                    id="clientSecret"
                    type="password"
                    placeholder={t('credentials.clientSecretPlaceholderAdd')}
                    value={clientSecret}
                    onChange={(e) => setClientSecret(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </>
            )}

            {/* Profile ARN */}
            <div className="space-y-2">
              <label htmlFor="profileArn" className={LABEL}>
                {t('credentials.profileArnLabel')}
              </label>
              <Input
                id="profileArn"
                placeholder="arn:aws:codewhisperer:<region>:<account-id>:profile/<profile-id>"
                value={profileArn}
                onChange={(e) => setProfileArn(e.target.value)}
                disabled={isPending}
              />
              <p className="text-[11px] leading-[1.55] text-ink-3">
                {t('credentials.profileArnHintAdd')}
              </p>
            </div>

            {/* 优先级 */}
            <div className="space-y-2">
              <label htmlFor="priority" className={LABEL}>
                {t('credentials.priorityFieldLabel')}
              </label>
              <Input
                id="priority"
                type="number"
                min="0"
                placeholder={t('credentials.priorityPlaceholder')}
                value={priority}
                onChange={(e) => setPriority(e.target.value)}
                disabled={isPending}
              />
              <p className="text-[11px] leading-[1.55] text-ink-3">
                {t('credentials.priorityHint')}
              </p>
            </div>

            {/* Machine ID */}
            <div className="space-y-2">
              <label htmlFor="machineId" className={LABEL}>
                {t('credentials.machineIdLabel')}
              </label>
              <Input
                id="machineId"
                placeholder={t('credentials.machineIdPlaceholderAdd')}
                value={machineId}
                onChange={(e) => setMachineId(e.target.value)}
                disabled={isPending}
              />
              <p className="text-[11px] leading-[1.55] text-ink-3">
                {t('credentials.machineIdHint')}
              </p>
            </div>

            {/* 代理配置 */}
            <div className="space-y-2">
              <label className={LABEL}>{t('credentials.proxyConfigLabel')}</label>
              <Input
                id="proxyUrl"
                placeholder={t('credentials.proxyUrlPlaceholderAdd')}
                value={proxyUrl}
                onChange={(e) => setProxyUrl(e.target.value)}
                disabled={isPending}
              />
              <div className="grid grid-cols-2 gap-2">
                <Input
                  id="proxyUsername"
                  placeholder={t('credentials.proxyUsernamePlaceholder')}
                  value={proxyUsername}
                  onChange={(e) => setProxyUsername(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  id="proxyPassword"
                  type="password"
                  placeholder={t('credentials.proxyPasswordPlaceholder')}
                  value={proxyPassword}
                  onChange={(e) => setProxyPassword(e.target.value)}
                  disabled={isPending}
                />
              </div>
              <p className="text-[11px] leading-[1.55] text-ink-3">
                {t('credentials.proxyHintAdd')}
              </p>
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isPending}
            >
              {t('common.cancel')}
            </Button>
            <Button type="submit" disabled={isPending}>
              {isPending ? t('credentials.addingButton') : t('credentials.addButton')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
