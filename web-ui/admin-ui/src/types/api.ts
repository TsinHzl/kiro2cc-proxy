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
  /** 账号添加时间（RFC3339；旧账号为首次加载补写值） */
  createdAt?: string | null
  authMethod: string | null
  hasProfileArn: boolean
  /** OIDC Client ID（IdC / external_idp；非机密，编辑弹窗只读展示） */
  clientId?: string
  /** Profile ARN（非机密，编辑弹窗只读展示） */
  profileArn?: string
  /** 账号级 Machine ID（非机密，编辑弹窗只读展示） */
  machineId?: string
  email?: string
  nickname?: string
  /** 登录来源 / IdP 标识（social 账号为 Google / GitHub；external_idp 为 AzureAD 等） */
  provider?: string
  /** 账号级 Auth Region（未配置时回退全局） */
  authRegion?: string
  /** 账号级 API Region（未配置时回退全局） */
  apiRegion?: string
  refreshTokenHash?: string
  successCount: number
  lastUsedAt: string | null
  hasProxy: boolean
  proxyUrl?: string
  /** 账号级 thinking adaptive 注入开关（客户端请求 thinking: adaptive 时按此开关注入 Kiro） */
  thinkingAdaptive: boolean
  healthStatus: 'healthy' | 'warning' | 'degraded' | 'unhealthy' | 'disabled'
  throttleCount: number
  /**
   * 禁用原因；与 `disabled` 配套使用。
   * - quota_exceeded：上游 402 MONTHLY_REQUEST_COUNT 触发，跨自然月自动恢复
   * - 其他：手动 / 连续失败 / refreshToken 撤销 等需人工介入
   */
  disabledReason?:
    | 'manual'
    | 'too_many_failures'
    | 'quota_exceeded'
    | 'invalid_refresh_token'
    | 'too_many_refresh_failures'
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
  /** 主配额已用（无 breakdown 时缺失） */
  baseUsage?: number
  /** 主配额上限 */
  baseLimit?: number
  /** 免费试用额度（仅 ACTIVE 时返回） */
  freeTrial?: FreeTrialBalance
  /** 生效奖励列表（仅 ACTIVE） */
  bonuses?: BonusBalance[]
  /** 资源计费详情 */
  resourceDetail?: ResourceDetailBalance
  /** 原始订阅类型（Q_DEVELOPER_STANDALONE_PRO 等） */
  subscriptionType?: string
  /** 是否可升级 */
  upgradeCapable?: boolean
  /** 用户 ID（d-xxxx 长格式） */
  userId?: string
}

// 免费试用额度分解
export interface FreeTrialBalance {
  currentUsage: number
  usageLimit: number
  expiresAt: number | null
}

// 生效奖励额度
export interface BonusBalance {
  code: string
  name: string
  currentUsage: number
  usageLimit: number
  expiresAt: number | null
  status: string
}

// 资源计费详情
export interface ResourceDetailBalance {
  resourceType?: string
  unit?: string
  currency?: string
  overageRate?: number
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
  profileArn?: string
  priority?: number
  disabled?: boolean
  authRegion?: string
  apiRegion?: string
  machineId?: string
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
  /** 登录来源 / IdP 标识（social 账号：Google / GitHub） */
  provider?: string
}

// 更新凭据请求
export interface UpdateCredentialRequest {
  refreshToken?: string
  authMethod?: string
  email?: string
  nickname?: string
  clientId?: string
  clientSecret?: string
  profileArn?: string
  authRegion?: string
  apiRegion?: string
  machineId?: string
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
  /** 登录来源 / IdP 标识（social 账号：Google / GitHub；空串清除该字段） */
  provider?: string
  /** 账号级 thinking adaptive 注入开关（可选，不传则不更新该字段） */
  thinkingAdaptive?: boolean
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
  limitUnit: 'usd' | 'credits'
  durationDays: number | null
  activatedAt: string | null
  boundCredentialIds?: number[]
}

export interface CreateApiKeyRequest {
  name: string
  expiresAt?: string | null
  spendingLimit?: number | null
  limitUnit?: 'usd' | 'credits'
  durationDays?: number | null
  boundCredentialIds?: number[] | null
}

