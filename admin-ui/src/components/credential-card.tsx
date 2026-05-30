// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from 'sonner'
import { RefreshCw, Wallet, Trash2, Loader2, Pencil, FileText } from 'lucide-react'
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
import { Input } from '@/components/ui/input'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type { CredentialStatusItem, BalanceResponse } from '@/types/api'
import {
  useSetDisabled,
  useSetPriority,
  useResetFailure,
  useDeleteCredential,
} from '@/hooks/use-credentials'
import { EditCredentialDialog } from './edit-credential-dialog'

interface CredentialCardProps {
  credential: CredentialStatusItem
  onViewBalance: (id: number) => void
  onViewDetail: (id: number) => void
  selected: boolean
  onToggleSelect: () => void
  balance: BalanceResponse | null
  loadingBalance: boolean
  rpm?: number
}

function formatLastUsed(lastUsedAt: string | null): string {
  if (!lastUsedAt) return '从未使用'
  const date = new Date(lastUsedAt)
  const now = new Date()
  const diff = now.getTime() - date.getTime()
  if (diff < 0) return '刚刚'
  const seconds = Math.floor(diff / 1000)
  if (seconds < 60) return `${seconds} 秒前`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes} 分钟前`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours} 小时前`
  const days = Math.floor(hours / 24)
  return `${days} 天前`
}

type HealthStatus = CredentialStatusItem['healthStatus']

const HEALTH_CONFIG: Record<HealthStatus, { label: string; className: string; dotClass: string }> = {
  healthy:   { label: '健康',   className: 'bg-neon-green/10 text-neon-green border-neon-green/30',     dotClass: 'bg-green-400' },
  warning:   { label: '警告',   className: 'bg-neon-yellow/10 text-neon-yellow border-neon-yellow/30', dotClass: 'bg-yellow-400' },
  degraded:  { label: '降级',   className: 'bg-orange-500/10 text-orange-400 border-orange-500/30',    dotClass: 'bg-orange-400' },
  unhealthy: { label: '不健康', className: 'bg-neon-red/10 text-neon-red border-neon-red/30',          dotClass: 'bg-red-400' },
  disabled:  { label: '已禁用', className: 'bg-gray-500/10 text-gray-400 border-gray-500/30',          dotClass: 'bg-gray-400' },
}

