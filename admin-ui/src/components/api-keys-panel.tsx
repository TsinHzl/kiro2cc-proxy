// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useRef, useEffect, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Copy, Plus, Check, Clock, RotateCw, DollarSign, Search, Loader2, Link2, ChevronDown, X, FileText, Eye, EyeOff, Eraser, Box } from 'lucide-react'
import { toast } from 'sonner'
import { PageHead } from '@/components/page-head'
import { Delta, FootSep, Metric, MetricAside, MetricFoot, MetricValue, MetricsBar, Ring, Sparkline } from '@/components/metrics'
import { SearchBox, Segmented, Toolbar, UpdatedAgo, type SegmentedOption } from '@/components/toolbar'
import { ApiKeyTable } from '@/components/api-key-table'
import { ApiKeyRow, quotaTone, type KeyStatus } from '@/components/api-key-row'
import { ApiKeyPanelFoot } from '@/components/api-key-panel-foot'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { useQueryClient } from '@tanstack/react-query'
import { useApiKeys, useCreateApiKey, useUpdateApiKey, useDeleteApiKey, useAllUsage, useResetKeyUsage, useRpm, useCredentials, useCredentialBalances, useDailyUsage } from '@/hooks/use-credentials'
import {
  deleteApiKey as deleteApiKeyApi,
  resetKeyUsage as resetKeyUsageApi,
  updateApiKey as updateApiKeyApi,
} from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import { copyToClipboard as writeToClipboard } from '@/lib/clipboard'
import { importToCcSwitch, type CcSwitchApp } from '@/lib/ccswitch'
import { formatTokenCount, localeTag } from '@/lib/locale'
import type { ApiKeyItem, UsageSummary } from '@/types/api'

interface ApiKeysPanelProps {
  onViewDetail: (key: ApiKeyItem) => void
}

type KeyStatusFilter = 'all' | KeyStatus
type SortBy = 'newest' | 'cost-desc' | 'cost-asc'

/** 到期预警阈值：距今 ≤ 7 天 */
const EXPIRING_SOON_MS = 7 * 24 * 60 * 60 * 1000

/** 每页行数，与账号管理页一致 */
const ITEMS_PER_PAGE = 50

