// Copyright (c) 2026 Harllan He. Licensed under MIT.
import axios from 'axios'
import { storage } from '@/lib/storage'
import type {
  CredentialsStatusResponse,
  BalanceResponse,
  SuccessResponse,
  SetDisabledRequest,
  SetPriorityRequest,
  AddCredentialRequest,
  AddCredentialResponse,
  UpdateCredentialRequest,
  ApiKeyItem,
  CreateApiKeyRequest,
  UpdateApiKeyRequest,
  UsageSummary,
  RpmSnapshot,
  UsageRecordsResponse,
  DailySummary,
  CredentialDaySummary,
  CredentialRangeSummary,
  CredentialUsageSummary,
  ThrottleLogsResponse,
  FailureLogsResponse,
  ModelsResponse,
  ChatSession,
  ChatSessionMeta,
  ReleaseNotesResponse,
  DumpFilesResponse,
} from '@/types/api'

// 创建 axios 实例
const api = axios.create({
  baseURL: '/api/admin',
  headers: {
    'Content-Type': 'application/json',
  },
})

// 请求拦截器添加 API Key
api.interceptors.request.use((config) => {
  const apiKey = storage.getApiKey()
  if (apiKey) {
    config.headers['x-api-key'] = apiKey
  }
  return config
})

// 获取所有凭据状态
export async function getCredentials(): Promise<CredentialsStatusResponse> {
  const { data } = await api.get<CredentialsStatusResponse>('/credentials')
  return data
}

// 设置凭据禁用状态
export async function setCredentialDisabled(
  id: number,
  disabled: boolean
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(
    `/credentials/${id}/disabled`,
    { disabled } as SetDisabledRequest
  )
  return data
}

// 设置凭据优先级
export async function setCredentialPriority(
  id: number,
  priority: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(
    `/credentials/${id}/priority`,
    { priority } as SetPriorityRequest
  )
  return data
}

// 重置失败计数
export async function resetCredentialFailure(
  id: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/reset`)
  return data
}

// 获取凭据余额
export async function getCredentialBalance(id: number): Promise<BalanceResponse> {
  const { data } = await api.get<BalanceResponse>(`/credentials/${id}/balance`)
  return data
}

// 添加新凭据
export async function addCredential(
  req: AddCredentialRequest
): Promise<AddCredentialResponse> {
  const { data } = await api.post<AddCredentialResponse>('/credentials', req)
  return data
}

// 删除凭据
export async function deleteCredential(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/credentials/${id}`)
  return data
}

// 更新凭据
export async function updateCredential(id: number, req: UpdateCredentialRequest): Promise<SuccessResponse> {
  const { data } = await api.put<SuccessResponse>(`/credentials/${id}`, req)
  return data
}

// 获取负载均衡模式
export async function getLoadBalancingMode(): Promise<{ mode: 'priority' | 'balanced' }> {
  const { data } = await api.get<{ mode: 'priority' | 'balanced' }>('/config/load-balancing')
  return data
}

// 设置负载均衡模式
export async function setLoadBalancingMode(mode: 'priority' | 'balanced'): Promise<{ mode: 'priority' | 'balanced' }> {
  const { data } = await api.put<{ mode: 'priority' | 'balanced' }>('/config/load-balancing', { mode })
  return data
}

// 获取 Suggestion Mode 放行开关
export async function getSuggestionMode(): Promise<{ enabled: boolean }> {
  const { data } = await api.get<{ enabled: boolean }>('/config/suggestion-mode')
  return data
}

// 设置 Suggestion Mode 放行开关
export async function setSuggestionMode(enabled: boolean): Promise<{ success: boolean; message: string }> {
  const { data } = await api.put<{ success: boolean; message: string }>('/config/suggestion-mode', { enabled })
  return data
}

// 获取运行时配置（maxRpm / port / proxyUrl / writeChunkingMaxLines）
export async function getRuntimeConfig(): Promise<{
  maxRpmPerCredential: number
  port: number
  proxyUrl: string | null
  writeChunkingMaxLines: number
}> {
  const { data } = await api.get<{
    maxRpmPerCredential: number
    port: number
    proxyUrl: string | null
    writeChunkingMaxLines: number
  }>('/config/runtime')
  return data
}

// 设置运行时配置（仅更新传入字段；maxRpm / writeChunkingMaxLines 热生效，port/proxyUrl 重启后生效）
export async function setRuntimeConfig(payload: {
  maxRpmPerCredential?: number
  writeChunkingMaxLines?: number
  port?: number
  proxyUrl?: string
}): Promise<{ success: boolean; message: string }> {
  const { data } = await api.put<{ success: boolean; message: string }>('/config/runtime', payload)
  return data
}

// 获取客户端 token 直通开关
export async function getClientTokenPassthrough(): Promise<{ enabled: boolean }> {
  const { data } = await api.get<{ enabled: boolean }>('/config/client-token-passthrough')
  return data
}

// 设置客户端 token 直通开关
export async function setClientTokenPassthrough(enabled: boolean): Promise<{ success: boolean; message: string }> {
  const { data } = await api.put<{ success: boolean; message: string }>('/config/client-token-passthrough', { enabled })
  return data
}

