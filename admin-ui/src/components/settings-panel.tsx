// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from 'sonner'
import { useTranslation } from 'react-i18next'
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  useLoadBalancingMode, useSetLoadBalancingMode,
  useAuthKeys, useSetAuthKeys,
} from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import { LANG_STORAGE_KEY } from '@/i18n'

export function SettingsPanel() {
  const { t, i18n } = useTranslation()
  const { data: loadBalancingData, isLoading: isLoadingMode } = useLoadBalancingMode()
  const { mutate: setLoadBalancingMode, isPending: isSettingMode } = useSetLoadBalancingMode()
  const { data: authKeysData, isLoading: isLoadingAuthKeys } = useAuthKeys()
  const { mutate: setAuthKeysMut, isPending: isSettingAuthKeys } = useSetAuthKeys()
  const [adminPswDraft, setAdminPswDraft] = useState('')
  const [editingAdminPsw, setEditingAdminPsw] = useState(false)

  const changeLanguage = (lang: 'zh' | 'en') => {
    i18n.changeLanguage(lang)
    localStorage.setItem(LANG_STORAGE_KEY, lang)
  }

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-semibold">{t('settings.title')}</h2>

      <div className="space-y-6">
        {/* 认证密钥 */}
        <div className="space-y-1.5">
          <p className="px-1 text-xs font-medium text-muted-foreground">{t('settings.authKeys')}</p>
          <Card>
            <CardContent className="divide-y divide-border p-0">
              <div className="space-y-2 p-4">
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium">{t('settings.adminPassword')}</span>
                  {!editingAdminPsw && (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => { setAdminPswDraft(''); setEditingAdminPsw(true) }}
                      disabled={isLoadingAuthKeys}
                    >
                      {t('common.edit')}
                    </Button>
                  )}
                </div>
                {editingAdminPsw ? (
                  <div className="flex gap-2">
                    <Input
                      type="text"
                      placeholder={t('settings.adminPasswordPlaceholder')}
                      value={adminPswDraft}
                      onChange={(e) => setAdminPswDraft(e.target.value)}
                      className="text-sm"
                    />
                    <Button
                      size="sm"
                      disabled={!adminPswDraft.trim() || isSettingAuthKeys}
                      onClick={() => {
                        setAuthKeysMut({ adminPsw: adminPswDraft.trim() }, {
                          onSuccess: () => {
                            toast.success(t('settings.adminPasswordUpdated'))
                            setEditingAdminPsw(false)
                            setAdminPswDraft('')
                          },
                          onError: (e) => toast.error(extractErrorMessage(e)),
                        })
                      }}
                    >
                      {t('common.save')}
                    </Button>
                    <Button variant="ghost" size="sm" onClick={() => setEditingAdminPsw(false)}>
                      {t('common.cancel')}
                    </Button>
                  </div>
                ) : (
                  <p className="text-xs text-muted-foreground font-mono">
                    {isLoadingAuthKeys ? t('common.loading') : authKeysData?.adminPsw ?? '—'}
                  </p>
                )}
              </div>
            </CardContent>
          </Card>
          <p className="px-1 text-xs text-muted-foreground">
            {t('settings.adminPasswordHint')}
          </p>
        </div>

        {/* 负载均衡 */}
        <div className="space-y-1.5">
          <p className="px-1 text-xs font-medium text-muted-foreground">{t('settings.loadBalancing')}</p>
          <Card>
            <CardContent className="p-0">
              <div className="flex items-center justify-between px-4 py-3">
                <span className="text-sm font-medium">{t('settings.loadBalancingMode')}</span>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    const newMode = loadBalancingData?.mode === 'priority' ? 'balanced' : 'priority'
                    setLoadBalancingMode(newMode, {
                      onSuccess: () => toast.success(t('settings.switchedTo', {
                        mode: newMode === 'priority' ? t('settings.priorityMode') : t('settings.balancedMode'),
                      })),
                      onError: (e) => toast.error(extractErrorMessage(e)),
                    })
                  }}
                  disabled={isLoadingMode || isSettingMode}
                >
                  {isLoadingMode ? t('common.loading') : loadBalancingData?.mode === 'priority' ? t('settings.priorityMode') : t('settings.balancedMode')}
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>

        {/* 语言 */}
        <div className="space-y-1.5">
          <p className="px-1 text-xs font-medium text-muted-foreground">{t('settings.language')}</p>
          <Card>
            <CardContent className="p-0">
              <div className="flex items-center justify-between px-4 py-3">
                <span className="text-sm font-medium">{t('settings.language')}</span>
                <div className="flex gap-2">
                  <Button
                    variant={i18n.language === 'zh' ? 'default' : 'outline'}
                    size="sm"
                    onClick={() => changeLanguage('zh')}
                    aria-pressed={i18n.language === 'zh'}
                  >
                    {t('settings.languageZh')}
                  </Button>
                  <Button
                    variant={i18n.language === 'en' ? 'default' : 'outline'}
                    size="sm"
                    onClick={() => changeLanguage('en')}
                    aria-pressed={i18n.language === 'en'}
                  >
                    {t('settings.languageEn')}
                  </Button>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  )
}