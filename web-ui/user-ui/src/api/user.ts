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

// 下载当前用户全部留存的请求日志（CSV）。
// 走 fetch + Blob：axios 默认按 JSON 解析响应，且错误响应体会变成 Blob，取不到后端错误信息。
export async function downloadUsageRecordsCsv(): Promise<void> {
  const res = await fetch('/api/user/usage/records/download', {
    headers: { 'x-api-key': storage.getApiKey() ?? '' },
    signal: AbortSignal.timeout(120_000),
  })
  if (!res.ok) {
    let message = `HTTP ${res.status}`
    try {
      const body = (await res.json()) as { error?: unknown }
      if (typeof body?.error === 'string' && body.error) message = body.error
    } catch {
      // 非 JSON（如反代返回 HTML）：保留 HTTP 状态码
    }
    throw new Error(message)
  }

  const blob = await res.blob()
  const disposition = res.headers.get('Content-Disposition')
  const name = disposition?.match(/filename="([^"]+)"/)?.[1] ?? 'usage-logs.csv'
  const objectUrl = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = objectUrl
  a.download = name
  a.rel = 'noopener'
  // 部分浏览器要求锚点已挂载才触发下载
  document.body.appendChild(a)
  a.click()
  a.remove()
  // click 后浏览器才异步读取 blob，延迟释放避免中断下载
  window.setTimeout(() => URL.revokeObjectURL(objectUrl), 60_000)
}

// 批量查询 IP 归属地
export async function getGeoBatch(
  ips: string[]
): Promise<Record<string, { country: string; regionName: string; city: string } | null>> {
  const { data } = await api.get<Record<string, { country: string; regionName: string; city: string } | null>>(
    '/geo/batch',
    { params: { ips: ips.join(',') } }
  )
  return data
}
