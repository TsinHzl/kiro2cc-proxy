// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from 'sonner'
import { useTranslation } from 'react-i18next'
import { CheckCircle2, XCircle, AlertCircle, Loader2 } from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { useCredentials, useAddCredential, useDeleteCredential } from '@/hooks/use-credentials'
import { getCredentialBalance, setCredentialDisabled } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import { sha256Hex } from '@/lib/hash'

interface BatchImportDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

interface CredentialInput {
  refreshToken: string
  email?: string
  clientId?: string
  clientSecret?: string
  region?: string
  authRegion?: string
  apiRegion?: string
  profileArn?: string
  priority?: number
  machineId?: string
}

interface VerificationResult {
  index: number
  status: 'pending' | 'checking' | 'verifying' | 'verified' | 'duplicate' | 'failed'
  error?: string
  usage?: string
  email?: string
  credentialId?: number
  rollbackStatus?: 'success' | 'failed' | 'skipped'
  rollbackError?: string
}

export function BatchImportDialog({ open, onOpenChange }: BatchImportDialogProps) {
  const { t } = useTranslation()
  const [jsonInput, setJsonInput] = useState('')
  const [importing, setImporting] = useState(false)
  const [progress, setProgress] = useState({ current: 0, total: 0 })
  const [currentProcessing, setCurrentProcessing] = useState<string>('')
  const [results, setResults] = useState<VerificationResult[]>([])

  const { data: existingCredentials } = useCredentials()
  const { mutateAsync: addCredential } = useAddCredential()
  const { mutateAsync: deleteCredential } = useDeleteCredential()

  const rollbackCredential = async (id: number): Promise<{ success: boolean; error?: string }> => {
    try {
      await setCredentialDisabled(id, true)
    } catch (error) {
      return {
        success: false,
        error: t('credentials.toastDisableFailed', { message: extractErrorMessage(error) }),
      }
    }

    try {
      await deleteCredential(id)
      return { success: true }
    } catch (error) {
      return {
        success: false,
        error: t('credentials.toastDeleteFailed', { message: extractErrorMessage(error) }),
      }
    }
  }

  const resetForm = () => {
    setJsonInput('')
    setProgress({ current: 0, total: 0 })
    setCurrentProcessing('')
    setResults([])
  }

  const handleBatchImport = async () => {
    try {
      // 1. 解析 JSON
      const parsed = JSON.parse(jsonInput)
      let credentials: CredentialInput[]
      if (Array.isArray(parsed)) {
        credentials = parsed
      } else if (parsed.accounts && Array.isArray(parsed.accounts)) {
        // KAM 导出格式：{ version, accounts: [...] }
        credentials = parsed.accounts
          .map((a: Record<string, any>) => ({
            refreshToken: a.credentials?.refreshToken,
            email: a.email || a.nickname,
            machineId: a.machineId,
            authRegion: a.credentials?.region,
            authMethod: a.credentials?.authMethod,
            clientId: a.credentials?.clientId || undefined,
            clientSecret: a.credentials?.clientSecret || undefined,
            profileArn: a.credentials?.profileArn || a.profileArn || undefined,
          }))
          .filter((c: CredentialInput) => c.refreshToken)
      } else {
        credentials = [parsed]
      }

      if (credentials.length === 0) {
        toast.error(t('credentials.toastNoImportable'))
        return
      }

      setImporting(true)
      setProgress({ current: 0, total: credentials.length })

      // 2. 初始化结果
      const initialResults: VerificationResult[] = credentials.map((_, i) => ({
        index: i + 1,
        status: 'pending'
      }))
      setResults(initialResults)

      // 3. 检测重复
      const existingTokenHashes = new Set(
        existingCredentials?.credentials
          .map(c => c.refreshTokenHash)
          .filter((hash): hash is string => Boolean(hash)) || []
      )

      let successCount = 0
      let duplicateCount = 0
      let failCount = 0
      let rollbackSuccessCount = 0
      let rollbackFailedCount = 0
      let rollbackSkippedCount = 0

      // 4. 导入并验活
      for (let i = 0; i < credentials.length; i++) {
        const cred = credentials[i]
        const token = cred.refreshToken.trim()
        const tokenHash = await sha256Hex(token)

        // 更新状态为检查中
        setCurrentProcessing(t('credentials.processingAccountProgress', { current: i + 1, total: credentials.length }))
        setResults(prev => {
          const newResults = [...prev]
          newResults[i] = { ...newResults[i], status: 'checking' }
          return newResults
        })

        // 检查重复
        if (existingTokenHashes.has(tokenHash)) {
          duplicateCount++
          const existingCred = existingCredentials?.credentials.find(c => c.refreshTokenHash === tokenHash)
          setResults(prev => {
            const newResults = [...prev]
            newResults[i] = {
              ...newResults[i],
              status: 'duplicate',
              error: t('credentials.accountAlreadyExists'),
              email: existingCred?.email || undefined
            }
            return newResults
          })
          setProgress({ current: i + 1, total: credentials.length })
          continue
        }

        // 更新状态为验活中
        setResults(prev => {
          const newResults = [...prev]
          newResults[i] = { ...newResults[i], status: 'verifying' }
          return newResults
        })

        let addedCredId: number | null = null

        try {
          // 添加凭据
          const clientId = cred.clientId?.trim() || undefined
          const clientSecret = cred.clientSecret?.trim() || undefined
          const authMethod = clientId && clientSecret ? 'idc' : 'social'

          // idc 模式下必须同时提供 clientId 和 clientSecret
          if (authMethod === 'social' && (clientId || clientSecret)) {
            throw new Error(t('credentials.idcRequiresBoth'))
          }

          const addedCred = await addCredential({
            refreshToken: token,
            authMethod,
            email: cred.email?.trim() || undefined,
            authRegion: cred.authRegion?.trim() || cred.region?.trim() || undefined,
            apiRegion: cred.apiRegion?.trim() || undefined,
            clientId,
            clientSecret,
            profileArn: cred.profileArn?.trim() || undefined,
            priority: cred.priority || 0,
            machineId: cred.machineId?.trim() || undefined,
          })

          addedCredId = addedCred.credentialId

          // 延迟 1 秒
          await new Promise(resolve => setTimeout(resolve, 1000))

          // 验活
          const balance = await getCredentialBalance(addedCred.credentialId)

          // 验活成功
          successCount++
          existingTokenHashes.add(tokenHash)
          setCurrentProcessing(t('credentials.verifySuccessPrefix', { name: addedCred.email || t('credentials.plainAccountIndex', { index: i + 1 }) }))
          setResults(prev => {
            const newResults = [...prev]
            newResults[i] = {
              ...newResults[i],
              status: 'verified',
              usage: `${balance.currentUsage}/${balance.usageLimit}`,
              email: addedCred.email || undefined,
              credentialId: addedCred.credentialId
            }
            return newResults
          })
        } catch (error) {
          // 验活失败，尝试回滚（先禁用再删除）
          let rollbackStatus: VerificationResult['rollbackStatus'] = 'skipped'
          let rollbackError: string | undefined

          if (addedCredId) {
            const rollbackResult = await rollbackCredential(addedCredId)
            if (rollbackResult.success) {
              rollbackStatus = 'success'
              rollbackSuccessCount++
            } else {
              rollbackStatus = 'failed'
              rollbackFailedCount++
              rollbackError = rollbackResult.error
            }
          } else {
            rollbackSkippedCount++
          }

          failCount++
          setResults(prev => {
            const newResults = [...prev]
            newResults[i] = {
              ...newResults[i],
              status: 'failed',
              error: extractErrorMessage(error),
              email: undefined,
              rollbackStatus,
              rollbackError,
            }
            return newResults
          })
        }

        setProgress({ current: i + 1, total: credentials.length })
      }

      // 显示结果
      if (failCount === 0 && duplicateCount === 0) {
        toast.success(t('credentials.toastImportVerifySuccess', { count: successCount }))
      } else {
        const failureSummary = failCount > 0
          ? t('credentials.failureSummarySuffix', { count: failCount, excluded: rollbackSuccessCount, notExcluded: rollbackFailedCount, noNeedExclude: rollbackSkippedCount })
          : ''
        toast.info(t('credentials.toastVerifyCompleteSummary', { success: successCount, duplicate: duplicateCount, failureSummary }))

        if (rollbackFailedCount > 0) {
          toast.warning(t('credentials.toastRollbackIncomplete', { count: rollbackFailedCount }))
        }
      }
    } catch (error) {
      toast.error(t('credentials.toastJsonError', { message: extractErrorMessage(error) }))
    } finally {
      setImporting(false)
    }
  }

  const getStatusIcon = (status: VerificationResult['status']) => {
    switch (status) {
      case 'pending':
        return <div className="w-5 h-5 rounded-full border-2 border-hairline-2" />
      case 'checking':
      case 'verifying':
        return <Loader2 className="w-5 h-5 animate-spin text-brand" />
      case 'verified':
        return <CheckCircle2 className="w-5 h-5 text-ok" />
      case 'duplicate':
        return <AlertCircle className="w-5 h-5 text-warn" />
      case 'failed':
        return <XCircle className="w-5 h-5 text-danger" />
    }
  }

  const getStatusText = (result: VerificationResult) => {
    switch (result.status) {
      case 'pending':
        return t('credentials.statusPending')
      case 'checking':
        return t('credentials.statusChecking')
      case 'verifying':
        return t('credentials.statusVerifying')
      case 'verified':
        return t('credentials.statusVerified')
      case 'duplicate':
        return t('credentials.statusDuplicate')
      case 'failed':
        if (result.rollbackStatus === 'success') return t('credentials.statusFailedExcluded')
        if (result.rollbackStatus === 'failed') return t('credentials.statusFailedNotExcluded')
        return t('credentials.statusFailedNotCreated')
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(newOpen) => {
        // 关闭时清空表单（但不在导入过程中清空）
        if (!newOpen && !importing) {
          resetForm()
        }
        onOpenChange(newOpen)
      }}
    >
      <DialogContent className="sm:max-w-2xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('credentials.batchImportDialogTitle')}</DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 py-4">
          <div className="space-y-2">
            <label className="text-[11.5px] font-medium text-ink-2">
              {t('credentials.jsonFormatAccountsLabel')}
            </label>
            <textarea
              placeholder={t('credentials.batchImportPlaceholder')}
              value={jsonInput}
              onChange={(e) => setJsonInput(e.target.value)}
              disabled={importing}
              className="min-h-[200px] w-full rounded-[7px] border border-hairline-2 bg-surface-2 px-2.5 py-2 font-mono text-[12px] text-ink outline-none transition-colors placeholder:text-ink-3 focus:border-brand disabled:cursor-not-allowed disabled:opacity-50"
            />
            <p className="text-[11px] leading-[1.55] text-ink-3">
              {t('credentials.batchImportHint')}
            </p>
          </div>

          {(importing || results.length > 0) && (
            <>
              {/* 进度条 */}
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span>{importing ? t('credentials.verifyingProgressLabel') : t('credentials.verifyCompleteLabel')}</span>
                  <span>{progress.current} / {progress.total}</span>
                </div>
                <div className="h-1 w-full overflow-hidden rounded-[3px] bg-track">
                  <div
                    className="h-full rounded-[3px] bg-brand transition-all"
                    style={{ width: `${(progress.current / progress.total) * 100}%` }}
                  />
                </div>
                {importing && currentProcessing && (
                  <div className="text-[11px] text-ink-3">
                    {currentProcessing}
                  </div>
                )}
              </div>

              {/* 统计 */}
              <div className="flex gap-4 text-sm">
                <span className="text-ok">
                  ✓ {t('credentials.statSuccessLabel')}: {results.filter(r => r.status === 'verified').length}
                </span>
                <span className="text-warn">
                  ⚠ {t('credentials.statDuplicateLabel')}: {results.filter(r => r.status === 'duplicate').length}
                </span>
                <span className="text-danger">
                  ✗ {t('credentials.statFailedLabel')}: {results.filter(r => r.status === 'failed').length}
                </span>
              </div>

              {/* 结果列表 */}
              <div className="max-h-[300px] divide-y divide-hairline overflow-y-auto rounded-[8px] border border-hairline">
                {results.map((result) => (
                  <div key={result.index} className="p-3">
                    <div className="flex items-start gap-3">
                      {getStatusIcon(result.status)}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <span className="text-sm font-medium">
                            {result.email || t('credentials.accountFallbackName', { id: result.index })}
                          </span>
                          <span className="text-[11px] text-ink-3">
                            {getStatusText(result)}
                          </span>
                        </div>
                        {result.usage && (
                          <div className="mt-1 text-[11px] text-ink-3">
                            {t('credentials.usageLabel', { usage: result.usage })}
                          </div>
                        )}
                        {result.error && (
                          <div className="mt-1 text-[11px] text-danger">
                            {result.error}
                          </div>
                        )}
                        {result.rollbackError && (
                          <div className="mt-1 text-[11px] text-danger">
                            {t('credentials.rollbackFailedLabel', { error: result.rollbackError })}
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>

        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={() => {
              onOpenChange(false)
              resetForm()
            }}
            disabled={importing}
          >
            {importing ? t('credentials.verifyingButton') : results.length > 0 ? t('common.close') : t('common.cancel')}
          </Button>
          {results.length === 0 && (
            <Button
              type="button"
              onClick={handleBatchImport}
              disabled={importing || !jsonInput.trim()}
            >
              {t('credentials.startImportVerifyButton')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
