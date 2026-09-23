// Copyright (c) 2026 Harllan He. Licensed under MIT.
// Dashboard 凭据列表区块（自 dashboard.tsx 拆出，纯代码搬移）
import { useTranslation } from 'react-i18next'
import { Card, CardContent } from '@/components/ui/card'
import { CredentialActionBar } from '@/components/dashboard/credential-action-bar'
import { AccountToolbar, type AccountStatusFilter } from '@/components/account-toolbar'
import { AccountTable } from '@/components/account-table'
import { AccountPanelFoot } from '@/components/account-panel-foot'
import { AccountRow } from '@/components/account-row'
import type { AccountSortKey, SortDirection } from '@/lib/account-state'
import type { CredentialStatusItem, BalanceResponse } from '@/types/api'

interface CredentialListProps {
  allCredentials: CredentialStatusItem[]
  disabledCredentialCount: number
  // 操作条
  handleRefresh: () => void
  handleQueryCurrentPageInfo: () => void
  queryingInfo: boolean
  queryInfoProgress: { current: number; total: number }
  handleClearAll: () => void
  openKamImport: () => void
  openBatchImport: () => void
  verifying: boolean
  verifyDialogOpen: boolean
  openVerifyDialog: () => void
  verifyProgress: { current: number; total: number }
  openAddDialog: () => void
  // 工具栏
  searchQuery: string
  onSearchChange: (v: string) => void
  statusFilter: AccountStatusFilter
  onStatusFilterChange: (v: AccountStatusFilter) => void
  counts: Record<AccountStatusFilter, number>
  dataUpdatedAt: number
  // 表格
  paged: CredentialStatusItem[]
  sorted: CredentialStatusItem[]
  allPagedSelected: boolean
  somePagedSelected: boolean
  toggleSelectPage: () => void
  sortKey: AccountSortKey | null
  sortDir: SortDirection
  handleSort: (key: AccountSortKey) => void
  isFiltered: boolean
  clearFilters: () => void
  selectedIds: Set<number>
  selectedDisabledCount: number
  handleBatchVerify: () => void
  handleBatchResetFailure: () => void
  handleBatchDelete: () => void
  deselectAll: () => void
  page: number
  totalPages: number
  itemsPerPage: number
  setCurrentPage: (v: number) => void
  balanceMap: Map<number, BalanceResponse>
  loadingBalanceIds: Set<number>
  rpmByCredential: Record<string, number> | undefined
  toggleSelect: (id: number) => void
  setFailureLogCredentialId: (id: number) => void
  setThrottleLogCredentialId: (id: number) => void
  onViewModels: (id: number) => void
  handleViewBalance: (id: number) => void
  setDetailCredentialId: (id: number) => void
  handleRefetchBalance: (id: number) => void
}

export function CredentialList({
  allCredentials,
  disabledCredentialCount,
  handleRefresh,
  handleQueryCurrentPageInfo,
  queryingInfo,
  queryInfoProgress,
  handleClearAll,
  openKamImport,
  openBatchImport,
  verifying,
  verifyDialogOpen,
  openVerifyDialog,
  verifyProgress,
  openAddDialog,
  searchQuery,
  onSearchChange,
  statusFilter,
  onStatusFilterChange,
  counts,
  dataUpdatedAt,
  paged,
  sorted,
  allPagedSelected,
  somePagedSelected,
  toggleSelectPage,
  sortKey,
  sortDir,
  handleSort,
  isFiltered,
  clearFilters,
  selectedIds,
  selectedDisabledCount,
  handleBatchVerify,
  handleBatchResetFailure,
  handleBatchDelete,
  deselectAll,
  page,
  totalPages,
  itemsPerPage,
  setCurrentPage,
  balanceMap,
  loadingBalanceIds,
  rpmByCredential,
  toggleSelect,
  setFailureLogCredentialId,
  setThrottleLogCredentialId,
  onViewModels,
  handleViewBalance,
  setDetailCredentialId,
  handleRefetchBalance,
}: CredentialListProps) {
  const { t } = useTranslation()

  return (
        <div className="space-y-4">
          {/* 操作条（设计稿 .actionbar）：6 项常驻操作，危险操作用竖分隔线隔离并染红 */}
          <CredentialActionBar
            allCredentials={allCredentials}
            disabledCredentialCount={disabledCredentialCount}
            handleRefresh={handleRefresh}
            handleQueryCurrentPageInfo={handleQueryCurrentPageInfo}
            queryingInfo={queryingInfo}
            queryInfoProgress={queryInfoProgress}
            handleClearAll={handleClearAll}
            openKamImport={openKamImport}
            openBatchImport={openBatchImport}
            verifying={verifying}
            verifyDialogOpen={verifyDialogOpen}
            openVerifyDialog={openVerifyDialog}
            verifyProgress={verifyProgress}
            openAddDialog={openAddDialog}
          />

          {/* 工具栏（设计稿 .toolbar）：搜索 + 状态分段筛选 + 更新时间 */}
          <AccountToolbar
            searchQuery={searchQuery}
            onSearchChange={onSearchChange}
            statusFilter={statusFilter}
            onStatusFilterChange={onStatusFilterChange}
            counts={counts}
            dataUpdatedAt={dataUpdatedAt}
          />

          {allCredentials.length === 0 ? (
            <Card>
              <CardContent className="py-8 text-center text-muted-foreground">
                {t('dashboard.noAccounts')}
              </CardContent>
            </Card>
          ) : (
            <AccountTable
              rowCount={paged.length}
              allSelected={allPagedSelected}
              someSelected={somePagedSelected}
              onToggleSelectAll={toggleSelectPage}
              sortKey={sortKey}
              sortDir={sortDir}
              onSort={handleSort}
              isFiltered={isFiltered}
              onClearFilters={clearFilters}
              footer={
                <AccountPanelFoot
                  selectedCount={selectedIds.size}
                  selectedDisabledCount={selectedDisabledCount}
                  onBatchVerify={handleBatchVerify}
                  onBatchRestore={handleBatchResetFailure}
                  onBatchDelete={handleBatchDelete}
                  onDeselectAll={deselectAll}
                  totalCount={sorted.length}
                  isFiltered={isFiltered}
                  page={page}
                  totalPages={totalPages}
                  itemsPerPage={itemsPerPage}
                  onPageChange={setCurrentPage}
                />
              }
            >
              {paged.map((credential, index) => (
                <AccountRow
                  key={credential.id}
                  credential={credential}
                  sequence={(page - 1) * itemsPerPage + index + 1}
                  balance={balanceMap.get(credential.id) ?? null}
                  loadingBalance={loadingBalanceIds.has(credential.id)}
                  rpm={rpmByCredential?.[String(credential.id)] ?? 0}
                  selected={selectedIds.has(credential.id)}
                  onToggleSelect={() => toggleSelect(credential.id)}
                  onViewFailureLog={(id) => setFailureLogCredentialId(id)}
                  onViewThrottleLog={(id) => setThrottleLogCredentialId(id)}
                  onViewModels={onViewModels}
                  onViewBalance={handleViewBalance}
                  onViewDetail={(id) => setDetailCredentialId(id)}
                  onRefetchBalance={handleRefetchBalance}
                />
              ))}
            </AccountTable>
          )}
        </div>
  )
}
