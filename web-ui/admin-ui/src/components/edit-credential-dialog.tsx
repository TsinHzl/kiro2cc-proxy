// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useLayoutEffect } from 'react'
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
import { Globe, KeyRound, Pencil, Settings2 } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Field, ReadOnlyField, Section } from '@/components/credential-form-kit'
import { useSetPriority, useUpdateCredential } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import { normalizeLoginSource } from '@/lib/account-state'
import type { CredentialStatusItem } from '@/types/api'

interface EditCredentialDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  credential: CredentialStatusItem
}

/** 打开时初始值快照：credentialId 标识归属账号，供渲染守卫判断快照是否已就绪 */
interface Snapshot {
  credentialId: number
  priority: string
  authRegion: string
  apiRegion: string
  nickname: string
  email: string
  clientId: string
  profileArn: string
  machineId: string
  proxyUrl: string
  loginSource: string
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
  const [priority, setPriority] = useState('')
  // 空串代表「不指定」，与 normalizeLoginSource 的 undefined 语义对齐
  const [loginSourceValue, setLoginSourceValue] = useState('')

  const { mutateAsync: updateAsync, isPending: updatePending } = useUpdateCredential()
  const { mutateAsync: setPriorityAsync, isPending: priorityPending } = useSetPriority()
  // 两个端点串行提交，任一进行中即锁表单
  const isPending = updatePending || priorityPending

  /** 打开时初始值快照；null = 快照不存在（弹窗关闭或尚未写入）。用 state 而非 ref：
   *  locked 派生与表单首帧渲染都依赖它，渲染期读 ref.current 是 React 明确的反模式。
   *  列表每 30s 自动刷新（TanStack Query 生成新引用），提交 diff 若直接与 credential
   *  比对，会在用户编辑中途后台恰逢字段后端值变化时误判「有变更」——以打开瞬间的快照为准。 */
  const [initial, setInitial] = useState<Snapshot | null>(null)

  // 仅在对话框打开时按当前凭据初始化表单：已锁定的有值字段只读展示，
  // 其余字段（含已锁定的无值字段）保持可编辑。依赖取 credential.id 而非整个 credential 对象 ——
  // 列表每 30s 自动刷新会为数据变动的行生成新引用，依赖整个对象会在编辑中途静默重置表单。
  // 用 useLayoutEffect 而非 useEffect：快照写入先于 paint，守卫帧不会被绘制成空窗
  useLayoutEffect(() => {
    if (open) {
      const authRegion = credential.authRegion || ''
      const apiRegion = credential.apiRegion || ''
      const nickname = credential.nickname || ''
      const email = credential.email || ''
      const clientId = credential.clientId || ''
      const profileArn = credential.profileArn || ''
      const machineId = credential.machineId || ''
      const proxyUrl = credential.proxyUrl || ''
      const loginSource = normalizeLoginSource(credential.provider) ?? ''
      const priority = String(credential.priority)
      setInitial({
        credentialId: credential.id,
        priority,
        authRegion, apiRegion, nickname, email, clientId,
        profileArn, machineId, proxyUrl, loginSource,
      })
      setAuthRegion(authRegion)
      setApiRegion(apiRegion)
      setNickname(nickname)
      setEmail(email)
      setClientId(clientId)
      setClientSecret('')
      setProfileArn(profileArn)
      setMachineId(machineId)
      setProxyUrl(proxyUrl)
      setProxyUsername('')
      setProxyPassword('')
      setPriority(priority)
      setLoginSourceValue(loginSource)
    } else {
      // 关闭即作废快照：同账号紧接着重开时守卫依然生效，不会显示上一会话的旧值
      setInitial(null)
    }
  }, [open, credential.id])

