// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useEffect } from 'react'
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
import { useUpdateCredential } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { CredentialStatusItem } from '@/types/api'

interface EditCredentialDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  credential: CredentialStatusItem
}

export function EditCredentialDialog({ open, onOpenChange, credential }: EditCredentialDialogProps) {
  const { t } = useTranslation()
  const [authRegion, setAuthRegion] = useState('')
  const [apiRegion, setApiRegion] = useState('')
  const [nickname, setNickname] = useState('')
  const [email, setEmail] = useState('')
  const [clientId, setClientId] = useState('')
  const [clientSecret, setClientSecret] = useState('')
  const [profileArn, setProfileArn] = useState('')
  const [machineId, setMachineId] = useState('')
  const [proxyUrl, setProxyUrl] = useState('')
  const [proxyUsername, setProxyUsername] = useState('')
  const [proxyPassword, setProxyPassword] = useState('')

  const { mutate, isPending } = useUpdateCredential()

  // 当对话框打开或凭据变化时，重置表单
  useEffect(() => {
    if (open) {
      setAuthRegion('')
      setApiRegion('')
      setNickname(credential.nickname || '')
      setEmail(credential.email || '')
      setClientId('')
      setClientSecret('')
      setProfileArn('')
      setMachineId('')
      setProxyUrl(credential.proxyUrl || '')
      setProxyUsername('')
      setProxyPassword('')
    }
  }, [open, credential])

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()

    // 构建只包含有变更的字段
    const data: Record<string, string> = {}
    if (authRegion !== '') data.authRegion = authRegion
    if (apiRegion !== '') data.apiRegion = apiRegion
    if (nickname !== (credential.nickname || '')) data.nickname = nickname
    if (email !== (credential.email || '')) data.email = email
    if (clientId !== '') data.clientId = clientId
    if (clientSecret !== '') data.clientSecret = clientSecret
    if (profileArn !== '') data.profileArn = profileArn
    if (machineId !== '') data.machineId = machineId
    if (proxyUrl !== (credential.proxyUrl || '')) data.proxyUrl = proxyUrl
    if (proxyUsername !== '') data.proxyUsername = proxyUsername
    if (proxyPassword !== '') data.proxyPassword = proxyPassword

    if (Object.keys(data).length === 0) {
      toast.info(t('credentials.toastNoFieldsToUpdate'))
      return
    }

    mutate(
      { id: credential.id, data },
      {
        onSuccess: (res) => {
          toast.success(res.message)
          onOpenChange(false)
        },
        onError: (error: unknown) => {
          toast.error(t('credentials.toastUpdateFailed', { message: extractErrorMessage(error) }))
        },
      }
    )
  }

  const isIdc = credential.authMethod === 'idc'

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('credentials.editDialogTitle', { id: credential.id })}</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex flex-col min-h-0 flex-1">
          <div className="space-y-4 py-4 overflow-y-auto flex-1 pr-1">
            <p className="text-xs text-muted-foreground">
              {t('credentials.editHintOnlyChanged')}
            </p>

            {/* 昵称 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">{t('credentials.nicknameLabel')}</label>
              <Input
                placeholder={t('credentials.nicknamePlaceholder')}
                value={nickname}
                onChange={(e) => setNickname(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* 用户名/邮箱 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">{t('credentials.emailLabel')}</label>
              <Input
                placeholder={t('credentials.emailPlaceholderEdit')}
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* Region 配置 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">{t('credentials.regionConfigLabel')}</label>
              <div className="grid grid-cols-2 gap-2">
                <Input
                  placeholder={t('credentials.authRegionPlaceholder')}
                  value={authRegion}
                  onChange={(e) => setAuthRegion(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  placeholder={t('credentials.apiRegionPlaceholder')}
                  value={apiRegion}
                  onChange={(e) => setApiRegion(e.target.value)}
                  disabled={isPending}
                />
              </div>
              <p className="text-xs text-muted-foreground">
                {t('credentials.regionHintEdit')}
              </p>
            </div>

            {/* IdC 字段 */}
            {isIdc && (
              <>
                <div className="space-y-2">
                  <label className="text-sm font-medium">{t('credentials.clientIdLabel')}</label>
                  <Input
                    placeholder={t('credentials.leaveBlankNoChange')}
                    value={clientId}
                    onChange={(e) => setClientId(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label className="text-sm font-medium">{t('credentials.clientSecretLabel')}</label>
                  <Input
                    type="password"
                    placeholder={t('credentials.leaveBlankNoChange')}
                    value={clientSecret}
                    onChange={(e) => setClientSecret(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </>
            )}

            {/* Profile ARN */}
            <div className="space-y-2">
              <label className="text-sm font-medium">{t('credentials.profileArnLabel')}</label>
              <Input
                placeholder={credential.hasProfileArn ? t('credentials.profileArnConfiguredPlaceholder') : t('credentials.leaveBlankNoChange')}
                value={profileArn}
                onChange={(e) => setProfileArn(e.target.value)}
                disabled={isPending}
              />
              <p className="text-xs text-muted-foreground">
                {t('credentials.profileArnHintEdit')}
              </p>
            </div>

            {/* Machine ID */}
            <div className="space-y-2">
              <label className="text-sm font-medium">{t('credentials.machineIdLabel')}</label>
              <Input
                placeholder={t('credentials.leaveBlankNoChange')}
                value={machineId}
                onChange={(e) => setMachineId(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* 代理配置 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">{t('credentials.proxyConfigLabel')}</label>
              <Input
                placeholder={t('credentials.proxyUrlPlaceholderEdit')}
                value={proxyUrl}
                onChange={(e) => setProxyUrl(e.target.value)}
                disabled={isPending}
              />
              <div className="grid grid-cols-2 gap-2">
                <Input
                  placeholder={t('credentials.proxyUsernamePlaceholder')}
                  value={proxyUsername}
                  onChange={(e) => setProxyUsername(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  type="password"
                  placeholder={t('credentials.proxyPasswordPlaceholder')}
                  value={proxyPassword}
                  onChange={(e) => setProxyPassword(e.target.value)}
                  disabled={isPending}
                />
              </div>
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
              {isPending ? t('credentials.updatingButton') : t('common.save')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
