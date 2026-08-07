// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useEffect, useRef, useId } from 'react'
import { RefreshCw, LogOut, Server, Plus, Upload, FileUp, Trash2, RotateCcw, CheckCircle2, Key, Settings, BarChart2, ScrollText, Boxes, Sun, Moon, Github, Info, History } from 'lucide-react'
import kiroIcon from '@/assets/kiro-icon.png'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { useTranslation } from 'react-i18next'
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
import { ThrottleLogPage } from '@/components/throttle-log-page'
import { FailureLogPage } from '@/components/failure-log-page'
import { SettingsPanel } from '@/components/settings-panel'
import { LogViewerPage } from '@/components/log-viewer-page'
import { useCredentials, useDeleteCredential, useResetFailure, useRpm, useDailyUsage, useServerInfo } from '@/hooks/use-credentials'
import { useTheme } from '@/hooks/use-theme'
import { DailyStatsPage } from '@/components/daily-stats-page'
import { ModelListPage } from '@/components/model-list-page'
import { ChangelogPage } from '@/components/changelog-page'
import { DailyDetailPage } from '@/components/daily-detail-page'
import { getCredentialBalance } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { BalanceResponse, ApiKeyItem } from '@/types/api'

interface DashboardProps {
  onLogout: () => void
}

function useCountUp(target: number, duration = 700) {
  const [value, setValue] = useState(target)
  const fromRef = useRef(target)
  useEffect(() => {
    const from = fromRef.current
    if (from === target) return
    const safeDuration = duration > 0 ? duration : 1
    const start = performance.now()
    let rafId: number
    const tick = (now: number) => {
      const t = Math.min(1, Math.max(0, (now - start) / safeDuration))
      const eased = 1 - Math.pow(1 - t, 3)
      const current = from + (target - from) * eased
      setValue(current)
      fromRef.current = current
      if (t < 1) rafId = requestAnimationFrame(tick)
    }
    rafId = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(rafId)
  }, [target, duration])
  return value
}

function CreditsProgressRing({ percent }: { percent: number }) {
  const { t } = useTranslation()
  const gradientId = useId()
  const safePercent = Number.isFinite(percent) ? percent : 0
  const clamped = Math.min(100, Math.max(0, safePercent))
  const displayPercent = useCountUp(clamped)
  const radius = 15
  const circumference = 2 * Math.PI * radius
  const offset = circumference * (1 - displayPercent / 100)
  return (
    <div
      className="relative h-16 w-16 shrink-0"
      role="img"
      aria-label={t('dashboard.creditsRingLabel', { percent: Math.round(clamped) })}
    >
      <svg viewBox="0 0 36 36" className="h-16 w-16 -rotate-90">
        <circle cx="18" cy="18" r={radius} fill="none" strokeWidth="4" className="stroke-purple-100 dark:stroke-purple-900/40" />
        <circle
          cx="18" cy="18" r={radius} fill="none"
          stroke={`url(#${gradientId})`}
          strokeWidth="4"
          strokeLinecap="round"
          strokeDasharray={circumference}
          strokeDashoffset={offset}
        />
        <defs>
          <linearGradient id={gradientId} x1="0%" y1="100%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="#c4b5fd" />
            <stop offset="100%" stopColor="#7c3aed" />
          </linearGradient>
        </defs>
      </svg>
      <span aria-hidden="true" className="absolute inset-0 flex items-center justify-center text-sm font-semibold text-purple-600 dark:text-purple-400">
        {Math.round(displayPercent)}%
      </span>
    </div>
  )
}

