// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { XCircle } from 'lucide-react'
import { EventLogPage, PAGE_SIZE } from '@/components/event-log-page'
import { useFailureLogs } from '@/hooks/use-credentials'

interface FailureLogPageProps {
  credentialId: number
  onBack: () => void
}

/** 失败日志页：展示全部交由 EventLogPage，此处只注入取数与色调/文案差异 */
export function FailureLogPage({ credentialId, onBack }: FailureLogPageProps) {
  const [page, setPage] = useState(1)
  const { data, isLoading, refetch } = useFailureLogs(credentialId, page, PAGE_SIZE)

  return (
    <EventLogPage
      credentialId={credentialId}
      onBack={onBack}
      page={page}
      onPage={setPage}
      data={data}
      isLoading={isLoading}
      refetch={refetch}
      tone="danger"
      icon={XCircle}
      badgeKey="logs.failureLogBadge"
      totalLabelKey="logs.recordCountLabel"
      cumulativeLabelKey="logs.cumulativeFailureCount"
      cumulativeHintKey="logs.includesPurgedHint"
      cumulativeOf={(c) => c.failureCount}
      titleKey="logs.failureEventsTitle"
      emptyKey="logs.emptyNoFailureRecords"
    />
  )
}
