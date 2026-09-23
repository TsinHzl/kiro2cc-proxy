// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useEffect, useMemo, useRef } from 'react'
import {Server, Key, Settings, BarChart2, ScrollText, Boxes, History} from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { useTranslation } from 'react-i18next'
import { storage } from '@/lib/storage'
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { type AccountStatusFilter } from '@/components/account-toolbar'
import { ApiKeysPanel } from '@/components/api-keys-panel'
import { ApiKeyDetailPage } from '@/components/api-key-detail-page'
import { CredentialDetailPage } from '@/components/credential-detail-page'
import { ThrottleLogPage } from '@/components/throttle-log-page'
import { FailureLogPage } from '@/components/failure-log-page'
import { SettingsPanel } from '@/components/settings-panel'
import { LogViewerPage } from '@/components/log-viewer-page'
import { useCredentials, useApiKeys, useDeleteCredential, useResetFailure, useRpm, useDailyUsage, useServerInfo } from '@/hooks/use-credentials'
import { useTheme } from '@/hooks/use-theme'
import { DailyStatsPage } from '@/components/daily-stats-page'
import { ModelListPage } from '@/components/model-list-page'
import { ChangelogPage } from '@/components/changelog-page'
import { DailyDetailPage } from '@/components/daily-detail-page'
import { getCredentialBalance } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import {
  accountLabel,
  deriveAccountState,
  sortCredentials,
  type AccountSortKey,
  type SortDirection,
} from '@/lib/account-state'
import type { ApiKeyItem } from '@/types/api'
import { Sidebar } from '@/components/dashboard/sidebar'
import { CredentialList } from '@/components/dashboard/credential-list'
import { type VerifyResult } from '@/components/batch-verify-dialog'
import { DashboardDialogs } from '@/components/dashboard/dialogs'
import { DashboardHeadMetrics } from '@/components/dashboard/head-metrics'
import { useBalanceFetcher } from '@/components/dashboard/use-balance-fetcher'
import { CREDITS_DELTA_MIN_BASE, formatLocalDate, SIDEBAR_COLLAPSED_STORAGE_KEY, SIDEBAR_TRANSITION_MS, readStoredSidebarCollapsed } from '@/components/dashboard/panel-constants'

interface DashboardProps {
  onLogout: () => void
}

