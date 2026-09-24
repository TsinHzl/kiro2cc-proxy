// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 清除无效 Key 对话框（自 api-keys-panel.tsx 拆出，纯代码搬移）
import { useTranslation } from 'react-i18next'
import { Loader2 } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type { ApiKeyItem } from '@/types/api'
import type { KeyStatus } from '@/components/api-key-row'

interface PurgeDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  invalidKeys: ApiKeyItem[]
  getKeyStatus: (key: ApiKeyItem) => KeyStatus
  handlePurge: () => void
  purging: boolean
}

export function PurgeDialog({
  open,
  onOpenChange,
  invalidKeys,
  getKeyStatus,
  handlePurge,
  purging,
}: PurgeDialogProps) {
  const { t } = useTranslation()

  return (
    <Dialog open={open} onOpenChange={(next) => !purging && onOpenChange(next)}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('apiKeys.purgeDialogTitle')}</DialogTitle>
          <DialogDescription>
            {t('apiKeys.purgeDialogDesc', { count: invalidKeys.length })}
          </DialogDescription>
        </DialogHeader>
        <div className="max-h-60 overflow-y-auto space-y-1 text-sm">
          {invalidKeys.map((k) => (
            <div key={k.id} className="flex items-center justify-between py-1 px-2 rounded bg-muted/50">
              <span>
                <code className="text-xs font-mono text-muted-foreground mr-2">{String(k.id).padStart(3, '0')}</code>
                {k.name}
              </span>
              <Badge variant={getKeyStatus(k) === 'disabled' ? 'destructive' : 'warning'} className="text-xs">
                {getKeyStatus(k) === 'disabled' ? t('apiKeys.statusDisabled') : t('apiKeys.statusExpired')}
              </Badge>
            </div>
          ))}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={purging}>{t('common.cancel')}</Button>
          <Button variant="destructive" onClick={handlePurge} disabled={purging}>
            {purging ? <><Loader2 className="h-4 w-4 mr-2 animate-spin" />{t('apiKeys.purgingButton')}</> : t('apiKeys.confirmPurgeButton', { count: invalidKeys.length })}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
