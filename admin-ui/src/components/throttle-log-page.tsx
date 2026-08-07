// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { ArrowLeft, AlertTriangle, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useCredentials, useThrottleLogs } from '@/hooks/use-credentials'
import { formatDateTime } from '@/lib/locale'

interface ThrottleLogPageProps {
  credentialId: number
  onBack: () => void
}

const PAGE_SIZE = 50

export function ThrottleLogPage({ credentialId, onBack }: ThrottleLogPageProps) {
  const { t } = useTranslation()
  const [page, setPage] = useState(1)
  const [selectedBody, setSelectedBody] = useState<string | null>(null)

  const { data: credentialsData } = useCredentials()
  const { data: logsData, isLoading, refetch } = useThrottleLogs(credentialId, page, PAGE_SIZE)

  const credential = credentialsData?.credentials.find((c) => c.id === credentialId)

  return (
    <div className="space-y-4">
      {/* 顶部导航 */}
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="sm" onClick={onBack} className="gap-1">
          <ArrowLeft className="h-4 w-4" />
          {t('common.back')}
        </Button>
        {credential && (
          <div className="flex items-center gap-2 flex-wrap">
            <code className="text-xs text-muted-foreground font-mono">#{credential.id}</code>
            <span className="font-semibold">{credential.nickname || credential.email || t('credentials.accountFallbackName', { id: credential.id })}</span>
            <Badge variant="secondary" className="gap-1">
              <AlertTriangle className="h-3 w-3" />
              {t('logs.throttleLogBadge')}
            </Badge>
          </div>
        )}
      </div>

      {/* 汇总卡片 */}
      <div className="grid gap-4 grid-cols-2 md:grid-cols-3">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
              <AlertTriangle className="h-3.5 w-3.5" />
              {t('logs.totalThrottleCount')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-orange-600 dark:text-orange-400">
              {logsData?.total ?? 0}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">{t('logs.last7DaysCount')}</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">
              {credential?.throttleCount ?? 0}
            </div>
            <div className="text-xs text-muted-foreground mt-0.5">{t('logs.throttleCountHint')}</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">{t('logs.recordLimitLabel')}</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">500</div>
            <div className="text-xs text-muted-foreground mt-0.5">{t('logs.limitExceededHint')}</div>
          </CardContent>
        </Card>
      </div>

      {/* 日志表格 */}
      <div>
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-sm font-medium text-muted-foreground">
            {t('logs.throttleEventsTitle')}
            {logsData && <span className="ml-1">{t('common.totalCountSuffix', { count: logsData.total })}</span>}
          </h3>
          <Button variant="ghost" size="sm" onClick={() => refetch()} disabled={isLoading}>
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
        </div>

        <Card>
          <CardContent className="p-0">
            {isLoading ? (
              <div className="py-8 text-center text-muted-foreground text-sm">{t('common.loading')}</div>
            ) : !logsData || logsData.records.length === 0 ? (
              <div className="py-8 text-center text-muted-foreground text-sm">{t('logs.emptyNoThrottleRecords')}</div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/50">
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('common.colTime')}</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('logs.colRequestType')}</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('logs.colStatusCode')}</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">{t('logs.colResponseSummary')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {logsData.records.map((record, idx) => (
                      <tr key={`${record.createdAt}-${idx}`} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                        <td className="px-4 py-2 text-xs text-muted-foreground whitespace-nowrap">
                          {formatDateTime(record.createdAt)}
                        </td>
                        <td className="px-4 py-2">
                          <Badge variant={record.requestType === 'mcp' ? 'default' : 'secondary'} className="text-xs">
                            {record.requestType.toUpperCase()}
                          </Badge>
                        </td>
                        <td className="px-4 py-2 font-mono text-xs text-orange-600 dark:text-orange-400">
                          {record.statusCode}
                        </td>
                        <td
                          className="px-4 py-2 text-xs text-muted-foreground max-w-[400px] truncate cursor-pointer hover:text-foreground hover:underline"
                          onClick={() => setSelectedBody(record.responseBody)}
                        >
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
              {t('common.prevPage')}
            </Button>
            <span className="text-sm text-muted-foreground">
              {t('common.pageInfoSimple', { current: page, total: logsData.totalPages })}
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setPage((p) => Math.min(logsData.totalPages, p + 1))}
              disabled={page === logsData.totalPages}
            >
              {t('common.nextPage')}
            </Button>
          </div>
        )}
      </div>

      <Dialog open={selectedBody !== null} onOpenChange={(open) => { if (!open) setSelectedBody(null) }}>
        <DialogContent className="max-w-2xl max-h-[80vh] flex flex-col">
          <DialogHeader>
            <DialogTitle>{t('logs.fullResponseTitle')}</DialogTitle>
          </DialogHeader>
          <pre className="text-xs font-mono whitespace-pre-wrap break-all overflow-y-auto flex-1 bg-muted/50 rounded p-3">
            {selectedBody}
          </pre>
        </DialogContent>
      </Dialog>
    </div>
  )
}
