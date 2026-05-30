// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useEffect, useRef } from 'react'
import { RefreshCw, LogOut, Server, Plus, Upload, FileUp, Trash2, RotateCcw, CheckCircle2, Key, Settings, BarChart2 } from 'lucide-react'
import kiroIcon from '@/assets/kiro-icon.png'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { storage } from '@/lib/storage'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { CredentialCard } from '@/components/credential-card'
import { BalanceDialog } from '@/components/balance-dialog'
import { AddCredentialDialog } from '@/components/add-credential-dialog'
import { BatchImportDialog } from '@/components/batch-import-dialog'
import { KamImportDialog } from '@/components/kam-import-dialog'
import { BatchVerifyDialog, type VerifyResult } from '@/components/batch-verify-dialog'
import { ApiKeysPanel } from '@/components/api-keys-panel'
import { ApiKeyDetailPage } from '@/components/api-key-detail-page'
import { CredentialDetailPage } from '@/components/credential-detail-page'
import { SettingsPanel } from '@/components/settings-panel'
import { useCredentials, useDeleteCredential, useResetFailure, useRpm, useDailyUsage } from '@/hooks/use-credentials'
import { DailyStatsPage } from '@/components/daily-stats-page'
import { DailyDetailPage } from '@/components/daily-detail-page'
import { getCredentialBalance } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { BalanceResponse, ApiKeyItem } from '@/types/api'

interface DashboardProps {
  onLogout: () => void
}

