import { useState } from 'react'
import { toast } from 'sonner'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  useLoadBalancingMode, useSetLoadBalancingMode,
  useAuthKeys, useSetAuthKeys,
} from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'

export function SettingsPanel() {
  const { data: loadBalancingData, isLoading: isLoadingMode } = useLoadBalancingMode()
  const { mutate: setLoadBalancingMode, isPending: isSettingMode } = useSetLoadBalancingMode()
  const { data: authKeysData, isLoading: isLoadingAuthKeys } = useAuthKeys()
  const { mutate: setAuthKeysMut, isPending: isSettingAuthKeys } = useSetAuthKeys()
  const [apiKeyDraft, setApiKeyDraft] = useState('')
  const [adminApiKeyDraft, setAdminApiKeyDraft] = useState('')
  const [editingApiKey, setEditingApiKey] = useState(false)
  const [editingAdminApiKey, setEditingAdminApiKey] = useState(false)

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-semibold">设置</h2>

      <div className="grid gap-4 md:grid-cols-2">
        {/* 认证密钥 */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">认证密钥</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">主 API Key</span>
                {!editingApiKey && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => { setApiKeyDraft(''); setEditingApiKey(true) }}
                    disabled={isLoadingAuthKeys}
                  >
                    修改
                  </Button>
                )}
              </div>
              {editingApiKey ? (
                <div className="flex gap-2">
                  <Input
                    type="text"
                    placeholder="输入新的 API Key"
                    value={apiKeyDraft}
                    onChange={(e) => setApiKeyDraft(e.target.value)}
                    className="text-sm"
                  />
                  <Button
                    size="sm"
                    disabled={!apiKeyDraft.trim() || isSettingAuthKeys}
                    onClick={() => {
                      setAuthKeysMut({ apiKey: apiKeyDraft.trim() }, {
                        onSuccess: () => {
                          toast.success('主 API Key 已更新')
                          setEditingApiKey(false)
                          setApiKeyDraft('')
                        },
                        onError: (e) => toast.error(extractErrorMessage(e)),
                      })
                    }}
                  >
                    保存
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => setEditingApiKey(false)}>
                    取消
                  </Button>
                </div>
              ) : (
                <p className="text-xs text-muted-foreground font-mono">
                  {isLoadingAuthKeys ? '加载中...' : authKeysData?.apiKey ?? '—'}
                </p>
              )}
            </div>
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">Admin API Key</span>
                {!editingAdminApiKey && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => { setAdminApiKeyDraft(''); setEditingAdminApiKey(true) }}
                    disabled={isLoadingAuthKeys}
                  >
                    修改
                  </Button>
                )}
              </div>
              {editingAdminApiKey ? (
                <div className="flex gap-2">
                  <Input
                    type="text"
                    placeholder="输入新的 Admin API Key"
                    value={adminApiKeyDraft}
                    onChange={(e) => setAdminApiKeyDraft(e.target.value)}
                    className="text-sm"
                  />
                  <Button
                    size="sm"
                    disabled={!adminApiKeyDraft.trim() || isSettingAuthKeys}
                    onClick={() => {
                      setAuthKeysMut({ adminApiKey: adminApiKeyDraft.trim() }, {
                        onSuccess: () => {
                          toast.success('Admin API Key 已更新，请使用新密钥重新登录')
                          setEditingAdminApiKey(false)
                          setAdminApiKeyDraft('')
                        },
                        onError: (e) => toast.error(extractErrorMessage(e)),
                      })
                    }}
                  >
                    保存
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => setEditingAdminApiKey(false)}>
                    取消
                  </Button>
                </div>
              ) : (
                <p className="text-xs text-muted-foreground font-mono">
                  {isLoadingAuthKeys ? '加载中...' : authKeysData?.adminApiKey ?? '—'}
                </p>
              )}
            </div>
            <p className="text-xs text-muted-foreground">
              修改后立即生效，旧密钥将失效。修改 Admin API Key 后需要用新密钥重新登录。
            </p>
          </CardContent>
        </Card>

        {/* 负载均衡 */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">负载均衡</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-center justify-between py-3">
              <span className="text-sm font-medium">均衡模式</span>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  const newMode = loadBalancingData?.mode === 'priority' ? 'balanced' : 'priority'
                  setLoadBalancingMode(newMode, {
                    onSuccess: () => toast.success(`已切换为${newMode === 'priority' ? '优先级模式' : '均衡负载'}`),
                    onError: (e) => toast.error(extractErrorMessage(e)),
                  })
                }}
                disabled={isLoadingMode || isSettingMode}
              >
                {isLoadingMode ? '加载中...' : loadBalancingData?.mode === 'priority' ? '优先级模式' : '均衡负载'}
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}