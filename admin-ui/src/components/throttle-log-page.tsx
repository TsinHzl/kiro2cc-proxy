// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { AlertTriangle } from 'lucide-react'
import { EventLogPage, PAGE_SIZE } from '@/components/event-log-page'
import { useThrottleLogs } from '@/hooks/use-credentials'

interface ThrottleLogPageProps {
  credentialId: number
  onBack: () => void
}

/** 限流日志页：展示全部交由 EventLogPage，此处只注入取数与色调/文案差异 */
export function ThrottleLogPage({ credentialId, onBack }: ThrottleLogPageProps) {
  const [page, setPage] = useState(1)
  const { data, isLoading, refetch } = useThrottleLogs(credentialId, page, PAGE_SIZE)

  return (
    <EventLogPage
      credentialId={credentialId}
      onBack={onBack}
      page={page}
      onPage={setPage}
      data={data}
      isLoading={isLoading}
      refetch={refetch}
      tone="warn"
      icon={AlertTriangle}
      badgeKey="logs.throttleLogBadge"
      totalLabelKey="logs.totalThrottleCount"
      cumulativeLabelKey="logs.last7DaysCount"
      cumulativeHintKey="logs.throttleCountHint"
      cumulativeOf={(c) => c.throttleCount}
      titleKey="logs.throttleEventsTitle"
      emptyKey="logs.emptyNoThrottleRecords"
    />
  )
}