  // 快照未就绪（弹窗关闭或刚打开、effect 尚未写入）只渲染 Dialog 骨架，不得向下派生 locked：
  // 弹窗在账号表格每行常驻挂载，关闭态 initial 为 null，若继续执行 `!!initial.email`
  // 会对 null 取属性抛 TypeError 致整页白屏。打开态 useLayoutEffect 在 paint 前写入
  // 快照，骨架帧不会暴露。守卫位于全部 hooks 之后，不破坏 hooks 顺序
  if (initial === null || initial.credentialId !== credential.id) {
    return <Dialog open={open} onOpenChange={onOpenChange} />
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()

    // 守卫保证表单仅在快照就绪后渲染，此分支理论不可达；防御性检查以收窄快照类型
    if (initial === null || initial.credentialId !== credential.id) return

    // 优先级不在 UpdateCredentialRequest 里，走独立端点；先校验避免半途失败。
    // 判据同样以打开时快照为基准：弹窗打开期间他人改了优先级时，不应以旧值静默覆盖
    const priorityChanged = priority !== initial.priority
    if (priorityChanged && !/^\d+$/.test(priority)) {
      toast.error(t('credentials.toastPriorityInvalid'))
      return
    }

    // 构建只包含有变更的字段。判据是与打开时初始快照（initial）比对：
    // 「有值即锁定」字段在此永不出现在 data（渲染层只读、值恒等于快照，比对天然不成立）；
    // 无值补录字段留空时与快照相等，同样不提交
    const data: Record<string, string> = {}
    if (authRegion !== initial.authRegion) data.authRegion = authRegion
    if (apiRegion !== initial.apiRegion) data.apiRegion = apiRegion
    if (nickname !== initial.nickname) data.nickname = nickname
    if (email !== initial.email) data.email = email
    if (clientId !== initial.clientId) data.clientId = clientId
    if (clientSecret !== '') data.clientSecret = clientSecret
    if (profileArn !== initial.profileArn) data.profileArn = profileArn
    if (machineId !== initial.machineId) data.machineId = machineId
    if (proxyUrl !== initial.proxyUrl) data.proxyUrl = proxyUrl
    // 代理地址被清空时同步清除认证信息：后端对 username/password 是「不传不修改、
    // 传空串即清除」语义，只清地址而不清账密会持久化出「无代理但残留凭据」的脏数据。
    // 代理 URL 恒可编辑，清空即删除代理
    if (proxyUrl === '' && initial.proxyUrl !== '') {
      data.proxyUsername = ''
      data.proxyPassword = ''
    } else {
      if (proxyUsername !== '') data.proxyUsername = proxyUsername
      if (proxyPassword !== '') data.proxyPassword = proxyPassword
    }
    // 登录来源：仅 social 账号且当前无值（未锁定）可改；空串表示清除。
    // 不写 authMethod —— 后端把它也算作身份变更，会白清 subscription_title
    if (isSocial && loginSourceValue !== initial.loginSource) data.provider = loginSourceValue

    if (!priorityChanged && Object.keys(data).length === 0) {
      toast.info(t('credentials.toastNoFieldsToUpdate'))
      return
    }

    const done: string[] = []
    try {
      if (priorityChanged) {
        done.push((await setPriorityAsync({ id: credential.id, priority: Number(priority) })).message)
      }
      if (Object.keys(data).length > 0) {
        done.push((await updateAsync({ id: credential.id, data })).message)
      }
      toast.success(done.join(' · '))
      onOpenChange(false)
    } catch (error) {
      // 串行提交下优先级先落地：失败时把已成功的部分一并告知，避免用户重复设置
      const failed = t('credentials.toastUpdateFailed', { message: extractErrorMessage(error) })
      toast.error(done.length > 0 ? `${done.join(' · ')} · ${failed}` : failed)
    }
  }

