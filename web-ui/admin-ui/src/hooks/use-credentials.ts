// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useQuery, useQueries, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  getCredentials,
  setCredentialDisabled,
  setCredentialPriority,
  resetCredentialFailure,
  getCredentialBalance,
  addCredential,
  deleteCredential,
  updateCredential,
  getLoadBalancingMode,
  getSuggestionMode,
  setSuggestionMode,
  getClientTokenPassthrough,
  setClientTokenPassthrough,
  getThinkingAsText,
  setThinkingAsText,
  getShowSessionCredits,
  setShowSessionCredits,
  getDumpKiroBody,
  setDumpKiroBody,
  getDumpFiles,
  getDebugLogs,
  setDebugLogs,
  getRuntimeConfig,
  setRuntimeConfig,
  setLoadBalancingMode,
  getServerInfo,
  getApiKeys,
  getChatSessions,
  deleteChatSession,
  clearChatSessions,
  createApiKey,
  updateApiKey,
  deleteApiKey,
  getAllUsage,
  resetKeyUsage,
  getRpm,
  getAuthKeys,
  setAuthKeys,
  getKeyUsageRecords,
  getCredentialUsageRecords,
  getCredentialUsageSummary,
  getCredentialTodaySummary,
  getCredentialRangeSummary,
  getDailyUsage,
  getDailyUsageRecords,
  getUsageRecordsByRange,
  getThrottleLogs,
  getFailureLogs,
  getModels,
  getCredentialModels,
  getChangelog,
} from '@/api/credentials'
import type { AddCredentialRequest, UpdateCredentialRequest, CreateApiKeyRequest, UpdateApiKeyRequest } from '@/types/api'

// 查询凭据列表
export function useCredentials() {
  return useQuery({
    queryKey: ['credentials'],
    queryFn: getCredentials,
  })
}

// 查询凭据余额
export function useCredentialBalance(id: number | null) {
  return useQuery({
    queryKey: ['credential-balance', id],
    queryFn: () => getCredentialBalance(id!),
    enabled: id !== null,
    retry: false, // 余额查询失败时不重试（避免重复请求被封禁的账号）
  })
}

// 批量查询多个凭据余额
export function useCredentialBalances(ids: number[]) {
  const results = useQueries({
    queries: ids.map((id) => ({
      queryKey: ['credential-balance', id],
      queryFn: () => getCredentialBalance(id),
      retry: false,
    })),
  })
  const balanceMap = new Map<number, import('@/types/api').BalanceResponse>()
  ids.forEach((id, i) => {
    const data = results[i]?.data
    if (data) balanceMap.set(id, data)
  })
  return balanceMap
}

// 设置禁用状态
export function useSetDisabled() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, disabled }: { id: number; disabled: boolean }) =>
      setCredentialDisabled(id, disabled),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 设置优先级
export function useSetPriority() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, priority }: { id: number; priority: number }) =>
      setCredentialPriority(id, priority),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 重置失败计数
export function useResetFailure() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => resetCredentialFailure(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 添加新凭据
export function useAddCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: AddCredentialRequest) => addCredential(req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 删除凭据
export function useDeleteCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => deleteCredential(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 更新凭据
export function useUpdateCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: number; data: UpdateCredentialRequest }) =>
      updateCredential(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['credentials'] })
    },
  })
}

// 获取负载均衡模式
export function useLoadBalancingMode() {
  return useQuery({
    queryKey: ['loadBalancingMode'],
    queryFn: getLoadBalancingMode,
  })
}

// 设置负载均衡模式
export function useSetLoadBalancingMode() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setLoadBalancingMode,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['loadBalancingMode'] })
    },
  })
}

// 获取 Suggestion Mode 放行开关
export function useSuggestionMode() {
  return useQuery({
    queryKey: ['suggestionMode'],
    queryFn: getSuggestionMode,
  })
}

// 设置 Suggestion Mode 放行开关
export function useSetSuggestionMode() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setSuggestionMode,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['suggestionMode'] })
    },
  })
}