export function Dashboard({ onLogout }: DashboardProps) {
  const { t } = useTranslation()
  const { theme, toggleTheme } = useTheme()
  const [activeTab, setActiveTab] = useState<'credentials' | 'apikeys' | 'settings' | 'logs' | 'models' | 'changelog'>('credentials')
  const [sidebarCollapsed, setSidebarCollapsed] = useState<boolean>(readStoredSidebarCollapsed)
  // 侧边栏内容（header/nav/footer 的 flex 方向、间距、文字显隐）无法被 CSS transition 平滑插值，
  // 故延迟到宽度动画半程、内容淡为透明时才瞬切，避免可见的布局跳变
  const [sidebarContentCollapsed, setSidebarContentCollapsed] = useState<boolean>(sidebarCollapsed)
  // 与 sidebarContentCollapsed 的瞬切时机配合：切换前淡出、切换后淡入，把布局跳变藏在不可见的瞬间
  const [sidebarContentFading, setSidebarContentFading] = useState(false)
  const [detailKeyId, setDetailKeyId] = useState<number | null>(null)
  const [detailCredentialId, setDetailCredentialId] = useState<number | null>(null)
  const [throttleLogCredentialId, setThrottleLogCredentialId] = useState<number | null>(null)
  const [failureLogCredentialId, setFailureLogCredentialId] = useState<number | null>(null)
  const [selectedCredentialId, setSelectedCredentialId] = useState<number | null>(null)
  const [balanceDialogOpen, setBalanceDialogOpen] = useState(false)
  const [modelsDialogOpen, setModelsDialogOpen] = useState(false)
  const [modelsCredentialId, setModelsCredentialId] = useState<number | null>(null)
  const [addDialogOpen, setAddDialogOpen] = useState(false)
  const [batchImportDialogOpen, setBatchImportDialogOpen] = useState(false)
  const [kamImportDialogOpen, setKamImportDialogOpen] = useState(false)
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set())
  const [verifyDialogOpen, setVerifyDialogOpen] = useState(false)
  const [verifying, setVerifying] = useState(false)
  const [verifyProgress, setVerifyProgress] = useState({ current: 0, total: 0 })
  const [verifyResults, setVerifyResults] = useState<Map<number, VerifyResult>>(new Map())
  const [queryingInfo, setQueryingInfo] = useState(false)
  const [queryInfoProgress, setQueryInfoProgress] = useState({ current: 0, total: 0 })
  const [dailyView, setDailyView] = useState<string | null>(null)
  const [dailyFromSidebar, setDailyFromSidebar] = useState(false)
  const cancelVerifyRef = useRef(false)
  // 单账号重查余额的防重入标记（见 handleRefetchBalance）
  const refetchingBalanceIds = useRef<Set<number>>(new Set())
  const [currentPage, setCurrentPage] = useState(1)
  const [searchQuery, setSearchQuery] = useState('')
  const [statusFilter, setStatusFilter] = useState<AccountStatusFilter>('all')
  const [sortKey, setSortKey] = useState<AccountSortKey | null>(null)
  const [sortDir, setSortDir] = useState<SortDirection>('asc')
  // 设计稿表体内滚 + 页脚显示「每页 50」
  const itemsPerPage = 50
  const queryClient = useQueryClient()
  const { data, isLoading, error, refetch, dataUpdatedAt } = useCredentials()
  const { data: serverInfo, isError: serverInfoError } = useServerInfo()
  const { data: apiKeys } = useApiKeys()
  const { data: rpmData } = useRpm()
  const { mutate: deleteCredential } = useDeleteCredential()
  const { mutate: resetFailure } = useResetFailure()
  const { data: dailyUsageData } = useDailyUsage()
  // 余额拉取逻辑群（缓存清理 / 首次拉取 / 返回刷新 / 切 tab 刷新 / 新增账号拉取 / 积分重算）
  const {
    balanceMap, setBalanceMap,
    loadingBalanceIds, setLoadingBalanceIds,
    liveCreditsTotal, setLiveCreditsTotal,
    liveCreditsQueried, setLiveCreditsQueried,
    isFetchingBalances, prevEnabledIdsRef,
  } = useBalanceFetcher({
    credentials: data?.credentials,
    refetch,
    queryClient,
    detailCredentialId,
    dailyView,
    activeTab,
  })

  const now = new Date()
  const todayLocal = formatLocalDate(now)
  const yesterday = new Date(now)
  yesterday.setDate(yesterday.getDate() - 1)
  const yesterdayLocal = formatLocalDate(yesterday)
  const todayStats = dailyUsageData?.find((d) => d.date === todayLocal) ?? null
  const yesterdayStats = dailyUsageData?.find((d) => d.date === yesterdayLocal) ?? null

  // ===== 指标条派生数据（仅用现有接口，缺数据一律降级为 null）=====
  const allCredentials = data?.credentials ?? []
  // 「异常」= error + warning；禁用与待查询不计入
  const abnormalCount = allCredentials.filter(c => {
    const state = deriveAccountState(c, balanceMap.has(c.id))
    return state === 'error' || state === 'warning'
  }).length
  // 启用账号的剩余百分比：需已查到余额且 usageLimit > 0，否则跳过
  const enabledRemaining = allCredentials.flatMap(c => {
    if (c.disabled) return []
    const balance = balanceMap.get(c.id)
    if (!balance || balance.usageLimit <= 0) return []
    // 脏数据（remaining > usageLimit）会让文本显示 134% 而环形图封顶，这里统一裁剪
    const percent = Math.min(100, Math.max(0, (balance.remaining / balance.usageLimit) * 100))
    return [{ name: accountLabel(c), percent }]
  })
  const avgRemainingPercent = enabledRemaining.length > 0
    ? enabledRemaining.reduce((sum, item) => sum + item.percent, 0) / enabledRemaining.length
    : null
  // 已消费积分合计：Σ(usageLimit − remaining)，仅对 usageLimit > 0 的账号累加（与 enabledRemaining 同源）
  const consumable = [...balanceMap.values()].filter(b => b.usageLimit > 0)
  const consumedCreditsTotal = consumable.length > 0
    ? consumable.reduce((sum, b) => sum + (typeof b.currentUsage === 'number' && !Number.isNaN(b.currentUsage) ? b.currentUsage : Math.max(0, b.usageLimit - b.remaining)), 0)
    : null
  // 日用量未加载时为 null（区别于「今天确实 0 次调用」）
  const todayRequests = dailyUsageData ? todayStats?.totalRequests ?? 0 : null
  const requestsDeltaPercent = todayRequests !== null && yesterdayStats && yesterdayStats.totalRequests > 0
    ? ((todayRequests - yesterdayStats.totalRequests) / yesterdayStats.totalRequests) * 100
    : null
  // 最近 7 天（日期升序），两条 sparkline 共用同一份切片
  const recent7 = (dailyUsageData ?? [])
    .slice()
    .sort((a, b) => a.date.localeCompare(b.date))
    .slice(-7)
  const requestTrend = recent7.map(d => d.totalRequests)
  // 今日 credits 消耗：口径与 todayRequests 一致（未加载为 null，已加载但无当日记录为 0）
  const todayCredits = dailyUsageData ? todayStats?.totalCredits ?? 0 : null
  // 后端偶发负值（缓存基线漂移），「已节省」语义下负数无意义，统一裁剪到 0
  const todayCreditsSaved = dailyUsageData ? Math.max(0, todayStats?.totalCreditsSaved ?? 0) : null
  // credits 是浮点量，分母仅守 > 0 会让极小基线（如 0.001）放大出无意义的百分比；抬到 0.5 起算
  const creditsDeltaPercent = todayCredits !== null && yesterdayStats && yesterdayStats.totalCredits >= CREDITS_DELTA_MIN_BASE
    ? ((todayCredits - yesterdayStats.totalCredits) / yesterdayStats.totalCredits) * 100
    : null
  const creditsTrend = recent7.map(d => d.totalCredits)
  // 后端无按天失败数，只能给账号池累计值
  const cumulativeFailures = allCredentials.reduce((sum, c) => sum + c.failureCount, 0)
  const cumulativeAttempts = allCredentials.reduce((sum, c) => sum + c.successCount + c.failureCount, 0)
  const cumulativeFailureRate = cumulativeAttempts > 0 ? (cumulativeFailures / cumulativeAttempts) * 100 : null

  // ===== 搜索 / 筛选 / 分页派生管道（排序状态在 T15 接表头时接入 sortCredentials）=====
  // 搜索：昵称 / 邮箱 / 账号 ID，不区分大小写
  const filtered = useMemo(() => {
    const q = searchQuery.trim().toLowerCase()
    if (!q) return allCredentials
    return allCredentials.filter(
      c =>
        (c.nickname ?? '').toLowerCase().includes(q) ||
        (c.email ?? '').toLowerCase().includes(q) ||
        String(c.id).includes(q),
    )
  }, [allCredentials, searchQuery])
  // 分段计数基于搜索后的集合；设计稿把 error + warning 合并为「异常」一段
  const stateCounts = useMemo(() => {
    const counts: Record<AccountStatusFilter, number> = {
      all: filtered.length,
      healthy: 0,
      abnormal: 0,
      disabled: 0,
      quota_exceeded: 0,
      pending: 0,
    }
    filtered.forEach(c => {
      const state = deriveAccountState(c, balanceMap.has(c.id))
      if (state === 'error' || state === 'warning') counts.abnormal += 1
      else counts[state] += 1
    })
    return counts
  }, [filtered, balanceMap])
  const visible = useMemo(() => {
    if (statusFilter === 'all') return filtered
    return filtered.filter(c => {
      const state = deriveAccountState(c, balanceMap.has(c.id))
      return statusFilter === 'abnormal' ? state === 'error' || state === 'warning' : state === statusFilter
    })
  }, [filtered, statusFilter, balanceMap])
  // 排序键快照（T15 CR Medium ②）：按「剩余额度」排序时批量查询余额，每个结果到达都会改变
  // 排序键，实时重排会让行位置持续跳动。改为只在查询静止时刷新快照 —— 查询期间维持既有顺序，
  // 全部返回后一次性重排
  const [remainingSnapshot, setRemainingSnapshot] = useState<Map<number, number>>(new Map())
  useEffect(() => {
    if (loadingBalanceIds.size > 0) return
    setRemainingSnapshot(new Map([...balanceMap].map(([id, b]) => [id, b.remaining])))
  }, [loadingBalanceIds, balanceMap])
  // 排序（design.md 决策 4）：sortKey 为 null 时保持后端返回顺序，与设计稿默认中性排序态一致
  const sorted = useMemo(
    () =>
      sortKey
        ? sortCredentials(visible, sortKey, sortDir, id => remainingSnapshot.get(id) ?? null)
        : visible,
    [visible, sortKey, sortDir, remainingSnapshot],
  )
  const totalPages = Math.max(1, Math.ceil(sorted.length / itemsPerPage))
  // 可见集合会随余额查询实时收缩（pending → healthy），页码在渲染期钳制，
  // 不依赖重置 effect 的异步时序，避免停留在越界的空白页且分页控件被隐藏
  const page = Math.min(currentPage, totalPages)
  const startIndex = (page - 1) * itemsPerPage
  const paged = sorted.slice(startIndex, startIndex + itemsPerPage)
  // 全选只作用于当前页；跨页已选项保留，汇总在页脚（T18）
  const allPagedSelected = paged.length > 0 && paged.every(c => selectedIds.has(c.id))
  const somePagedSelected = !allPagedSelected && paged.some(c => selectedIds.has(c.id))
  const isFiltered = searchQuery.trim() !== '' || statusFilter !== 'all'

  const disabledCredentialCount = data?.credentials.filter(credential => credential.disabled).length || 0
  const selectedDisabledCount = Array.from(selectedIds).filter(id => {
    const credential = data?.credentials.find(c => c.id === id)
    return Boolean(credential?.disabled)
  }).length

  // 凭据列表 / 搜索词 / 状态筛选任一变化时回到第一页（排序变化的重置在 handleSort 内）
  useEffect(() => {
    setCurrentPage(1)
  }, [data?.credentials.length, searchQuery, statusFilter])


  const handleViewBalance = (id: number) => {
    setSelectedCredentialId(id)
    setBalanceDialogOpen(true)
  }

  // 单账号重查余额（行内「更多 → 重新查询余额」）：与批量查询共用 loadingBalanceIds / balanceMap，
  // 写回 balanceMap 后由既有 effect 自动重算全局积分与排序快照
  const handleRefetchBalance = async (id: number) => {
    // 防重入用 ref：state 在同一事件循环内读到的是渲染期快照，挡不住连点
    if (refetchingBalanceIds.current.has(id)) return
    refetchingBalanceIds.current.add(id)
    setLoadingBalanceIds(prev => new Set(prev).add(id))
    try {
      const balance = await getCredentialBalance(id)
      setBalanceMap(prev => new Map(prev).set(id, balance))
    } catch (error) {
      toast.error(t('credentials.toastOpFailed', { message: extractErrorMessage(error) }))
    } finally {
      refetchingBalanceIds.current.delete(id)
      setLoadingBalanceIds(prev => {
        const next = new Set(prev)
        next.delete(id)
        return next
      })
    }
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

  const toggleSidebarCollapsed = () => {
    setSidebarCollapsed(prev => {
      const next = !prev
      localStorage.setItem(SIDEBAR_COLLAPSED_STORAGE_KEY, String(next))
      return next
    })
  }

  const isSidebarMount = useRef(true)
  useEffect(() => {
    if (isSidebarMount.current) {
      isSidebarMount.current = false
      return
    }
    setSidebarContentFading(true)
    const timer = window.setTimeout(() => {
      setSidebarContentCollapsed(sidebarCollapsed)
      setSidebarContentFading(false)
    }, SIDEBAR_TRANSITION_MS / 2)
    return () => window.clearTimeout(timer)
  }, [sidebarCollapsed])

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

  const toggleSelectPage = () => {
    const next = new Set(selectedIds)
    paged.forEach(c => (allPagedSelected ? next.delete(c.id) : next.add(c.id)))
    setSelectedIds(next)
  }

  // 同列再点切换升降序，换列一律从升序开始；排序改变后回到第一页
  const handleSort = (key: AccountSortKey) => {
    if (sortKey === key) {
      setSortDir(dir => (dir === 'asc' ? 'desc' : 'asc'))
    } else {
      setSortKey(key)
      setSortDir('asc')
    }
    setCurrentPage(1)
  }

  const clearFilters = () => {
    setSearchQuery('')
    setStatusFilter('all')
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

  const navGroups = [
    {
      title: t('dashboard.navMain'),
      items: [
        { key: 'credentials', label: t('dashboard.navCredentials'), icon: Server, count: data && data.credentials.length > 0 ? data.credentials.length : undefined, active: activeTab === 'credentials' && dailyView === null, onClick: () => { setActiveTab('credentials'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
        { key: 'apikeys', label: 'API Keys', icon: Key, count: apiKeys && apiKeys.length > 0 ? apiKeys.length : undefined, active: activeTab === 'apikeys', onClick: () => { setActiveTab('apikeys'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
        { key: 'daily', label: t('dashboard.navDailyStats'), icon: BarChart2, count: undefined, active: dailyView !== null, onClick: () => { setActiveTab('credentials'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView('list'); setDailyFromSidebar(true) } },
        { key: 'models', label: t('dashboard.navModels'), icon: Boxes, count: undefined, active: activeTab === 'models', onClick: () => { setActiveTab('models'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
      ],
    },
    {
      title: t('dashboard.navSystem'),
      items: [
        { key: 'logs', label: t('dashboard.navLogs'), icon: ScrollText, count: undefined, active: activeTab === 'logs', onClick: () => { setActiveTab('logs'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
        { key: 'changelog', label: t('dashboard.navChangelog'), icon: History, count: undefined, active: activeTab === 'changelog', onClick: () => { setActiveTab('changelog'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
        { key: 'settings', label: t('dashboard.navSettings'), icon: Settings, count: undefined, active: activeTab === 'settings', onClick: () => { setActiveTab('settings'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
      ],
    },
  ]


  return (
    <div className="flex min-h-screen bg-background">
      {/* 左侧 Sidebar（自本文件拆出，纯代码搬移） */}
      <Sidebar
        sidebarCollapsed={sidebarCollapsed}
        sidebarContentCollapsed={sidebarContentCollapsed}
        sidebarContentFading={sidebarContentFading}
        navGroups={navGroups}
        serverInfo={serverInfo}
        serverInfoError={!!serverInfoError}
        theme={theme}
        toggleSidebarCollapsed={toggleSidebarCollapsed}
        handleLogout={handleLogout}
        toggleTheme={toggleTheme}
      />

      {/* 主内容 */}
      <main className={`${sidebarCollapsed ? 'ml-16' : 'ml-[232px]'} flex-1 min-h-screen px-9 py-7 transition-all duration-200`}>
        {activeTab === 'logs' ? (
          <LogViewerPage />
        ) : activeTab === 'settings' ? (
          <SettingsPanel
            theme={theme}
            onToggleTheme={toggleTheme}
            sidebarCollapsed={sidebarCollapsed}
            onToggleSidebarCollapsed={toggleSidebarCollapsed}
          />
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
        {/* 页头 + 指标条（自本文件拆出，纯代码搬移） */}
        <DashboardHeadMetrics
          total={data?.total ?? 0}
          enabledCount={allCredentials.length - disabledCredentialCount}
          disabledCredentialCount={disabledCredentialCount}
          abnormalCount={abnormalCount}
          creditsTotal={liveCreditsTotal}
          creditsQueried={liveCreditsQueried}
          avgRemainingPercent={avgRemainingPercent}
          consumedCreditsTotal={consumedCreditsTotal}
          consumableUsageLimitSum={
            consumable.length > 0 ? consumable.reduce((sum, b) => sum + b.usageLimit, 0) : null
          }
          todayRequests={todayRequests}
          requestsDeltaPercent={requestsDeltaPercent}
          requestTrend={requestTrend}
          todayCredits={todayCredits}
          todayCreditsSaved={todayCreditsSaved}
          creditsDeltaPercent={creditsDeltaPercent}
          creditsTrend={creditsTrend}
          cumulativeFailures={cumulativeFailures}
          cumulativeFailureRate={cumulativeFailureRate}
          onTodayClick={() => { setDailyView('list'); setDailyFromSidebar(false) }}
        />

        {/* 凭据列表（自本文件拆出，纯代码搬移） */}
        <CredentialList
          allCredentials={allCredentials}
          disabledCredentialCount={disabledCredentialCount}
          handleRefresh={handleRefresh}
          handleQueryCurrentPageInfo={handleQueryCurrentPageInfo}
          queryingInfo={queryingInfo}
          queryInfoProgress={queryInfoProgress}
          handleClearAll={handleClearAll}
          openKamImport={() => setKamImportDialogOpen(true)}
          openBatchImport={() => setBatchImportDialogOpen(true)}
          verifying={verifying}
          verifyDialogOpen={verifyDialogOpen}
          openVerifyDialog={() => setVerifyDialogOpen(true)}
          verifyProgress={verifyProgress}
          openAddDialog={() => setAddDialogOpen(true)}
          searchQuery={searchQuery}
          onSearchChange={setSearchQuery}
          statusFilter={statusFilter}
          onStatusFilterChange={setStatusFilter}
          counts={stateCounts}
          dataUpdatedAt={dataUpdatedAt}
          paged={paged}
          sorted={sorted}
          allPagedSelected={allPagedSelected}
          somePagedSelected={somePagedSelected}
          toggleSelectPage={toggleSelectPage}
          sortKey={sortKey}
          sortDir={sortDir}
          handleSort={handleSort}
          isFiltered={isFiltered}
          clearFilters={clearFilters}
          selectedIds={selectedIds}
          selectedDisabledCount={selectedDisabledCount}
          handleBatchVerify={handleBatchVerify}
          handleBatchResetFailure={handleBatchResetFailure}
          handleBatchDelete={handleBatchDelete}
          deselectAll={deselectAll}
          page={page}
          totalPages={totalPages}
          itemsPerPage={itemsPerPage}
          setCurrentPage={setCurrentPage}
          balanceMap={balanceMap}
          loadingBalanceIds={loadingBalanceIds}
          rpmByCredential={rpmData?.byCredential}
          toggleSelect={toggleSelect}
          setFailureLogCredentialId={setFailureLogCredentialId}
          setThrottleLogCredentialId={setThrottleLogCredentialId}
          onViewModels={(id) => {
            setModelsCredentialId(id)
            setModelsDialogOpen(true)
          }}
          handleViewBalance={handleViewBalance}
          setDetailCredentialId={setDetailCredentialId}
          handleRefetchBalance={handleRefetchBalance}
        />
        </>
        )}
      </main>

      {/* 对话框群（自本文件拆出，纯代码搬移） */}
      <DashboardDialogs
        selectedCredentialId={selectedCredentialId}
        balanceDialogOpen={balanceDialogOpen}
        setBalanceDialogOpen={setBalanceDialogOpen}
        modelsCredentialId={modelsCredentialId}
        modelsDialogOpen={modelsDialogOpen}
        setModelsDialogOpen={setModelsDialogOpen}
        addDialogOpen={addDialogOpen}
        setAddDialogOpen={setAddDialogOpen}
        batchImportDialogOpen={batchImportDialogOpen}
        setBatchImportDialogOpen={setBatchImportDialogOpen}
        kamImportDialogOpen={kamImportDialogOpen}
        setKamImportDialogOpen={setKamImportDialogOpen}
        verifyDialogOpen={verifyDialogOpen}
        setVerifyDialogOpen={setVerifyDialogOpen}
        verifying={verifying}
        verifyProgress={verifyProgress}
        verifyResults={verifyResults}
        handleCancelVerify={handleCancelVerify}
      />
    </div>
  )
}
