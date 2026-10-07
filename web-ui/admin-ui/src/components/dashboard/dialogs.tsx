// Copyright (c) 2026 Harllan He. Licensed under MIT.
// Dashboard 对话框群区块（自 dashboard.tsx 拆出，纯代码搬移）
import { BalanceDialog } from '@/components/balance-dialog'
import { ModelsDialog } from '@/components/models-dialog'
import { AddCredentialDialog } from '@/components/add-credential-dialog'
import { BatchImportDialog } from '@/components/batch-import-dialog'
import { BatchVerifyDialog } from '@/components/batch-verify-dialog'
import type { VerifyResult } from '@/components/batch-verify-dialog'
import type { CredentialStatusItem } from '@/types/api'

interface DashboardDialogsProps {
  selectedCredentialId: number | null
  /** 当前选中账号的状态项；列表未加载完 / 账号已删除时为 null */
  selectedCredential: CredentialStatusItem | null
  balanceDialogOpen: boolean
  setBalanceDialogOpen: (v: boolean) => void
  modelsCredentialId: number | null
  modelsDialogOpen: boolean
  setModelsDialogOpen: (v: boolean) => void
  addDialogOpen: boolean
  setAddDialogOpen: (v: boolean) => void
  batchImportDialogOpen: boolean
  setBatchImportDialogOpen: (v: boolean) => void
  verifyDialogOpen: boolean
  setVerifyDialogOpen: (v: boolean) => void
  verifying: boolean
  verifyProgress: { current: number; total: number }
  verifyResults: Map<number, VerifyResult>
  handleCancelVerify: () => void
}

export function DashboardDialogs({
  selectedCredentialId,
  selectedCredential,
  balanceDialogOpen,
  setBalanceDialogOpen,
  modelsCredentialId,
  modelsDialogOpen,
  setModelsDialogOpen,
  addDialogOpen,
  setAddDialogOpen,
  batchImportDialogOpen,
  setBatchImportDialogOpen,
  verifyDialogOpen,
  setVerifyDialogOpen,
  verifying,
  verifyProgress,
  verifyResults,
  handleCancelVerify,
}: DashboardDialogsProps) {
  return (
    <>
      {/* 余额对话框 */}
      <BalanceDialog
        credentialId={selectedCredentialId}
        credential={selectedCredential}
        open={balanceDialogOpen}
        onOpenChange={setBalanceDialogOpen}
      />

      {/* 支持模型对话框 */}
      <ModelsDialog
        credentialId={modelsCredentialId}
        open={modelsDialogOpen}
        onOpenChange={setModelsDialogOpen}
      />

      {/* 添加凭据对话框 */}
      <AddCredentialDialog
        open={addDialogOpen}
        onOpenChange={setAddDialogOpen}
      />

      {/* 批量导入对话框 */}
      <BatchImportDialog
        open={batchImportDialogOpen}
        onOpenChange={setBatchImportDialogOpen}
      />

      {/* 批量验活对话框 */}
      <BatchVerifyDialog
        open={verifyDialogOpen}
        onOpenChange={setVerifyDialogOpen}
        verifying={verifying}
        progress={verifyProgress}
        results={verifyResults}
        onCancel={handleCancelVerify}
      />
    </>
  )
}
