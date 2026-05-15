import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
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
  setLoadBalancingMode,
  getServerInfo,
  getApiKeys,
  createApiKey,
  updateApiKey,
  deleteApiKey,
  getAllUsage,
  resetKeyUsage,
  getRpm,
  getAuthKeys,
  setAuthKeys,
} from '@/api/credentials'
import type { AddCredentialRequest, UpdateCredentialRequest, CreateApiKeyRequest, UpdateApiKeyRequest } from '@/types/api'

// 查询凭据列表
export function useCredentials() {
  return useQuery({
    queryKey: ['credentials'],
    queryFn: getCredentials,
    refetchInterval: 30000, // 每 30 秒刷新一次
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

// ============ API Key Hooks ============

// 获取服务器信息
export function useServerInfo() {
  return useQuery({
    queryKey: ['serverInfo'],
    queryFn: getServerInfo,
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
    mutationFn: (payload: { apiKey?: string; adminApiKey?: string }) => setAuthKeys(payload),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['auth-keys'] })
    },
  })
}