function HealthBadge({ status }: { status: HealthStatus }) {
  const cfg = HEALTH_CONFIG[status] ?? HEALTH_CONFIG.disabled
  return (
    <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium border ${cfg.className}`}>
      <span className="w-1.5 h-1.5 rounded-full bg-current opacity-80" />
      {cfg.label}
    </span>
  )
}

export function CredentialCard({
  credential,
  onViewBalance,
  onViewDetail,
  selected,
  onToggleSelect,
  balance,
  loadingBalance,
  rpm = 0,
}: CredentialCardProps) {
  const [editingPriority, setEditingPriority] = useState(false)
  const [priorityValue, setPriorityValue] = useState(String(credential.priority))
  const [showDeleteDialog, setShowDeleteDialog] = useState(false)
  const [showEditDialog, setShowEditDialog] = useState(false)

  const setDisabled = useSetDisabled()
  const setPriority = useSetPriority()
  const resetFailure = useResetFailure()
  const deleteCredential = useDeleteCredential()

  const handleToggleDisabled = () => {
    setDisabled.mutate(
      { id: credential.id, disabled: !credential.disabled },
      {
        onSuccess: (res) => toast.success(res.message),
        onError: (err) => toast.error('操作失败: ' + (err as Error).message),
      }
    )
  }

  const handlePriorityChange = () => {
    const newPriority = parseInt(priorityValue, 10)
    if (isNaN(newPriority) || newPriority < 0) {
      toast.error('优先级必须是非负整数')
      return
    }
    setPriority.mutate(
      { id: credential.id, priority: newPriority },
      {
        onSuccess: (res) => {
          toast.success(res.message)
          setEditingPriority(false)
        },
        onError: (err) => toast.error('操作失败: ' + (err as Error).message),
      }
    )
  }

  const handleReset = () => {
    resetFailure.mutate(credential.id, {
      onSuccess: (res) => toast.success(res.message),
      onError: (err) => toast.error('操作失败: ' + (err as Error).message),
    })
  }

  const handleDelete = () => {
    if (!credential.disabled) {
      toast.error('请先禁用账号再删除')
      setShowDeleteDialog(false)
      return
    }
    deleteCredential.mutate(credential.id, {
      onSuccess: (res) => {
        toast.success(res.message)
        setShowDeleteDialog(false)
      },
      onError: (err) => toast.error('删除失败: ' + (err as Error).message),
    })
  }

  return (
    <>
      <Card className={[
        credential.isCurrent ? 'ring-2 ring-primary' : '',
        credential.disabled ? 'opacity-60' : '',
      ].filter(Boolean).join(' ')}>
        <CardContent className="py-3 px-3 sm:px-4">
          <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2">
            {/* 左侧信息区 */}
            <div className="flex items-start gap-2.5 min-w-0 flex-1">
              <Checkbox
                checked={selected}
                onCheckedChange={onToggleSelect}
                className="mt-0.5 shrink-0"
              />
              <div className="min-w-0 flex-1">
                {/* 行1：标识 + 状态徽章 */}
                <div className="flex items-center gap-2 flex-wrap">
                  <code className="text-xs text-muted-foreground font-mono">#{String(credential.id).padStart(3, '0')}</code>
                  <span className="font-medium truncate">
                    {credential.nickname || `账号 #${credential.id}`}
                  </span>
                  <HealthBadge status={credential.healthStatus} />
                  {credential.isCurrent && <Badge variant="success">当前</Badge>}
                  {credential.disabled && <Badge variant="destructive">已禁用</Badge>}
                </div>

                {/* 行2：账号 + 最后调用 */}
                <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 mt-1 text-xs text-muted-foreground">
                  {credential.email && <span>{credential.email}</span>}
                  <span>调用：{formatLastUsed(credential.lastUsedAt)}</span>
                  {credential.hasProxy && credential.proxyUrl && (
                    <span className="text-blue-500 truncate max-w-[200px]">代理：{credential.proxyUrl}</span>
                  )}
                  {credential.hasProfileArn && (
                    <Badge variant="secondary" className="text-xs h-4">Profile ARN</Badge>
                  )}
                </div>

                {/* 行3：数值统计 */}
                <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 mt-1 text-xs">
                  <span className="text-muted-foreground flex items-center gap-1">
                    优先级：
                    {editingPriority ? (
                      <span className="inline-flex items-center gap-1">
                        <Input
                          type="number"
                          value={priorityValue}
                          onChange={(e) => setPriorityValue(e.target.value)}
                          className="w-14 h-6 text-xs px-1"
                          min="0"
                          autoFocus
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') handlePriorityChange()
                            if (e.key === 'Escape') { setEditingPriority(false); setPriorityValue(String(credential.priority)) }
                          }}
                        />
                        <button className="text-green-500 hover:text-green-400" onClick={handlePriorityChange} disabled={setPriority.isPending}>✓</button>
                        <button className="text-muted-foreground hover:text-foreground" onClick={() => { setEditingPriority(false); setPriorityValue(String(credential.priority)) }}>✕</button>
                      </span>
                    ) : (
                      <span
                        className="font-medium text-foreground cursor-pointer hover:underline"
                        onClick={() => setEditingPriority(true)}
                      >
                        {credential.priority}
                      </span>
                    )}
                  </span>
                  <span className={credential.failureCount > 0 ? 'text-red-500 font-medium' : 'text-muted-foreground'}>
                    失败：{credential.failureCount}
                  </span>
                  <span className="text-muted-foreground">成功：{credential.successCount}</span>
                  <span className={credential.throttleCount > 0 ? 'text-orange-500 font-medium' : 'text-muted-foreground'}>
                    限流：{credential.throttleCount}
                  </span>
                  <span className="text-blue-600 dark:text-blue-400 font-medium">RPM {rpm}</span>
                  <span className="text-muted-foreground">
                    剩余：
                    {loadingBalance ? (
                      <Loader2 className="inline w-3 h-3 animate-spin ml-1" />
                    ) : balance ? (
                      <span className="font-medium text-foreground">
                        {balance.remaining.toFixed(1)}/{balance.usageLimit.toFixed(1)}
                        <span className="text-muted-foreground ml-1">({(100 - balance.usagePercentage).toFixed(0)}%剩)</span>
                      </span>
                    ) : (
                      <span>未知</span>
                    )}
                  </span>
                  {balance && (
                    <span className="text-muted-foreground/60 text-xs">
                      {balance.subscriptionTitle}
                    </span>
                  )}
                </div>
              </div>
            </div>

            {/* 右侧操作区 */}
            <div className="flex items-center gap-1 sm:ml-2 self-end sm:self-auto shrink-0">
              <Switch
                checked={!credential.disabled}
                onCheckedChange={handleToggleDisabled}
                disabled={setDisabled.isPending}
              />
              <Button
                variant="ghost" size="sm"
                className="h-8 w-8 p-0"
                onClick={() => onViewBalance(credential.id)}
                title="查看余额"
              >
                <Wallet className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost" size="sm"
                className="h-8 w-8 p-0"
                onClick={() => onViewDetail(credential.id)}
                title="查看日志"
              >
                <FileText className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost" size="sm"
                className="h-8 w-8 p-0"
                onClick={() => setShowEditDialog(true)}
                title="编辑"
              >
                <Pencil className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost" size="sm"
                className="h-8 w-8 p-0 text-muted-foreground hover:text-foreground"
                onClick={handleReset}
                disabled={resetFailure.isPending || credential.failureCount === 0}
                title="重置失败计数"
              >
                <RefreshCw className="h-4 w-4" />
              </Button>
              <Button
                variant="ghost" size="sm"
                className="h-8 w-8 p-0 text-destructive hover:text-destructive"
                onClick={() => setShowDeleteDialog(true)}
                disabled={!credential.disabled}
                title={!credential.disabled ? '需要先禁用账号才能删除' : '删除'}
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <Dialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>确认删除账号</DialogTitle>
            <DialogDescription>
              您确定要删除账号 #{credential.id} 吗？此操作无法撤销。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowDeleteDialog(false)} disabled={deleteCredential.isPending}>
              取消
            </Button>
            <Button variant="destructive" onClick={handleDelete} disabled={deleteCredential.isPending || !credential.disabled}>
              确认删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <EditCredentialDialog
        open={showEditDialog}
        onOpenChange={setShowEditDialog}
        credential={credential}
      />
    </>
  )
}