// ============ 运行时配置 Hooks ============

// 获取运行时配置（maxRpm / port / proxyUrl / writeChunkingMaxLines）
export function useRuntimeConfig() {
  return useQuery({
    queryKey: ['runtimeConfig'],
    queryFn: getRuntimeConfig,
  })
}

// 设置运行时配置（仅更新传入字段；maxRpm / writeChunkingMaxLines 热生效，port/proxyUrl 重启后生效）
export function useSetRuntimeConfig() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setRuntimeConfig,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['runtimeConfig'] })
    },
  })
}

// ============ API Key Hooks ============

// 获取客户端 token 直通开关
export function useClientTokenPassthrough() {
  return useQuery({
    queryKey: ['clientTokenPassthrough'],
    queryFn: getClientTokenPassthrough,
  })
}

// 设置客户端 token 直通开关
export function useSetClientTokenPassthrough() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setClientTokenPassthrough,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['clientTokenPassthrough'] })
    },
  })
}

// 获取思考文本化开关
export function useThinkingAsText() {
  return useQuery({
    queryKey: ['thinkingAsText'],
    queryFn: getThinkingAsText,
  })
}

// 设置思考文本化开关
export function useSetThinkingAsText() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setThinkingAsText,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['thinkingAsText'] })
    },
  })
}

// 获取会话 credits 汇总行开关
export function useShowSessionCredits() {
  return useQuery({
    queryKey: ['showSessionCredits'],
    queryFn: getShowSessionCredits,
  })
}

// 设置会话 credits 汇总行开关
export function useSetShowSessionCredits() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setShowSessionCredits,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['showSessionCredits'] })
    },
  })
}

// 获取 Kiro 请求体落盘取证开关
export function useDumpKiroBody() {
  return useQuery({
    queryKey: ['dumpKiroBody'],
    queryFn: getDumpKiroBody,
  })
}

// 设置 Kiro 请求体落盘取证开关
export function useSetDumpKiroBody() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setDumpKiroBody,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['dumpKiroBody'] })
    },
  })
}

// 查询 dump 落盘文件清单（`enabled` 由弹窗开关状态驱动，弹窗关闭时不请求）
export function useDumpFiles(enabled: boolean) {
  return useQuery({
    queryKey: ['dumpFiles'],
    queryFn: getDumpFiles,
    enabled,
  })
}

// 获取 DEBUG 日志开关
export function useDebugLogs() {
  return useQuery({
    queryKey: ['debugLogs'],
    queryFn: getDebugLogs,
  })
}

// 设置 DEBUG 日志开关
export function useSetDebugLogs() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: setDebugLogs,
    // 后端先热切换再持久化：持久化失败会返回 500 但运行时已变，所以成败都要重新取真实状态，
    // 并返回 Promise 让 isPending 覆盖刷新过程
    onSettled: () => queryClient.invalidateQueries({ queryKey: ['debugLogs'] }),
  })
}

// 获取服务器信息
export function useServerInfo() {
  return useQuery({
    queryKey: ['serverInfo'],
    queryFn: getServerInfo,
    // 侧栏状态点需反映后端存活：全局默认 refetchOnWindowFocus: false 且无轮询，
    // 不加此项则页面停留期间后端断开永无触发点，状态点不会转灰
    refetchInterval: 30_000,
  })
}

// 查询 API Key 列表
export function useApiKeys() {
  return useQuery({
    queryKey: ['apiKeys'],
    queryFn: getApiKeys,
  })
}

// 创建 API Key
export function useCreateApiKey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (req: CreateApiKeyRequest) => createApiKey(req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['apiKeys'] })
    },
  })
}

// 更新 API Key
export function useUpdateApiKey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, data }: { id: number; data: UpdateApiKeyRequest }) =>
      updateApiKey(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['apiKeys'] })
    },
  })
}

// 删除 API Key
export function useDeleteApiKey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => deleteApiKey(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['apiKeys'] })
    },
  })
}

// ============ API Key 用量 Hooks ============

