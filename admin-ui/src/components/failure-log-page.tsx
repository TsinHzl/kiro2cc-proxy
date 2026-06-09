// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { ArrowLeft, XCircle } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { useCredentials } from '@/hooks/use-credentials'
import { LogViewerPage } from '@/components/log-viewer-page'

interface FailureLogPageProps {
  credentialId: number
  onBack: () => void
}

export function FailureLogPage({ credentialId, onBack }: FailureLogPageProps) {
  const { data: credentialsData } = useCredentials()
  const credential = credentialsData?.credentials.find((c) => c.id === credentialId)

  const keyword = credential?.email || credential?.nickname || ''

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: 'calc(100vh - 56px)' }}>
      {/* 顶部导航 */}
      <div className="flex items-center gap-3 mb-4">
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
      <div className="grid gap-4 grid-cols-2 md:grid-cols-3 mb-4 shrink-0">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
              <XCircle className="h-3.5 w-3.5" />
              累计失败次数
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-red-500 dark:text-red-400">
              {credential?.failureCount ?? 0}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">过滤条件</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-sm font-medium">级别：ERROR</div>
            {keyword && <div className="text-xs text-muted-foreground mt-0.5 truncate" title={keyword}>关键词：{keyword}</div>}
          </CardContent>
        </Card>
      </div>

      {/* 日志查看器（嵌入模式，预设 ERROR + 账号 email） */}
      <div className="flex-1 min-h-0">
        <LogViewerPage embedded initialLevelFilter="ERROR" initialKeyword={keyword} />
      </div>
    </div>
  )
}