// 获取思考文本化开关
export async function getThinkingAsText(): Promise<{ enabled: boolean }> {
  const { data } = await api.get<{ enabled: boolean }>('/config/thinking-as-text')
  return data
}

// 设置思考文本化开关
export async function setThinkingAsText(enabled: boolean): Promise<{ success: boolean; message: string }> {
  const { data } = await api.put<{ success: boolean; message: string }>('/config/thinking-as-text', { enabled })
  return data
}

// 获取会话 credits 汇总行开关
export async function getShowSessionCredits(): Promise<{ enabled: boolean }> {
  const { data } = await api.get<{ enabled: boolean }>('/config/show-session-credits')
  return data
}

// 设置会话 credits 汇总行开关
export async function setShowSessionCredits(enabled: boolean): Promise<{ success: boolean; message: string }> {
  const { data } = await api.put<{ success: boolean; message: string }>('/config/show-session-credits', { enabled })
  return data
}

// 获取账号保活开关与间隔（小时）
export async function getCredentialKeepAlive(): Promise<{ enabled: boolean; hours: number }> {
  const { data } = await api.get<{ enabled: boolean; hours: number }>('/config/credential-keep-alive')
  return data
}

// 设置账号保活开关
export async function setCredentialKeepAlive(enabled: boolean): Promise<{ success: boolean; message: string }> {
  const { data } = await api.put<{ success: boolean; message: string }>('/config/credential-keep-alive', { enabled })
  return data
}

// 获取 Kiro 请求体落盘取证开关
export async function getDumpKiroBody(): Promise<{ enabled: boolean }> {
  const { data } = await api.get<{ enabled: boolean }>('/config/dump-kiro-body')
  return data
}

// 设置 Kiro 请求体落盘取证开关
export async function setDumpKiroBody(enabled: boolean): Promise<{ success: boolean; message: string }> {
  const { data } = await api.put<{ success: boolean; message: string }>('/config/dump-kiro-body', { enabled })
  return data
}

// 获取 DEBUG 日志开关
export async function getDebugLogs(): Promise<{ enabled: boolean }> {
  const { data } = await api.get<{ enabled: boolean }>('/config/debug-logs')
  return data
}

// 设置 DEBUG 日志开关（全局热生效：控制台 / 日志文件 / 日志页三处一致）
export async function setDebugLogs(enabled: boolean): Promise<{ success: boolean; message: string }> {
  const { data } = await api.put<{ success: boolean; message: string }>('/config/debug-logs', { enabled })
  return data
}

// 获取 dump 落盘文件清单（仅 JSON；二进制下载走 fetch + Blob，不经本层）
export async function getDumpFiles(): Promise<DumpFilesResponse> {
  const { data } = await api.get<DumpFilesResponse>('/dump-kiro-body/files')
  return data
}

// ============ 服务器信息 ============

// 获取服务器信息
export async function getServerInfo(): Promise<{ version: string }> {
  const { data } = await api.get<{ version: string }>('/server-info')
  return data
}

// ============ API Key 管理 ============

// 获取所有 API Key
export async function getApiKeys(): Promise<ApiKeyItem[]> {
  const { data } = await api.get<ApiKeyItem[]>('/api-keys')
  return data
}

// 创建 API Key
export async function createApiKey(req: CreateApiKeyRequest): Promise<ApiKeyItem> {
  const { data } = await api.post<ApiKeyItem>('/api-keys', req)
  return data
}

// 更新 API Key
export async function updateApiKey(id: number, req: UpdateApiKeyRequest): Promise<ApiKeyItem> {
  const { data } = await api.put<ApiKeyItem>(`/api-keys/${id}`, req)
  return data
}

// 删除 API Key
export async function deleteApiKey(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/api-keys/${id}`)
  return data
}

// ============ API Key 用量 ============

// 获取所有 API Key 用量概览
export async function getAllUsage(): Promise<UsageSummary[]> {
  const { data } = await api.get<UsageSummary[]>('/api-keys/usage')
  return data
}

// 获取单个 API Key 用量
export async function getKeyUsage(id: number): Promise<UsageSummary> {
  const { data } = await api.get<UsageSummary>(`/api-keys/${id}/usage`)
  return data
}

// 重置单个 API Key 用量
export async function resetKeyUsage(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/api-keys/${id}/usage`)
  return data
}

// 分页获取单个 API Key 的原始请求记录
export async function getKeyUsageRecords(
  id: number,
  page: number,
  pageSize: number
): Promise<UsageRecordsResponse> {
  const { data } = await api.get<UsageRecordsResponse>(
    `/api-keys/${id}/usage/records`,
    { params: { page, page_size: pageSize } }
  )
  return data
}

export async function getCredentialUsageRecords(
  id: number,
  page: number,
  pageSize: number
): Promise<UsageRecordsResponse> {
  const { data } = await api.get<UsageRecordsResponse>(
    `/credentials/${id}/usage/records`,
    { params: { page, page_size: pageSize } }
  )
  return data
}

