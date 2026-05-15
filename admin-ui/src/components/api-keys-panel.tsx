import { useState } from 'react'
import { Copy, Plus, Pencil, Trash2, Key, Check, Clock, BarChart3, RotateCcw, DollarSign, ArrowDownWideNarrow, Search, Loader2 } from 'lucide-react'
import { toast } from 'sonner'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { useQueryClient } from '@tanstack/react-query'
import { useApiKeys, useCreateApiKey, useUpdateApiKey, useDeleteApiKey, useServerInfo, useAllUsage, useResetKeyUsage, useRpm } from '@/hooks/use-credentials'
import { deleteApiKey as deleteApiKeyApi } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { ApiKeyItem, UsageSummary } from '@/types/api'

export function ApiKeysPanel() {
  const [createDialogOpen, setCreateDialogOpen] = useState(false)
  const [editingKey, setEditingKey] = useState<ApiKeyItem | null>(null)
  const [newName, setNewName] = useState('')
  const [newMode, setNewMode] = useState<'date' | 'quota'>('quota')
  const [newDuration, setNewDuration] = useState<number | null>(1) // 数值，null 表示永不过期
  const [newDurationUnit, setNewDurationUnit] = useState<'days' | 'hours'>('days')
  const [newSpendingLimit, setNewSpendingLimit] = useState(100)
  const [editName, setEditName] = useState('')
  const [editMode, setEditMode] = useState<'date' | 'quota'>('date')
  const [editDuration, setEditDuration] = useState<number | null | string>(1)
  const [editDurationUnit, setEditDurationUnit] = useState<'days' | 'hours'>('days')
  const [editSpendingLimit, setEditSpendingLimit] = useState(50)
  const [copiedId, setCopiedId] = useState<number | null>(null)
  const [copiedMaster, setCopiedMaster] = useState(false)
  const [copiedUrl, setCopiedUrl] = useState(false)
  const [sortBy, setSortBy] = useState<'newest' | 'cost-desc' | 'cost-asc'>('newest')
  const [searchQuery, setSearchQuery] = useState('')
  const [purgeDialogOpen, setPurgeDialogOpen] = useState(false)
  const [purging, setPurging] = useState(false)

  const quickDurationOptions = [
    { label: '1 小时', value: 1, unit: 'hours' as const },
    { label: '3 小时', value: 3, unit: 'hours' as const },
    { label: '6 小时', value: 6, unit: 'hours' as const },
    { label: '12 小时', value: 12, unit: 'hours' as const },
    { label: '1 天', value: 1, unit: 'days' as const },
    { label: '3 天', value: 3, unit: 'days' as const },
    { label: '7 天', value: 7, unit: 'days' as const },
  ]

  const toDays = (value: number, unit: 'days' | 'hours') => unit === 'hours' ? value / 24 : value

  const formatDuration = (days: number) => {
    if (days < 1) {
      const hours = Math.round(days * 24 * 100) / 100
      return `${hours} 小时`
    }
    return `${days} 天`
  }

  const { data: apiKeys, isLoading } = useApiKeys()
  const { data: serverInfo } = useServerInfo()
  const { data: usageData, dataUpdatedAt } = useAllUsage()
  const { data: rpmData } = useRpm()
  const queryClient = useQueryClient()
  const { mutate: createKey, isPending: isCreating } = useCreateApiKey()
  const { mutate: updateKey } = useUpdateApiKey()
  const { mutate: deleteKey } = useDeleteApiKey()
  const { mutate: resetUsage } = useResetKeyUsage()

  // 构建 key_id -> usage 的映射
  const usageMap = new Map<number, UsageSummary>()
  usageData?.forEach((u) => usageMap.set(u.apiKeyId, u))

  const formatTokens = (tokens: number): string => {
    if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)}M`
    if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}K`
    return tokens.toString()
  }

  const formatCost = (cost: number): string => {
    return `$${cost.toFixed(4)}`
  }

  const handleResetUsage = (key: ApiKeyItem) => {
    if (!confirm(`确定要重置 "${key.name}" 的用量记录吗？`)) return
    resetUsage(key.id, {
      onSuccess: () => toast.success('用量已重置'),
      onError: (err) => toast.error(extractErrorMessage(err)),
    })
  }

  const getKeyStatus = (key: ApiKeyItem): 'active' | 'disabled' | 'expired' | 'pending' => {
    if (!key.enabled) return 'disabled'
    if (key.expiresAt && new Date(key.expiresAt) <= new Date()) return 'expired'
    if (key.durationDays != null && !key.activatedAt) return 'pending'
    return 'active'
  }

  // 获取所有无效 Key（已禁用 + 已过期）
  const invalidKeys = (apiKeys ?? []).filter((k) => {
    const s = getKeyStatus(k)
    return s === 'disabled' || s === 'expired'
  })

  const handlePurge = async () => {
    setPurging(true)
    let deleted = 0
    for (const key of invalidKeys) {
      try {
        await deleteApiKeyApi(key.id)
        deleted++
      } catch {
        // 单个失败不中断
      }
    }
    setPurging(false)
    setPurgeDialogOpen(false)
    queryClient.invalidateQueries({ queryKey: ['apiKeys'] })
    queryClient.invalidateQueries({ queryKey: ['apiKeyUsage'] })
    toast.success(`已清除 ${deleted} 个无效 Key`)
  }

  const copyToClipboard = async (text: string, target: 'url' | 'master' | number) => {
    await navigator.clipboard.writeText(text)
    if (target === 'url') {
      setCopiedUrl(true)
      setTimeout(() => setCopiedUrl(false), 2000)
    } else if (target === 'master') {
      setCopiedMaster(true)
      setTimeout(() => setCopiedMaster(false), 2000)
    } else {
      setCopiedId(target)
      setTimeout(() => setCopiedId(null), 2000)
    }
    toast.success('已复制到剪贴板')
  }

  const handleCreate = () => {
    createKey(
      {
        name: newName,
        ...(newMode === 'date'
          ? newDuration !== null
            ? { durationDays: toDays(newDuration, newDurationUnit) }
            : {}
          : { spendingLimit: newSpendingLimit }),
      },
      {
        onSuccess: () => {
          toast.success('API Key 创建成功')
          setCreateDialogOpen(false)
          setNewName('')
          setNewMode('quota')
          setNewDuration(1)
          setNewDurationUnit('days')
          setNewSpendingLimit(100)
        },
        onError: (err) => toast.error(`创建失败: ${extractErrorMessage(err)}`),
      }
    )
  }

  const handleUpdate = () => {
    if (!editingKey) return
    const duration = editDuration === '' ? null : editDuration
    const data: Record<string, unknown> = { name: editName || undefined }
    if (editMode === 'date') {
      if (duration !== null) {
        data.durationDays = toDays(Number(duration), editDurationUnit)
        // 活跃 Key 不清除 expiresAt，由后端增量计算
        if (getKeyStatus(editingKey) !== 'active') {
          data.expiresAt = null
        }
      } else {
        data.durationDays = null
        data.expiresAt = null
      }
      data.spendingLimit = null // 清除额度限制
    } else {
      data.spendingLimit = editSpendingLimit
      data.expiresAt = null // 清除过期时间
      data.durationDays = null // 清除懒激活
    }
    updateKey(
      { id: editingKey.id, data },
      {
        onSuccess: () => {
          toast.success('已更新')
          setEditingKey(null)
        },
        onError: (err) => toast.error(`更新失败: ${extractErrorMessage(err)}`),
      }
    )
  }

  const handleToggleEnabled = (key: ApiKeyItem) => {
    updateKey(
      { id: key.id, data: { enabled: !key.enabled } },
      {
        onSuccess: () => toast.success(key.enabled ? '已禁用' : '已启用'),
        onError: (err) => toast.error(extractErrorMessage(err)),
      }
    )
  }

  const handleDelete = (key: ApiKeyItem) => {
    if (!confirm(`确定要删除 "${key.name}" 的 API Key 吗？`)) return
    deleteKey(key.id, {
      onSuccess: () => toast.success('已删除'),
      onError: (err) => toast.error(extractErrorMessage(err)),
    })
  }

  const openEdit = (key: ApiKeyItem) => {
    setEditingKey(key)
    setEditName(key.name)
    // 根据 key 类型设置编辑模式
    if (key.spendingLimit != null) {
      setEditMode('quota')
      setEditSpendingLimit(key.spendingLimit)
      setEditDuration(1)
    } else {
      setEditMode('date')
      setEditSpendingLimit(50)
      if (key.durationDays != null && key.durationDays < 1) {
        setEditDuration(Math.round(key.durationDays * 24 * 100) / 100)
        setEditDurationUnit('hours')
      } else {
        setEditDuration(key.durationDays ?? 1)
        setEditDurationUnit('days')
      }
    }
  }

  const maskKey = (key: string) => key.slice(0, 7) + '...' + key.slice(-4)

  const formatDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleString('zh-CN', {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit',
    })
  }

  const formatSerial = (id: number) => `#${String(id).padStart(3, '0')}`

  // 将名称解析为数值（用于编号去重比较），非纯数字返回 null
  const parseNameAsNumber = (name: string): number | null => {
    const trimmed = name.trim()
    if (!/^\d+$/.test(trimmed)) return null
    return parseInt(trimmed, 10)
  }

  // 获取所有已存在的编号数值集合
  const existingNumbers = new Set(
    (apiKeys ?? []).map(k => parseNameAsNumber(k.name)).filter((n): n is number => n !== null)
  )

  // 生成不重复的随机 4 位编号
  const generateUniqueSerial = (): string => {
    for (let i = 0; i < 100; i++) {
      const num = Math.floor(Math.random() * 9999) + 1 // 1-9999
      if (!existingNumbers.has(num)) return String(num).padStart(4, '0')
    }
    // fallback: 找最大值 +1
    const max = existingNumbers.size > 0 ? Math.max(...existingNumbers) : 0
    return String(max + 1).padStart(4, '0')
  }

  // 检查当前输入的名称是否与已有编号冲突
  const nameConflict = (() => {
    const num = parseNameAsNumber(newName)
    if (num === null) return false
    return existingNumbers.has(num)
  })()

  const filteredKeys = (apiKeys ?? []).filter((key) => {
    if (!searchQuery.trim()) return true
    const q = searchQuery.trim().toLowerCase()
    const serialStr = String(key.id).padStart(3, '0')
    return serialStr.includes(q) || String(key.id).includes(q) || key.name.toLowerCase().includes(q)
  })
  return (
    <div className="space-y-4">
      {/* 服务信息 */}
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
            <Key className="h-4 w-4" />
            服务连接信息
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-center justify-between">
            <div>
              <div className="text-xs text-muted-foreground">API Base URL</div>
              <code className="text-sm break-all">{window.location.origin}</code>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => copyToClipboard(window.location.origin, 'url')}
            >
              {copiedUrl ? <Check className="h-4 w-4 text-green-500" /> : <Copy className="h-4 w-4" />}
            </Button>
          </div>
          <div className="flex items-center justify-between">
            <div>
              <div className="text-xs text-muted-foreground">主 API Key</div>
              <code className="text-sm">{serverInfo?.masterApiKey ? maskKey(serverInfo.masterApiKey) : '加载中...'}</code>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => serverInfo?.masterApiKey && copyToClipboard(serverInfo.masterApiKey, 'master')}
              disabled={!serverInfo?.masterApiKey}
            >
              {copiedMaster ? <Check className="h-4 w-4 text-green-500" /> : <Copy className="h-4 w-4" />}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* 统计卡片 */}
      {(() => {
        const all = apiKeys ?? []
        const active = all.filter((k) => getKeyStatus(k) === 'active').length
        const pending = all.filter((k) => getKeyStatus(k) === 'pending').length
        const disabled = all.filter((k) => getKeyStatus(k) === 'disabled').length
        const expired = all.filter((k) => getKeyStatus(k) === 'expired').length
        return (
          <div className="grid gap-4 grid-cols-2 md:grid-cols-5">
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium text-muted-foreground">总数</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold">{all.length}</div>
              </CardContent>
            </Card>
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium text-muted-foreground">启用中</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-green-600">{active}</div>
              </CardContent>
            </Card>
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium text-muted-foreground">待激活</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-gray-500">{pending}</div>
              </CardContent>
            </Card>
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium text-muted-foreground">已禁用</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-red-600">{disabled}</div>
              </CardContent>
            </Card>
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-sm font-medium text-muted-foreground">已过期</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="text-2xl font-bold text-orange-500">{expired}</div>
              </CardContent>
            </Card>
          </div>
        )
      })()}

      {/* API Key 列表 */}
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-semibold">API Key 管理</h2>
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1">
            <ArrowDownWideNarrow className="h-4 w-4 text-muted-foreground" />
            <Button size="sm" variant={sortBy === 'newest' ? 'default' : 'outline'} onClick={() => setSortBy('newest')}>最新</Button>
            <Button size="sm" variant={sortBy === 'cost-desc' ? 'default' : 'outline'} onClick={() => setSortBy('cost-desc')}>费用↓</Button>
            <Button size="sm" variant={sortBy === 'cost-asc' ? 'default' : 'outline'} onClick={() => setSortBy('cost-asc')}>费用↑</Button>
          </div>
          <Button onClick={() => { setNewName(generateUniqueSerial()); setCreateDialogOpen(true) }} size="sm">
            <Plus className="h-4 w-4 mr-2" />
            创建 Key
          </Button>
          {invalidKeys.length > 0 && (
            <Button variant="outline" size="sm" onClick={() => setPurgeDialogOpen(true)}>
              <Trash2 className="h-4 w-4 mr-2" />
              清除无效 ({invalidKeys.length})
            </Button>
          )}
        </div>
      </div>
      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
        <Input
          placeholder="搜索编号或名称..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="pl-9"
        />
      </div>
      {isLoading ? (
        <Card>
          <CardContent className="py-8 text-center text-muted-foreground">加载中...</CardContent>
        </Card>
      ) : !apiKeys || apiKeys.length === 0 ? (
        <Card>
          <CardContent className="py-8 text-center text-muted-foreground">
            暂无用户 API Key，点击"创建 Key"添加
          </CardContent>
        </Card>
      ) : filteredKeys.length === 0 ? (
        <Card>
          <CardContent className="py-8 text-center text-muted-foreground">
            未找到匹配的 API Key
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-3">
          {[...filteredKeys].sort((a, b) => {
            if (sortBy === 'cost-desc') {
              return (usageMap.get(b.id)?.totalCost ?? 0) - (usageMap.get(a.id)?.totalCost ?? 0)
            }
            if (sortBy === 'cost-asc') {
              return (usageMap.get(a.id)?.totalCost ?? 0) - (usageMap.get(b.id)?.totalCost ?? 0)
            }
            return new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
          }).map((apiKey) => {
            const status = getKeyStatus(apiKey)
            const usage = usageMap.get(apiKey.id)
            return (
              <Card key={apiKey.id} className={status === 'disabled' || status === 'expired' ? 'opacity-60' : ''}>
                <CardContent className="py-3 px-3 sm:px-4">
                  <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2">
                    <div className="flex items-center gap-3 min-w-0 flex-1">
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <code className="text-xs text-muted-foreground font-mono">{formatSerial(apiKey.id)}</code>
                          <span className="font-medium truncate">{apiKey.name}</span>
                          <Badge variant={status === 'active' ? 'success' : status === 'pending' ? 'secondary' : status === 'expired' ? 'warning' : 'destructive'}>
                            {status === 'active' ? '启用' : status === 'pending' ? '待激活' : status === 'expired' ? '已过期' : '已禁用'}
                          </Badge>
                        </div>
                        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 mt-1 text-xs text-muted-foreground">
                          <code>{maskKey(apiKey.key)}</code>
                          <span>创建: {formatDate(apiKey.createdAt)}</span>
                          {apiKey.spendingLimit != null ? (
                            <span className="flex items-center gap-1">
                              <DollarSign className="h-3 w-3" />
                              额度: ${(usage?.totalCost ?? 0).toFixed(2)} / ${apiKey.spendingLimit.toFixed(2)}
                            </span>
                          ) : apiKey.durationDays != null && !apiKey.activatedAt ? (
                            <span className="flex items-center gap-1">
                              <Clock className="h-3 w-3" />
                              有效期: {formatDuration(apiKey.durationDays)}（首次使用后激活）
                            </span>
                          ) : apiKey.durationDays != null && apiKey.expiresAt ? (
                            <span className="flex items-center gap-1">
                              <Clock className="h-3 w-3" />
                              到期: {formatDate(apiKey.expiresAt)}（{formatDuration(apiKey.durationDays)}）
                            </span>
                          ) : apiKey.expiresAt ? (
                            <span className="flex items-center gap-1">
                              <Clock className="h-3 w-3" />
                              到期: {formatDate(apiKey.expiresAt)}
                            </span>
                          ) : null}
                        </div>
                        {/* 用量信息（始终显示） */}
                        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 mt-1.5 text-xs">
                          <span className="flex items-center gap-1 text-muted-foreground">
                            <BarChart3 className="h-3 w-3" />
                            {usage?.totalRequests ?? 0} 次请求
                          </span>
                          <span className="text-blue-600 dark:text-blue-400 font-medium">
                            RPM {rpmData?.byApiKey?.[String(apiKey.id)] ?? 0}
                          </span>
                          <span className="text-muted-foreground">
                            入 {formatTokens(usage?.totalInputTokens ?? 0)} / 出 {formatTokens(usage?.totalOutputTokens ?? 0)}
                          </span>
                          <span className="font-medium text-orange-600 dark:text-orange-400">
                            {formatCost(usage?.totalCost ?? 0)}
                          </span>
                          {usage && usage.totalRequests > 0 && (
                            <Button
                              variant="ghost"
                              size="sm"
                              className="h-5 w-5 p-0 text-muted-foreground hover:text-destructive"
                              onClick={() => handleResetUsage(apiKey)}
                              title="重置用量"
                            >
                              <RotateCcw className="h-3 w-3" />
                            </Button>
                          )}
                          {dataUpdatedAt > 0 && (
                            <span className="text-muted-foreground/60">
                              · {new Date(dataUpdatedAt).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' })}
                            </span>
                          )}
                        </div>
                      </div>
                    </div>
                    <div className="flex items-center gap-1 sm:ml-2 self-end sm:self-auto">
                      <Button variant="ghost" size="sm" onClick={() => copyToClipboard(`订单编号: ${apiKey.name}\nBase URL: ${window.location.origin}\nAPI Key: ${apiKey.key}`, apiKey.id)} title="复制 URL 和 Key">
                        {copiedId === apiKey.id ? <Check className="h-4 w-4 text-green-500" /> : <Copy className="h-4 w-4" />}
                      </Button>
                      <Switch checked={apiKey.enabled} onCheckedChange={() => handleToggleEnabled(apiKey)} />
                      <Button variant="ghost" size="sm" onClick={() => openEdit(apiKey)} title="编辑">
                        <Pencil className="h-4 w-4" />
                      </Button>
                      <Button variant="ghost" size="sm" onClick={() => handleDelete(apiKey)} title="删除" className="text-destructive hover:text-destructive">
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>
                </CardContent>
              </Card>
            )
          })}
        </div>
      )}
      {/* 创建对话框 */}
      <Dialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>创建 API Key</DialogTitle>
            <DialogDescription>为用户创建一个新的 API Key</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <label className="text-sm font-medium">编号</label>
              <Input
                placeholder="4 位编号，如 0001"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
              />
              {nameConflict && (
                <p className="text-xs text-destructive mt-1">该编号已存在，请更换</p>
              )}
            </div>
            <div>
              <label className="text-sm font-medium">限制方式</label>
              <div className="flex gap-2 mt-2">
                <Button
                  type="button"
                  size="sm"
                  variant={newMode === 'date' ? 'default' : 'outline'}
                  onClick={() => setNewMode('date')}
                >
                  <Clock className="h-3.5 w-3.5 mr-1.5" />
                  按日期
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant={newMode === 'quota' ? 'default' : 'outline'}
                  onClick={() => setNewMode('quota')}
                >
                  <DollarSign className="h-3.5 w-3.5 mr-1.5" />
                  按额度
                </Button>
              </div>
            </div>
            {newMode === 'date' ? (
              <div>
                <label className="text-sm font-medium">有效期</label>
                <div className="flex flex-wrap gap-2 mt-2">
                  {quickDurationOptions.map((opt) => (
                    <Button
                      key={opt.label}
                      type="button"
                      size="sm"
                      variant={newDuration === opt.value && newDurationUnit === opt.unit ? 'default' : 'outline'}
                      onClick={() => { setNewDuration(opt.value); setNewDurationUnit(opt.unit) }}
                    >
                      {opt.label}
                    </Button>
                  ))}
                  <Button
                    type="button"
                    size="sm"
                    variant={newDuration === null ? 'default' : 'outline'}
                    onClick={() => setNewDuration(null)}
                  >
                    永不过期
                  </Button>
                </div>
                {newDuration !== null && (
                  <div className="flex items-center gap-2 mt-2">
                    <Input
                      type="number"
                      min={1}
                      value={newDuration}
                      onChange={(e) => setNewDuration(Math.max(1, Number(e.target.value)))}
                      className="w-24"
                    />
                    <div className="flex gap-1">
                      <Button type="button" size="sm" variant={newDurationUnit === 'hours' ? 'default' : 'outline'} onClick={() => setNewDurationUnit('hours')}>小时</Button>
                      <Button type="button" size="sm" variant={newDurationUnit === 'days' ? 'default' : 'outline'} onClick={() => setNewDurationUnit('days')}>天</Button>
                    </div>
                  </div>
                )}
                <div className="text-xs text-muted-foreground mt-2">
                  <Clock className="h-3 w-3 inline mr-1" />
                  {newDuration !== null ? `首次使用后 ${newDuration} ${newDurationUnit === 'hours' ? '小时' : '天'}到期` : '永不过期'}
                </div>
              </div>
            ) : (
              <div>
                <label className="text-sm font-medium">额度上限（美元）</label>
                <div className="flex flex-wrap gap-2 mt-2">
                  {[100, 500, 1000].map((amount) => (
                    <Button
                      key={amount}
                      type="button"
                      size="sm"
                      variant={newSpendingLimit === amount ? 'default' : 'outline'}
                      onClick={() => setNewSpendingLimit(amount)}
                    >
                      ${amount}
                    </Button>
                  ))}
                </div>
                <div className="flex items-center gap-2 mt-2">
                  <span className="text-sm text-muted-foreground">自定义 $</span>
                  <Input
                    type="text"
                    inputMode="numeric"
                    value={newSpendingLimit || ''}
                    onChange={(e) => {
                      const v = e.target.value.replace(/\D/g, '')
                      setNewSpendingLimit(v === '' ? 0 : Number(v))
                    }}
                    onFocus={(e) => e.target.select()}
                    className="w-32"
                  />
                </div>
                <div className="text-xs text-muted-foreground mt-2">
                  <DollarSign className="h-3 w-3 inline mr-1" />
                  累计用量达到 ${newSpendingLimit} 后自动停用
                </div>
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateDialogOpen(false)}>取消</Button>
            <Button onClick={handleCreate} disabled={!newName.trim() || nameConflict || isCreating}>
              {isCreating ? '创建中...' : '创建'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 编辑对话框 */}
      <Dialog open={!!editingKey} onOpenChange={(open) => !open && setEditingKey(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>编辑 API Key</DialogTitle>
            <DialogDescription>修改备注或续期</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <label className="text-sm font-medium">备注名称</label>
              <Input
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
              />
            </div>
            <div>
              <label className="text-sm font-medium">限制方式</label>
              <div className="flex gap-2 mt-2">
                <Button
                  type="button"
                  size="sm"
                  variant={editMode === 'date' ? 'default' : 'outline'}
                  onClick={() => setEditMode('date')}
                >
                  <Clock className="h-3.5 w-3.5 mr-1.5" />
                  按日期
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant={editMode === 'quota' ? 'default' : 'outline'}
                  onClick={() => setEditMode('quota')}
                >
                  <DollarSign className="h-3.5 w-3.5 mr-1.5" />
                  按额度
                </Button>
              </div>
            </div>
            {editMode === 'date' ? (
              <div>
                <label className="text-sm font-medium">续期时长</label>
                {editingKey?.activatedAt ? (
                  <div className="text-xs text-muted-foreground mt-1">
                    已激活: {formatDate(editingKey.activatedAt)}
                    {editingKey.expiresAt && ` · 到期: ${formatDate(editingKey.expiresAt)}`}
                  </div>
                ) : editingKey?.durationDays != null ? (
                  <div className="text-xs text-muted-foreground mt-1">
                    待激活（{formatDuration(editingKey.durationDays)}）
                  </div>
                ) : editingKey?.expiresAt && new Date(editingKey.expiresAt) > new Date() ? (
                  <div className="text-xs text-muted-foreground mt-1">
                    当前到期: {new Date(editingKey.expiresAt).toLocaleString('zh-CN', { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })}
                  </div>
                ) : null}
                <div className="flex flex-wrap gap-2 mt-2">
                  {quickDurationOptions.map((opt) => (
                    <Button
                      key={opt.label}
                      type="button"
                      size="sm"
                      variant={editDuration === opt.value && editDurationUnit === opt.unit ? 'default' : 'outline'}
                      onClick={() => { setEditDuration(opt.value); setEditDurationUnit(opt.unit) }}
                    >
                      {opt.label}
                    </Button>
                  ))}
                  <Button
                    type="button"
                    size="sm"
                    variant={editDuration === null ? 'default' : 'outline'}
                    onClick={() => setEditDuration(null)}
                  >
                    永不过期
                  </Button>
                </div>
                {editDuration !== null && (
                  <div className="flex items-center gap-2 mt-2">
                    <Input
                      type="number"
                      min={1}
                      value={editDuration}
                      onChange={(e) => {
                        const v = e.target.value
                        setEditDuration(v === '' ? '' : Math.max(1, Number(v)))
                      }}
                      className="w-24"
                    />
                    <div className="flex gap-1">
                      <Button type="button" size="sm" variant={editDurationUnit === 'hours' ? 'default' : 'outline'} onClick={() => setEditDurationUnit('hours')}>小时</Button>
                      <Button type="button" size="sm" variant={editDurationUnit === 'days' ? 'default' : 'outline'} onClick={() => setEditDurationUnit('days')}>天</Button>
                    </div>
                  </div>
                )}
                <div className="text-xs text-muted-foreground mt-2">
                  <Clock className="h-3 w-3 inline mr-1" />
                  {editDuration !== null && editDuration !== ''
                    ? (editingKey && getKeyStatus(editingKey) === 'active'
                        ? `将在当前到期时间上续期 ${editDuration} ${editDurationUnit === 'hours' ? '小时' : '天'}`
                        : `首次使用后 ${editDuration} ${editDurationUnit === 'hours' ? '小时' : '天'}到期`)
                    : '永不过期'}
                </div>
              </div>
            ) : (
              <div>
                <label className="text-sm font-medium">额度上限（美元）</label>
                <div className="flex items-center gap-2 mt-2">
                  <span className="text-sm text-muted-foreground">$</span>
                  <Input
                    type="number"
                    min={1}
                    step={1}
                    value={editSpendingLimit}
                    onChange={(e) => setEditSpendingLimit(Number(e.target.value))}
                    className="w-32"
                  />
                </div>
                <div className="text-xs text-muted-foreground mt-2">
                  <DollarSign className="h-3 w-3 inline mr-1" />
                  累计用量达到 ${editSpendingLimit} 后自动停用
                </div>
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditingKey(null)}>取消</Button>
            <Button onClick={handleUpdate}>保存</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 清除无效 Key 对话框 */}
      <Dialog open={purgeDialogOpen} onOpenChange={(open) => !purging && setPurgeDialogOpen(open)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>清除无效 API Key</DialogTitle>
            <DialogDescription>
              将删除以下 {invalidKeys.length} 个已禁用或已过期的 Key，此操作不可撤销。
            </DialogDescription>
          </DialogHeader>
          <div className="max-h-60 overflow-y-auto space-y-1 text-sm">
            {invalidKeys.map((k) => (
              <div key={k.id} className="flex items-center justify-between py-1 px-2 rounded bg-muted/50">
                <span>
                  <code className="text-xs font-mono text-muted-foreground mr-2">{String(k.id).padStart(3, '0')}</code>
                  {k.name}
                </span>
                <Badge variant={getKeyStatus(k) === 'disabled' ? 'destructive' : 'warning'} className="text-xs">
                  {getKeyStatus(k) === 'disabled' ? '已禁用' : '已过期'}
                </Badge>
              </div>
            ))}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPurgeDialogOpen(false)} disabled={purging}>取消</Button>
            <Button variant="destructive" onClick={handlePurge} disabled={purging}>
              {purging ? <><Loader2 className="h-4 w-4 mr-2 animate-spin" />清除中...</> : `确认清除 (${invalidKeys.length})`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
