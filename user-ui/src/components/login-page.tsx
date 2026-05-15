import { useState } from 'react'
import { KeyRound, Loader2 } from 'lucide-react'
import { storage } from '@/lib/storage'
import { login } from '@/api/user'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { toast } from 'sonner'
import type { LoginResponse } from '@/types/api'

interface LoginPageProps {
  onLogin: (data: LoginResponse) => void
}

export function LoginPage({ onLogin }: LoginPageProps) {
  const [apiKey, setApiKey] = useState('')
  const [loading, setLoading] = useState(false)

  // 从输入中提取 API Key（支持粘贴整段发货信息）
  const extractApiKey = (input: string): string => {
    const match = input.match(/sk-[a-zA-Z0-9]+/)
    return match ? match[0] : input.trim()
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const key = extractApiKey(apiKey)
    if (!key) return

    setLoading(true)
    try {
      const data = await login(key)
      storage.setApiKey(key)
      onLogin(data)
    } catch (err: unknown) {
      const axiosErr = err as { response?: { data?: { error?: string } } }
      const msg = axiosErr.response?.data?.error || '登录失败，请检查 API Key'
      toast.error(msg)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
            <KeyRound className="h-6 w-6 text-primary" />
          </div>
          <CardTitle className="text-2xl">额度用量监控</CardTitle>
          <CardDescription>
            请输入您的 API Key 或粘贴发货信息查看用量数据
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <textarea
                placeholder={"sk-... 或粘贴发货信息"}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                className="flex w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 text-center font-mono resize-none"
                rows={3}
              />
            </div>
            <Button type="submit" className="w-full" disabled={!apiKey.trim() || loading}>
              {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
              {loading ? '验证中...' : '查看用量'}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