export function Dashboard({ onLogout }: DashboardProps) {
  const { t } = useTranslation()
  const { theme, toggleTheme } = useTheme()
  const [activeTab, setActiveTab] = useState<'credentials' | 'apikeys' | 'settings' | 'logs' | 'models' | 'changelog'>('credentials')
  const [detailKeyId, setDetailKeyId] = useState<number | null>(null)
  const [detailCredentialId, setDetailCredentialId] = useState<number | null>(null)
  const [throttleLogCredentialId, setThrottleLogCredentialId] = useState<number | null>(null)
  const [failureLogCredentialId, setFailureLogCredentialId] = useState<number | null>(null)
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
  const [liveCreditsCapacity, setLiveCreditsCapacity] = useState(0)
  const [dailyView, setDailyView] = useState<string | null>(null)
  const [dailyFromSidebar, setDailyFromSidebar] = useState(false)
  const cancelVerifyRef = useRef(false)
  const prevTabRef = useRef<'credentials' | 'apikeys' | 'settings' | 'logs' | 'models' | 'changelog' | null>(null)
  const prevDetailCredentialId = useRef<number | null>(null)
  const prevDailyView = useRef<string | null>(null)
  const initialBalanceFetchDone = useRef(false)
  const isFetchingBalances = useRef(false)
  const prevEnabledIdsRef = useRef<Set<number> | null>(null)
  const [currentPage, setCurrentPage] = useState(1)
  const itemsPerPage = 12
  const queryClient = useQueryClient()
  const { data, isLoading, error, refetch } = useCredentials()
  const { data: serverInfo } = useServerInfo()
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

  // 批量拉取结束后补检：拉取期间是否有新账号加入
  const patchMissedCredentials = async (fetchedIds: Set<number>) => {
    const latestIds = (credentialsRef.current || []).filter(c => !c.disabled).map(c => c.id)
    const missed = latestIds.filter(id => !fetchedIds.has(id))
    for (const id of missed) {
      setLoadingBalanceIds(prev => { const next = new Set(prev); next.add(id); return next })
      try {
        const balance = await getCredentialBalance(id)
        setBalanceMap(prev => { const next = new Map(prev); next.set(id, balance); return next })
      } catch (_) {
        // 静默失败
      } finally {
        setLoadingBalanceIds(prev => { const next = new Set(prev); next.delete(id); return next })
      }
    }
    prevEnabledIdsRef.current = new Set(latestIds)
  }

  // 启动时首次加载凭据后自动拉取余额
  useEffect(() => {
    if (!data?.credentials || initialBalanceFetchDone.current) return
    initialBalanceFetchDone.current = true
    const ids = data.credentials.filter(c => !c.disabled).map(c => c.id)
    if (ids.length === 0) return
    isFetchingBalances.current = true
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
      await patchMissedCredentials(new Set(ids))
      isFetchingBalances.current = false
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
      isFetchingBalances.current = true
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
        await patchMissedCredentials(new Set(ids))
        isFetchingBalances.current = false
      })()
    }
    prevTabRef.current = activeTab
  }, [activeTab]) // eslint-disable-line react-hooks/exhaustive-deps

  // 添加/删除账号后自动拉取新账号余额
  useEffect(() => {
    if (!data?.credentials || !initialBalanceFetchDone.current || isFetchingBalances.current) return

    const currentEnabledIds = new Set(
      data.credentials.filter(c => !c.disabled).map(c => c.id)
    )

    if (prevEnabledIdsRef.current === null) {
      prevEnabledIdsRef.current = currentEnabledIds
      return
    }

    const prevIds = prevEnabledIdsRef.current
    const added = [...currentEnabledIds].filter(id => !prevIds.has(id))
    prevEnabledIdsRef.current = currentEnabledIds

    if (added.length === 0) return

    let aborted = false
    isFetchingBalances.current = true
    ;(async () => {
      for (const id of added) {
        if (aborted) break
        setLoadingBalanceIds(prev => { const next = new Set(prev); next.add(id); return next })
        try {
          const balance = await getCredentialBalance(id)
          if (!aborted) {
            setBalanceMap(prev => { const next = new Map(prev); next.set(id, balance); return next })
          }
        } catch (_) {
          // 静默失败
        } finally {
          if (!aborted) {
            setLoadingBalanceIds(prev => { const next = new Set(prev); next.delete(id); return next })
          }
        }
      }
      isFetchingBalances.current = false
    })()
    return () => { aborted = true; isFetchingBalances.current = false }
  }, [data?.credentials]) // eslint-disable-line react-hooks/exhaustive-deps

  // balanceMap 变化后（添加/删除/清理）重新计算全局积分
  useEffect(() => {
    if (!initialBalanceFetchDone.current || isFetchingBalances.current) return

    let total = 0
    let capacity = 0
    balanceMap.forEach(b => { total += b.remaining; capacity += b.usageLimit })
    setLiveCreditsTotal(balanceMap.size > 0 ? total : null)
    setLiveCreditsQueried(balanceMap.size)
    setLiveCreditsCapacity(capacity)
  }, [balanceMap]) // eslint-disable-line react-hooks/exhaustive-deps

  const handleViewBalance = (id: number) => {
    setSelectedCredentialId(id)
    setBalanceDialogOpen(true)
  }

  const handleRefresh = () => {
    refetch()
    toast.success(t('dashboard.toastRefreshed'))
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
      toast.error(t('dashboard.toastSelectToDelete'))
      return
    }

    const disabledIds = Array.from(selectedIds).filter(id => {
      const credential = data?.credentials.find(c => c.id === id)
      return Boolean(credential?.disabled)
    })

    if (disabledIds.length === 0) {
      toast.error(t('dashboard.toastNoDisabledSelected'))
      return
    }

    const skippedCount = selectedIds.size - disabledIds.length
    const skippedText = skippedCount > 0 ? t('dashboard.skippedSuffix', { count: skippedCount }) : ''

    if (!confirm(t('dashboard.confirmDeleteDisabled', { count: disabledIds.length, skipped: skippedText }))) {
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

    const skippedResultText = skippedCount > 0 ? t('dashboard.skippedResultSuffix', { count: skippedCount }) : ''

    if (failCount === 0) {
      toast.success(t('dashboard.toastDeleteDisabledSuccess', { count: successCount, skipped: skippedResultText }))
    } else {
      toast.warning(t('dashboard.toastDeleteDisabledPartial', { success: successCount, fail: failCount, skipped: skippedResultText }))
    }

    deselectAll()
  }

  // 批量恢复异常
  const handleBatchResetFailure = async () => {
    if (selectedIds.size === 0) {
      toast.error(t('dashboard.toastSelectToRestore'))
      return
    }

    const failedIds = Array.from(selectedIds).filter(id => {
      const cred = data?.credentials.find(c => c.id === id)
      return cred && cred.failureCount > 0
    })

    if (failedIds.length === 0) {
      toast.error(t('dashboard.toastNoFailedSelected'))
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
      toast.success(t('dashboard.toastRestoreSuccess', { count: successCount }))
    } else {
      toast.warning(t('dashboard.toastRestorePartial', { success: successCount, fail: failCount }))
    }

    deselectAll()
  }

  // 一键清除所有已禁用凭据
  const handleClearAll = async () => {
    if (!data?.credentials || data.credentials.length === 0) {
      toast.error(t('dashboard.toastNoClearable'))
      return
    }

    const disabledCredentials = data.credentials.filter(credential => credential.disabled)

    if (disabledCredentials.length === 0) {
      toast.error(t('dashboard.noClearableDisabled'))
      return
    }

    if (!confirm(t('dashboard.confirmClearAll', { count: disabledCredentials.length }))) {
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
      toast.success(t('dashboard.toastClearAllSuccess', { count: successCount }))
    } else {
      toast.warning(t('dashboard.toastClearAllPartial', { success: successCount, fail: failCount }))
    }

    deselectAll()
  }

  // 查询所有凭据信息（逐个查询，避免瞬时并发）
  const handleQueryCurrentPageInfo = async () => {
    const allCredentials = data?.credentials || []

    if (allCredentials.length === 0) {
      toast.error(t('dashboard.toastNoQueryable'))
      return
    }

    const ids = allCredentials
      .filter(credential => !credential.disabled)
      .map(credential => credential.id)

    if (ids.length === 0) {
      toast.error(t('dashboard.toastNoQueryableEnabled'))
      return
    }

    setQueryingInfo(true)
    isFetchingBalances.current = true
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
    isFetchingBalances.current = false
    prevEnabledIdsRef.current = new Set(ids)

    if (failCount === 0) {
      toast.success(t('dashboard.toastQueryDone', { success: successCount, total: ids.length }))
    } else {
      toast.warning(t('dashboard.toastQueryPartial', { success: successCount, fail: failCount }))
    }
  }

  // 批量验活
  const handleBatchVerify = async () => {
    if (selectedIds.size === 0) {
      toast.error(t('dashboard.toastSelectToVerify'))
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
        toast.info(t('dashboard.toastVerifyCancelled'))
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
      toast.success(t('dashboard.toastVerifyDone', { success: successCount, total: ids.length }))
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
          <p className="text-muted-foreground">{t('common.loading')}</p>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-background p-4">
        <Card className="w-full max-w-md">
          <CardContent className="pt-6 text-center">
            <div className="text-red-500 mb-4">{t('common.loadFailed')}</div>
            <p className="text-muted-foreground mb-4">{(error as Error).message}</p>
            <div className="space-x-2">
              <Button onClick={() => refetch()}>{t('common.retry')}</Button>
              <Button variant="outline" onClick={handleLogout}>{t('common.relogin')}</Button>
            </div>
          </CardContent>
        </Card>
      </div>
    )
  }

  return (
    <div className="flex min-h-screen bg-background">
      {/* 左侧 Sidebar */}
      <aside className="w-[232px] bg-background border-r border-border fixed top-0 left-0 bottom-0 flex flex-col z-10">
        <div className="px-[22px] py-5 flex items-center justify-between gap-2.5 border-b border-border">
          <a
            href="https://github.com/TsinHzl/kiro2cc-proxy"
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-2.5 group"
          >
            <img src={kiroIcon} alt="Kiro" className="h-8 w-8 rounded-lg" />
            <div>
              <div className="text-[15px] font-semibold tracking-[-0.01em] group-hover:text-primary transition-colors">Kiro2CCProxy</div>
              <div className="text-[11px] text-muted-foreground mt-0.5 group-hover:text-primary transition-colors">{t('dashboard.consoleSubtitle')}</div>
            </div>
          </a>
          <Button variant="ghost" size="icon" className="h-7 w-7" onClick={handleLogout} title={t('common.logout')}>
            <LogOut className="h-3.5 w-3.5" />
          </Button>
        </div>
        <nav className="flex-1 py-3 px-2.5 overflow-y-auto">
          <div className="mb-[18px]">
            <div className="text-[10px] uppercase tracking-[.08em] text-muted-foreground dark:text-muted-foreground/70 px-3 pb-1.5 font-semibold">{t('dashboard.navMain')}</div>
            {[
              { label: t('dashboard.navCredentials'), icon: <Server className="w-4 h-4 shrink-0" />, active: activeTab === 'credentials' && dailyView === null, onClick: () => { setActiveTab('credentials'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
              { label: 'API Keys', icon: <Key className="w-4 h-4 shrink-0" />, active: activeTab === 'apikeys', onClick: () => { setActiveTab('apikeys'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
              { label: t('dashboard.navDailyStats'), icon: <BarChart2 className="w-4 h-4 shrink-0" />, active: dailyView !== null, onClick: () => { setActiveTab('credentials'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView('list'); setDailyFromSidebar(true) } },
              { label: t('dashboard.navModels'), icon: <Boxes className="w-4 h-4 shrink-0" />, active: activeTab === 'models', onClick: () => { setActiveTab('models'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
            ].map(({ label, icon, active, onClick }) => (
              <button key={label} onClick={onClick}
                className={`flex w-full items-center gap-2.5 px-3 py-2 text-[13px] font-medium rounded-md transition-all mb-0.5 ${active ? 'text-foreground bg-card dark:bg-secondary' : 'text-muted-foreground hover:text-foreground hover:bg-secondary dark:hover:bg-card'}`}
                style={active ? { boxShadow: 'inset 2px 0 0 hsl(var(--primary))' } : undefined}
              >
                {icon}{label}
              </button>
            ))}
          </div>
          <div>
            <div className="text-[10px] uppercase tracking-[.08em] text-muted-foreground dark:text-muted-foreground/70 px-3 pb-1.5 font-semibold">{t('dashboard.navSystem')}</div>
            <button
              onClick={() => { setActiveTab('logs'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) }}
              className={`flex w-full items-center gap-2.5 px-3 py-2 text-[13px] font-medium rounded-md transition-all mb-0.5 ${activeTab === 'logs' ? 'text-foreground bg-card dark:bg-secondary' : 'text-muted-foreground hover:text-foreground hover:bg-secondary dark:hover:bg-card'}`}
              style={activeTab === 'logs' ? { boxShadow: 'inset 2px 0 0 hsl(var(--primary))' } : undefined}
            >
              <ScrollText className="w-4 h-4 shrink-0" />
              <span>{t('dashboard.navLogs')}</span>
            </button>
            <button
              onClick={() => { setActiveTab('changelog'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) }}
              className={`flex w-full items-center gap-2.5 px-3 py-2 text-[13px] font-medium rounded-md transition-all mb-0.5 ${activeTab === 'changelog' ? 'text-foreground bg-card dark:bg-secondary' : 'text-muted-foreground hover:text-foreground hover:bg-secondary dark:hover:bg-card'}`}
              style={activeTab === 'changelog' ? { boxShadow: 'inset 2px 0 0 hsl(var(--primary))' } : undefined}
            >
              <History className="w-4 h-4 shrink-0" />
              <span>{t('dashboard.navChangelog')}</span>
            </button>
            <button
              onClick={() => { setActiveTab('settings'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) }}
              className={`flex w-full items-center gap-2.5 px-3 py-2 text-[13px] font-medium rounded-md transition-all mb-0.5 ${activeTab === 'settings' ? 'text-foreground bg-card dark:bg-secondary' : 'text-muted-foreground hover:text-foreground hover:bg-secondary dark:hover:bg-card'}`}
              style={activeTab === 'settings' ? { boxShadow: 'inset 2px 0 0 hsl(var(--primary))' } : undefined}
            >
              <Settings className="w-4 h-4 shrink-0" />
              <span>{t('dashboard.navSettings')}</span>
            </button>
          </div>
        </nav>
        <div className="px-[18px] py-3 border-t border-border flex items-center justify-between">
          <a
            href="https://github.com/TsinHzl/kiro2cc-proxy"
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-1.5 text-[11px] font-mono text-foreground/70 hover:text-foreground transition-colors"
          >
            <Github className="h-3.5 w-3.5" />
            kiro2cc-proxy v{serverInfo?.version ?? '...'}
          </a>
          <Button variant="ghost" size="icon" className="h-6 w-6" onClick={toggleTheme} title={theme === 'dark' ? t('dashboard.toggleLightMode') : t('dashboard.toggleDarkMode')}>
            {theme === 'dark' ? <Sun className="h-3 w-3" /> : <Moon className="h-3 w-3" />}
          </Button>
        </div>
      </aside>

      {/* 主内容 */}
      <main className="ml-[232px] flex-1 min-h-screen px-9 py-7">
        {activeTab === 'logs' ? (
          <LogViewerPage />
        ) : activeTab === 'settings' ? (
          <SettingsPanel />
        ) : activeTab === 'models' ? (
          <ModelListPage />
        ) : activeTab === 'changelog' ? (
          <ChangelogPage />
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
            showBack={!dailyFromSidebar}
            onBack={() => setDailyView(null)}
            onViewDay={(date) => setDailyView(date)}
          />
        ) : dailyView !== null ? (
          <DailyDetailPage
            date={dailyView}
            onBack={() => setDailyView('list')}
          />
        ) : failureLogCredentialId !== null ? (
          <FailureLogPage
            credentialId={failureLogCredentialId}
            onBack={() => setFailureLogCredentialId(null)}
          />
        ) : throttleLogCredentialId !== null ? (
          <ThrottleLogPage
            credentialId={throttleLogCredentialId}
            onBack={() => setThrottleLogCredentialId(null)}
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
            <h1 className="text-[22px] font-bold tracking-[-0.02em]">{t('dashboard.navCredentials')}</h1>
            <p className="text-[13px] text-muted-foreground mt-0.5">{t('dashboard.pageSubtitle')}</p>
          </div>
        </div>
        {/* 统计卡片 */}
        <div className="grid gap-4 grid-cols-2 md:grid-cols-4 mb-6">
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                {t('dashboard.statTotal')}
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">{data?.total || 0}</div>
            </CardContent>
          </Card>
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                {t('dashboard.statAvailable')}
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold text-green-600">{data?.available || 0}</div>
            </CardContent>
          </Card>
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                {t('dashboard.statGlobalCredits')}
              </CardTitle>
            </CardHeader>
            <CardContent className="flex items-center justify-between gap-2">
              <div className="min-w-0">
                <div className="text-2xl font-bold text-orange-600 truncate">
                  {liveCreditsTotal !== null ? liveCreditsTotal.toFixed(1) : '-'}
                </div>
                {liveCreditsTotal !== null && (
                  <div className="mt-1 text-xs text-muted-foreground truncate">
                    {t('dashboard.statQueried', { queried: liveCreditsQueried, total: data?.credentials.length || 0 })}
                  </div>
                )}
              </div>
              {liveCreditsTotal !== null && (
                <CreditsProgressRing
                  percent={liveCreditsCapacity > 0 ? (liveCreditsTotal ?? 0) / liveCreditsCapacity * 100 : 0}
                />
              )}
            </CardContent>
          </Card>
          <Card
            className="cursor-pointer hover:border-primary/50 transition-colors"
            onClick={() => { setDailyView('list'); setDailyFromSidebar(false) }}
          >
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">
                {t('dashboard.statTodayUsage')}
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
        </div>

        {/* 凭据列表 */}
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              {selectedIds.size > 0 && (
                <div className="flex items-center gap-2">
                  <Badge variant="secondary">{t('dashboard.selectedCount', { count: selectedIds.size })}</Badge>
                  <Button onClick={deselectAll} size="sm" variant="ghost">
                    {t('dashboard.deselectAll')}
                  </Button>
                </div>
              )}
            </div>
            <div className="flex flex-wrap gap-2">
              {selectedIds.size > 0 && (
                <>
                  <Button onClick={handleBatchVerify} size="sm" variant="outline">
                    <CheckCircle2 className="h-4 w-4 sm:mr-2" />
                    <span className="hidden sm:inline">{t('dashboard.batchVerify')}</span>
                  </Button>
                  <Button onClick={handleBatchResetFailure} size="sm" variant="outline">
                    <RotateCcw className="h-4 w-4 sm:mr-2" />
                    <span className="hidden sm:inline">{t('dashboard.batchRestore')}</span>
                  </Button>
                  <Button
                    onClick={handleBatchDelete}
                    size="sm"
                    variant="destructive"
                    disabled={selectedDisabledCount === 0}
                    title={selectedDisabledCount === 0 ? t('dashboard.deleteDisabledOnly') : undefined}
                  >
                    <Trash2 className="h-4 w-4 sm:mr-2" />
                    <span className="hidden sm:inline">{t('dashboard.batchDelete')}</span>
                  </Button>
                </>
              )}
              {verifying && !verifyDialogOpen && (
                <Button onClick={() => setVerifyDialogOpen(true)} size="sm" variant="secondary">
                  <CheckCircle2 className="h-4 w-4 mr-2 animate-spin" />
                  {t('dashboard.verifyingProgress', { current: verifyProgress.current, total: verifyProgress.total })}
                </Button>
              )}
              <Button onClick={handleRefresh} size="sm" variant="outline">
                <RefreshCw className="h-4 w-4 sm:mr-2" />
                <span className="hidden sm:inline">{t('dashboard.refreshList')}</span>
              </Button>
              {data?.credentials && data.credentials.length > 0 && (
                <Button
                  onClick={handleQueryCurrentPageInfo}
                  size="sm"
                  variant="outline"
                  disabled={queryingInfo}
                >
                  <Info className={`h-4 w-4 sm:mr-2 ${queryingInfo ? 'animate-pulse' : ''}`} />
                  <span className="hidden sm:inline">{queryingInfo ? t('dashboard.queryingProgress', { current: queryInfoProgress.current, total: queryInfoProgress.total }) : t('dashboard.queryInfo')}</span>
                </Button>
              )}
              {data?.credentials && data.credentials.length > 0 && (
                <Button
                  onClick={handleClearAll}
                  size="sm"
                  variant="outline"
                  className="text-destructive hover:text-destructive"
                  disabled={disabledCredentialCount === 0}
                  title={disabledCredentialCount === 0 ? t('dashboard.noClearableDisabled') : undefined}
                >
                  <Trash2 className="h-4 w-4 sm:mr-2" />
                  <span className="hidden sm:inline">{t('dashboard.clearDisabled')}</span>
                </Button>
              )}
              <Button onClick={() => setKamImportDialogOpen(true)} size="sm" variant="outline">
                <FileUp className="h-4 w-4 sm:mr-2" />
                <span className="hidden sm:inline">{t('dashboard.kamImport')}</span>
              </Button>
              <Button onClick={() => setBatchImportDialogOpen(true)} size="sm" variant="outline">
                <Upload className="h-4 w-4 sm:mr-2" />
                <span className="hidden sm:inline">{t('dashboard.batchImport')}</span>
              </Button>
              <Button onClick={() => setAddDialogOpen(true)} size="sm">
                <Plus className="h-4 w-4 sm:mr-2" />
                <span className="hidden sm:inline">{t('dashboard.addAccount')}</span>
              </Button>
            </div>
          </div>
          {data?.credentials.length === 0 ? (
            <Card>
              <CardContent className="py-8 text-center text-muted-foreground">
                {t('dashboard.noAccounts')}
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
                    onViewThrottleLog={(id) => setThrottleLogCredentialId(id)}
                    onViewFailureLog={(id) => setFailureLogCredentialId(id)}
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
                    {t('common.prevPage')}
                  </Button>
                  <span className="text-sm text-muted-foreground">
                    <span className="sm:hidden">{currentPage}/{totalPages}</span>
                    <span className="hidden sm:inline">{t('dashboard.pageInfo', { current: currentPage, total: totalPages, count: data?.credentials.length })}</span>
                  </span>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setCurrentPage(p => Math.min(totalPages, p + 1))}
                    disabled={currentPage === totalPages}
                  >
                    {t('common.nextPage')}
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
