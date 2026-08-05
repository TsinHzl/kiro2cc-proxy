// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { useModels } from '@/hooks/use-credentials'

const PROVIDER_STYLES: Record<string, { text: string; dot: string }> = {
  anthropic: { text: 'text-purple-600 dark:text-purple-400', dot: 'bg-purple-500' },
  openai: { text: 'text-green-600 dark:text-green-400', dot: 'bg-green-500' },
  deepseek: { text: 'text-blue-600 dark:text-blue-400', dot: 'bg-blue-500' },
  minimax: { text: 'text-orange-600 dark:text-orange-400', dot: 'bg-orange-500' },
  glm: { text: 'text-pink-600 dark:text-pink-400', dot: 'bg-pink-500' },
  qwen: { text: 'text-cyan-600 dark:text-cyan-400', dot: 'bg-cyan-500' },
  kiro: { text: 'text-muted-foreground', dot: 'bg-muted-foreground' },
}

const DEFAULT_PROVIDER_STYLE = { text: 'text-foreground', dot: 'bg-muted-foreground' }

function getProviderStyle(ownedBy: string): { text: string; dot: string } {
  return PROVIDER_STYLES[ownedBy] ?? DEFAULT_PROVIDER_STYLE
}

export function ModelListPage() {
  const { data, isLoading, refetch } = useModels()
  const models = data?.data ?? []

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <h2 className="text-xl font-semibold">支持模型</h2>
        <Button variant="ghost" size="sm" onClick={() => refetch()} disabled={isLoading} className="ml-auto">
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </Button>
      </div>

      <Card>
        <CardContent className="p-0">
          {isLoading ? (
            <div className="py-8 text-center text-muted-foreground text-sm">加载中...</div>
          ) : models.length === 0 ? (
            <div className="py-8 text-center text-muted-foreground text-sm">暂无可用模型</div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b bg-muted/50">
                    <th className="text-left px-4 py-2 font-medium text-muted-foreground">模型 ID</th>
                    <th className="text-left px-4 py-2 font-medium text-muted-foreground">显示名称</th>
                    <th className="text-left px-4 py-2 font-medium text-muted-foreground">提供方</th>
                    <th className="text-right px-4 py-2 font-medium text-muted-foreground">Max Tokens</th>
                    <th className="text-right px-4 py-2 font-medium text-muted-foreground">费率倍率</th>
                  </tr>
                </thead>
                <tbody>
                  {models.map((model) => {
                    const style = getProviderStyle(model.owned_by)
                    return (
                    <tr key={model.id} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                      <td className={`px-4 py-2 font-mono text-xs ${style.text}`}>{model.id}</td>
                      <td className="px-4 py-2">{model.display_name}</td>
                      <td className="px-4 py-2 text-muted-foreground">
                        <span className="inline-flex items-center gap-1.5">
                          <span className={`inline-block w-2 h-2 rounded-full ${style.dot}`} />
                          {model.owned_by}
                        </span>
                      </td>
                      <td className="px-4 py-2 text-right tabular-nums">{model.max_tokens.toLocaleString()}</td>
                      <td className="px-4 py-2 text-right tabular-nums">
                        {model.rate_multiplier != null ? `${model.rate_multiplier.toFixed(2)}x` : '—'}
                      </td>
                    </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
