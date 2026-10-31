// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from '@/lib/toast'
import { useTranslation } from 'react-i18next'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import { Globe, KeyRound, Settings2, UserRoundPlus } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Field, Section } from '@/components/credential-form-kit'
import { useAddCredential } from '@/hooks/use-credentials'
import { cn, extractErrorMessage } from '@/lib/utils'
import { normalizeLoginSource } from '@/lib/account-state'

interface AddCredentialDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

type AuthMethod = 'social' | 'idc'

export function AddCredentialDialog({ open, onOpenChange }: AddCredentialDialogProps) {
  const { t } = useTranslation()
  const [refreshToken, setRefreshToken] = useState('')
  const [email, setEmail] = useState('')
  const [authMethod, setAuthMethod] = useState<AuthMethod>('social')
  const [loginSource, setLoginSource] = useState('')
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
    setLoginSource('')
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
        // 仅在 social 下提交：切到 idc 后 loginSource 不会自动清空，无条件下发会把 IdC 账号写成脏数据
        provider: authMethod === 'social' ? normalizeLoginSource(loginSource) : undefined,
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
          <div className="flex items-center gap-2.5">
            <span className="grid size-[34px] shrink-0 place-items-center rounded-[9px] bg-brand-soft text-brand">
              <UserRoundPlus className="size-4" aria-hidden="true" />
            </span>
            <DialogTitle>{t('credentials.addDialogTitle')}</DialogTitle>
          </div>
          <DialogDescription>{t('credentials.addDialogSubtitle')}</DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex flex-col min-h-0 flex-1">
          {/* 与编辑弹窗同款：卡片右缘对齐 X 图标右缘（19px），滚动条隐藏、滚动仍可用 */}
          <div className="mr-0.5 space-y-5 overflow-y-auto flex-1 py-1 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden sm:-mr-1.5">
            {/* 核心凭据 */}
            <Section icon={KeyRound} title={t('credentials.sectionCore')}>
              <Field
                label={t('credentials.refreshTokenLabel')}
                htmlFor="refreshToken"
                required
              >
                <Input
                  id="refreshToken"
                  type="password"
                  prefix={<KeyRound className="size-[13px]" />}
                  placeholder={t('credentials.refreshTokenPlaceholder')}
                  value={refreshToken}
                  onChange={(e) => setRefreshToken(e.target.value)}
                  disabled={isPending}
                />
              </Field>

              <Field label={t('credentials.emailLabel')} htmlFor="email">
                <Input
                  id="email"
                  type="text"
                  placeholder={t('credentials.emailPlaceholderAdd')}
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  disabled={isPending}
                />
              </Field>

              {/* group 分段器无单一控件可关联，label 不出 htmlFor，可访问名称由下方 aria-label 承载 */}
              <Field label={t('credentials.authMethodLabel')}>
                <div
                  className="grid grid-cols-2 gap-2"
                  role="group"
                  aria-label={t('credentials.authMethodLabel')}
                >
                  {(['social', 'idc'] as const).map((m) => {
                    const on = authMethod === m
                    return (
                      <button
                        key={m}
                        type="button"
                        aria-pressed={on}
                        disabled={isPending}
                        onClick={() => setAuthMethod(m)}
                        className={cn(
                          'flex h-[31px] items-center justify-center gap-1.5 rounded-[7px] border-[0.5px] text-[12px] transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand disabled:pointer-events-none disabled:opacity-50',
                          on
                            ? 'border-brand bg-brand-soft font-semibold text-brand'
                            : 'border-hairline-2 bg-surface font-medium text-ink-2 hover:text-ink'
                        )}
                      >
                        {t(`credentials.authMethod_${m}`)}
                      </button>
                    )
                  })}
                </div>
              </Field>

              {/* 登录来源（仅 social 需要；IdC 的 BuilderId 由 authMethod 推断） */}
              {authMethod === 'social' && (
                <Field label={t('credentials.loginSourceLabel')} htmlFor="loginSource">
                  <Select
                    id="loginSource"
                    value={loginSource}
                    onChange={(e) => setLoginSource(e.target.value)}
                    disabled={isPending}
                  >
                    <option value="">{t('credentials.loginSourceUnspecified')}</option>
                    <option value="Google">Google</option>
                    <option value="GitHub">GitHub</option>
                  </Select>
                </Field>
              )}
            </Section>

            {/* 身份与区域 */}
            <Section icon={Globe} title={t('credentials.sectionIdentityRegion')}>
              <Field label={t('credentials.regionConfigLabel')} hint={t('credentials.regionHintAdd')}>
                {/* 同 authMethod 分段器口径：双输入无单一控件可关联，可访问名称由 group + aria-label 承载 */}
                <div
                  className="grid grid-cols-2 gap-2"
                  role="group"
                  aria-label={t('credentials.regionConfigLabel')}
                >
                  <Input
                    id="authRegion"
                    placeholder={t('credentials.authRegionPlaceholder')}
                    value={authRegion}
                    onChange={(e) => setAuthRegion(e.target.value)}
                    disabled={isPending}
                  />
                  <Input
                    id="apiRegion"
                    placeholder={t('credentials.apiRegionPlaceholder')}
                    value={apiRegion}
                    onChange={(e) => setApiRegion(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </Field>

              {/* IdC/Builder-ID/IAM 额外字段 */}
              {authMethod === 'idc' && (
                <>
                  <Field
                    label={t('credentials.clientIdLabel')}
                    htmlFor="clientId"
                    required
                  >
                    <Input
                      id="clientId"
                      placeholder={t('credentials.clientIdPlaceholderAdd')}
                      value={clientId}
                      onChange={(e) => setClientId(e.target.value)}
                      disabled={isPending}
                    />
                  </Field>
                  <Field
                    label={t('credentials.clientSecretLabel')}
                    htmlFor="clientSecret"
                    required
                  >
                    <Input
                      id="clientSecret"
                      type="password"
                      placeholder={t('credentials.clientSecretPlaceholderAdd')}
                      value={clientSecret}
                      onChange={(e) => setClientSecret(e.target.value)}
                      disabled={isPending}
                    />
                  </Field>
                </>
              )}

              <Field
                label={t('credentials.profileArnLabel')}
                htmlFor="profileArn"
                hint={t('credentials.profileArnHintAdd')}
              >
                <Input
                  id="profileArn"
                  placeholder="arn:aws:codewhisperer:<region>:<account-id>:profile/<profile-id>"
                  value={profileArn}
                  onChange={(e) => setProfileArn(e.target.value)}
                  disabled={isPending}
                />
              </Field>
            </Section>

            {/* 运行与网络 */}
            <Section icon={Settings2} title={t('credentials.sectionRuntimeNetwork')}>
              <Field
                label={t('credentials.priorityFieldLabel')}
                htmlFor="priority"
                hint={t('credentials.priorityHint')}
              >
                <Input
                  id="priority"
                  type="number"
                  min="0"
                  placeholder={t('credentials.priorityPlaceholder')}
                  value={priority}
                  onChange={(e) => setPriority(e.target.value)}
                  disabled={isPending}
                />
              </Field>

              <Field
                label={t('credentials.machineIdLabel')}
                htmlFor="machineId"
                hint={t('credentials.machineIdHint')}
              >
                <Input
                  id="machineId"
                  placeholder={t('credentials.machineIdPlaceholderAdd')}
                  value={machineId}
                  onChange={(e) => setMachineId(e.target.value)}
                  disabled={isPending}
                />
              </Field>

              <Field
                label={t('credentials.proxyConfigLabel')}
                htmlFor="proxyUrl"
                hint={t('credentials.proxyHintAdd')}
              >
                <Input
                  id="proxyUrl"
                  placeholder={t('credentials.proxyUrlPlaceholderAdd')}
                  value={proxyUrl}
                  onChange={(e) => setProxyUrl(e.target.value)}
                  disabled={isPending}
                />
                <div className="grid grid-cols-2 gap-2 mt-2">
                  {/* 无独立 label 可关联（Field 的 htmlFor 指向 proxyUrl），placeholder 消失即失名，补 aria-label */}
                  <Input
                    id="proxyUsername"
                    aria-label={t('credentials.proxyUsernamePlaceholder')}
                    placeholder={t('credentials.proxyUsernamePlaceholder')}
                    value={proxyUsername}
                    onChange={(e) => setProxyUsername(e.target.value)}
                    disabled={isPending}
                  />
                  <Input
                    id="proxyPassword"
                    type="password"
                    aria-label={t('credentials.proxyPasswordPlaceholder')}
                    placeholder={t('credentials.proxyPasswordPlaceholder')}
                    value={proxyPassword}
                    onChange={(e) => setProxyPassword(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </Field>
            </Section>
          </div>

          {/* 同编辑弹窗：按钮右缘与卡片、X 图标三者对齐（19px） */}
          <DialogFooter className="mr-0.5 pt-4 sm:-mr-1.5">
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
