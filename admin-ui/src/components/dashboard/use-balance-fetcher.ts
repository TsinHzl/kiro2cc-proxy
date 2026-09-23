// Copyright (c) 2026 Harllan He. Licensed under MIT.
// Dashboard 余额拉取逻辑（自 dashboard.tsx 拆出，纯代码搬移）
import { useState, useEffect, useRef } from 'react'
import type { QueryClient } from '@tanstack/react-query'
import { getCredentialBalance } from '@/api/credentials'
import type { BalanceResponse, CredentialStatusItem } from '@/types/api'

type DashboardTab = 'credentials' | 'apikeys' | 'settings' | 'logs' | 'models' | 'changelog'

interface UseBalanceFetcherParams {
  credentials: CredentialStatusItem[] | undefined
  refetch: () => unknown
  queryClient: QueryClient
  detailCredentialId: number | null
  dailyView: string | null
  activeTab: DashboardTab
}

export function useBalanceFetcher({
  credentials,
  refetch,
  queryClient,
  detailCredentialId,
  dailyView,
  activeTab,
}: UseBalanceFetcherParams) {
  const [balanceMap, setBalanceMap] = useState<Map<number, BalanceResponse>>(new Map())
  const [loadingBalanceIds, setLoadingBalanceIds] = useState<Set<number>>(new Set())
  const [liveCreditsTotal, setLiveCreditsTotal] = useState<number | null>(null)
  const [liveCreditsQueried, setLiveCreditsQueried] = useState(0)
  const prevTabRef = useRef<DashboardTab | null>(null)
  const prevDetailCredentialId = useRef<number | null>(null)
  const prevDailyView = useRef<string | null>(null)
  const initialBalanceFetchDone = useRef(false)
  const isFetchingBalances = useRef(false)
  const prevEnabledIdsRef = useRef<Set<number> | null>(null)
  const credentialsRef = useRef(credentials)

  // 只保留当前仍存在的凭据缓存，避免删除后残留旧数据
  useEffect(() => {
    if (!credentials) {
      setBalanceMap(new Map())
      setLoadingBalanceIds(new Set())
      return
    }

    const validIds = new Set(credentials.map(credential => credential.id))

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
  }, [credentials])

  // 始终保持 ref 与最新 credentials 同步
  useEffect(() => {
    credentialsRef.current = credentials
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
    if (!credentials || initialBalanceFetchDone.current) return
    initialBalanceFetchDone.current = true
    const ids = credentials.filter(c => !c.disabled).map(c => c.id)
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
  }, [credentials]) // eslint-disable-line react-hooks/exhaustive-deps

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
    if (!credentials || !initialBalanceFetchDone.current || isFetchingBalances.current) return

    const currentEnabledIds = new Set(
      credentials.filter(c => !c.disabled).map(c => c.id)
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
  }, [credentials]) // eslint-disable-line react-hooks/exhaustive-deps

  // balanceMap 变化后（添加/删除/清理）重新计算全局积分
  useEffect(() => {
    if (!initialBalanceFetchDone.current || isFetchingBalances.current) return

    let total = 0
    balanceMap.forEach(b => { total += b.remaining })
    setLiveCreditsTotal(balanceMap.size > 0 ? total : null)
    setLiveCreditsQueried(balanceMap.size)
  }, [balanceMap]) // eslint-disable-line react-hooks/exhaustive-deps

  return {
    balanceMap,
    setBalanceMap,
    loadingBalanceIds,
    setLoadingBalanceIds,
    liveCreditsTotal,
    setLiveCreditsTotal,
    liveCreditsQueried,
    setLiveCreditsQueried,
    isFetchingBalances,
    prevEnabledIdsRef,
  }
}
