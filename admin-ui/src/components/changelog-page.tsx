// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useTranslation } from 'react-i18next'
import { RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'
import { useChangelog } from '@/hooks/use-credentials'

export function ChangelogPage() {
  const { t, i18n } = useTranslation()
  const { data, isLoading, refetch } = useChangelog()
  const notes = data?.data ?? []
  const lang = i18n.language === 'en' ? 'en' : 'zh'

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <h2 className="text-xl font-semibold">{t('changelog.pageTitle')}</h2>
        <div className="ml-auto flex items-center gap-2">
          <Button variant="ghost" size="sm" onClick={() => refetch()} disabled={isLoading}>
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </div>

      {isLoading ? (
        <Card>
          <CardContent className="py-8 text-center text-muted-foreground text-sm">{t('common.loading')}</CardContent>
        </Card>
      ) : notes.length === 0 ? (
        <Card>
          <CardContent className="py-8 text-center text-muted-foreground text-sm">{t('changelog.emptyNoChangelog')}</CardContent>
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