export function Dashboard({ onLogout }: DashboardProps) {
  const [activeTab, setActiveTab] = useState<'credentials' | 'apikeys' | 'settings'>('credentials')
  const [detailKeyId, setDetailKeyId] = useState<number | null>(null)
  const [detailCredentialId, setDetailCredentialId] = useState<number | null>(null)
  const [selectedCredentialId, setSelectedCredentialId] = useState<number | null>(null)
  const [balanceDialogOpen, setBalanceDialogOpen] = useState(false)
  const [addDialogOpen, setAddDialogOpen] = useState(false)
  const [batchImportDialogOpen, setBatchImportDialogOpen] = useState(false)
  const [kamImportDialogOpen, setKamImportDialogOpen] = useState(false)
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set())
  const [verifyDialogOpen, setVerifyDialogOpen] = useState(false)
  const [verifying, setVerifying] = useState(false)
  const [verifyProgress, setVerifyProgress] = useState({ current: 0, total: 0 })
  const [verifyResults, setVerifyResults] = useState<Map<number, VerifyResult>>(new Map())
  const [balanceMap, setBalanceMap] = useState<Map<number, BalanceResponse>>(new Map())
  const [loadingBalanceIds, setLoadingBalanceIds] = useState<Set<number>>(new Set())
  const [queryingInfo, setQueryingInfo] = useState(false)
  const [queryInfoProgress, setQueryInfoProgress] = useState({ current: 0, total: 0 })
  const [liveCreditsTotal, setLiveCreditsTotal] = useState<number | null>(null)
  const [liveCreditsQueried, setLiveCreditsQueried] = useState(0)
  const [dailyView, setDailyView] = useState<string | null>(null)
  const cancelVerifyRef = useRef(false)
  const prevTabRef = useRef<'credentials' | 'apikeys' | 'settings' | null>(null)
  const prevDetailCredentialId = useRef<number | null>(null)
  const prevDailyView = useRef<string | null>(null)
  const initialBalanceFetchDone = useRef(false)
  const [currentPage, setCurrentPage] = useState(1)
  const itemsPerPage = 12
  const queryClient = useQueryClient()
  const { data, isLoading, error, refetch } = useCredentials()
  const credentialsRef = useRef(data?.credentials)
  const { data: rpmData } = useRpm()
  const { mutate: deleteCredential } = useDeleteCredential()
  const { mutate: resetFailure } = useResetFailure()
  const { data: dailyUsageData } = useDailyUsage()

  const todayLocal = (() => {
    const d = new Date()
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  })()
  const todayStats = dailyUsageData?.find((d) => d.date === todayLocal) ?? null

  // 计算分页
  const totalPages = Math.ceil((data?.credentials.length || 0) / itemsPerPage)
  const startIndex = (currentPage - 1) * itemsPerPage
  const endIndex = startIndex + itemsPerPage
  const currentCredentials = data?.credentials.slice(startIndex, endIndex) || []
  const disabledCredentialCount = data?.credentials.filter(credential => credential.disabled).length || 0
  const selectedDisabledCount = Array.from(selectedIds).filter(id => {
    const credential = data?.credentials.find(c => c.id === id)
    return Boolean(credential?.disabled)
  }).length

  // 当凭据列表变化时重置到第一页
  useEffect(() => {
    setCurrentPage(1)
  }, [data?.credentials.length])

  // 只保留当前仍存在的凭据缓存，避免删除后残留旧数据
  useEffect(() => {
    if (!data?.credentials) {
      setBalanceMap(new Map())
      setLoadingBalanceIds(new Set())
      return
    }

    const validIds = new Set(data.credentials.map(credential => credential.id))

    setBalanceMap(prev => {
      const next = new Map<number, BalanceResponse>()
      prev.forEach((value, id) => {
        if (validIds.has(id)) {
          next.set(id, value)
        }
      })
      return next.size === prev.size ? prev : next
    })

    setLoadingBalanceIds(prev => {
      if (prev.size === 0) {
        return prev
      }
      const next = new Set<number>()
      prev.forEach(id => {
        if (validIds.has(id)) {
          next.add(id)
        }
      })
      return next.size === prev.size ? prev : next
    })
  }, [data?.credentials])

  // 始终保持 ref 与最新 credentials 同步
  useEffect(() => {
    credentialsRef.current = data?.credentials
  })

  // 启动时首次加载凭据后自动拉取余额
  useEffect(() => {
    if (!data?.credentials || initialBalanceFetchDone.current) return
    initialBalanceFetchDone.current = true
    const ids = data.credentials.filter(c => !c.disabled).map(c => c.id)
    if (ids.length === 0) return
    ;(async () => {
      let runningTotal = 0
      let queried = 0
      setLiveCreditsTotal(0)
      setLiveCreditsQueried(0)
      for (const id of ids) {
        setLoadingBalanceIds(prev => { const next = new Set(prev); next.add(id); return next })
        try {
          const balance = await getCredentialBalance(id)
          runningTotal += balance.remaining
          setBalanceMap(prev => { const next = new Map(prev); next.set(id, balance); return next })
          setLiveCreditsTotal(runningTotal)
        } catch (_) {
          // 静默失败
        } finally {
          setLoadingBalanceIds(prev => { const next = new Set(prev); next.delete(id); return next })
          setLiveCreditsQueried(++queried)
        }
      }
    })()
  }, [data?.credentials]) // eslint-disable-line react-hooks/exhaustive-deps

  // 从详情页/日志页返回主视图时刷新数据
  useEffect(() => {
    const returningFromDetail = prevDetailCredentialId.current !== null && detailCredentialId === null
    const returningFromDaily = prevDailyView.current !== null && dailyView === null
    if (returningFromDetail || returningFromDaily) {
      refetch()
      queryClient.invalidateQueries({ queryKey: ['dailyUsage'] })
    }
    prevDetailCredentialId.current = detailCredentialId
    prevDailyView.current = dailyView
  }, [detailCredentialId, dailyView]) // eslint-disable-line react-hooks/exhaustive-deps

  // 切换到凭据管理页时静默刷新所有余额
  useEffect(() => {
    if (prevTabRef.current !== null && prevTabRef.current !== 'credentials' && activeTab === 'credentials') {
      refetch()
      queryClient.invalidateQueries({ queryKey: ['dailyUsage'] })
      const ids = (credentialsRef.current || []).filter(c => !c.disabled).map(c => c.id)
      if (ids.length === 0) {
        prevTabRef.current = activeTab
        return
      }
      ;(async () => {
        let runningTotal = 0
        let queried = 0
        setLiveCreditsTotal(0)
        setLiveCreditsQueried(0)
        for (const id of ids) {
          setLoadingBalanceIds(prev => { const next = new Set(prev); next.add(id); return next })
          try {
            const balance = await getCredentialBalance(id)
            runningTotal += balance.remaining
            setBalanceMap(prev => { const next = new Map(prev); next.set(id, balance); return next })
            setLiveCreditsTotal(runningTotal)
          } catch (_) {
            // 静默失败
          } finally {
            setLoadingBalanceIds(prev => { const next = new Set(prev); next.delete(id); return next })
            setLiveCreditsQueried(++queried)
          }
        }
      })()
    }
    prevTabRef.current = activeTab
  }, [activeTab]) // eslint-disable-line react-hooks/exhaustive-deps

  const handleViewBalance = (id: number) => {
    setSelectedCredentialId(id)
    setBalanceDialogOpen(true)
  }

  const handleRefresh = () => {
    refetch()
    toast.success('已刷新账号列表')
  }

  const handleLogout = () => {
    storage.removeApiKey()
    queryClient.clear()
    onLogout()
  }

  // 选择管理
  const toggleSelect = (id: number) => {
    const newSelected = new Set(selectedIds)
    if (newSelected.has(id)) {
      newSelected.delete(id)
    } else {
      newSelected.add(id)
    }
    setSelectedIds(newSelected)
  }

  const deselectAll = () => {
    setSelectedIds(new Set())
  }

  // 批量删除（仅删除已禁用项）
  const handleBatchDelete = async () => {
    if (selectedIds.size === 0) {
      toast.error('请先选择要删除的账号')
      return
    }

    const disabledIds = Array.from(selectedIds).filter(id => {
      const credential = data?.credentials.find(c => c.id === id)
      return Boolean(credential?.disabled)
    })

    if (disabledIds.length === 0) {
      toast.error('选中的账号中没有已禁用项')
      return
    }

    const skippedCount = selectedIds.size - disabledIds.length
    const skippedText = skippedCount > 0 ? `（将跳过 ${skippedCount} 个未禁用账号）` : ''

    if (!confirm(`确定要删除 ${disabledIds.length} 个已禁用账号吗？此操作无法撤销。${skippedText}`)) {
      return
    }

    let successCount = 0
    let failCount = 0

    for (const id of disabledIds) {
      try {
        await new Promise<void>((resolve, reject) => {
          deleteCredential(id, {
            onSuccess: () => {
              successCount++
              resolve()
            },
            onError: (err) => {
              failCount++
              reject(err)
            }
          })
        })
      } catch (error) {
        // 错误已在 onError 中处理
      }
    }

    const skippedResultText = skippedCount > 0 ? `，已跳过 ${skippedCount} 个未禁用账号` : ''

    if (failCount === 0) {
      toast.success(`成功删除 ${successCount} 个已禁用账号${skippedResultText}`)
    } else {
      toast.warning(`删除已禁用账号：成功 ${successCount} 个，失败 ${failCount} 个${skippedResultText}`)
    }

    deselectAll()
  }

  // 批量恢复异常
  const handleBatchResetFailure = async () => {
    if (selectedIds.size === 0) {
      toast.error('请先选择要恢复的账号')
      return
    }

    const failedIds = Array.from(selectedIds).filter(id => {
      const cred = data?.credentials.find(c => c.id === id)
      return cred && cred.failureCount > 0
    })

    if (failedIds.length === 0) {
      toast.error('选中的账号中没有失败的账号')
      return
    }

    let successCount = 0
    let failCount = 0

    for (const id of failedIds) {
      try {
        await new Promise<void>((resolve, reject) => {
          resetFailure(id, {
            onSuccess: () => {
              successCount++
              resolve()
            },
            onError: (err) => {
              failCount++
              reject(err)
            }
          })
        })
      } catch (error) {
        // 错误已在 onError 中处理
      }
    }

    if (failCount === 0) {
      toast.success(`成功恢复 ${successCount} 个账号`)
    } else {
      toast.warning(`成功 ${successCount} 个，失败 ${failCount} 个`)
    }

    deselectAll()
  }

  // 一键清除所有已禁用凭据
  const handleClearAll = async () => {
    if (!data?.credentials || data.credentials.length === 0) {
      toast.error('没有可清除的账号')
      return
    }

    const disabledCredentials = data.credentials.filter(credential => credential.disabled)

    if (disabledCredentials.length === 0) {
      toast.error('没有可清除的已禁用账号')
      return
    }

    if (!confirm(`确定要清除所有 ${disabledCredentials.length} 个已禁用账号吗？此操作无法撤销。`)) {
      return
    }

    let successCount = 0
    let failCount = 0

    for (const credential of disabledCredentials) {
      try {
        await new Promise<void>((resolve, reject) => {
          deleteCredential(credential.id, {
            onSuccess: () => {
              successCount++
              resolve()
            },
            onError: (err) => {
              failCount++
              reject(err)
            }
          })
        })
      } catch (error) {
        // 错误已在 onError 中处理
      }
    }

    if (failCount === 0) {
      toast.success(`成功清除所有 ${successCount} 个已禁用账号`)
    } else {
      toast.warning(`清除已禁用账号：成功 ${successCount} 个，失败 ${failCount} 个`)
    }

    deselectAll()
  }

  // 查询所有凭据信息（逐个查询，避免瞬时并发）
  const handleQueryCurrentPageInfo = async () => {
    const allCredentials = data?.credentials || []

    if (allCredentials.length === 0) {
      toast.error('没有可查询的账号')
      return
    }

    const ids = allCredentials
      .filter(credential => !credential.disabled)
      .map(credential => credential.id)

    if (ids.length === 0) {
      toast.error('没有可查询的启用账号')
      return
    }

    setQueryingInfo(true)
    setQueryInfoProgress({ current: 0, total: ids.length })
    setLiveCreditsTotal(0)
    setLiveCreditsQueried(0)

    let successCount = 0
    let failCount = 0
    let runningTotal = 0

    for (let i = 0; i < ids.length; i++) {
      const id = ids[i]

      setLoadingBalanceIds(prev => {
        const next = new Set(prev)
        next.add(id)
        return next
      })

      try {
        const balance = await getCredentialBalance(id)
        successCount++
        runningTotal += balance.remaining

        setBalanceMap(prev => {
          const next = new Map(prev)
          next.set(id, balance)
          return next
        })

        setLiveCreditsTotal(runningTotal)
        setLiveCreditsQueried(i + 1)
      } catch (error) {
        failCount++
        setLiveCreditsQueried(i + 1)
      } finally {
        setLoadingBalanceIds(prev => {
          const next = new Set(prev)
          next.delete(id)
          return next
        })
      }

      setQueryInfoProgress({ current: i + 1, total: ids.length })
    }

    setQueryingInfo(false)

    if (failCount === 0) {
      toast.success(`查询完成：成功 ${successCount}/${ids.length}`)
    } else {
      toast.warning(`查询完成：成功 ${successCount} 个，失败 ${failCount} 个`)
    }
  }

  // 批量验活
  const handleBatchVerify = async () => {
    if (selectedIds.size === 0) {
      toast.error('请先选择要验活的账号')
      return
    }

    // 初始化状态
    setVerifying(true)
    cancelVerifyRef.current = false
    const ids = Array.from(selectedIds)
    setVerifyProgress({ current: 0, total: ids.length })

    let successCount = 0

    // 初始化结果，所有凭据状态为 pending
    const initialResults = new Map<number, VerifyResult>()
    ids.forEach(id => {
      initialResults.set(id, { id, status: 'pending' })
    })
    setVerifyResults(initialResults)
    setVerifyDialogOpen(true)

    // 开始验活
    for (let i = 0; i < ids.length; i++) {
      // 检查是否取消
      if (cancelVerifyRef.current) {
        toast.info('已取消验活')
        break
      }

      const id = ids[i]

      // 更新当前凭据状态为 verifying
      setVerifyResults(prev => {
        const newResults = new Map(prev)
        newResults.set(id, { id, status: 'verifying' })
        return newResults
      })

      try {
        const balance = await getCredentialBalance(id)
        successCount++

        // 更新为成功状态
        setVerifyResults(prev => {
          const newResults = new Map(prev)
          newResults.set(id, {
            id,
            status: 'success',
            usage: `${balance.currentUsage}/${balance.usageLimit}`
          })
          return newResults
        })
      } catch (error) {
        // 更新为失败状态
        setVerifyResults(prev => {
          const newResults = new Map(prev)
          newResults.set(id, {
            id,
            status: 'failed',
            error: extractErrorMessage(error)
          })
          return newResults
        })
      }

      // 更新进度
      setVerifyProgress({ current: i + 1, total: ids.length })

      // 添加延迟防止封号（最后一个不需要延迟）
      if (i < ids.length - 1 && !cancelVerifyRef.current) {
        await new Promise(resolve => setTimeout(resolve, 2000))
      }
    }

    setVerifying(false)

    if (!cancelVerifyRef.current) {
      toast.success(`验活完成：成功 ${successCount}/${ids.length}`)
    }
  }

  // 取消验活
  const handleCancelVerify = () => {
    cancelVerifyRef.current = true
    setVerifying(false)
  }

  // 切换负载均衡模式
  if (isLoading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background">
        <div className="text-center">
          <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-primary mx-auto mb-4"></div>
          <p className="text-muted-foreground">加载中...</p>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background p-4">
        <Card className="w-full max-w-md">
          <CardContent className="pt-6 text-center">
            <div className="text-red-500 mb-4">加载失败</div>
            <p className="text-muted-foreground mb-4">{(error as Error).message}</p>
            <div className="space-x-2">
              <Button onClick={() => refetch()}>重试</Button>
              <Button variant="outline" onClick={handleLogout}>重新登录</Button>
            </div>
          </CardContent>
        </Card>
      </div>
    )
  }

  return (
    <div className="flex min-h-screen bg-background">
      {/* 左侧 Sidebar */}
      <aside className="w-[232px] bg-card border-r border-border fixed top-0 left-0 bottom-0 flex flex-col z-10">
        <div className="px-[22px] py-5 flex items-center gap-2.5 border-b border-border">
          <img src={kiroIcon} alt="Kiro" className="h-8 w-8 rounded-lg" />
          <div>
            <div className="text-[15px] font-semibold tracking-[-0.01em]">Kiro2CCProxy</div>
            <div className="text-[11px] text-muted-foreground mt-0.5">Admin 控制台</div>
          </div>
        </div>
        <nav className="flex-1 py-3 px-2.5 overflow-y-auto">
          <div className="mb-[18px]">
            <div className="text-[10px] uppercase tracking-[.08em] text-muted-foreground/70 px-3 pb-1.5 font-semibold">主要</div>
            {[
              { label: '账号管理', icon: <Server className="w-4 h-4 shrink-0" />, active: activeTab === 'credentials' && dailyView === null, onClick: () => { setActiveTab('credentials'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
              { label: 'API Keys', icon: <Key className="w-4 h-4 shrink-0" />, active: activeTab === 'apikeys', onClick: () => { setActiveTab('apikeys'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
              { label: '每日统计', icon: <BarChart2 className="w-4 h-4 shrink-0" />, active: dailyView !== null, onClick: () => { setActiveTab('credentials'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView('list') } },
            ].map(({ label, icon, active, onClick }) => (
              <button key={label} onClick={onClick}
                className={`flex w-full items-center gap-2.5 px-3 py-2 text-[13px] font-medium rounded-md transition-all mb-0.5 ${active ? 'text-foreground bg-secondary' : 'text-muted-foreground hover:text-foreground hover:bg-secondary'}`}
                style={active ? { boxShadow: 'inset 2px 0 0 hsl(var(--primary))' } : undefined}
              >
                {icon}{label}
              </button>
            ))}
          </div>
          <div>
            <div className="text-[10px] uppercase tracking-[.08em] text-muted-foreground/70 px-3 pb-1.5 font-semibold">系统</div>
            <button
              onClick={() => { setActiveTab('settings'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) }}
              className={`flex w-full items-center gap-2.5 px-3 py-2 text-[13px] font-medium rounded-md transition-all mb-0.5 ${activeTab === 'settings' ? 'text-foreground bg-secondary' : 'text-muted-foreground hover:text-foreground hover:bg-secondary'}`}
              style={activeTab === 'settings' ? { boxShadow: 'inset 2px 0 0 hsl(var(--primary))' } : undefined}
            >
              <Settings className="w-4 h-4 shrink-0" />
              <span>设置</span>
            </button>
          </div>
        </nav>
        <div className="px-[18px] py-3 border-t border-border flex items-center justify-between">
          <span className="text-[11px] font-mono text-muted-foreground/50">kiro2cc-proxy</span>
          <div className="flex items-center gap-1">
            <Button variant="ghost" size="icon" className="h-7 w-7" onClick={handleRefresh} title="刷新">
              <RefreshCw className="h-3.5 w-3.5" />
            </Button>
            <Button variant="ghost" size="icon" className="h-7 w-7" onClick={handleLogout} title="退出登录">
              <LogOut className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>
      </aside>

      {/* 主内容 */}
      <main className="ml-[232px] flex-1 min-h-screen px-9 py-7">
        {activeTab === 'settings' ? (
          <SettingsPanel />
        ) : activeTab === 'apikeys' ? (
          detailKeyId !== null ? (
            <ApiKeyDetailPage
              keyId={detailKeyId}
              onBack={() => setDetailKeyId(null)}
            />
          ) : (
            <ApiKeysPanel onViewDetail={(key: ApiKeyItem) => setDetailKeyId(key.id)} />
          )
        ) : dailyView === 'list' ? (
          <DailyStatsPage
            onBack={() => setDailyView(null)}
            onViewDay={(date) => setDailyView(date)}
          />
        ) : dailyView !== null ? (
          <DailyDetailPage
            date={dailyView}
            onBack={() => setDailyView('list')}
          />
        ) : detailCredentialId !== null ? (
          <CredentialDetailPage
            credentialId={detailCredentialId}
            onBack={() => setDetailCredentialId(null)}
          />
        ) : (
        <>
        {/* Page Header */}
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-[22px] font-bold tracking-[-0.02em]">账号管理</h1>
            <p className="text-[13px] text-muted-foreground mt-0.5">管理 Kiro 账号与负载均衡</p>
          </div>
        </div>
        {/* 统计卡片 */}
        <div className="grid gap-4 grid-cols-2 md:grid-cols-5 mb-6">
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                账号总数
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{data?.total || 0}</div>
            </CardContent>
          </Card>
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                可用账号
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold text-green-600">{data?.available || 0}</div>
            </CardContent>
          </Card>
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                全局积分
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold text-orange-600">
                {liveCreditsTotal !== null ? liveCreditsTotal.toFixed(1) : '-'}
              </div>
              {liveCreditsTotal !== null && (
                <div className="mt-1 space-y-1">
                  <div className="flex items-center justify-between text-xs text-muted-foreground">
                    <span>{liveCreditsQueried}/{data?.credentials.length || 0} 已查询</span>
                  </div>
                  {queryingInfo && (
                    <div className="h-1.5 w-full rounded-full bg-muted overflow-hidden">
                      <div
                        className="h-full rounded-full bg-orange-500 transition-all duration-300"
                        style={{ width: `${(data?.credentials.length || 0) > 0 ? (liveCreditsQueried / (data?.credentials.length || 1)) * 100 : 0}%` }}
                      />
                    </div>
                  )}
                </div>
              )}
            </CardContent>
          </Card>
          <Card
            className="cursor-pointer hover:border-primary/50 transition-colors"
            onClick={() => setDailyView('list')}
          >
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                今日用量
              </CardTitle>
            </CardHeader>
            <CardContent>
              {todayStats ? (
                <div>
                  <div className="text-xl font-bold text-blue-600 dark:text-blue-400">
                    {todayStats.totalCredits.toFixed(2)} Credits
                  </div>
                  <div className="text-sm text-orange-600 dark:text-orange-400 font-medium mt-0.5">
                    ${todayStats.totalCost.toFixed(4)}
                  </div>
                </div>
              ) : (
                <div className="text-2xl font-bold text-muted-foreground">—</div>
              )}
            </CardContent>
          </Card>
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                全局 RPM
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold text-blue-600">{rpmData?.global ?? '-'}</div>
            </CardContent>
          </Card>
        </div>

        {/* 凭据列表 */}
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              {selectedIds.size > 0 && (
                <div className="flex items-center gap-2">
                  <Badge variant="secondary">已选择 {selectedIds.size} 个</Badge>
                  <Button onClick={deselectAll} size="sm" variant="ghost">
                    取消选择
                  </Button>
                </div>
              )}
            </div>
            <div className="flex flex-wrap gap-2">
              {selectedIds.size > 0 && (
                <>
                  <Button onClick={handleBatchVerify} size="sm" variant="outline">
                    <CheckCircle2 className="h-4 w-4 sm:mr-2" />
                    <span className="hidden sm:inline">批量验活</span>
                  </Button>
                  <Button onClick={handleBatchResetFailure} size="sm" variant="outline">
                    <RotateCcw className="h-4 w-4 sm:mr-2" />
                    <span className="hidden sm:inline">恢复异常</span>
                  </Button>
                  <Button
                    onClick={handleBatchDelete}
                    size="sm"
                    variant="destructive"
                    disabled={selectedDisabledCount === 0}
                    title={selectedDisabledCount === 0 ? '只能删除已禁用账号' : undefined}
                  >
                    <Trash2 className="h-4 w-4 sm:mr-2" />
                    <span className="hidden sm:inline">批量删除</span>
                  </Button>
                </>
              )}
              {verifying && !verifyDialogOpen && (
                <Button onClick={() => setVerifyDialogOpen(true)} size="sm" variant="secondary">
                  <CheckCircle2 className="h-4 w-4 mr-2 animate-spin" />
                  验活中... {verifyProgress.current}/{verifyProgress.total}
                </Button>
              )}
              {data?.credentials && data.credentials.length > 0 && (
                <Button
                  onClick={handleQueryCurrentPageInfo}
                  size="sm"
                  variant="outline"
                  disabled={queryingInfo}
                >
                  <RefreshCw className={`h-4 w-4 sm:mr-2 ${queryingInfo ? 'animate-spin' : ''}`} />
                  <span className="hidden sm:inline">{queryingInfo ? `查询中... ${queryInfoProgress.current}/${queryInfoProgress.total}` : '查询信息'}</span>
                </Button>
              )}
              {data?.credentials && data.credentials.length > 0 && (
                <Button
                  onClick={handleClearAll}
                  size="sm"
                  variant="outline"
                  className="text-destructive hover:text-destructive"
                  disabled={disabledCredentialCount === 0}
                  title={disabledCredentialCount === 0 ? '没有可清除的已禁用账号' : undefined}
                >
                  <Trash2 className="h-4 w-4 sm:mr-2" />
                  <span className="hidden sm:inline">清除已禁用</span>
                </Button>
              )}
              <Button onClick={() => setKamImportDialogOpen(true)} size="sm" variant="outline">
                <FileUp className="h-4 w-4 sm:mr-2" />
                <span className="hidden sm:inline">Kiro Account Manager 导入</span>
              </Button>
              <Button onClick={() => setBatchImportDialogOpen(true)} size="sm" variant="outline">
                <Upload className="h-4 w-4 sm:mr-2" />
                <span className="hidden sm:inline">批量导入</span>
              </Button>
              <Button onClick={() => setAddDialogOpen(true)} size="sm">
                <Plus className="h-4 w-4 sm:mr-2" />
                <span className="hidden sm:inline">添加账号</span>
              </Button>
            </div>
          </div>
          {data?.credentials.length === 0 ? (
            <Card>
              <CardContent className="py-8 text-center text-muted-foreground">
                暂无账号
              </CardContent>
            </Card>
          ) : (
            <>
              <div className="space-y-2">
                {currentCredentials.map((credential) => (
                  <CredentialCard
                    key={credential.id}
                    credential={credential}
                    onViewBalance={handleViewBalance}
                    onViewDetail={(id) => setDetailCredentialId(id)}
                    selected={selectedIds.has(credential.id)}
                    onToggleSelect={() => toggleSelect(credential.id)}
                    balance={balanceMap.get(credential.id) || null}
                    loadingBalance={loadingBalanceIds.has(credential.id)}
                    rpm={rpmData?.byCredential?.[String(credential.id)] ?? 0}
                  />
                ))}
              </div>

              {/* 分页控件 */}
              {totalPages > 1 && (
                <div className="flex justify-center items-center gap-2 sm:gap-4 mt-6">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setCurrentPage(p => Math.max(1, p - 1))}
                    disabled={currentPage === 1}
                  >
                    上一页
                  </Button>
                  <span className="text-sm text-muted-foreground">
                    <span className="sm:hidden">{currentPage}/{totalPages}</span>
                    <span className="hidden sm:inline">第 {currentPage} / {totalPages} 页（共 {data?.credentials.length} 个账号）</span>
                  </span>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setCurrentPage(p => Math.min(totalPages, p + 1))}
                    disabled={currentPage === totalPages}
                  >
                    下一页
                  </Button>
                </div>
              )}
            </>
          )}
        </div>
        </>
        )}
      </main>

      {/* 余额对话框 */}
      <BalanceDialog
        credentialId={selectedCredentialId}
        open={balanceDialogOpen}
        onOpenChange={setBalanceDialogOpen}
      />

      {/* 添加凭据对话框 */}
      <AddCredentialDialog
        open={addDialogOpen}
        onOpenChange={setAddDialogOpen}
      />

      {/* 批量导入对话框 */}
      <BatchImportDialog
        open={batchImportDialogOpen}
        onOpenChange={setBatchImportDialogOpen}
      />

      {/* KAM 账号导入对话框 */}
      <KamImportDialog
        open={kamImportDialogOpen}
        onOpenChange={setKamImportDialogOpen}
      />

      {/* 批量验活对话框 */}
      <BatchVerifyDialog
        open={verifyDialogOpen}
        onOpenChange={setVerifyDialogOpen}
        verifying={verifying}
        progress={verifyProgress}
        results={verifyResults}
        onCancel={handleCancelVerify}
      />
    </div>
  )
}
