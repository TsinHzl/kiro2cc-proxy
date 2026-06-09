// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { ArrowLeft, XCircle, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { useCredentials, useFailureLogs } from '@/hooks/use-credentials'

interface FailureLogPageProps {
  credentialId: number
  onBack: () => void
}

function formatDate(dateStr: string): string {
  return new Date(dateStr).toLocaleString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}

const PAGE_SIZE = 50

export function FailureLogPage({ credentialId, onBack }: FailureLogPageProps) {
  const [page, setPage] = useState(1)

  const { data: credentialsData } = useCredentials()
  const { data: logsData, isLoading, refetch } = useFailureLogs(credentialId, page, PAGE_SIZE)

  const credential = credentialsData?.credentials.find((c) => c.id === credentialId)

  return (
    <div className="space-y-4">
      {/* 顶部导航 */}
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="sm" onClick={onBack} className="gap-1">
          <ArrowLeft className="h-4 w-4" />
          返回
        </Button>
        {credential && (
          <div className="flex items-center gap-2 flex-wrap">
            <code className="text-xs text-muted-foreground font-mono">#{credential.id}</code>
            <span className="font-semibold">{credential.nickname || credential.email || `账号 #${credential.id}`}</span>
            <Badge variant="destructive" className="gap-1">
              <XCircle className="h-3 w-3" />
              失败日志
            </Badge>
          </div>
        )}
      </div>

      {/* 汇总卡片 */}
      <div className="grid gap-4 grid-cols-2 md:grid-cols-3">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
              <XCircle className="h-3.5 w-3.5" />
              日志记录数
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-red-500 dark:text-red-400">
              {logsData?.total ?? 0}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">累计失败次数</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {credential?.failureCount ?? 0}
            </div>
            <div className="text-xs text-muted-foreground mt-0.5">含历史已清理记录</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">记录上限</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">500</div>
            <div className="text-xs text-muted-foreground mt-0.5">超出淘汰最旧记录</div>
          </CardContent>
        </Card>
      </div>

      {/* 日志表格 */}
      <div>
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-sm font-medium text-muted-foreground">
            失败事件
            {logsData && <span className="ml-1">（共 {logsData.total} 条）</span>}
          </h3>
          <Button variant="ghost" size="sm" onClick={() => refetch()} disabled={isLoading}>
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
        </div>

        <Card>
          <CardContent className="p-0">
            {isLoading ? (
              <div className="py-8 text-center text-muted-foreground text-sm">加载中...</div>
            ) : !logsData || logsData.records.length === 0 ? (
              <div className="py-8 text-center text-muted-foreground text-sm">暂无失败记录</div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/50">
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">时间</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">请求类型</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">状态码</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">响应摘要</th>
                    </tr>
                  </thead>
                  <tbody>
                    {logsData.records.map((record, idx) => (
                      <tr key={`${record.createdAt}-${idx}`} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                        <td className="px-4 py-2 text-xs text-muted-foreground whitespace-nowrap">
                          {formatDate(record.createdAt)}
                        </td>
                        <td className="px-4 py-2">
                          <Badge variant={record.requestType === 'mcp' ? 'default' : 'secondary'} className="text-xs">
                            {record.requestType.toUpperCase()}
                          </Badge>
                        </td>
                        <td className="px-4 py-2 font-mono text-xs text-red-500 dark:text-red-400">
                          {record.statusCode}
                        </td>
                        <td className="px-4 py-2 text-xs text-muted-foreground max-w-[400px] truncate" title={record.responseBody}>
                          {record.responseBody}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </CardContent>
        </Card>

        {/* 分页控件 */}
        {logsData && logsData.totalPages > 1 && (
          <div className="flex justify-center items-center gap-4 mt-4">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              disabled={page === 1}
            >
              上一页
            </Button>
            <span className="text-sm text-muted-foreground">
              第 {page} / {logsData.totalPages} 页
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setPage((p) => Math.min(logsData.totalPages, p + 1))}
              disabled={page === logsData.totalPages}
            >
              下一页
            </Button>
          </div>
        )}
      </div>
    </div>
  )
}
