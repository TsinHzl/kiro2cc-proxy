// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useRef, useState } from 'react'
import { toast } from 'sonner'
import { useTranslation } from 'react-i18next'
import {
  CheckCircle2,
  XCircle,
  AlertCircle,
  Loader2,
  Braces,
  Eraser,
  Download,
  FolderOpen,
  Lightbulb,
} from 'lucide-react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { useCredentials, useAddCredential, useSetDisabled } from '@/hooks/use-credentials'
import { getCredentialBalance } from '@/api/credentials'
import { KAM_RELEASES_URL } from '@/lib/constants'
import { extractErrorMessage } from '@/lib/utils'
import { sha256Hex } from '@/lib/hash'
import { normalizeLoginSource } from '@/lib/account-state'

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
  /** 登录来源（仅 Google / GitHub 会被归一化后写入） */
  provider?: string
}

interface VerificationResult {
  index: number
  status:
    | 'pending'
    | 'checking'
    | 'verifying'
    | 'verified'
    | 'added_unverified'
    | 'activation_unknown'
    | 'duplicate'
    | 'failed'
  error?: string
  usage?: string
  email?: string
  credentialId?: number
}

const MAX_FILE_SIZE = 2 * 1024 * 1024

export function BatchImportDialog({ open, onOpenChange }: BatchImportDialogProps) {
  const { t } = useTranslation()
  const [jsonInput, setJsonInput] = useState('')
  const [importing, setImporting] = useState(false)
  const [progress, setProgress] = useState({ current: 0, total: 0 })
  const [currentProcessing, setCurrentProcessing] = useState<string>('')
  const [results, setResults] = useState<VerificationResult[]>([])
  const [dragActive, setDragActive] = useState(false)

  const fileInputRef = useRef<HTMLInputElement>(null)
  const codeBoxRef = useRef<HTMLDivElement>(null)

  const { data: existingCredentials } = useCredentials()
  const { mutateAsync: addCredential } = useAddCredential()
  const { mutateAsync: setDisabled } = useSetDisabled()

  const resetForm = () => {
    setJsonInput('')
    setProgress({ current: 0, total: 0 })
    setCurrentProcessing('')
    setResults([])
    setDragActive(false)
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
            // 登录来源：KAM 在 credentials.provider 与顶层 idp 两处都可能带；
            // 归一化后仅 Google / GitHub 入库，其余（含 BuilderId）不写入
            provider:
              normalizeLoginSource(a.credentials?.provider) ?? normalizeLoginSource(a.idp),
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
      let unverifiedCount = 0
      let activationUnknownCount = 0
      let duplicateCount = 0
      let failCount = 0

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
            provider: cred.provider,
            disabled: true,
          })
          existingTokenHashes.add(tokenHash)

          try {
            const balance = await getCredentialBalance(addedCred.credentialId)

            try {
              await setDisabled({ id: addedCred.credentialId, disabled: false })
              successCount++
              setCurrentProcessing(t('credentials.verifySuccessPrefix', {
                name: addedCred.email || t('credentials.plainAccountIndex', { index: i + 1 }),
              }))
              setResults(prev => {
                const newResults = [...prev]
                newResults[i] = {
                  ...newResults[i],
                  status: 'verified',
                  usage: `${balance.currentUsage}/${balance.usageLimit}`,
                  email: addedCred.email || undefined,
                  credentialId: addedCred.credentialId,
                }
                return newResults
              })
            } catch (error) {
              activationUnknownCount++
              setResults(prev => {
                const newResults = [...prev]
                newResults[i] = {
                  ...newResults[i],
                  status: 'activation_unknown',
                  error: t('credentials.accountActivationUnknown', {
                    message: extractErrorMessage(error),
                  }),
                  usage: `${balance.currentUsage}/${balance.usageLimit}`,
                  email: addedCred.email || undefined,
                  credentialId: addedCred.credentialId,
                }
                return newResults
              })
            }
          } catch (error) {
            unverifiedCount++
            setResults(prev => {
              const newResults = [...prev]
              newResults[i] = {
                ...newResults[i],
                status: 'added_unverified',
                error: t('credentials.accountAddedVerificationFailed', {
                  message: extractErrorMessage(error),
                }),
                email: cred.email?.trim() || undefined,
                credentialId: addedCred.credentialId,
              }
              return newResults
            })
          }
        } catch (error) {
          failCount++
          setResults(prev => {
            const newResults = [...prev]
            newResults[i] = {
              ...newResults[i],
              status: 'failed',
              error: extractErrorMessage(error),
              email: undefined,
            }
            return newResults
          })
        }

        setProgress({ current: i + 1, total: credentials.length })
      }

      // 显示结果
      if (
        failCount === 0
        && unverifiedCount === 0
        && activationUnknownCount === 0
        && duplicateCount === 0
      ) {
        toast.success(t('credentials.toastImportVerifySuccess', { count: successCount }))
      } else {
        toast.info(t('credentials.toastVerifyCompleteSummary', {
          success: successCount,
          unverified: unverifiedCount,
          activationUnknown: activationUnknownCount,
          duplicate: duplicateCount,
          failed: failCount,
        }))
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
      case 'added_unverified':
        return <AlertCircle className="w-5 h-5 text-warn" />
      case 'activation_unknown':
        return <AlertCircle className="w-5 h-5 text-danger" />
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
      case 'added_unverified':
        return t('credentials.statusAddedUnverified')
      case 'activation_unknown':
        return t('credentials.statusActivationUnknown')
      case 'duplicate':
        return t('credentials.statusDuplicate')
      case 'failed':
        return t('credentials.statusFailedNotCreated')
    }
  }

  const handleFormat = () => {
    try {
      setJsonInput(JSON.stringify(JSON.parse(jsonInput), null, 2))
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      toast.error(t('credentials.toastJsonError', { message }))
    }
  }

  const handleClear = () => {
    setJsonInput('')
  }

  const readUploadedFile = (file: File) => {
    const isJson = file.name.endsWith('.json') || file.type === 'application/json'
    if (!isJson) {
      toast.error(t('credentials.importFileTypeError'))
      return
    }
    if (file.size > MAX_FILE_SIZE) {
      toast.error(t('credentials.importFileSizeError'))
      return
    }
    const reader = new FileReader()
    reader.onload = () => {
      setJsonInput(String(reader.result ?? ''))
    }
    reader.onerror = () => {
      toast.error(t('credentials.importFileReadError'))
    }
    reader.readAsText(file)
  }

  const handleSelectFile = () => {
    fileInputRef.current?.click()
  }

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (file) readUploadedFile(file)
    e.target.value = ''
  }

  const handleDragEnter = (e: React.DragEvent) => {
    e.preventDefault()
    if (importing) return
    setDragActive(true)
  }

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault()
    if (importing) return
    setDragActive(true)
  }

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault()
    // 仅当离开整个容器（而非进入内部子元素）时取消高亮
    if (!codeBoxRef.current?.contains(e.relatedTarget as Node)) {
      setDragActive(false)
    }
  }

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault()
    setDragActive(false)
    if (importing) return
    const file = e.dataTransfer.files?.[0]
    if (file) readUploadedFile(file)
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
          <DialogTitle className="flex items-center gap-2.5">
            {t('credentials.batchImportDialogTitle')}
            <span className="rounded-[8px] border border-sky-100 bg-sky-100 px-3 py-2 text-[11px] font-medium tracking-[0.2px] text-sky-700 dark:border-sky-900 dark:bg-sky-950 dark:text-sky-400">
              {t('credentials.importBadge')}
            </span>
          </DialogTitle>
        </DialogHeader>

        <div className="flex-1 overflow-y-auto space-y-4 pb-4">
          <div className="space-y-2">
            {/* 快捷工具栏 */}
            <div className="flex items-center justify-between">
              <label className="text-[11.5px] font-medium text-ink-2">
                {t('credentials.jsonFormatAccountsLabel')}
              </label>
              <div className="flex items-center gap-1">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-7 gap-1.5 px-2 text-[11.5px] text-ink-2"
                  onClick={handleSelectFile}
                  disabled={importing}
                >
                  <FolderOpen className="h-3.5 w-3.5" />
                  {t('credentials.importSelectFile')}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-7 gap-1.5 px-2 text-[11.5px] text-ink-2"
                  onClick={handleFormat}
                  disabled={importing || !jsonInput.trim()}
                >
                  <Braces className="h-3.5 w-3.5" />
                  {t('credentials.importToolbarFormat')}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="h-7 gap-1.5 px-2 text-[11.5px] text-ink-2"
                  onClick={handleClear}
                  disabled={importing || !jsonInput.trim()}
                >
                  <Eraser className="h-3.5 w-3.5" />
                  {t('credentials.importToolbarClear')}
                </Button>
              </div>
            </div>

            {/* Dropzone：输入区容器，支持拖拽 .json 文件 */}
            <div
              ref={codeBoxRef}
              onDragEnter={handleDragEnter}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
              className={`relative rounded-[8px] border bg-surface-2 transition-colors focus-within:border-brand ${
                dragActive ? 'border-brand' : 'border-hairline-2'
              }`}
            >
              <textarea
                placeholder={t('credentials.batchImportPlaceholder')}
                value={jsonInput}
                onChange={(e) => setJsonInput(e.target.value)}
                disabled={importing}
                spellCheck={false}
                className="h-[250px] w-full resize-none rounded-[8px] bg-transparent px-3 py-2.5 font-mono text-[12px] leading-[1.6] text-ink outline-none placeholder:text-ink-3 disabled:cursor-not-allowed disabled:opacity-50"
              />

              {/* 拖拽悬停覆盖层 */}
              {dragActive && (
                <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-[8px] border-2 border-dashed border-brand bg-brand/5">
                  <div className="flex flex-col items-center gap-2">
                    <Download className="h-6 w-6 animate-bounce text-brand" />
                    <span className="text-[13px] font-medium text-brand">
                      {t('credentials.importDropOverlay')}
                    </span>
                  </div>
                </div>
              )}

              <input
                ref={fileInputRef}
                type="file"
                accept=".json,application/json"
                className="hidden"
                onChange={handleFileChange}
                disabled={importing}
              />
            </div>

            {/* 琥珀色验活规则提示卡片 */}
            <div className="flex items-center gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 dark:border-amber-900/60 dark:bg-amber-950/40">
              <Lightbulb className="h-3.5 w-3.5 shrink-0 text-amber-500 dark:text-amber-400" />
              <p className="text-[11px] font-medium leading-[1.55] text-amber-600 dark:text-amber-500/80">
                {t('credentials.batchImportHint')}
              </p>
            </div>
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
                  ⚠ {t('credentials.statUnverifiedLabel')}: {results.filter(r => r.status === 'added_unverified').length}
                </span>
                <span className="text-danger">
                  ! {t('credentials.statActivationUnknownLabel')}: {results.filter(r => r.status === 'activation_unknown').length}
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
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>

        <DialogFooter className="sm:justify-between">
          <Button
            type="button"
            variant="outline"
            onClick={() => window.open(KAM_RELEASES_URL, '_blank', 'noopener,noreferrer')}
          >
            {t('credentials.downloadKamButton')}
          </Button>
          <div className="flex flex-col-reverse gap-2 sm:flex-row">
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
                {importing && <Loader2 className="h-4 w-4 animate-spin" />}
                {t('credentials.startImportVerifyButton')}
              </Button>
            )}
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