// 获取单账号全部留存记录的用量汇总（不限日期）
export async function getCredentialUsageSummary(
  id: number
): Promise<CredentialUsageSummary> {
  const { data } = await api.get<CredentialUsageSummary>(
    `/credentials/${id}/usage/summary`
  )
  return data
}

// 获取单账号 CST 今日的用量汇总
export async function getCredentialTodaySummary(
  id: number
): Promise<CredentialDaySummary> {
  const { data } = await api.get<CredentialDaySummary>(
    `/credentials/${id}/usage/today`
  )
  return data
}

// 获取单账号 CST 最近 N 天（含今日）的用量汇总（7days / 30days）
export async function getCredentialRangeSummary(
  id: number,
  range: '7days' | '30days'
): Promise<CredentialRangeSummary> {
  const { data } = await api.get<CredentialRangeSummary>(
    `/credentials/${id}/usage/${range}`
  )
  return data
}

// ============ RPM 监控 ============

// 获取实时 RPM 数据
export async function getRpm(): Promise<RpmSnapshot> {
  const { data } = await api.get<RpmSnapshot>('/rpm')
  return data
}

// ============ 认证密钥管理 ============

export async function getAuthKeys(): Promise<{ adminPsw: string }> {
  const { data } = await api.get<{ adminPsw: string }>('/config/auth-keys')
  return data
}

export async function setAuthKeys(payload: { adminPsw?: string }): Promise<{ success: boolean; message: string }> {
  const { data } = await api.put<{ success: boolean; message: string }>('/config/auth-keys', payload)
  return data
}

// ============ 支持的模型 ============

export async function getModels(): Promise<ModelsResponse> {
  const { data } = await api.get<ModelsResponse>('/models')
  return data
}

// 获取指定账号支持的模型
export async function getCredentialModels(id: number): Promise<ModelsResponse> {
  const { data } = await api.get<ModelsResponse>(`/credentials/${id}/models`)
  return data
}

// ============ 对话会话 ============

export async function getChatSessions(): Promise<ChatSessionMeta[]> {
  const { data } = await api.get<ChatSessionMeta[]>('/chat/sessions')
  return data
}

export async function getChatSession<M>(id: string): Promise<ChatSession<M>> {
  const { data } = await api.get<ChatSession<M>>(`/chat/sessions/${id}`)
  return data
}

export async function createChatSession(title: string, messages: unknown[]): Promise<ChatSessionMeta> {
  const { data } = await api.post<ChatSessionMeta>('/chat/sessions', { title, messages })
  return data
}

export async function updateChatSession(
  id: string,
  messages: unknown[],
  title?: string
): Promise<ChatSessionMeta> {
  const { data } = await api.put<ChatSessionMeta>(`/chat/sessions/${id}`, { title, messages })
  return data
}

export async function deleteChatSession(id: string): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/chat/sessions/${id}`)
  return data
}

export async function clearChatSessions(): Promise<{ success: boolean; removed: number }> {
  const { data } = await api.delete<{ success: boolean; removed: number }>('/chat/sessions')
  return data
}

// ============ 更新日志 ============

export async function getChangelog(): Promise<ReleaseNotesResponse> {
  const { data } = await api.get<ReleaseNotesResponse>('/changelog')
  return data
}

// ============ 每日用量统计 ============

export async function getDailyUsage(): Promise<DailySummary[]> {
  const { data } = await api.get<DailySummary[]>('/usage/daily')
  return data
}

export async function getDailyUsageRecords(
  date: string,
  page: number,
  pageSize: number
): Promise<UsageRecordsResponse> {
  const { data } = await api.get<UsageRecordsResponse>(
    `/usage/daily/${date}/records`,
    { params: { page, page_size: pageSize } }
  )
  return data
}

// 获取 CST 闭区间 [from, to] 的原始请求记录（降序保留最新 10000 条后分页）
export async function getUsageRecordsByRange(
  from: string,
  to: string,
  page: number,
  pageSize: number
): Promise<UsageRecordsResponse> {
  const { data } = await api.get<UsageRecordsResponse>('/usage/records', {
    params: { from, to, page, page_size: pageSize },
  })
  return data
}

// ============ 失败日志 ============

export async function getFailureLogs(
  id: number,
  page: number,
  pageSize: number
): Promise<FailureLogsResponse> {
  const { data } = await api.get<FailureLogsResponse>(
    `/credentials/${id}/failure-logs`,
    { params: { page, page_size: pageSize } }
  )
  return data
}

// ============ 限流日志 ============

export async function getThrottleLogs(
  id: number,
  page: number,
  pageSize: number
): Promise<ThrottleLogsResponse> {
  const { data } = await api.get<ThrottleLogsResponse>(
    `/credentials/${id}/throttle-logs`,
    { params: { page, page_size: pageSize } }
  )
  return data
}

// ============ IP 归属地 ============

export async function getGeoBatch(
  ips: string[]
): Promise<Record<string, { country: string; regionName: string; city: string } | null>> {
  const { data } = await api.get<Record<string, { country: string; regionName: string; city: string } | null>>(
    '/geo/batch',
    { params: { ips: ips.join(',') } }
  )
  return data
}