// 查询所有 API Key 用量
export function useAllUsage() {
  return useQuery({
    queryKey: ['apiKeyUsage'],
    queryFn: getAllUsage,
    refetchInterval: 60000, // 每 60 秒刷新
  })
}

// 重置 API Key 用量
export function useResetKeyUsage() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => resetKeyUsage(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['apiKeyUsage'] })
    },
  })
}

// 查询单个 API Key 的分页原始记录
export function useKeyUsageRecords(id: number, page: number, pageSize = 50) {
  return useQuery({
    queryKey: ['apiKeyUsageRecords', id, page, pageSize],
    queryFn: () => getKeyUsageRecords(id, page, pageSize),
    enabled: id > 0,
  })
}

// 查询单个凭据的分页原始记录
export function useCredentialUsageRecords(id: number, page: number, pageSize = 50) {
  return useQuery({
    queryKey: ['credentialUsageRecords', id, page, pageSize],
    queryFn: () => getCredentialUsageRecords(id, page, pageSize),
    enabled: id > 0,
  })
}

// 查询单账号全部留存记录的用量汇总（60s 自动刷新；enabled=false 时停止请求与轮询）
export function useCredentialUsageSummary(id: number, enabled = true) {
  return useQuery({
    queryKey: ['credentialUsageSummary', id],
    queryFn: () => getCredentialUsageSummary(id),
    enabled: enabled && id > 0,
    refetchInterval: 60000,
  })
}

// 查询单账号 CST 今日用量汇总（60s 自动刷新；enabled=false 时停止请求与轮询）
export function useCredentialTodaySummary(id: number, enabled = true) {
  return useQuery({
    queryKey: ['credentialTodaySummary', id],
    queryFn: () => getCredentialTodaySummary(id),
    enabled: enabled && id > 0,
    refetchInterval: 60000,
  })
}

// 查询单账号 CST 最近 7 天用量汇总（60s 自动刷新；enabled=false 时停止请求与轮询）
export function useCredential7DaysSummary(id: number, enabled = true) {
  return useQuery({
    queryKey: ['credential7DaysSummary', id],
    queryFn: () => getCredentialRangeSummary(id, '7days'),
    enabled: enabled && id > 0,
    refetchInterval: 60000,
  })
}

// 查询单账号 CST 最近 30 天用量汇总（60s 自动刷新；enabled=false 时停止请求与轮询）
export function useCredential30DaysSummary(id: number, enabled = true) {
  return useQuery({
    queryKey: ['credential30DaysSummary', id],
    queryFn: () => getCredentialRangeSummary(id, '30days'),
    enabled: enabled && id > 0,
    refetchInterval: 60000,
  })
}

// ============ RPM 监控 Hooks ============

// 查询实时 RPM 数据（每 5 秒刷新）
export function useRpm() {
  return useQuery({
    queryKey: ['rpm'],
    queryFn: getRpm,
    refetchInterval: 5000,
  })
}

// ============ 认证密钥 Hooks ============

export function useAuthKeys() {
  return useQuery({
    queryKey: ['auth-keys'],
    queryFn: getAuthKeys,
  })
}

export function useSetAuthKeys() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (payload: { adminPsw?: string }) => setAuthKeys(payload),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['auth-keys'] })
    },
  })
}

// ============ 支持的模型 Hooks ============

export function useModels() {
  return useQuery({
    queryKey: ['models'],
    queryFn: getModels,
  })
}

// 查询单账号支持的模型（失败不重试，避免对被封禁账号重复请求）
export function useCredentialModels(id: number | null) {
  return useQuery({
    queryKey: ['credential-models', id],
    queryFn: () => getCredentialModels(id!),
    enabled: id !== null,
    retry: false,
  })
}

// ============ 更新日志 Hooks ============