/** 本地时区 YYYY-MM-DD（与日用量接口的日期口径一致） */
function formatLocalDate(d: Date): string {
  const month = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${d.getFullYear()}-${month}-${day}`
}

export function ApiKeysPanel({ onViewDetail }: ApiKeysPanelProps) {
  const { t } = useTranslation()
  const [createDialogOpen, setCreateDialogOpen] = useState(false)
  const [editingKey, setEditingKey] = useState<ApiKeyItem | null>(null)
  const [newName, setNewName] = useState('')
  const [newMode, setNewMode] = useState<'date' | 'quota'>('quota')
  const [newDuration, setNewDuration] = useState<number | null>(1) // 数值，null 表示永不过期
  const [newDurationUnit, setNewDurationUnit] = useState<'days' | 'hours'>('days')
  const [newSpendingLimit, setNewSpendingLimit] = useState(100)
  const [newLimitUnit, setNewLimitUnit] = useState<'usd' | 'credits'>('usd')
  /** 是否选择"无限额度"，默认开启（不限制消费额度） */
  const [newUnlimited, setNewUnlimited] = useState(true)
  const [newBoundCredentialIds, setNewBoundCredentialIds] = useState<number[]>([])
  const [editName, setEditName] = useState('')
  const [editMode, setEditMode] = useState<'date' | 'quota'>('date')
  const [editDuration, setEditDuration] = useState<number | null | string>(1)
  const [editDurationUnit, setEditDurationUnit] = useState<'days' | 'hours'>('days')
  const [editBoundCredentialIds, setEditBoundCredentialIds] = useState<number[]>([])
  const [editSpendingLimit, setEditSpendingLimit] = useState(50)
  const [editLimitUnit, setEditLimitUnit] = useState<'usd' | 'credits'>('usd')
  const [copiedId, setCopiedId] = useState<number | null>(null)
  const [copiedType, setCopiedType] = useState<'cc' | 'codex' | null>(null)
  const [sortBy, setSortBy] = useState<SortBy>('newest')
  const [searchQuery, setSearchQuery] = useState('')
  const [statusFilter, setStatusFilter] = useState<KeyStatusFilter>('all')
  const [revealAll, setRevealAll] = useState(false)
  /** 跨页保留的勾选集合（与账号管理页一致） */
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set())
  const [currentPage, setCurrentPage] = useState(1)
  const [batching, setBatching] = useState(false)
  const [purgeDialogOpen, setPurgeDialogOpen] = useState(false)
  const [purging, setPurging] = useState(false)
  const [createCredDropdownOpen, setCreateCredDropdownOpen] = useState(false)
  const [editCredDropdownOpen, setEditCredDropdownOpen] = useState(false)
  const [credSearchQuery, setCredSearchQuery] = useState('')
  const createCredDropdownRef = useRef<HTMLDivElement>(null)
  const editCredDropdownRef = useRef<HTMLDivElement>(null)

  const quickDurationOptions = [
    { value: 1, unit: 'hours' as const },
    { value: 3, unit: 'hours' as const },
    { value: 6, unit: 'hours' as const },
    { value: 12, unit: 'hours' as const },
    { value: 1, unit: 'days' as const },
    { value: 3, unit: 'days' as const },
    { value: 7, unit: 'days' as const },
  ]

  const unitLabel = (unit: 'days' | 'hours') => t(unit === 'hours' ? 'apiKeys.hoursUnit' : 'apiKeys.daysUnit')

  const toDays = (value: number, unit: 'days' | 'hours') => unit === 'hours' ? value / 24 : value

  const formatDuration = (days: number) => {
    if (days < 1) {
      const hours = Math.round(days * 24 * 100) / 100
      return `${hours} ${t('apiKeys.hoursUnit')}`
    }
    return `${days} ${t('apiKeys.daysUnit')}`
  }

  const { data: credentials } = useCredentials()
  const { data: apiKeys, isLoading, refetch: refetchKeys } = useApiKeys()
  const { data: usageData, dataUpdatedAt, refetch: refetchUsage } = useAllUsage()
  const { data: rpmData } = useRpm()
  const { data: dailyUsageData } = useDailyUsage()
  const queryClient = useQueryClient()
  const { mutate: createKey, isPending: isCreating } = useCreateApiKey()
  const { mutate: updateKey } = useUpdateApiKey()
  const { mutate: deleteKey } = useDeleteApiKey()
  const { mutate: resetUsage } = useResetKeyUsage()

  // 构建 credential id -> CredentialStatusItem 映射
  const credentialMap = new Map(
    (credentials?.credentials ?? []).map((c) => [c.id, c])
  )

  // 批量查询所有凭据余额（含未绑定的，供下拉选择时展示）
  const allCredIds = (credentials?.credentials ?? []).map((c) => c.id)
  const credentialBalanceMap = useCredentialBalances(allCredIds)

  // 点击外部关闭下拉
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (createCredDropdownRef.current && !createCredDropdownRef.current.contains(e.target as Node)) {
        setCreateCredDropdownOpen(false)
      }
      if (editCredDropdownRef.current && !editCredDropdownRef.current.contains(e.target as Node)) {
        setEditCredDropdownOpen(false)
      }
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [])

  // 构建 key_id -> usage 的映射
  const usageMap = new Map<number, UsageSummary>()
  usageData?.forEach((u) => usageMap.set(u.apiKeyId, u))

  const handleResetUsage = (key: ApiKeyItem) => {
    if (!confirm(t('apiKeys.confirmResetUsage', { name: key.name }))) return
    resetUsage(key.id, {
      onSuccess: () => toast.success(t('apiKeys.toastUsageReset')),
      onError: (err) => toast.error(extractErrorMessage(err)),
    })
  }

  const getKeyStatus = (key: ApiKeyItem): KeyStatus => {
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
    toast.success(t('apiKeys.toastPurgeSuccess', { count: deleted }))
  }

  /** 跳转 cc-switch 导入本行 Key；协议未注册时退化为提示 + 复制链接 */
  const handleImportToCcSwitch = (app: CcSwitchApp, apiKey: ApiKeyItem) => {
    importToCcSwitch(
      app,
      { apiKey: apiKey.key, keyName: apiKey.name },
      {
        onNotInstalled: (link) => {
          toast.error(t('apiKeys.ccSwitchNotInstalled'), {
            action: {
              label: t('apiKeys.ccSwitchCopyLink'),
              onClick: () => {
                void writeToClipboard(link)
              },
            },
          })
        },
      }
    )
  }
  const copyToClipboard = async (text: string, target: 'cc' | 'codex' | number) => {
    await writeToClipboard(text)
    if (target === 'cc' || target === 'codex') {
      setCopiedType(target)
      setTimeout(() => setCopiedType(null), 2000)
    } else {
      setCopiedId(target)
      setTimeout(() => setCopiedId(null), 2000)
    }
    toast.success(t('apiKeys.toastCopiedToClipboard'))
  }

  const handleCreate = () => {
    createKey(
      {
        name: newName,
        ...(newMode === 'date'
          ? newDuration !== null
            ? { durationDays: toDays(newDuration, newDurationUnit) }
            : {}
          : { spendingLimit: newUnlimited ? null : newSpendingLimit, limitUnit: newLimitUnit }),
        boundCredentialIds: newBoundCredentialIds.length > 0 ? newBoundCredentialIds : null,
      },
      {
        onSuccess: () => {
          toast.success(t('apiKeys.toastCreateSuccess'))
          setCreateDialogOpen(false)
          setNewName('')
          setNewMode('quota')
          setNewDuration(1)
          setNewDurationUnit('days')
          setNewSpendingLimit(100)
          setNewLimitUnit('usd')
          setNewUnlimited(true)
          setNewBoundCredentialIds([])
        },
        onError: (err) => toast.error(t('apiKeys.toastCreateFailed', { message: extractErrorMessage(err) })),
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
      data.limitUnit = editLimitUnit
      data.expiresAt = null // 清除过期时间
      data.durationDays = null // 清除懒激活
    }
    data.boundCredentialIds = editBoundCredentialIds.length > 0 ? editBoundCredentialIds : null
    updateKey(
      { id: editingKey.id, data },
      {
        onSuccess: () => {
          toast.success(t('apiKeys.toastUpdated'))
          setEditingKey(null)
        },
        onError: (err) => toast.error(t('apiKeys.toastUpdateFailed', { message: extractErrorMessage(err) })),
      }
    )
  }

  const handleToggleEnabled = (key: ApiKeyItem) => {
    updateKey(
      { id: key.id, data: { enabled: !key.enabled } },
      {
        onSuccess: () => toast.success(key.enabled ? t('apiKeys.statusDisabled') : t('apiKeys.toastEnabled')),
        onError: (err) => toast.error(extractErrorMessage(err)),
      }
    )
  }

  const handleDelete = (key: ApiKeyItem) => {
    if (!confirm(t('apiKeys.confirmDelete', { name: key.name }))) return
    deleteKey(key.id, {
      onSuccess: () => toast.success(t('apiKeys.toastDeleted')),
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
      setEditLimitUnit(key.limitUnit ?? 'usd')
      setEditDuration(1)
    } else {
      setEditMode('date')
      setEditSpendingLimit(50)
      setEditLimitUnit('usd')
      if (key.durationDays != null && key.durationDays < 1) {
        setEditDuration(Math.round(key.durationDays * 24 * 100) / 100)
        setEditDurationUnit('hours')
      } else {
        setEditDuration(key.durationDays ?? 1)
        setEditDurationUnit('days')
      }
    }
    setEditBoundCredentialIds(key.boundCredentialIds ?? [])
  }

  const formatDate = (dateStr: string) => {
    return new Date(dateStr).toLocaleString(localeTag(), {
      year: 'numeric', month: '2-digit', day: '2-digit',
      hour: '2-digit', minute: '2-digit',
    })
  }

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

  // refetch() 不抛异常（TanStack Query 默认 throwOnError: false），故检查返回结果的 isError 而非 try/catch
  const handleRefresh = async () => {
    const results = await Promise.all([refetchKeys(), refetchUsage()])
    const failed = results.find((r) => r.isError)
    if (failed) {
      toast.error(extractErrorMessage(failed.error))
      return
    }
    toast.success(t('apiKeys.toastRefreshed'))
  }

  // ===== 指标条派生 =====
  const allKeys = apiKeys ?? []
  const statusOf = new Map<number, KeyStatus>(allKeys.map((k) => [k.id, getKeyStatus(k)]))
  const statusCounts: Record<KeyStatusFilter, number> = {
    all: allKeys.length,
    active: 0,
    pending: 0,
    disabled: 0,
    expired: 0,
  }
  allKeys.forEach((k) => {
    const s = statusOf.get(k.id)
    if (s) statusCounts[s] += 1
  })

  const nowMs = Date.now()
  // 已过期的不再计入「即将到期」（left > 0 过滤）
  const expiringSoonCount = allKeys.filter((k) => {
    if (!k.expiresAt) return false
    const left = new Date(k.expiresAt).getTime() - nowMs
    return left > 0 && left <= EXPIRING_SOON_MS
  }).length

  // 今日请求：后端仅提供全局按天汇总（无按 Key 拆分的按天数据），故本卡为全局口径
  const todayStats = dailyUsageData?.find((d) => d.date === formatLocalDate(new Date(nowMs))) ?? null
  const yesterdayStats = dailyUsageData?.find((d) => d.date === formatLocalDate(new Date(nowMs - 86_400_000))) ?? null
  const todayRequests = dailyUsageData ? todayStats?.totalRequests ?? 0 : null
  const requestsDeltaPercent =
    todayRequests !== null && yesterdayStats && yesterdayStats.totalRequests > 0
      ? ((todayRequests - yesterdayStats.totalRequests) / yesterdayStats.totalRequests) * 100
      : null
  const requestTrend = (dailyUsageData ?? [])
    .slice()
    .sort((a, b) => a.date.localeCompare(b.date))
    .slice(-7)
    .map((d) => d.totalRequests)

  // 累计口径限于当前 Key 集合，已删除 Key 的历史用量不计入
  const cumulative = allKeys.reduce(
    (acc, k) => {
      const u = usageMap.get(k.id)
      if (u) {
        acc.requests += u.totalRequests
        acc.tokens += u.totalInputTokens + u.totalOutputTokens
      }
      return acc
    },
    { requests: 0, tokens: 0 }
  )
  const earliestCreated = allKeys.reduce<string | null>(
    (min, k) => (min === null || k.createdAt < min ? k.createdAt : min),
    null
  )
  const sinceLabel = earliestCreated
    ? new Date(earliestCreated).toLocaleDateString(localeTag(), { month: '2-digit', day: '2-digit' })
    : null

  // 配额占用最高：按 limitUnit 分流计量口径（credits 比 totalCredits，usd 比 totalCost）
  const topQuota = allKeys.reduce<{ name: string; used: number; limit: number; unit: 'usd' | 'credits'; percent: number } | null>(
    (top, k) => {
      if (k.spendingLimit == null || k.spendingLimit <= 0) return top
      const u = usageMap.get(k.id)
      const used = k.limitUnit === 'credits' ? u?.totalCredits ?? 0 : u?.totalCost ?? 0
      const percent = (used / k.spendingLimit) * 100
      if (top !== null && top.percent >= percent) return top
      return { name: k.name, used, limit: k.spendingLimit, unit: k.limitUnit, percent }
    },
    null
  )

  const statusSegments: SegmentedOption<KeyStatusFilter>[] = [
    { key: 'all', label: t('credentials.filterAll'), count: statusCounts.all },
    { key: 'active', label: t('apiKeys.statusActive'), pipClass: 'bg-ok', count: statusCounts.active },
    { key: 'pending', label: t('apiKeys.statusPending'), pipClass: 'bg-brand', count: statusCounts.pending },
    { key: 'disabled', label: t('apiKeys.statusDisabled'), pipClass: 'bg-ink-3', count: statusCounts.disabled },
    { key: 'expired', label: t('apiKeys.statusExpired'), pipClass: 'bg-warn', count: statusCounts.expired },
  ]
  const sortSegments: SegmentedOption<SortBy>[] = [
    { key: 'newest', label: t('apiKeys.sortNewest') },
    { key: 'cost-desc', label: t('apiKeys.sortCostDesc') },
    { key: 'cost-asc', label: t('apiKeys.sortCostAsc') },
  ]

  // 不做 useMemo：statusOf 依赖 getKeyStatus 内的当前时间，缓存会让筛选结果与实时状态徽章脱节
  const filteredKeys = (apiKeys ?? []).filter((key) => {
    if (statusFilter !== 'all' && statusOf.get(key.id) !== statusFilter) return false
    if (!searchQuery.trim()) return true
    const q = searchQuery.trim().toLowerCase()
    const serialStr = String(key.id).padStart(3, '0')
    return serialStr.includes(q) || String(key.id).includes(q) || key.name.toLowerCase().includes(q)
  })

  const isFiltered = searchQuery.trim() !== '' || statusFilter !== 'all'

  // 筛选/排序变化后回到首页，避免停留在已不存在的页码上
  useEffect(() => {
    setCurrentPage(1)
  }, [searchQuery, statusFilter, sortBy])

  // usageData 是排序依据（usageMap 每次渲染重建，不能进依赖数组）
  // 注：filteredKeys 每次渲染都是新数组引用，此处缓存实质不生效，仅作意图声明；
  //     当前量级（几十到几百条 Key）排序开销可忽略，不为此引入节流时间源
  const sorted = useMemo(() => {
    const costOf = (k: ApiKeyItem) => usageMap.get(k.id)?.totalCost ?? 0
    return [...filteredKeys].sort((a, b) => {
      if (sortBy === 'cost-desc') return costOf(b) - costOf(a)
      if (sortBy === 'cost-asc') return costOf(a) - costOf(b)
      return new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
    })
  }, [filteredKeys, sortBy, usageData])

  const totalPages = Math.max(1, Math.ceil(sorted.length / ITEMS_PER_PAGE))
  // 删除后总页数可能收缩，钳制到有效范围而不改 state（避免额外一轮渲染）
  const page = Math.min(currentPage, totalPages)
  const paged = sorted.slice((page - 1) * ITEMS_PER_PAGE, page * ITEMS_PER_PAGE)

  const allPagedSelected = paged.length > 0 && paged.every((k) => selectedIds.has(k.id))
  const somePagedSelected = !allPagedSelected && paged.some((k) => selectedIds.has(k.id))
  // 行内单条删除不会清理 selectedIds，计数与批量操作一律以现存 Key 为准，避免对已删除 id 发请求
  const selectedExistingIds = allKeys.filter((k) => selectedIds.has(k.id)).map((k) => k.id)
  const selectedEnabledCount = allKeys.filter((k) => selectedIds.has(k.id) && k.enabled).length

  const toggleSelect = (id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const toggleSelectPage = () => {
    setSelectedIds((prev) => {
      const next = new Set(prev)
      if (allPagedSelected) paged.forEach((k) => next.delete(k.id))
      else paged.forEach((k) => next.add(k.id))
      return next
    })
  }

  /** 批量操作：逐个串行调用，单个失败不中断，结束后统一失效缓存并清空勾选 */
  const runBatch = async (ids: number[], op: (id: number) => Promise<unknown>, successKey: string) => {
    setBatching(true)
    let done = 0
    for (const id of ids) {
      try {
        await op(id)
        done++
      } catch {
        // 单个失败不中断
      }
    }
    setBatching(false)
    queryClient.invalidateQueries({ queryKey: ['apiKeys'] })
    queryClient.invalidateQueries({ queryKey: ['apiKeyUsage'] })
    setSelectedIds(new Set())
    if (done < ids.length) {
      toast.warning(t('apiKeys.toastBatchPartial', { done, total: ids.length }))
      return
    }
    toast.success(t(successKey, { count: done }))
  }

  const handleBatchDisable = () => {
    const ids = allKeys.filter((k) => selectedIds.has(k.id) && k.enabled).map((k) => k.id)
    if (ids.length === 0) return
    if (!confirm(t('apiKeys.confirmBatchDisable', { count: ids.length }))) return
    void runBatch(ids, (id) => updateApiKeyApi(id, { enabled: false }), 'apiKeys.toastBatchDisabled')
  }

  const handleBatchResetUsage = () => {
    const ids = selectedExistingIds
    if (ids.length === 0) return
    if (!confirm(t('apiKeys.confirmBatchReset', { count: ids.length }))) return
    void runBatch(ids, resetKeyUsageApi, 'apiKeys.toastBatchReset')
  }

  /** 表格无独立到期列，把有效期/待激活说明降级为状态徽章的 title */
  const statusTitleOf = (key: ApiKeyItem): string | undefined => {
    if (key.durationDays != null && !key.activatedAt) {
      return t('apiKeys.validityPending', { duration: formatDuration(key.durationDays) })
    }
    if (key.durationDays != null && key.expiresAt) {
      return t('apiKeys.expiresWithDuration', {
        date: formatDate(key.expiresAt),
        duration: formatDuration(key.durationDays),
      })
    }
    if (key.expiresAt) return t('apiKeys.expiresLabel', { date: formatDate(key.expiresAt) })
    return undefined
  }

  /** 剩余天数（向上取整，最少 1 天）；已过期或超出预警窗口返回 null */
  const expiringInDaysOf = (key: ApiKeyItem): number | null => {
    if (!key.expiresAt) return null
    const left = new Date(key.expiresAt).getTime() - nowMs
    if (left <= 0 || left > EXPIRING_SOON_MS) return null
    return Math.max(1, Math.ceil(left / 86_400_000))
  }

  const boundOf = (key: ApiKeyItem) =>
    (key.boundCredentialIds ?? []).map((id) => {
      const bal = credentialBalanceMap.get(id)
      return {
        label: credentialMap.get(id)?.email ?? `#${id}`,
        balance: bal
          ? t('apiKeys.boundBalanceCompact', {
              remaining: bal.remaining.toFixed(2),
              limit: bal.usageLimit.toFixed(2),
              percent: (100 - bal.usagePercentage).toFixed(0),
            })
          : null,
      }
    })

  return (
    <div>
      {/* 页头（设计稿 .head）：面包屑 + 19px 标题 + 同基线副标题 + 右侧状态标签与文档入口 */}
      <PageHead
        crumb={[t('dashboard.navMain'), t('apiKeys.pageTitle')]}
        title={t('apiKeys.pageTitle')}
        note={t('apiKeys.pageSubtitle')}
        actions={
          <>
            <span className="inline-flex h-5 shrink-0 items-center gap-[5px] rounded-md border border-ok-line bg-ok-soft px-[7px] text-[11px] font-semibold text-ok">
              <span className="h-[5px] w-[5px] shrink-0 animate-pulse rounded-full bg-ok ring-[2.5px] ring-ok-soft" aria-hidden="true" />
              {t('apiKeys.serviceRunningTag')}
            </span>
            <a
              href="https://github.com/TsinHzl/kiro2cc-proxy#readme"
              target="_blank"
              rel="noopener noreferrer"
              className="group inline-flex h-[31px] items-center gap-1.5 rounded-[7px] px-[11px] text-[12.5px] font-medium text-ink-2 transition-colors hover:bg-surface-3 hover:text-ink"
            >
              <FileText className="h-3.5 w-3.5 text-ink-3 transition-colors group-hover:text-ink-2" />
              {t('apiKeys.docsLink')}
            </a>
          </>
        }
      />

      {/* 连接卡（设计稿 .conn）：客户端接入所需的 Base URL 与兼容协议一次给全。
          设计稿右侧的「在新窗口打开」未实现 —— 本服务无可浏览端点（/v1/* 均需认证），渲染即死交互 */}
      <section className="flex shrink-0 flex-wrap items-center gap-4 rounded-[11px] border border-hairline bg-surface px-4 py-[13px] shadow-hair">
        <div className="min-w-0">
          <div className="flex items-center gap-[5px] text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3">
            <Link2 className="size-[13px] shrink-0" />
            {t('apiKeys.connCcBaseUrlLabel')}
          </div>
          <div className="mt-1 truncate font-mono text-[14px] font-medium tracking-[-.01em]">
            <span className="text-ink-3">{`${window.location.protocol}//`}</span>
            {window.location.host}
          </div>
        </div>
        <span aria-hidden="true" className="mx-0.5 hidden w-px self-stretch bg-hairline sm:block" />
        <div className="min-w-0">
          <div className="flex items-center gap-[5px] text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3">
            <Link2 className="size-[13px] shrink-0" />
            {t('apiKeys.connCodexBaseUrlLabel')}
          </div>
          <div className="mt-1 truncate font-mono text-[14px] font-medium tracking-[-.01em]">
            <span className="text-ink-3">{`${window.location.protocol}//`}</span>
            {window.location.host}
            <span className="text-ink-3">/v1</span>
          </div>
        </div>
        <span aria-hidden="true" className="mx-0.5 hidden w-px self-stretch bg-hairline sm:block" />
        <div className="min-w-0">
          <div className="flex items-center gap-[5px] text-[10.5px] font-semibold uppercase tracking-[.07em] text-ink-3">
            <Box className="size-[13px] shrink-0" />
            {t('apiKeys.connProtocolLabel')}
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-1.5">
            {['Anthropic Messages', 'Claude Code /cc/v1', 'Codex Responses', 'OpenAI Chat'].map((protocol) => (
              <span
                key={protocol}
                className="whitespace-nowrap rounded-[5px] border border-hairline bg-surface-3 px-1.5 py-px text-[10.5px] font-semibold tracking-[.03em] text-ink-2"
              >
                {protocol}
              </span>
            ))}
          </div>
        </div>
        <div className="ml-auto flex shrink-0 flex-wrap items-center gap-2">
          {/* 复制成功时图标转 ok 色：基类 [&_svg]:text-ink-3 特异性更高，必须同修饰符组覆盖 */}
          <Button
            variant="outline"
            className={copiedType === 'cc' ? '[&_svg]:text-ok hover:[&_svg]:text-ok' : ''}
            onClick={() => copyToClipboard(window.location.origin, 'cc')}
          >
            {copiedType === 'cc' ? <Check /> : <Copy />}
            {t('apiKeys.connCopyCcUrl')}
          </Button>
          <Button
            variant="outline"
            className={copiedType === 'codex' ? '[&_svg]:text-ok hover:[&_svg]:text-ok' : ''}
            onClick={() => copyToClipboard(`${window.location.origin}/v1`, 'codex')}
          >
            {copiedType === 'codex' ? <Check /> : <Copy />}
            {t('apiKeys.connCopyCodexUrl')}
          </Button>
        </div>
      </section>

      {/* 指标条（设计稿 .metrics） */}
      <div className="mt-[15px]">
        <MetricsBar>
          <Metric label={t('apiKeys.metricKeysLabel')}>
            <MetricValue value={String(statusCounts.all)} unit={t('apiKeys.metricKeysUnit')} />
            <MetricFoot>
              <span>
                <b className="font-medium text-ink-2">{statusCounts.active}</b> {t('apiKeys.statusActive')}
              </span>
              {statusCounts.pending > 0 && (
                <>
                  <FootSep />
                  <span>
                    <b className="font-medium text-ink-2">{statusCounts.pending}</b> {t('apiKeys.statusPending')}
                  </span>
                </>
              )}
              <FootSep />
              <span>
                <b className="font-medium text-ink-2">{statusCounts.disabled}</b> {t('apiKeys.statusDisabled')}
              </span>
              {statusCounts.expired > 0 && (
                <>
                  <FootSep />
                  <span>
                    <b className="font-medium text-ink-2">{statusCounts.expired}</b> {t('apiKeys.statusExpired')}
                  </span>
                </>
              )}
              {expiringSoonCount > 0 && (
                <>
                  <FootSep />
                  <span className="font-semibold text-warn">
                    {t('apiKeys.metricExpiringSoon', { count: expiringSoonCount })}
                  </span>
                </>
              )}
            </MetricFoot>
          </Metric>

          <Metric label={t('apiKeys.metricTodayLabel')}>
            <MetricValue
              value={todayRequests === null ? '—' : todayRequests.toLocaleString(localeTag())}
              trailing={requestsDeltaPercent === null ? undefined : <Delta percent={requestsDeltaPercent} />}
            />
            <MetricFoot className="truncate pr-[92px]">
              <span>
                {t('apiKeys.metricTodayCost')}{' '}
                <b className="font-medium text-ink-2">${(todayStats?.totalCost ?? 0).toFixed(2)}</b>
              </span>
              <FootSep />
              <span>
                {t('apiKeys.metricTodayCredits')}{' '}
                <b className="font-medium text-ink-2">{(todayStats?.totalCredits ?? 0).toFixed(1)}</b>
              </span>
            </MetricFoot>
            {requestTrend.length >= 2 && (
              <MetricAside>
                <Sparkline values={requestTrend} />
              </MetricAside>
            )}
          </Metric>

          <Metric label={t('apiKeys.metricTotalLabel')}>
            <MetricValue value={cumulative.requests.toLocaleString(localeTag())} />
            <MetricFoot>
              <span>
                Token <b className="font-medium text-ink-2">{formatTokenCount(cumulative.tokens)}</b>
              </span>
              {sinceLabel && (
                <>
                  <FootSep />
                  <span>{t('apiKeys.metricSince', { date: sinceLabel })}</span>
                </>
              )}
            </MetricFoot>
          </Metric>

          <Metric label={t('apiKeys.metricQuotaLabel')}>
            <MetricValue
              value={topQuota === null ? '—' : String(Math.round(topQuota.percent))}
              unit={topQuota === null ? undefined : '%'}
            />
            {topQuota === null ? (
              <div className="mt-[3px] text-[11px] text-ink-3">{t('apiKeys.metricQuotaEmpty')}</div>
            ) : (
              <div className="mt-[3px] truncate pr-14 text-[11px] text-ink-3">
                Key <b className="font-medium text-ink-2">{topQuota.name}</b>
                {' · '}
                <span className={`font-semibold ${quotaTone(topQuota.percent).text}`}>
                  {topQuota.unit === 'credits'
                    ? t('apiKeys.metricQuotaUsedCredits', { used: topQuota.used.toFixed(1), limit: topQuota.limit })
                    : t('apiKeys.metricQuotaUsedUsd', { used: topQuota.used.toFixed(2), limit: topQuota.limit })}
                </span>
              </div>
            )}
            <MetricAside>
              <Ring
                percent={topQuota === null ? null : topQuota.percent}
                tone={topQuota === null ? undefined : quotaTone(topQuota.percent).stroke}
              />
            </MetricAside>
          </Metric>
        </MetricsBar>
      </div>

      {/* 操作条（设计稿 .actionbar）：危险操作用竖分隔线隔离并染红；「导出配置」依赖未实现的后端能力，不渲染 */}
      <div className="flex flex-wrap items-center gap-[7px] pt-[15px]">
        <Button variant="outline" onClick={handleRefresh}>
          <RotateCw />
          {t('apiKeys.refreshList')}
        </Button>
        <Button variant="outline" aria-pressed={revealAll} onClick={() => setRevealAll((v) => !v)}>
          {revealAll ? <EyeOff /> : <Eye />}
          {revealAll ? t('apiKeys.hideKeys') : t('apiKeys.revealKeys')}
        </Button>
        {/* 分隔线随危险按钮一起显隐，无无效 Key 时不留孤立竖线 */}
        {invalidKeys.length > 0 && (
          <>
            <span aria-hidden="true" className="mx-0.5 h-[19px] w-px shrink-0 bg-hairline-2" />
            <Button variant="destructive" onClick={() => setPurgeDialogOpen(true)}>
              <Eraser />
              {t('apiKeys.purgeButton', { count: invalidKeys.length })}
            </Button>
          </>
        )}
        <div className="ml-auto flex items-center gap-2">
          <Segmented value={sortBy} onChange={setSortBy} groupLabel={t('apiKeys.sortGroupLabel')} options={sortSegments} />
          <Button onClick={() => { setNewName(generateUniqueSerial()); setCreateDialogOpen(true) }}>
            <Plus />
            {t('apiKeys.createButton')}
          </Button>
        </div>
      </div>

      {/* 工具条（设计稿 .toolbar） */}
      <Toolbar>
        <SearchBox
          value={searchQuery}
          onChange={setSearchQuery}
          placeholder={t('apiKeys.searchPlaceholder')}
          clearLabel={t('credentials.searchClear')}
        />
        <Segmented
          value={statusFilter}
          onChange={setStatusFilter}
          groupLabel={t('credentials.filterGroupLabel')}
          options={statusSegments}
        />
        <UpdatedAgo dataUpdatedAt={dataUpdatedAt} />
      </Toolbar>

      {/* 表格（设计稿 #p-keys .panel）：10 列 + sticky 表头 + 面板脚分页 */}
      <ApiKeyTable
        rowCount={paged.length}
        emptyLoading={isLoading}
        emptyText={
          isLoading ? t('common.loading') : allKeys.length === 0 ? t('apiKeys.emptyNoKeys') : t('apiKeys.emptyNoMatch')
        }
        allSelected={allPagedSelected}
        someSelected={somePagedSelected}
        onToggleSelectAll={toggleSelectPage}
        isFiltered={isFiltered}
        onClearFilters={() => {
          setSearchQuery('')
          setStatusFilter('all')
        }}
        footer={
          <ApiKeyPanelFoot
            selectedCount={selectedExistingIds.length}
            selectedEnabledCount={selectedEnabledCount}
            busy={batching}
            onBatchDisable={handleBatchDisable}
            onBatchResetUsage={handleBatchResetUsage}
            onDeselectAll={() => setSelectedIds(new Set())}
            totalCount={sorted.length}
            isFiltered={isFiltered}
            page={page}
            totalPages={totalPages}
            itemsPerPage={ITEMS_PER_PAGE}
            onPageChange={setCurrentPage}
          />
        }
      >
        {paged.map((apiKey) => (
          <ApiKeyRow
            key={apiKey.id}
            apiKey={apiKey}
            status={statusOf.get(apiKey.id) ?? 'active'}
            statusTitle={statusTitleOf(apiKey)}
            expiringInDays={expiringInDaysOf(apiKey)}
            usage={usageMap.get(apiKey.id)}
            rpm={rpmData?.byApiKey?.[String(apiKey.id)] ?? 0}
            bound={boundOf(apiKey)}
            selected={selectedIds.has(apiKey.id)}
            onToggleSelect={() => toggleSelect(apiKey.id)}
            revealed={revealAll}
            copied={copiedId === apiKey.id}
            createdTitle={t('apiKeys.createdLabel', { date: formatDate(apiKey.createdAt) })}
            onCopy={() =>
              copyToClipboard(
                t('apiKeys.copyContent', {
                  name: apiKey.name,
                  ccUrl: window.location.origin,
                  codexUrl: `${window.location.origin}/v1`,
                  key: apiKey.key,
                }),
                apiKey.id
              )
            }
            onImportClaude={() => handleImportToCcSwitch('claude', apiKey)}
            onImportCodex={() => handleImportToCcSwitch('codex', apiKey)}
            onViewDetail={() => onViewDetail(apiKey)}
            onEdit={() => openEdit(apiKey)}
            onDelete={() => handleDelete(apiKey)}
            onToggleEnabled={() => handleToggleEnabled(apiKey)}
            onResetUsage={() => handleResetUsage(apiKey)}
          />
        ))}
      </ApiKeyTable>
      {/* 创建对话框 */}
      <Dialog open={createDialogOpen} onOpenChange={setCreateDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('apiKeys.createDialogTitle')}</DialogTitle>
            <DialogDescription>{t('apiKeys.createDialogDesc')}</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <label className="text-sm font-medium">{t('apiKeys.serialLabel')}</label>
              <Input
                placeholder={t('apiKeys.serialPlaceholder')}
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
              />
              {nameConflict && (
                <p className="text-xs text-destructive mt-1">{t('apiKeys.serialConflict')}</p>
              )}
            </div>
            <div>
              <label className="text-sm font-medium">{t('apiKeys.limitModeLabel')}</label>
              <div className="flex gap-2 mt-2">
                <Button
                  type="button"
                  size="sm"
                  variant={newMode === 'date' ? 'default' : 'outline'}
                  onClick={() => setNewMode('date')}
                >
                  <Clock className="h-3.5 w-3.5 mr-1.5" />
                  {t('apiKeys.byDateButton')}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant={newMode === 'quota' ? 'default' : 'outline'}
                  onClick={() => setNewMode('quota')}
                >
                  <DollarSign className="h-3.5 w-3.5 mr-1.5" />
                  {t('apiKeys.byQuotaButton')}
                </Button>
              </div>
            </div>
            {newMode === 'date' ? (
              <div>
                <label className="text-sm font-medium">{t('apiKeys.validityLabel')}</label>
                <div className="flex flex-wrap gap-2 mt-2">
                  {quickDurationOptions.map((opt) => (
                    <Button
                      key={`${opt.value}-${opt.unit}`}
                      type="button"
                      size="sm"
                      variant={newDuration === opt.value && newDurationUnit === opt.unit ? 'default' : 'outline'}
                      onClick={() => { setNewDuration(opt.value); setNewDurationUnit(opt.unit) }}
                    >
                      {opt.value} {unitLabel(opt.unit)}
                    </Button>
                  ))}
                  <Button
                    type="button"
                    size="sm"
                    variant={newDuration === null ? 'default' : 'outline'}
                    onClick={() => setNewDuration(null)}
                  >
                    {t('apiKeys.neverExpires')}
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
                      <Button type="button" size="sm" variant={newDurationUnit === 'hours' ? 'default' : 'outline'} onClick={() => setNewDurationUnit('hours')}>{t('apiKeys.hoursUnit')}</Button>
                      <Button type="button" size="sm" variant={newDurationUnit === 'days' ? 'default' : 'outline'} onClick={() => setNewDurationUnit('days')}>{t('apiKeys.daysUnit')}</Button>
                    </div>
                  </div>
                )}
                <div className="text-xs text-muted-foreground mt-2">
                  <Clock className="h-3 w-3 inline mr-1" />
                  {newDuration !== null ? t('apiKeys.activatesAfterFirstUse', { value: newDuration, unit: unitLabel(newDurationUnit) }) : t('apiKeys.neverExpires')}
                </div>
              </div>
            ) : (
              <div>
                <label className="text-sm font-medium">{t('apiKeys.meteringUnitLabel')}</label>
                <div className="flex gap-2 mt-2">
                  <Button type="button" size="sm" variant={newLimitUnit === 'usd' ? 'default' : 'outline'} onClick={() => setNewLimitUnit('usd')}>{t('apiKeys.usdEstimate')}</Button>
                  <Button type="button" size="sm" variant={newLimitUnit === 'credits' ? 'default' : 'outline'} onClick={() => setNewLimitUnit('credits')}>{t('apiKeys.realCredits')}</Button>
                </div>
                <label className="text-sm font-medium mt-3 block">
                  {t('apiKeys.quotaLimitLabel', { unit: newLimitUnit === 'credits' ? 'credits' : t('apiKeys.unitUsd') })}
                </label>
                <div className="flex flex-wrap gap-2 mt-2">
                  <Button
                    type="button"
                    size="sm"
                    variant={newUnlimited ? 'default' : 'outline'}
                    onClick={() => setNewUnlimited(true)}
                  >
                    {t('apiKeys.unlimitedQuotaButton')}
                  </Button>
                  {(newLimitUnit === 'credits' ? [1000, 5000, 10000] : [100, 500, 1000]).map((amount) => (
                    <Button
                      key={amount}
                      type="button"
                      size="sm"
                      variant={!newUnlimited && newSpendingLimit === amount ? 'default' : 'outline'}
                      onClick={() => { setNewUnlimited(false); setNewSpendingLimit(amount) }}
                    >
                      {newLimitUnit === 'credits' ? amount : `$${amount}`}
                    </Button>
                  ))}
                </div>
                {!newUnlimited && (
                  <div className="flex items-center gap-2 mt-2">
                    <span className="text-sm text-muted-foreground">
                      {newLimitUnit === 'credits' ? t('apiKeys.customCredits') : t('apiKeys.customUsd')}
                    </span>
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
                )}
                <div className="text-xs text-muted-foreground mt-2">
                  <DollarSign className="h-3 w-3 inline mr-1" />
                  {newUnlimited
                    ? t('apiKeys.unlimitedQuotaHint')
                    : t('apiKeys.quotaAutoStopHint', { amount: newLimitUnit === 'credits' ? `${newSpendingLimit} credits` : `$${newSpendingLimit}` })}
                </div>
              </div>
            )}
            {credentials && credentials.credentials && credentials.credentials.length > 0 && (
              <div>
                <label className="text-sm font-medium">{t('apiKeys.boundAccountsLabel')}</label>
                <p className="text-xs text-muted-foreground mt-0.5">{t('apiKeys.bindAccountsHint')}</p>
                <CredentialMultiSelect
                  credentials={credentials.credentials}
                  balanceMap={credentialBalanceMap}
                  selected={newBoundCredentialIds}
                  onChange={setNewBoundCredentialIds}
                  dropdownRef={createCredDropdownRef}
                  open={createCredDropdownOpen}
                  onOpenChange={setCreateCredDropdownOpen}
                  searchQuery={credSearchQuery}
                  onSearchChange={setCredSearchQuery}
                />
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateDialogOpen(false)}>{t('common.cancel')}</Button>
            <Button onClick={handleCreate} disabled={!newName.trim() || nameConflict || isCreating}>
              {isCreating ? t('apiKeys.creatingButton') : t('apiKeys.createConfirmButton')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 编辑对话框 */}
      <Dialog open={!!editingKey} onOpenChange={(open) => !open && setEditingKey(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('apiKeys.editDialogTitle')}</DialogTitle>
            <DialogDescription>{t('apiKeys.editDialogDesc')}</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div>
              <label className="text-sm font-medium">{t('apiKeys.remarkNameLabel')}</label>
              <Input
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
              />
            </div>
            <div>
              <label className="text-sm font-medium">{t('apiKeys.limitModeLabel')}</label>
              <div className="flex gap-2 mt-2">
                <Button
                  type="button"
                  size="sm"
                  variant={editMode === 'date' ? 'default' : 'outline'}
                  onClick={() => setEditMode('date')}
                >
                  <Clock className="h-3.5 w-3.5 mr-1.5" />
                  {t('apiKeys.byDateButton')}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant={editMode === 'quota' ? 'default' : 'outline'}
                  onClick={() => setEditMode('quota')}
                >
                  <DollarSign className="h-3.5 w-3.5 mr-1.5" />
                  {t('apiKeys.byQuotaButton')}
                </Button>
              </div>
            </div>
            {editMode === 'date' ? (
              <div>
                <label className="text-sm font-medium">{t('apiKeys.renewDurationLabel')}</label>
                {editingKey?.activatedAt ? (
                  <div className="text-xs text-muted-foreground mt-1">
                    {t('apiKeys.activatedAtLabel', { date: formatDate(editingKey.activatedAt) })}
                    {editingKey.expiresAt && t('apiKeys.expiresSuffix', { date: formatDate(editingKey.expiresAt) })}
                  </div>
                ) : editingKey?.durationDays != null ? (
                  <div className="text-xs text-muted-foreground mt-1">
                    {t('apiKeys.pendingWithDuration', { duration: formatDuration(editingKey.durationDays) })}
                  </div>
                ) : editingKey?.expiresAt && new Date(editingKey.expiresAt) > new Date() ? (
                  <div className="text-xs text-muted-foreground mt-1">
                    {t('apiKeys.currentExpiryLabel', { date: new Date(editingKey.expiresAt).toLocaleString(localeTag(), { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) })}
                  </div>
                ) : null}
                <div className="flex flex-wrap gap-2 mt-2">
                  {quickDurationOptions.map((opt) => (
                    <Button
                      key={`${opt.value}-${opt.unit}`}
                      type="button"
                      size="sm"
                      variant={editDuration === opt.value && editDurationUnit === opt.unit ? 'default' : 'outline'}
                      onClick={() => { setEditDuration(opt.value); setEditDurationUnit(opt.unit) }}
                    >
                      {opt.value} {unitLabel(opt.unit)}
                    </Button>
                  ))}
                  <Button
                    type="button"
                    size="sm"
                    variant={editDuration === null ? 'default' : 'outline'}
                    onClick={() => setEditDuration(null)}
                  >
                    {t('apiKeys.neverExpires')}
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
                      <Button type="button" size="sm" variant={editDurationUnit === 'hours' ? 'default' : 'outline'} onClick={() => setEditDurationUnit('hours')}>{t('apiKeys.hoursUnit')}</Button>
                      <Button type="button" size="sm" variant={editDurationUnit === 'days' ? 'default' : 'outline'} onClick={() => setEditDurationUnit('days')}>{t('apiKeys.daysUnit')}</Button>
                    </div>
                  </div>
                )}
                <div className="text-xs text-muted-foreground mt-2">
                  <Clock className="h-3 w-3 inline mr-1" />
                  {editDuration !== null && editDuration !== ''
                    ? (editingKey && getKeyStatus(editingKey) === 'active'
                        ? t('apiKeys.renewOnCurrentExpiry', { value: editDuration, unit: unitLabel(editDurationUnit) })
                        : t('apiKeys.activatesAfterFirstUse', { value: editDuration, unit: unitLabel(editDurationUnit) }))
                    : t('apiKeys.neverExpires')}
                </div>
              </div>
            ) : (
              <div>
                <label className="text-sm font-medium">{t('apiKeys.meteringUnitLabel')}</label>
                <div className="flex gap-2 mt-2">
                  <Button type="button" size="sm" variant={editLimitUnit === 'usd' ? 'default' : 'outline'} onClick={() => setEditLimitUnit('usd')}>{t('apiKeys.usdEstimate')}</Button>
                  <Button type="button" size="sm" variant={editLimitUnit === 'credits' ? 'default' : 'outline'} onClick={() => setEditLimitUnit('credits')}>{t('apiKeys.realCredits')}</Button>
                </div>
                <label className="text-sm font-medium mt-3 block">
                  {t('apiKeys.quotaLimitLabel', { unit: editLimitUnit === 'credits' ? 'credits' : t('apiKeys.unitUsd') })}
                </label>
                <div className="flex items-center gap-2 mt-2">
                  <span className="text-sm text-muted-foreground">{editLimitUnit === 'credits' ? '' : '$'}</span>
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
                  {t('apiKeys.quotaAutoStopHint', { amount: editLimitUnit === 'credits' ? `${editSpendingLimit} credits` : `$${editSpendingLimit}` })}
                </div>
              </div>
            )}
            {credentials && credentials.credentials && credentials.credentials.length > 0 && (
              <div>
                <label className="text-sm font-medium">{t('apiKeys.boundAccountsLabel')}</label>
                <p className="text-xs text-muted-foreground mt-0.5">{t('apiKeys.bindAccountsHint')}</p>
                <CredentialMultiSelect
                  credentials={credentials.credentials}
                  balanceMap={credentialBalanceMap}
                  selected={editBoundCredentialIds}
                  onChange={setEditBoundCredentialIds}
                  dropdownRef={editCredDropdownRef}
                  open={editCredDropdownOpen}
                  onOpenChange={setEditCredDropdownOpen}
                  searchQuery={credSearchQuery}
                  onSearchChange={setCredSearchQuery}
                />
              </div>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditingKey(null)}>{t('common.cancel')}</Button>
            <Button onClick={handleUpdate}>{t('common.save')}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 清除无效 Key 对话框 */}
      <Dialog open={purgeDialogOpen} onOpenChange={(open) => !purging && setPurgeDialogOpen(open)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('apiKeys.purgeDialogTitle')}</DialogTitle>
            <DialogDescription>
              {t('apiKeys.purgeDialogDesc', { count: invalidKeys.length })}
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
                  {getKeyStatus(k) === 'disabled' ? t('apiKeys.statusDisabled') : t('apiKeys.statusExpired')}
                </Badge>
              </div>
            ))}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPurgeDialogOpen(false)} disabled={purging}>{t('common.cancel')}</Button>
            <Button variant="destructive" onClick={handlePurge} disabled={purging}>
              {purging ? <><Loader2 className="h-4 w-4 mr-2 animate-spin" />{t('apiKeys.purgingButton')}</> : t('apiKeys.confirmPurgeButton', { count: invalidKeys.length })}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