export interface UpdateApiKeyRequest {
  name?: string
  enabled?: boolean
  expiresAt?: string | null
  spendingLimit?: number | null
  limitUnit?: 'usd' | 'credits'
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
  totalCredits: number
  totalCreditsSaved?: number
  byModel: ModelUsage[]
}

export interface ModelUsage {
  model: string
  requests: number
  inputTokens: number
  outputTokens: number
  cost: number
  credits: number
  creditsSaved: number
}

// 支持的模型条目（字段名与后端 Model 结构体保持一致，非 camelCase）
export interface ModelItem {
  id: string
  object: string
  created: number
  owned_by: string
  display_name: string
  type: string
  max_tokens: number
  rate_multiplier?: number | null
  /** 模型发布日期（YYYY-MM-DD）；未登记为 null，前端据此实时计算「最近新增」标记 */
  released_at?: string | null
  /** 官方模型描述；上游未返回或回退静态表时为 null */
  description?: string | null
  /** 上游声明的输入模态（`TEXT` / `IMAGE`）；上游未返回或回退静态表时为空数组 */
  input_types?: string[]
}

export interface ModelsResponse {
  object: string
  data: ModelItem[]
}

// 对话会话（后端 chat_session.rs；消息结构由前端 playground/types.ts 定义，后端不解析）
export interface ChatSessionMeta {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  messageCount: number
}

export interface ChatSession<M = unknown> extends Omit<ChatSessionMeta, 'messageCount'> {
  messages: M[]
}

// 更新日志（字段名与后端 AdminReleaseNote 结构体保持一致，非 camelCase）
export interface BilingualText {
  zh: string
  en: string
}

export interface ReleaseNoteGroup {
  title_zh: string
  title_en: string
  items: BilingualText[]
}

export interface ReleaseNote {
  version: string
  is_latest: boolean
  groups: ReleaseNoteGroup[]
}

export interface ReleaseNotesResponse {
  object: string
  data: ReleaseNote[]
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
  cacheCreationInputTokens?: number
  createdAt: string
  credentialId?: number
  credentialLabel?: string
  clientIp?: string
  effort?: string
  firstTokenMs?: number
  durationMs?: number
  thinkingTokenCount?: number
  isStream?: boolean
  userAgent?: string
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
  date: string          // "2026-05-21" CST (UTC+8)，与 get_daily_summaries() 的聚合口径一致
  totalRequests: number
  totalCost: number
  totalCredits: number
  totalCreditsSaved?: number
  totalInputTokens: number      // 累计输入 tokens（含 cache 命中部分）
  totalOutputTokens: number
  totalCacheReadTokens: number  // 累计 cache 命中的输入 tokens
}

// 单账号全部留存记录的用量汇总（后端 /credentials/:id/usage/summary，不限日期）
export interface CredentialUsageSummary {
  credentialId: number
  totalRequests: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCost: number
  totalCredits: number
  totalCreditsSaved?: number
  totalCacheReadTokens: number
}

// 单账号在某 CST 日期的用量汇总（后端 /credentials/:id/usage/today）
export interface CredentialDaySummary {
  date: string           // "2026-06-26" CST (UTC+8)
  credentialId: number
  totalRequests: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCost: number
  totalCredits: number
  totalCreditsSaved?: number
}

// 单账号在 CST 最近 N 天（含今日）的用量汇总（后端 /credentials/:id/usage/7days、/30days）
export interface CredentialRangeSummary {
  credentialId: number
  fromDate: string       // "2026-10-16" CST，区间起始日（含）
  toDate: string         // 区间结束日（含），恒为今日
  totalRequests: number
  totalInputTokens: number
  totalOutputTokens: number
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

// 单个 dump 文件元数据（后端 /dump-kiro-body/files，字段为 camelCase 序列化）
export interface DumpFileMeta {
  name: string
  sizeBytes: number
  /** 取文件名内毫秒时间戳，不取文件系统 mtime（后端决策 9） */
  createdAtMs: number
}

// dump 文件清单响应（后端 /dump-kiro-body/files）
export interface DumpFilesResponse {
  files: DumpFileMeta[]
  totalBytes: number
  /** 后端保留的 dump 文件数上限，用于向管理员展示滚动清理语义 */
  maxFiles: number
  /** 归档总量上限，前端据此判断是否提示截断，不硬编码阈值 */
  maxArchiveBytes: number
}
