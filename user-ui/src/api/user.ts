import axios from 'axios'
import { storage } from '@/lib/storage'
import type { LoginRequest, LoginResponse, UsageResponse } from '@/types/api'

const api = axios.create({
  baseURL: '/api/user',
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

// 登录验证
export async function login(apiKey: string): Promise<LoginResponse> {
  const { data } = await api.post<LoginResponse>('/login', { apiKey } as LoginRequest)
  return data
}

// 获取用量数据
export async function getUsage(): Promise<UsageResponse> {
  const { data } = await api.get<UsageResponse>('/usage')
  return data
}