interface CredentialMultiSelectProps {
  credentials: import('@/types/api').CredentialStatusItem[]
  balanceMap: Map<number, import('@/types/api').BalanceResponse>
  selected: number[]
  onChange: (ids: number[]) => void
  dropdownRef: React.RefObject<HTMLDivElement>
  open: boolean
  onOpenChange: (open: boolean) => void
  searchQuery: string
  onSearchChange: (q: string) => void
}

function CredentialMultiSelect({
  credentials,
  balanceMap,
  selected,
  onChange,
  dropdownRef,
  open,
  onOpenChange,
  searchQuery,
  onSearchChange,
}: CredentialMultiSelectProps) {
  const { t } = useTranslation()
  const filtered = credentials.filter((c) => {
    if (!searchQuery.trim()) return true
    const q = searchQuery.trim().toLowerCase()
    return (
      String(c.id).includes(q) ||
      (c.email ?? '').toLowerCase().includes(q)
    )
  })

  const toggle = (id: number) => {
    onChange(selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id])
  }

  return (
    <div className="relative mt-2" ref={dropdownRef}>
      {/* 触发器 */}
      <button
        type="button"
        onClick={() => { onOpenChange(!open); onSearchChange('') }}
        className="w-full flex items-center justify-between gap-2 rounded-md border border-input bg-background px-3 py-2 text-sm shadow-sm hover:bg-accent/50 transition-colors"
      >
        <div className="flex flex-wrap gap-1 flex-1 min-w-0">
          {selected.length === 0 ? (
            <span className="text-muted-foreground">{t('apiKeys.globalPolicyNoBind')}</span>
          ) : (
            selected.map((id) => {
              const cred = credentials.find((c) => c.id === id)
              return (
                <span
                  key={id}
                  className="inline-flex items-center gap-1 rounded-full bg-violet-100 dark:bg-violet-900/50 text-violet-700 dark:text-violet-300 border border-violet-200 dark:border-violet-700 px-2 py-0.5 text-xs font-medium"
                >
                  {cred?.email ?? `#${id}`}
                  <span
                    role="button"
                    tabIndex={0}
                    className="hover:text-destructive cursor-pointer"
                    onClick={(e) => { e.stopPropagation(); toggle(id) }}
                    onKeyDown={(e) => e.key === 'Enter' && toggle(id)}
                  >
                    <X className="h-3 w-3" />
                  </span>
                </span>
              )
            })
          )}
        </div>
        <ChevronDown className={`h-4 w-4 shrink-0 text-muted-foreground transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>

      {/* 下拉面板 */}
      {open && (
        <div className="absolute z-50 mt-1 w-full rounded-md border bg-popover shadow-md">
          <div className="p-2 border-b">
            <div className="relative">
              <Search className="absolute left-2 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
              <input
                autoFocus
                type="text"
                placeholder={t('apiKeys.searchCredentialPlaceholder')}
                value={searchQuery}
                onChange={(e) => onSearchChange(e.target.value)}
                className="w-full rounded-sm border-0 bg-transparent pl-7 pr-2 py-1 text-sm outline-none placeholder:text-muted-foreground"
              />
            </div>
          </div>
          <div className="max-h-48 overflow-y-auto py-1">
            {filtered.length === 0 ? (
              <div className="px-3 py-2 text-sm text-muted-foreground">{t('apiKeys.noMatchingAccounts')}</div>
            ) : (
              filtered.map((cred) => {
                const bal = balanceMap.get(cred.id)
                const isSelected = selected.includes(cred.id)
                return (
                  <button
                    key={cred.id}
                    type="button"
                    onClick={() => toggle(cred.id)}
                    className={`w-full flex items-start gap-2 px-3 py-2 text-sm hover:bg-accent transition-colors text-left ${isSelected ? 'bg-violet-50 dark:bg-violet-950/30' : ''}`}
                  >
                    <div className={`mt-0.5 h-4 w-4 shrink-0 rounded border flex items-center justify-center ${isSelected ? 'bg-violet-600 border-violet-600 text-white' : 'border-input'}`}>
                      {isSelected && <Check className="h-3 w-3" />}
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-1.5 flex-wrap">
                        <span className="font-medium">{cred.email ?? t('credentials.accountFallbackName', { id: cred.id })}</span>
                        <span className="text-xs text-muted-foreground">#{cred.id}</span>
                        {cred.disabled && <span className="text-xs text-destructive">{t('apiKeys.statusDisabled')}</span>}
                      </div>
                      {bal ? (
                        <div className="text-xs text-muted-foreground mt-0.5">
                          {t('apiKeys.remainingUsageLabel', { remaining: bal.remaining.toFixed(2), limit: bal.usageLimit.toFixed(2) })}
                          <span className="ml-1">{t('apiKeys.remainingPercentSuffix', { percent: (100 - bal.usagePercentage).toFixed(1) })}</span>
                        </div>
                      ) : (
                        <div className="text-xs text-muted-foreground mt-0.5">{t('apiKeys.balanceNotLoaded')}</div>
                      )}
                    </div>
                  </button>
                )
              })
            )}
          </div>
        </div>
      )}
    </div>
  )
}
