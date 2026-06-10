// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 凭据状态响应
export interface CredentialsStatusResponse {
  total: number
  available: number
  currentId: number
  credentials: CredentialStatusItem[]
}

// 单个凭据状态
export interface CredentialStatusItem {
  id: number
  priority: number
  disabled: boolean
  failureCount: number
  isCurrent: boolean
  expiresAt: string | null
  authMethod: string | null
  hasProfileArn: boolean
  email?: string
  nickname?: string
  refreshTokenHash?: string
  successCount: number
  lastUsedAt: string | null
  hasProxy: boolean
  proxyUrl?: string
  healthStatus: 'healthy' | 'warning' | 'degraded' | 'unhealthy' | 'disabled'
  throttleCount: number
}

// 余额响应
export interface BalanceResponse {
  id: number
  subscriptionTitle: string | null
  currentUsage: number
  usageLimit: number
  remaining: number
  usagePercentage: number
  nextResetAt: number | null
}

// 成功响应
export interface SuccessResponse {
  success: boolean
  message: string
}

// 错误响应
export interface AdminErrorResponse {
  error: {
    type: string
    message: string
  }
}

// 请求类型
export interface SetDisabledRequest {
  disabled: boolean
}

export interface SetPriorityRequest {
  priority: number
}

// 添加凭据请求
export interface AddCredentialRequest {
  refreshToken: string
  authMethod?: 'social' | 'idc'
  email?: string
  nickname?: string
  clientId?: string
  clientSecret?: string
  priority?: number
  authRegion?: string
  apiRegion?: string
  machineId?: string
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
}

// 更新凭据请求
export interface UpdateCredentialRequest {
  refreshToken?: string
  authMethod?: string
  email?: string
  nickname?: string
  clientId?: string
  clientSecret?: string
  authRegion?: string
  apiRegion?: string
  machineId?: string
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
}

// 添加凭据响应
export interface AddCredentialResponse {
  success: boolean
  message: string
  credentialId: number
  email?: string
}

// API Key 类型
export interface ApiKeyItem {
  id: number
  key: string
  name: string
  enabled: boolean
  createdAt: string
  expiresAt: string | null
  spendingLimit: number | null
  durationDays: number | null
  activatedAt: string | null
  boundCredentialIds?: number[]
}

export interface CreateApiKeyRequest {
  name: string
  expiresAt?: string | null
  spendingLimit?: number | null
  durationDays?: number | null
  boundCredentialIds?: number[] | null
}

export interface UpdateApiKeyRequest {
  name?: string
  enabled?: boolean
  expiresAt?: string | null
  spendingLimit?: number | null
  durationDays?: number | null
  boundCredentialIds?: number[] | null
}

// API Key 用量汇总
export interface UsageSummary {
  apiKeyId: number
  totalRequests: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCost: number
  totalCreditsSaved?: number
  byModel: ModelUsage[]
}

export interface ModelUsage {
  model: string
  requests: number
  inputTokens: number
  outputTokens: number
  cost: number
}

// RPM 实时监控
export interface RpmSnapshot {
  global: number
  byCredential: Record<string, number>
  byApiKey: Record<string, number>
}

// 单条原始请求记录
export interface UsageRecord {
  model: string
  inputTokens: number
  outputTokens: number
  estimatedCost: number
  creditsUsed?: number
  creditsSaved?: number
  cacheReadInputTokens?: number
  createdAt: string
  credentialId?: number
  credentialLabel?: string
  clientIp?: string
}

// 分页原始记录响应
export interface UsageRecordsResponse {
  records: UsageRecord[]
  total: number
  page: number
  pageSize: number
  totalPages: number
}

// 每日用量汇总
export interface DailySummary {
  date: string          // "2026-05-21" UTC
  totalRequests: number
  totalCost: number
  totalCredits: number
  totalCreditsSaved?: number
}

// 单条限流日志记录
export interface ThrottleLogRecord {
  credentialId: number
  requestType: string
  statusCode: number
  responseBody: string
  createdAt: string
}

// 限流日志分页响应
export interface ThrottleLogsResponse {
  records: ThrottleLogRecord[]
  total: number
  page: number
  pageSize: number
  totalPages: number
}

// 单条失败日志记录
export interface FailureLogRecord {
  credentialId: number
  requestType: string
  statusCode: number
  responseBody: string
  createdAt: string
}

// 失败日志分页响应
export interface FailureLogsResponse {
  records: FailureLogRecord[]
  total: number
  page: number
  pageSize: number
  totalPages: number
}