  const isIdc = credential.authMethod === 'idc'
  // 仅 social 账号的 provider 是「登录来源」；external_idp 的 provider 存的是 IdP 名称，不可被此处覆盖
  const isSocial = credential.authMethod === 'social'
  // 有值即锁定的只读字段集合。判据与提交 diff 同源（initial 打开时快照）而非实时 credential：
  // 列表 30s 刷新会让实时值中途翻转锁定态，出现「展示已锁定但提交仍覆盖该字段」的不一致
  const lockNote = t('credentials.lockedFieldNote')
  const locked = {
    email: !!initial.email,
    loginSource: isSocial && !!initial.loginSource,
    authRegion: !!initial.authRegion,
    clientId: !!initial.clientId,
    profileArn: !!initial.profileArn,
    machineId: !!initial.machineId,
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg max-h-[85vh] flex flex-col">
        <DialogHeader>
          <div className="flex items-center gap-2.5">
            <span className="grid size-[34px] shrink-0 place-items-center rounded-[9px] bg-brand-soft text-brand">
              <Pencil className="size-4" aria-hidden="true" />
            </span>
            <DialogTitle>{t('credentials.editDialogTitle', { id: credential.id })}</DialogTitle>
          </div>
          {/* 副标题承载原滚动区顶部的 editHintOnlyChanged 提示，语义不变且始终可见 */}
          <DialogDescription>{t('credentials.editHintOnlyChanged')}</DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex flex-col min-h-0 flex-1">
          {/* 卡片右缘对齐 X 图标右缘：图标距弹窗右边框 = right-4(16) + p-0.5(2) + border(1) = 19px。
              弹窗内边距 p-4 / sm:p-6，故外边距取 mr-0.5 / sm:-mr-1.5 时卡片恰好落在 19px。
              滚动条整体隐藏（无槽位），滚动仍可用，各平台对齐口径一致 */}
          <div className="mr-0.5 space-y-5 overflow-y-auto flex-1 py-1 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden sm:-mr-1.5">
            {/* 核心凭据 */}
            <Section icon={KeyRound} title={t('credentials.sectionCore')}>
              {/* 昵称是本地备注名而非账号固有信息，允许反复编辑 */}
              <Field label={t('credentials.nicknameLabel')} htmlFor="nickname">
                <Input
                  id="nickname"
                  placeholder={t('credentials.nicknamePlaceholder')}
                  value={nickname}
                  onChange={(e) => setNickname(e.target.value)}
                  disabled={isPending}
                />
              </Field>

              {locked.email ? (
                <ReadOnlyField
                  label={t('credentials.emailLabel')}
                  value={email}
                  lockNote={lockNote}
                />
              ) : (
                <Field label={t('credentials.emailLabel')} htmlFor="email">
                  <Input
                    id="email"
                    placeholder={t('credentials.emailPlaceholderEdit')}
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    disabled={isPending}
                  />
                </Field>
              )}

              {/* 登录来源（仅 social；IdC 的 BuilderId 由 authMethod 推断、external_idp 的 provider 为 IdP 名称，二者均不可改）。
                  有值锁定只读，无值仍可选（补录场景） */}
              {isSocial &&
                (locked.loginSource ? (
                  <ReadOnlyField
                    label={t('credentials.loginSourceLabel')}
                    value={t(`credentials.loginSource_${loginSourceValue}`)}
                    lockNote={lockNote}
                  />
                ) : (
                  <Field
                    label={t('credentials.loginSourceLabel')}
                    htmlFor="loginSource"
                    hint={t('credentials.loginSourceHintEdit')}
                  >
                    <Select
                      id="loginSource"
                      value={loginSourceValue}
                      onChange={(e) => setLoginSourceValue(e.target.value)}
                      disabled={isPending}
                    >
                      <option value="">{t('credentials.loginSourceUnspecified')}</option>
                      <option value="Google">Google</option>
                      <option value="GitHub">GitHub</option>
                    </Select>
                  </Field>
                ))}
            </Section>

            {/* 身份与区域 */}
            <Section icon={Globe} title={t('credentials.sectionIdentityRegion')}>
              <Field label={t('credentials.regionConfigLabel')} hint={t('credentials.regionHintEdit')}>
                {/* Auth Region 影响 token 刷新，有值即锁定；API Region 可反复编辑。
                    只读侧省略 label（由外层 Field 统一承载），两侧框体同为 31px 同顶对齐。
                    双输入/只读混合无单一控件可关联，可访问名称由 group + aria-label 承载（同 authMethod 口径） */}
                <div
                  className="grid grid-cols-2 gap-2"
                  role="group"
                  aria-label={t('credentials.regionConfigLabel')}
                >
                  {locked.authRegion ? (
                    <ReadOnlyField value={authRegion} mono lockNote={lockNote} />
                  ) : (
                    <Input
                      id="authRegion"
                      placeholder={t('credentials.authRegionPlaceholder')}
                      value={authRegion}
                      onChange={(e) => setAuthRegion(e.target.value)}
                      disabled={isPending}
                    />
                  )}
                  <Input
                    id="apiRegion"
                    placeholder={t('credentials.apiRegionPlaceholder')}
                    value={apiRegion}
                    onChange={(e) => setApiRegion(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </Field>

              {/* IdC 字段：clientId 有值锁定；clientSecret 永不下发，恒可编辑（留空不修改） */}
              {isIdc && (
                <>
                  {locked.clientId ? (
                    <ReadOnlyField
                      label={t('credentials.clientIdLabel')}
                      value={clientId}
                      mono
                      lockNote={lockNote}
                    />
                  ) : (
                    <Field label={t('credentials.clientIdLabel')} htmlFor="clientId">
                      <Input
                        id="clientId"
                        placeholder={t('credentials.leaveBlankNoChange')}
                        value={clientId}
                        onChange={(e) => setClientId(e.target.value)}
                        disabled={isPending}
                      />
                    </Field>
                  )}
                  <Field label={t('credentials.clientSecretLabel')} htmlFor="clientSecret">
                    <Input
                      id="clientSecret"
                      type="password"
                      placeholder={t('credentials.leaveBlankNoChange')}
                      value={clientSecret}
                      onChange={(e) => setClientSecret(e.target.value)}
                      disabled={isPending}
                    />
                  </Field>
                </>
              )}

              {locked.profileArn ? (
                <ReadOnlyField
                  label={t('credentials.profileArnLabel')}
                  value={profileArn}
                  mono
                  lockNote={lockNote}
                />
              ) : (
                <Field
                  label={t('credentials.profileArnLabel')}
                  htmlFor="profileArn"
                  hint={t('credentials.profileArnHintEdit')}
                >
                  <Input
                    id="profileArn"
                    placeholder={credential.hasProfileArn ? t('credentials.profileArnConfiguredPlaceholder') : t('credentials.leaveBlankNoChange')}
                    value={profileArn}
                    onChange={(e) => setProfileArn(e.target.value)}
                    disabled={isPending}
                  />
                </Field>
              )}
            </Section>

            {/* 运行与网络 */}
            <Section icon={Settings2} title={t('credentials.sectionRuntimeNetwork')}>
              {/* 优先级为本地调度参数而非账号固有信息，保持可编辑（用户决策） */}
              <Field
                label={t('credentials.priorityFieldLabel')}
                htmlFor="priority"
                hint={t('credentials.priorityHint')}
              >
                <Input
                  id="priority"
                  type="number"
                  min={0}
                  step={1}
                  inputMode="numeric"
                  placeholder={t('credentials.priorityPlaceholder')}
                  value={priority}
                  onChange={(e) => setPriority(e.target.value)}
                  disabled={isPending}
                />
              </Field>

              {locked.machineId ? (
                <ReadOnlyField
                  label={t('credentials.machineIdLabel')}
                  value={machineId}
                  mono
                  lockNote={lockNote}
                />
              ) : (
                <Field label={t('credentials.machineIdLabel')} htmlFor="machineId">
                  <Input
                    id="machineId"
                    placeholder={t('credentials.leaveBlankNoChange')}
                    value={machineId}
                    onChange={(e) => setMachineId(e.target.value)}
                    disabled={isPending}
                  />
                </Field>
              )}

              {/* 代理是可反复调整的运行参数，URL 恒可编辑；账密留空不修改 */}
              <Field label={t('credentials.proxyConfigLabel')} htmlFor="proxyUrl">
                <Input
                  id="proxyUrl"
                  placeholder={t('credentials.proxyUrlPlaceholderEdit')}
                  value={proxyUrl}
                  onChange={(e) => setProxyUrl(e.target.value)}
                  disabled={isPending}
                />
              </Field>
              <div className="grid grid-cols-2 gap-2">
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
            </Section>
          </div>

          {/* 与卡片同口径：按钮右缘同样落在 X 图标右缘 19px 处 */}
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
              {isPending ? t('credentials.updatingButton') : t('common.save')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
