// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useEffect, useMemo, useRef } from 'react'
import { RefreshCw, LogOut, Server, Plus, Upload, FileUp, Trash2, CheckCircle2, Key, Settings, BarChart2, ScrollText, Boxes, Sun, Moon, Info, History, PanelLeftClose, PanelLeftOpen, FileText } from 'lucide-react'
import { useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { useTranslation } from 'react-i18next'
import { storage } from '@/lib/storage'
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { AccountRow } from '@/components/account-row'
import { AccountMetrics } from '@/components/account-metrics'
import { AccountToolbar, type AccountStatusFilter } from '@/components/account-toolbar'
import { AccountTable } from '@/components/account-table'
import { AccountPanelFoot } from '@/components/account-panel-foot'
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
import { useCredentials, useDeleteCredential, useResetFailure, useRpm, useDailyUsage, useServerInfo, CREDENTIALS_REFETCH_INTERVAL_MS } from '@/hooks/use-credentials'
import { useTheme } from '@/hooks/use-theme'
import { DailyStatsPage } from '@/components/daily-stats-page'
import { ModelListPage } from '@/components/model-list-page'
import { ChangelogPage } from '@/components/changelog-page'
import { DailyDetailPage } from '@/components/daily-detail-page'
import { PageHead } from '@/components/page-head'
import { getCredentialBalance } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import {
  accountLabel,
  deriveAccountState,
  sortCredentials,
  type AccountSortKey,
  type SortDirection,
} from '@/lib/account-state'
import type { BalanceResponse, ApiKeyItem } from '@/types/api'

interface DashboardProps {
  onLogout: () => void
}

// 侧栏身份区：后端仅有单一管理员口令，无用户名概念 → 名称固定，头像取首字母
const ADMIN_NAME = 'admin'

/**
 * credits 环比的最小昨日基线。
 *
 * credits 是浮点量，可任意接近 0，若沿用调用次数那套「分母 > 0」守卫，昨日 0.001、
 * 今日 5 会算出 ↑499900%，而 Delta 徽标不做上限裁剪，会把这种无意义的值直接渲染出来。
 * 调用次数不存在该问题（整数分母最小为 1，放大倍数天然有上限）。
 */
const CREDITS_DELTA_MIN_BASE = 0.5

/** 本地时区 YYYY-MM-DD（日用量接口按本地日期对齐） */
function formatLocalDate(d: Date) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

/** 操作条按钮基类（设计稿 .btn）：31px 高 / 7px 圆角 / 15px 图标 */
const ACTION_BTN_BASE =
  'inline-flex h-[31px] items-center gap-1.5 whitespace-nowrap rounded-[7px] border px-[11px] text-[12.5px] font-medium shadow-hair transition-colors [&_svg]:size-[15px] [&_svg]:shrink-0 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-brand disabled:pointer-events-none disabled:opacity-50'
/** 常规态（设计稿 .btn） */
const ACTION_BTN = `${ACTION_BTN_BASE} border-hairline-2 bg-surface text-ink-2 [&_svg]:text-ink-3 hover:border-ink-3 hover:bg-surface-2 hover:text-ink hover:[&_svg]:text-ink-2`
/** 危险态（设计稿 .btn-danger） */
const ACTION_BTN_DANGER = `${ACTION_BTN_BASE} border-danger-line bg-surface text-danger [&_svg]:text-danger hover:border-danger hover:bg-danger-soft`
/** 主按钮（设计稿 .btn-primary） */
const ACTION_BTN_PRIMARY = `${ACTION_BTN_BASE} border-transparent bg-brand font-semibold text-brand-fg [&_svg]:text-brand-fg [&_svg]:opacity-90 hover:bg-brand-hover`
/** 竖分隔线（设计稿 .actionbar .vdiv） */
const ACTION_VDIV = 'mx-0.5 h-[19px] w-px shrink-0 bg-hairline-2'

const SIDEBAR_COLLAPSED_STORAGE_KEY = 'sidebar-collapsed'
// 与 aside/main 的 Tailwind `duration-200` 宽度过渡保持一致，Logo 头部布局延迟这么久才跟随切换
const SIDEBAR_TRANSITION_MS = 200

function readStoredSidebarCollapsed(): boolean {
  return localStorage.getItem(SIDEBAR_COLLAPSED_STORAGE_KEY) === 'true'
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
  const [dailyFromSidebar, setDailyFromSidebar] = useState(false)
  const cancelVerifyRef = useRef(false)
  const prevTabRef = useRef<'credentials' | 'apikeys' | 'settings' | 'logs' | 'models' | 'changelog' | null>(null)
  const prevDetailCredentialId = useRef<number | null>(null)
  const prevDailyView = useRef<string | null>(null)
  const initialBalanceFetchDone = useRef(false)
  const isFetchingBalances = useRef(false)
  // 单账号重查余额的防重入标记（见 handleRefetchBalance）
  const refetchingBalanceIds = useRef<Set<number>>(new Set())
  const prevEnabledIdsRef = useRef<Set<number> | null>(null)
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
  const credentialsRef = useRef(data?.credentials)
  const { data: rpmData } = useRpm()
  const { mutate: deleteCredential } = useDeleteCredential()
  const { mutate: resetFailure } = useResetFailure()
  const { data: dailyUsageData } = useDailyUsage()

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
  const lowestAccount = enabledRemaining.length > 0
    ? enabledRemaining.reduce((min, item) => (item.percent < min.percent ? item : min))
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
      all: filtered.length, healthy: 0, abnormal: 0, disabled: 0, pending: 0,
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
    balanceMap.forEach(b => { total += b.remaining })
    setLiveCreditsTotal(balanceMap.size > 0 ? total : null)
    setLiveCreditsQueried(balanceMap.size)
  }, [balanceMap]) // eslint-disable-line react-hooks/exhaustive-deps

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

  // 侧栏导航：分组 + 项数据驱动，count 为 undefined 时不渲染计数（避免数据未加载时出现误导性的 0）
  // 页脚状态点：加载中 / 请求失败时 serverInfo 为 undefined；已有缓存后端再断开则由 isError 兜底转灰
  const serverHealthy = !!serverInfo?.version && !serverInfoError
  const serverStatusLabel = serverHealthy
    ? `kiro2cc-proxy v${serverInfo.version} · ${t('dashboard.serviceRunning')}`
    : t('dashboard.serviceUnknown')

  const navGroups = [
    {
      title: t('dashboard.navMain'),
      items: [
        { key: 'credentials', label: t('dashboard.navCredentials'), icon: Server, count: data?.credentials.length, active: activeTab === 'credentials' && dailyView === null, onClick: () => { setActiveTab('credentials'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
        { key: 'apikeys', label: 'API Keys', icon: Key, count: undefined, active: activeTab === 'apikeys', onClick: () => { setActiveTab('apikeys'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) } },
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
      {/* 左侧 Sidebar */}
      <aside className={`${sidebarCollapsed ? 'w-16' : 'w-[232px]'} bg-sidebar bg-grid-dot border-r border-hairline fixed top-0 left-0 bottom-0 flex flex-col z-10 transition-all duration-200`}>
        {/* 内容区整体做一次透明度过渡：把 header/nav/footer 所有跟随收起态瞬时切换的布局
            （flex 方向、文字显隐、ml-auto）都藏在这次淡出淡入的不可见瞬间，避免逐处单独处理时互相错位 */}
        <div className={`flex h-full flex-col transition-opacity duration-100 ${sidebarContentFading ? 'opacity-0' : 'opacity-100'}`}>
        <div className={`flex items-center border-b border-hairline ${sidebarContentCollapsed ? 'flex-col gap-2 px-2 py-3' : 'gap-2.5 px-4 pt-4 pb-3.5'}`}>
          <a
            href="https://github.com/TsinHzl/kiro2cc-proxy"
            target="_blank"
            rel="noopener noreferrer"
            className={`flex items-center group min-w-0 ${sidebarContentCollapsed ? '' : 'gap-2.5'}`}
          >
            {/* 方案 4（Aurora Prism）图标自带圆角底座与极光边框，故不再套品牌渐变方块；
                随主题切换 dark / light 两版，与设计稿 preview.html 的实机模拟一致 */}
            {/* 必须走 BASE_URL 拼接：vite 的 base('/admin/') 只重写 index.html 的
                href/src，不改 TS 源码字符串，写死 "/logo-*.svg" 会打到根路径 404 */}
            <img
              src={`${import.meta.env.BASE_URL}logo-aurora-dark.svg`}
              alt="Kiro2CCProxy"
              className="hidden h-[30px] w-[30px] shrink-0 rounded-[7px] shadow-hair dark:block"
            />
            <img
              src={`${import.meta.env.BASE_URL}logo-aurora-light.svg`}
              alt="Kiro2CCProxy"
              className="h-[30px] w-[30px] shrink-0 rounded-[7px] shadow-hair dark:hidden"
            />
            {!sidebarContentCollapsed && (
              <div className="min-w-0">
                <div className="text-[13.5px] font-semibold leading-[1.2] tracking-[-.01em] group-hover:text-brand transition-colors">Kiro2CCProxy</div>
                <div className="text-[10.5px] tracking-[.02em] text-ink-3 group-hover:text-brand transition-colors">{t('dashboard.consoleSubtitle')}</div>
              </div>
            )}
          </a>
          <Button
            variant="ghost"
            size="icon"
            className={`h-7 w-7 shrink-0 text-ink-3 hover:bg-surface-3 hover:text-ink-2 ${sidebarContentCollapsed ? '' : 'ml-auto'}`}
            onClick={toggleSidebarCollapsed}
            title={sidebarCollapsed ? t('dashboard.expandSidebar') : t('dashboard.collapseSidebar')}
            aria-label={sidebarCollapsed ? t('dashboard.expandSidebar') : t('dashboard.collapseSidebar')}
          >
            {sidebarCollapsed ? <PanelLeftOpen className="h-3.5 w-3.5" /> : <PanelLeftClose className="h-3.5 w-3.5" />}
          </Button>
        </div>
        <nav className="flex-1 overflow-y-auto px-2 py-3">
          <TooltipProvider delayDuration={200}>
            {navGroups.map((group, groupIndex) => (
              <div key={group.title}>
                {sidebarContentCollapsed ? (
                  /* 设计稿 .shell.is-collapsed .nav-group：标题降级为 1px 分隔线，首组不渲染 */
                  groupIndex > 0 && (
                    <div role="separator" aria-label={group.title} className="mx-[14px] my-[9px] h-px bg-hairline-2" />
                  )
                ) : (
                  <div className={`px-2.5 pb-1.5 text-[10px] font-semibold uppercase tracking-[.09em] text-ink-3 ${groupIndex === 0 ? 'pt-0.5' : 'pt-3'}`}>
                    {group.title}
                  </div>
                )}
                {group.items.map(({ key, label, icon: Icon, count, active, onClick }) => {
                  // tooltip 与 aria-label 同源：收起态文案 / 计数被隐藏，可访问名称仍完整
                  const fullLabel = count === undefined ? label : `${label} · ${count}`
                  const item = (
                    <button
                      key={key}
                      onClick={onClick}
                      aria-label={fullLabel}
                      aria-current={active ? 'true' : undefined}
                      className={`relative mb-0.5 flex h-[33px] w-full items-center rounded-[7px] text-[12.5px] transition-colors ${sidebarContentCollapsed ? 'justify-center' : 'gap-[9px] px-2.5'} ${active ? 'bg-brand-soft font-semibold text-brand' : 'font-[450] text-ink-2 hover:bg-surface-3 hover:text-ink'}`}
                    >
                      {active && (
                        /* -left-2 与 <nav> 的 px-2 数值耦合：竖条要贴在侧栏左边缘（padding-box x=0），
                           两处必须同步；展开态与 64px 收起态共用此几何，对齐设计稿 .nav-item::before{left:-8px} */
                        <span className="absolute -left-2 top-2 bottom-2 w-[2.5px] rounded-r-[3px] bg-brand" aria-hidden="true" />
                      )}
                      <Icon className="w-4 h-4 shrink-0" />
                      {!sidebarContentCollapsed && (
                        <>
                          <span className="truncate">{label}</span>
                          {count !== undefined && (
                            <span className={`ml-auto text-[10.5px] font-medium ${active ? 'text-brand' : 'text-ink-3'}`}>{count}</span>
                          )}
                        </>
                      )}
                    </button>
                  )
                  return sidebarContentCollapsed ? (
                    <Tooltip key={key}>
                      <TooltipTrigger asChild>{item}</TooltipTrigger>
                      <TooltipContent side="right">{fullLabel}</TooltipContent>
                    </Tooltip>
                  ) : (
                    item
                  )
                })}
              </div>
            ))}
          </TooltipProvider>
        </nav>
        {/* 身份区（设计稿 .side-user）：头像 + 名称 / 角色 + 退出（hover 转 danger） */}
        <div className={`flex items-center border-t border-hairline ${sidebarContentCollapsed ? 'flex-col gap-[9px] py-2.5' : 'gap-[9px] px-3 py-2.5'}`}>
          <div aria-hidden="true" className="grid h-7 w-7 shrink-0 place-items-center rounded-[8px] border border-hairline-2 bg-surface-3 text-[11.5px] font-bold text-ink-2">
            {ADMIN_NAME.charAt(0).toUpperCase()}
          </div>
          {!sidebarContentCollapsed && (
            <div className="min-w-0">
              <div className="text-[12px] font-semibold leading-[1.3]">{ADMIN_NAME}</div>
              <div className="text-[10px] text-ink-3">{t('dashboard.adminRole')}</div>
            </div>
          )}
          <Button
            variant="ghost"
            size="icon"
            className={`h-7 w-7 shrink-0 text-ink-3 hover:bg-danger-soft hover:text-danger ${sidebarContentCollapsed ? '' : 'ml-auto'}`}
            onClick={handleLogout}
            title={t('common.logout')}
            aria-label={t('common.logout')}
          >
            <LogOut className="h-3.5 w-3.5" />
          </Button>
        </div>
        {/* 页脚（设计稿 .side-foot）：运行状态点 + 版本号 + 主题切换 */}
        <div className={`flex items-center border-t border-hairline ${sidebarContentCollapsed ? 'flex-col gap-[9px] py-2.5' : 'gap-2 px-[14px] py-2.5'}`}>
          <span
            role="img"
            aria-label={serverStatusLabel}
            title={serverStatusLabel}
            className={`h-1.5 w-1.5 shrink-0 rounded-full ring-[3px] ${serverHealthy ? 'bg-ok ring-ok-soft' : 'bg-ink-3 ring-surface-3'}`}
          />
          {/* 加载中 / 请求失败时连同版本号一并隐藏，只留灰点，避免展示 `v...` 这类无效版本 */}
          {!sidebarContentCollapsed && serverHealthy && (
            <a
              href="https://github.com/TsinHzl/kiro2cc-proxy"
              target="_blank"
              rel="noopener noreferrer"
              className="truncate text-[10.5px] text-ink-3 hover:text-brand transition-colors"
            >
              kiro2cc-proxy v{serverInfo.version}
            </a>
          )}
          <Button
            variant="ghost"
            size="icon"
            className={`h-7 w-7 shrink-0 text-ink-3 hover:bg-surface-3 hover:text-ink-2 ${sidebarContentCollapsed ? '' : 'ml-auto'}`}
            onClick={toggleTheme}
            title={theme === 'dark' ? t('dashboard.toggleLightMode') : t('dashboard.toggleDarkMode')}
            aria-label={theme === 'dark' ? t('dashboard.toggleLightMode') : t('dashboard.toggleDarkMode')}
          >
            {theme === 'dark' ? <Sun className="h-3.5 w-3.5" /> : <Moon className="h-3.5 w-3.5" />}
          </Button>
        </div>
        </div>
      </aside>

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
        {/* 页头（设计稿 .head）：面包屑 + 19px 标题 + 同基线副标题 + 右侧刷新标签与文档入口 */}
        <PageHead
          crumb={[t('dashboard.navMain'), t('dashboard.navCredentials')]}
          title={t('dashboard.navCredentials')}
          note={t('dashboard.pageSubtitle')}
          actions={
            <>
              <span className="inline-flex h-5 shrink-0 items-center gap-[5px] rounded-md border border-ok-line bg-ok-soft px-[7px] text-[11px] font-semibold text-ok">
                <span className="h-[5px] w-[5px] shrink-0 animate-pulse rounded-full bg-ok ring-[2.5px] ring-ok-soft" aria-hidden="true" />
                {t('dashboard.autoRefreshTag', { seconds: Math.round(CREDENTIALS_REFETCH_INTERVAL_MS / 1000) })}
              </span>
              <a
                href="https://github.com/TsinHzl/kiro2cc-proxy#readme"
                target="_blank"
                rel="noopener noreferrer"
                className="group inline-flex h-[31px] items-center gap-1.5 rounded-[7px] px-[11px] text-[12.5px] font-medium text-ink-2 transition-colors hover:bg-surface-3 hover:text-ink"
              >
                <FileText className="h-3.5 w-3.5 text-ink-3 transition-colors group-hover:text-ink-2" />
                {t('dashboard.docs')}
              </a>
            </>
          }
        />
        {/* 指标条（设计稿 .metrics） */}
        <div className="mb-[15px]">
          <AccountMetrics
            total={data?.total ?? 0}
            enabledCount={allCredentials.length - disabledCredentialCount}
            disabledCount={disabledCredentialCount}
            abnormalCount={abnormalCount}
            creditsTotal={liveCreditsTotal}
            creditsQueried={liveCreditsQueried}
            avgRemainingPercent={avgRemainingPercent}
            lowestAccount={lowestAccount}
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
        </div>

        {/* 凭据列表 */}
        <div className="space-y-4">
          {/* 操作条（设计稿 .actionbar）：6 项常驻操作，危险操作用竖分隔线隔离并染红 */}
          <div className="flex flex-wrap items-center gap-[7px]">
            <button type="button" onClick={handleRefresh} aria-label={t('dashboard.refreshList')} className={ACTION_BTN}>
              <RefreshCw />
              <span className="hidden sm:inline">{t('dashboard.refreshList')}</span>
            </button>
            {allCredentials.length > 0 && (
              <button
                type="button"
                onClick={handleQueryCurrentPageInfo}
                disabled={queryingInfo}
                aria-label={t('dashboard.queryInfo')}
                className={ACTION_BTN}
              >
                <Info className={queryingInfo ? 'animate-pulse' : ''} />
                <span className="hidden sm:inline">
                  {queryingInfo
                    ? t('dashboard.queryingProgress', { current: queryInfoProgress.current, total: queryInfoProgress.total })
                    : t('dashboard.queryInfo')}
                </span>
              </button>
            )}
            {/* 「清除已禁用」两侧的竖线随按钮一起显隐，空列表时不留孤立分隔线 */}
            {allCredentials.length > 0 && (
              <>
                <span aria-hidden="true" className={ACTION_VDIV} />
                <button
                  type="button"
                  onClick={handleClearAll}
                  disabled={disabledCredentialCount === 0}
                  title={disabledCredentialCount === 0 ? t('dashboard.noClearableDisabled') : undefined}
                  aria-label={t('dashboard.clearDisabled')}
                  className={ACTION_BTN_DANGER}
                >
                  <Trash2 />
                  <span className="hidden sm:inline">{t('dashboard.clearDisabled')}</span>
                </button>
                <span aria-hidden="true" className={ACTION_VDIV} />
              </>
            )}
            <button
              type="button"
              onClick={() => setKamImportDialogOpen(true)}
              aria-label={t('dashboard.kamImport')}
              className={ACTION_BTN}
            >
              <FileUp />
              <span className="hidden sm:inline">{t('dashboard.kamImport')}</span>
            </button>
            <button
              type="button"
              onClick={() => setBatchImportDialogOpen(true)}
              aria-label={t('dashboard.batchImport')}
              className={ACTION_BTN}
            >
              <Upload />
              <span className="hidden sm:inline">{t('dashboard.batchImport')}</span>
            </button>
            {/* 验活进度浮动入口：设计稿无此项，为保留既有能力挂在主按钮左侧 */}
            {verifying && !verifyDialogOpen && (
              <button type="button" onClick={() => setVerifyDialogOpen(true)} className={ACTION_BTN}>
                <CheckCircle2 className="animate-spin" />
                {t('dashboard.verifyingProgress', { current: verifyProgress.current, total: verifyProgress.total })}
              </button>
            )}
            <button
              type="button"
              onClick={() => setAddDialogOpen(true)}
              aria-label={t('dashboard.addAccount')}
              className={`${ACTION_BTN_PRIMARY} ml-auto`}
            >
              <Plus />
              <span className="hidden sm:inline">{t('dashboard.addAccount')}</span>
            </button>
          </div>

          {/* 工具栏（设计稿 .toolbar）：搜索 + 状态分段筛选 + 更新时间 */}
          <AccountToolbar
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            statusFilter={statusFilter}
            onStatusFilterChange={setStatusFilter}
            counts={stateCounts}
            dataUpdatedAt={dataUpdatedAt}
          />

          {allCredentials.length === 0 ? (
            <Card>
              <CardContent className="py-8 text-center text-muted-foreground">
                {t('dashboard.noAccounts')}
              </CardContent>
            </Card>
          ) : (
            <AccountTable
              rowCount={paged.length}
              allSelected={allPagedSelected}
              someSelected={somePagedSelected}
              onToggleSelectAll={toggleSelectPage}
              sortKey={sortKey}
              sortDir={sortDir}
              onSort={handleSort}
              isFiltered={isFiltered}
              onClearFilters={clearFilters}
              footer={
                <AccountPanelFoot
                  selectedCount={selectedIds.size}
                  selectedDisabledCount={selectedDisabledCount}
                  onBatchVerify={handleBatchVerify}
                  onBatchRestore={handleBatchResetFailure}
                  onBatchDelete={handleBatchDelete}
                  onDeselectAll={deselectAll}
                  totalCount={sorted.length}
                  isFiltered={isFiltered}
                  page={page}
                  totalPages={totalPages}
                  itemsPerPage={itemsPerPage}
                  onPageChange={setCurrentPage}
                />
              }
            >
              {paged.map((credential) => (
                <AccountRow
                  key={credential.id}
                  credential={credential}
                  balance={balanceMap.get(credential.id) ?? null}
                  loadingBalance={loadingBalanceIds.has(credential.id)}
                  rpm={rpmData?.byCredential?.[String(credential.id)] ?? 0}
                  selected={selectedIds.has(credential.id)}
                  onToggleSelect={() => toggleSelect(credential.id)}
                  onViewFailureLog={(id) => setFailureLogCredentialId(id)}
                  onViewThrottleLog={(id) => setThrottleLogCredentialId(id)}
                  onViewBalance={handleViewBalance}
                  onViewDetail={(id) => setDetailCredentialId(id)}
                  onRefetchBalance={handleRefetchBalance}
                />
              ))}
            </AccountTable>
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