// 查询 GitHub 最新版本号（用于侧栏版本号旁的更新红点）
// 优先 /releases/latest；仓库未创建正式 Release 时（404）回退到 /tags 取最新 semver tag，
// 这样推 tag 即生效，不依赖手动建 Release。请求失败 / 无 tag 时返回 null，红点静默不展示
export function useLatestGitHubVersion() {
  return useQuery({
    queryKey: ['latestGitHubVersion'],
    queryFn: async (): Promise<string | null> => {
      const rel = await fetch('https://api.github.com/repos/TsinHzl/kiro2cc-proxy/releases/latest')
      if (rel.ok) {
        const data = (await rel.json()) as { tag_name?: string }
        const tag = data.tag_name?.trim().replace(/^v/i, '') ?? ''
        return /^\d+(\.\d+)*$/.test(tag) ? tag : null
      }
      // 仅在明确不存在正式 Release（404）时才回退 tags；其他错误状态不能证明无 Release，红点静默
      if (rel.status !== 404) return null
      const res = await fetch('https://api.github.com/repos/TsinHzl/kiro2cc-proxy/tags?per_page=100')
      if (!res.ok) return null
      const tags = (await res.json()) as { name?: string }[]
      const versions = tags
        .map(t => t.name?.trim().replace(/^v/i, '') ?? '')
        .filter(v => /^\d+(\.\d+)*$/.test(v))
      if (versions.length === 0) return null
      // tags 端点按创建时间倒序返回；per_page=100 取首页即可覆盖全部 tag，避免前 10 个非
      // semver tag（如 rc）挤占窗口导致漏报；仍做一次比较避免顺序意外
      return versions.reduce((a, b) => (isNewerVersion(b, a) ? b : a))
    },
    staleTime: 60 * 60 * 1000,
    retry: false,
  })
}

// 语义化版本比较：a 是否大于 b（纯数字逐段比较，缺段按 0 处理）
export function isNewerVersion(a: string, b: string): boolean {
  const pa = a.split('.')
  const pb = b.split('.')
  const len = Math.max(pa.length, pb.length)
  for (let i = 0; i < len; i++) {
    const na = Number(pa[i]) || 0
    const nb = Number(pb[i]) || 0
    if (na > nb) return true
    if (na < nb) return false
  }
  return false
}

export function useChangelog() {
  return useQuery({
    queryKey: ['changelog'],
    queryFn: getChangelog,
  })
}

// ============ 每日用量统计 Hooks ============

export function useDailyUsage() {
  return useQuery({
    queryKey: ['dailyUsage'],
    queryFn: getDailyUsage,
    refetchInterval: 60000,
  })
}

export function useDailyUsageRecords(date: string, page: number, pageSize = 50) {
  return useQuery({
    queryKey: ['dailyUsageRecords', date, page, pageSize],
    queryFn: () => getDailyUsageRecords(date, page, pageSize),
    enabled: !!date,
  })
}

export function useUsageRecordsByRange(
  from: string,
  to: string,
  page: number,
  pageSize = 50
) {
  return useQuery({
    queryKey: ['usageRecordsByRange', from, to, page, pageSize],
    queryFn: () => getUsageRecordsByRange(from, to, page, pageSize),
    enabled: !!from && !!to,
  })
}

// ============ 失败日志 Hooks ============

export function useFailureLogs(id: number, page: number, pageSize = 50) {
  return useQuery({
    queryKey: ['failureLogs', id, page, pageSize],
    queryFn: () => getFailureLogs(id, page, pageSize),
    enabled: id > 0,
  })
}

// ============ 限流日志 Hooks ============

export function useThrottleLogs(id: number, page: number, pageSize = 50) {
  return useQuery({
    queryKey: ['throttleLogs', id, page, pageSize],
    queryFn: () => getThrottleLogs(id, page, pageSize),
    enabled: id > 0,
  })
}

// ============ 对话会话 Hooks ============

export const CHAT_SESSIONS_KEY = ['chatSessions'] as const

export function useChatSessions() {
  return useQuery({
    queryKey: CHAT_SESSIONS_KEY,
    queryFn: getChatSessions,
  })
}

export function useDeleteChatSession() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => deleteChatSession(id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_KEY }),
  })
}

export function useClearChatSessions() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: () => clearChatSessions(),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: CHAT_SESSIONS_KEY }),
  })
}
