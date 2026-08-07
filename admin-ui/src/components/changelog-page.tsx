// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'
import { useChangelog } from '@/hooks/use-credentials'

type Lang = 'zh' | 'en'

export function ChangelogPage() {
  const { data, isLoading, refetch } = useChangelog()
  const [lang, setLang] = useState<Lang>('zh')
  const notes = data?.data ?? []

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <h2 className="text-xl font-semibold">更新日志</h2>
        <div className="ml-auto flex items-center gap-2">
          <Button variant={lang === 'zh' ? 'secondary' : 'ghost'} size="sm" onClick={() => setLang('zh')}>
            中文
          </Button>
          <Button variant={lang === 'en' ? 'secondary' : 'ghost'} size="sm" onClick={() => setLang('en')}>
            EN
          </Button>
          <Button variant="ghost" size="sm" onClick={() => refetch()} disabled={isLoading}>
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </div>

      {isLoading ? (
        <Card>
          <CardContent className="py-8 text-center text-muted-foreground text-sm">加载中...</CardContent>
        </Card>
      ) : notes.length === 0 ? (
        <Card>
          <CardContent className="py-8 text-center text-muted-foreground text-sm">暂无更新日志</CardContent>
        </Card>
      ) : (
        <div className="space-y-4">
          {notes.map((note) => (
            <Card key={note.version}>
              <CardContent className="space-y-3 pt-6">
                <div className="flex items-center gap-2">
                  <span className="font-mono font-semibold">v{note.version}</span>
                  <span className="text-sm text-muted-foreground">{note.date}</span>
                  {note.is_latest && (
                    <Badge variant="success">NEW</Badge>
                  )}
                </div>
                <div className="space-y-3">
                  {note.groups.map((group, idx) => (
                    <div key={idx}>
                      <div className="text-sm font-medium text-muted-foreground mb-1">
                        {lang === 'zh' ? group.title_zh : group.title_en}
                      </div>
                      <ul className="list-disc list-inside space-y-0.5 text-sm">
                        {group.items.map((item, itemIdx) => (
                          <li key={itemIdx}>{lang === 'zh' ? item.zh : item.en}</li>
                        ))}
                      </ul>
                    </div>
                  ))}
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  )
}
