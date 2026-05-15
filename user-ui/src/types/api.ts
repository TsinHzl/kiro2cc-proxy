export interface LoginRequest {
  apiKey: string
}

export interface LoginResponse {
  id: number
  name: string
  spendingLimit: number | null
  totalCost: number
  expiresAt: string | null
  durationDays: number | null
  activatedAt: string | null
}

export interface ModelUsage {
  model: string
  requests: number
  inputTokens: number
  outputTokens: number
  cost: number
}

export interface UsageResponse {
  id: number
  name: string
  spendingLimit: number | null
  expiresAt: string | null
  durationDays: number | null
  activatedAt: string | null
  totalRequests: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCost: number
  byModel: ModelUsage[]
}
