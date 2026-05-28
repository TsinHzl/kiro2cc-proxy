// Copyright (c) 2026 Harllan He. Licensed under MIT.
import axios from 'axios'
import { storage } from '@/lib/storage'
import type { LoginRequest, LoginResponse, UsageResponse, UsageRecordsPage } from '@/types/api'

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

// 获取分页请求日志
export async function getUsageRecords(page = 1, pageSize = 50): Promise<UsageRecordsPage> {
  const { data } = await api.get<UsageRecordsPage>('/usage/records', {
    params: { page, page_size: pageSize },
  })
  return data
}
