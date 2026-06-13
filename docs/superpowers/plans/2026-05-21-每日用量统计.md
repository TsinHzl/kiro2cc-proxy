# Daily Usage Stats Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在凭据管理页顶部新增"今日用量"卡片，点击进入按天汇总列表，再点击某天查看当天原始日志（最多 2000 条）。

**Architecture:** 后端在 `UsageTracker` 新增两个方法做内存聚合，暴露两个新 REST 端点；前端新增类型/API/Hook/两个页面组件，并在 dashboard 扩展导航状态。

**Tech Stack:** Rust (axum, chrono, serde), React 18, TypeScript, TanStack Query, Tailwind CSS, shadcn/ui

---

## File Map

| 操作 | 文件 |
|------|------|
| Modify | `src/model/usage.rs` |
| Modify | `src/admin/api_keys.rs` |
| Modify | `src/admin/router.rs` |
| Modify | `admin-ui/src/types/api.ts` |
| Modify | `admin-ui/src/api/credentials.ts` |
| Modify | `admin-ui/src/hooks/use-credentials.ts` |
| Create | `admin-ui/src/components/daily-stats-page.tsx` |
| Create | `admin-ui/src/components/daily-detail-page.tsx` |
| Modify | `admin-ui/src/components/dashboard.tsx` |

---

### Task 1: 后端 — UsageTracker 新增 DailySummary 类型和 get_daily_summaries()

**Files:**
- Modify: `src/model/usage.rs`

- [ ] **Step 1: 在 usage.rs 末尾添加 DailySummary 结构体和 get_daily_summaries 方法**

在 `src/model/usage.rs` 文件末尾（第 463 行之后）追加：

```rust
/// 按日期汇总的用量
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySummary {
    pub date: String,
    pub total_requests: u64,
    pub total_cost: f64,
    pub total_credits: f64,
}

impl UsageTracker {
    /// 按 UTC 日期聚合所有记录，返回按日期降序的汇总列表
    pub fn get_daily_summaries(&self) -> Vec<DailySummary> {
        use std::collections::BTreeMap;
        let records = self.records.read();
        let mut map: BTreeMap<String, (u64, f64, f64)> = BTreeMap::new();
        for r in records.iter() {
            let date = r.created_at.format("%Y-%m-%d").to_string();
            let entry = map.entry(date).or_default();
            entry.0 += 1;
            entry.1 += r.estimated_cost;
            entry.2 += r.credits_used.unwrap_or(r.estimated_cost / 0.72);
        }
        let mut result: Vec<DailySummary> = map
            .into_iter()
            .map(|(date, (reqs, cost, credits))| DailySummary {
                date,
                total_requests: reqs,
                total_cost: cost,
                total_credits: credits,
            })
            .collect();
        result.sort_by(|a, b| b.date.cmp(&a.date));
        result
    }

    /// 分页查询指定 UTC 日期的原始记录，硬限总量 2000 条
    pub fn get_records_paged_by_date(
        &self,
        date: &str,
        page: usize,
        page_size: usize,
        credential_labels: &std::collections::HashMap<u64, String>,
    ) -> UsageRecordsPage {
        const MAX_TOTAL: usize = 2000;
        let page_size = page_size.min(500).max(1);

        let owned: Vec<UsageRecord> = {
            let records = self.records.read();
            records
                .iter()
                .filter(|r| r.created_at.format("%Y-%m-%d").to_string() == date)
                .cloned()
                .collect()
        };

        let mut sorted = owned;
        sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        sorted.truncate(MAX_TOTAL);

        let total = sorted.len();
        if total == 0 {
            return UsageRecordsPage {
                records: vec![],
                total: 0,
                page: 1,
                page_size,
                total_pages: 0,
            };
        }

        let total_pages = (total + page_size - 1) / page_size;
        let page = page.max(1).min(total_pages);
        let start = (page - 1) * page_size;

        let items: Vec<UsageRecordItem> = sorted
            .into_iter()
            .skip(start)
            .take(page_size)
            .map(|r| {
                let credential_label = r
                    .credential_id
                    .and_then(|cid| credential_labels.get(&cid).cloned());
                UsageRecordItem {
                    model: r.model,
                    input_tokens: r.input_tokens,
                    output_tokens: r.output_tokens,
                    estimated_cost: r.estimated_cost,
                    credits_used: r.credits_used,
                    created_at: r.created_at,
                    credential_id: r.credential_id,
                    credential_label,
                    client_ip: r.client_ip,
                }
            })
            .collect();

        UsageRecordsPage {
            records: items,
            total,
            page,
            page_size,
            total_pages,
        }
    }
}
```

- [ ] **Step 2: 编译验证**

```bash
cd /path/to/kiro2cc-proxy
cargo check 2>&1 | grep -E "error|warning: unused"
```

Expected: 无 error

- [ ] **Step 3: Commit**

```bash
git add src/model/usage.rs
git commit -m "feat: add DailySummary type and daily aggregation methods to UsageTracker"
```

---

### Task 4: 前端 — 新增类型和 API 函数

**Files:**
- Modify: `admin-ui/src/types/api.ts`
- Modify: `admin-ui/src/api/credentials.ts`
- Modify: `admin-ui/src/hooks/use-credentials.ts`

- [ ] **Step 1: 在 types/api.ts 末尾追加 DailySummary 类型**

```ts
// 每日用量汇总
export interface DailySummary {
  date: string          // "2026-05-21" UTC
  totalRequests: number
  totalCost: number
  totalCredits: number
}
```

- [ ] **Step 2: 在 api/credentials.ts 顶部 import 中加入 DailySummary，末尾追加两个函数**

顶部 import 加入：
```ts
import type {
  // ...existing imports...
  DailySummary,
} from '@/types/api'
```

末尾追加：
```ts
// ============ 每日用量统计 ============

export async function getDailyUsage(): Promise<DailySummary[]> {
  const { data } = await api.get<DailySummary[]>('/usage/daily')
  return data
}

export async function getDailyUsageRecords(
  date: string,
  page: number,
  pageSize: number
): Promise<UsageRecordsResponse> {
  const { data } = await api.get<UsageRecordsResponse>(
    `/usage/daily/${date}/records`,
    { params: { page, page_size: pageSize } }
  )
  return data
}
```

- [ ] **Step 3: 在 hooks/use-credentials.ts 顶部 import 加入新函数，末尾追加两个 Hook**

顶部 import 加入：
```ts
import {
  // ...existing imports...
  getDailyUsage,
  getDailyUsageRecords,
} from '@/api/credentials'
```

末尾追加：
```ts
// ============ 每日用量统计 Hooks ============

export function useDailyUsage() {
  return useQuery({
    queryKey: ['dailyUsage'],
    queryFn: getDailyUsage,
    refetchInterval: 60000,
  })
}

export function useDailyUsageRecords(date: string, page: number, pageSize = 50) {
  return useQuery({
    queryKey: ['dailyUsageRecords', date, page, pageSize],
    queryFn: () => getDailyUsageRecords(date, page, pageSize),
    enabled: !!date,
  })
}
```

- [ ] **Step 4: 编译验证**

```bash
cd admin-ui && npx tsc --noEmit 2>&1 | grep "error"
```

Expected: 无 error

- [ ] **Step 5: Commit**

```bash
git add admin-ui/src/types/api.ts admin-ui/src/api/credentials.ts admin-ui/src/hooks/use-credentials.ts
git commit -m "feat: add DailySummary type, API functions, and hooks for daily usage stats"
```

---

### Task 5: 前端 — 新增 DailyStatsPage 组件

**Files:**
- Create: `admin-ui/src/components/daily-stats-page.tsx`

- [ ] **Step 1: 创建 daily-stats-page.tsx**

```tsx
import { ArrowLeft, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { useDailyUsage } from '@/hooks/use-credentials'

interface DailyStatsPageProps {
  onBack: () => void
  onViewDay: (date: string) => void
}

function formatDate(dateStr: string): string {
  return new Date(dateStr + 'T00:00:00Z').toLocaleDateString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit', timeZone: 'UTC',
  })
}

export function DailyStatsPage({ onBack, onViewDay }: DailyStatsPageProps) {
  const { data, isLoading, refetch } = useDailyUsage()

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="sm" onClick={onBack} className="gap-1">
          <ArrowLeft className="h-4 w-4" />
          返回
        </Button>
        <h2 className="text-xl font-semibold">每日用量统计</h2>
        <Button variant="ghost" size="sm" onClick={() => refetch()} disabled={isLoading} className="ml-auto">
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </Button>
      </div>

      <Card>
        <CardContent className="p-0">
          {isLoading ? (
            <div className="py-8 text-center text-muted-foreground text-sm">加载中...</div>
          ) : !data || data.length === 0 ? (
            <div className="py-8 text-center text-muted-foreground text-sm">暂无用量记录</div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b bg-muted/50">
                    <th className="text-left px-4 py-2 font-medium text-muted-foreground">日期</th>
                    <th className="text-right px-4 py-2 font-medium text-muted-foreground">请求数</th>
                    <th className="text-right px-4 py-2 font-medium text-muted-foreground">Credits</th>
                    <th className="text-right px-4 py-2 font-medium text-muted-foreground">费用 ($)</th>
                  </tr>
                </thead>
                <tbody>
                  {data.map((row) => (
                    <tr
                      key={row.date}
                      className="border-b last:border-0 hover:bg-muted/30 transition-colors cursor-pointer"
                      onClick={() => onViewDay(row.date)}
                    >
                      <td className="px-4 py-2 font-medium">{formatDate(row.date)}</td>
                      <td className="px-4 py-2 text-right tabular-nums">{row.totalRequests}</td>
                      <td className="px-4 py-2 text-right tabular-nums font-medium text-blue-600 dark:text-blue-400">
                        {row.totalCredits.toFixed(4)}
                      </td>
                      <td className="px-4 py-2 text-right tabular-nums font-medium text-orange-600 dark:text-orange-400">
                        ${row.totalCost.toFixed(4)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
```

- [ ] **Step 2: 编译验证**

```bash
cd admin-ui && npx tsc --noEmit 2>&1 | grep "error"
```

Expected: 无 error

- [ ] **Step 3: Commit**

```bash
git add admin-ui/src/components/daily-stats-page.tsx
git commit -m "feat: add DailyStatsPage component"
```

---

### Task 6: 前端 — 新增 DailyDetailPage 组件

**Files:**
- Create: `admin-ui/src/components/daily-detail-page.tsx`

- [ ] **Step 1: 创建 daily-detail-page.tsx**

```tsx
import { useState } from 'react'
import { ArrowLeft, BarChart3, DollarSign, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { useDailyUsageRecords } from '@/hooks/use-credentials'
import { useIpGeo } from '@/hooks/use-ip-geo'

interface DailyDetailPageProps {
  date: string
  onBack: () => void
}

const MODEL_COLORS: Record<string, string> = {
  opus: 'text-purple-600 dark:text-purple-400',
  sonnet: 'text-blue-600 dark:text-blue-400',
  haiku: 'text-green-600 dark:text-green-400',
}

function getModelColor(model: string): string {
  const lower = model.toLowerCase()
  for (const [key, cls] of Object.entries(MODEL_COLORS)) {
    if (lower.includes(key)) return cls
  }
  return 'text-muted-foreground'
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

function formatCost(cost: number): string {
  return `$${cost.toFixed(4)}`
}

function formatDateTime(dateStr: string): string {
  return new Date(dateStr).toLocaleString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}

function formatDateLabel(dateStr: string): string {
  return new Date(dateStr + 'T00:00:00Z').toLocaleDateString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit', timeZone: 'UTC',
  })
}

const PAGE_SIZE = 50

export function DailyDetailPage({ date, onBack }: DailyDetailPageProps) {
  const [page, setPage] = useState(1)
  const { data: recordsData, isLoading, refetch } = useDailyUsageRecords(date, page, PAGE_SIZE)

  const records = recordsData?.records ?? []
  const pageIps = records.map((r) => r.clientIp).filter((ip): ip is string => !!ip)
  const geoMap = useIpGeo(pageIps)

  const totalCost = records.reduce((s, r) => s + r.estimatedCost, 0)
  const totalCredits = records.reduce((s, r) => s + (r.creditsUsed ?? r.estimatedCost / 0.72), 0)

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="sm" onClick={onBack} className="gap-1">
          <ArrowLeft className="h-4 w-4" />
          返回
        </Button>
        <span className="font-semibold">{formatDateLabel(date)} 用量详情</span>
      </div>

      <div className="grid gap-4 grid-cols-2 sm:grid-cols-3">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
              <BarChart3 className="h-3.5 w-3.5" />
              总请求数
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{recordsData?.total ?? 0}</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-1">
              <DollarSign className="h-3.5 w-3.5" />
              本页费用
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-orange-600 dark:text-orange-400">
              {formatCost(totalCost)}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">本页 Credits</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-blue-600 dark:text-blue-400">
              {totalCredits.toFixed(4)}
            </div>
          </CardContent>
        </Card>
      </div>

      <div>
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-sm font-medium text-muted-foreground">
            请求日志
            {recordsData && <span className="ml-1">（共 {recordsData.total} 条）</span>}
          </h3>
          <Button variant="ghost" size="sm" onClick={() => refetch()} disabled={isLoading}>
            <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
          </Button>
        </div>

        <Card>
          <CardContent className="p-0">
            {isLoading ? (
              <div className="py-8 text-center text-muted-foreground text-sm">加载中...</div>
            ) : records.length === 0 ? (
              <div className="py-8 text-center text-muted-foreground text-sm">暂无请求记录</div>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/50">
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">时间</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">IP</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">凭据</th>
                      <th className="text-left px-4 py-2 font-medium text-muted-foreground">模型</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">Input</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">Output</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">费用</th>
                      <th className="text-right px-4 py-2 font-medium text-muted-foreground">Credits</th>
                    </tr>
                  </thead>
                  <tbody>
                    {records.map((record, idx) => {
                      const geo = record.clientIp ? geoMap.get(record.clientIp) : undefined
                      return (
                        <tr key={`${record.createdAt}-${record.model}-${idx}`} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                          <td className="px-4 py-2 text-xs text-muted-foreground whitespace-nowrap">
                            {formatDateTime(record.createdAt)}
                          </td>
                          <td className="px-4 py-2 text-xs text-muted-foreground whitespace-nowrap">
                            {record.clientIp ? (
                              <span title={record.clientIp}>
                                <span className="font-mono">{geo?.displayIp ?? record.clientIp}</span>
                                {geo?.country && <span className="ml-1 text-muted-foreground/60">{geo.country}·{geo.city}</span>}
                              </span>
                            ) : '—'}
                          </td>
                          <td className="px-4 py-2 text-xs text-muted-foreground max-w-[120px] truncate" title={record.credentialLabel}>
                            {record.credentialLabel ?? '—'}
                          </td>
                          <td className={`px-4 py-2 font-mono text-xs ${getModelColor(record.model)}`}>
                            {record.model}
                          </td>
                          <td className="px-4 py-2 text-right tabular-nums">{formatTokens(record.inputTokens)}</td>
                          <td className="px-4 py-2 text-right tabular-nums">{formatTokens(record.outputTokens)}</td>
                          <td className="px-4 py-2 text-right tabular-nums font-medium text-orange-600 dark:text-orange-400">
                            {formatCost(record.estimatedCost)}
                          </td>
                          <td className="px-4 py-2 text-right tabular-nums font-medium text-blue-600 dark:text-blue-400">
                            {record.creditsUsed != null ? record.creditsUsed.toFixed(4) : (record.estimatedCost / 0.72).toFixed(4)}
                            {record.creditsUsed != null && <span className="ml-1 text-xs text-green-500">✓</span>}
                          </td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </CardContent>
        </Card>

        {recordsData && recordsData.totalPages > 1 && (
          <div className="flex justify-center items-center gap-4 mt-4">
            <Button variant="outline" size="sm" onClick={() => setPage((p) => Math.max(1, p - 1))} disabled={page === 1}>
              上一页
            </Button>
            <span className="text-sm text-muted-foreground">第 {page} / {recordsData.totalPages} 页</span>
            <Button variant="outline" size="sm" onClick={() => setPage((p) => Math.min(recordsData.totalPages, p + 1))} disabled={page === recordsData.totalPages}>
              下一页
            </Button>
          </div>
        )}
      </div>
    </div>
  )
}
```

- [ ] **Step 2: 编译验证**

```bash
cd admin-ui && npx tsc --noEmit 2>&1 | grep "error"
```

Expected: 无 error

- [ ] **Step 3: Commit**

```bash
git add admin-ui/src/components/daily-detail-page.tsx
git commit -m "feat: add DailyDetailPage component"
```

---

### Task 7: 前端 — 更新 dashboard.tsx

**Files:**
- Modify: `admin-ui/src/components/dashboard.tsx`

- [ ] **Step 1: 添加 import**

在 dashboard.tsx 顶部 import 区域添加：
```ts
import { DailyStatsPage } from '@/components/daily-stats-page'
import { DailyDetailPage } from '@/components/daily-detail-page'
import { useDailyUsage } from '@/hooks/use-credentials'
```

- [ ] **Step 2: 添加 dailyView 状态和 useDailyUsage hook**

在现有 `useState` 声明区域（约第 30-51 行）添加：
```ts
const [dailyView, setDailyView] = useState<string | null>(null)
```

在现有 hook 调用区域（约第 61-65 行）添加：
```ts
const { data: dailyUsageData } = useDailyUsage()
```

- [ ] **Step 3: 计算今日数据**

在 `const queryClient = useQueryClient()` 之后添加：
```ts
const todayUtc = new Date().toISOString().slice(0, 10)
const todayStats = dailyUsageData?.find((d) => d.date === todayUtc) ?? null
```

- [ ] **Step 4: 更新统计卡片网格**

找到第 610 行：
```tsx
<div className="grid gap-4 md:grid-cols-4 mb-6">
```
改为：
```tsx
<div className="grid gap-4 grid-cols-2 md:grid-cols-5 mb-6">
```

- [ ] **Step 5: 在"全局 RPM"卡片之前插入"今日用量"卡片**

找到"全局 RPM"卡片（`全局 RPM` 文字所在的 `<Card>`），在其之前插入：
```tsx
<Card
  className="cursor-pointer hover:border-primary/50 transition-colors"
  onClick={() => setDailyView('list')}
>
  <CardHeader className="pb-2">
    <CardTitle className="text-sm font-medium text-muted-foreground">
      今日用量
    </CardTitle>
  </CardHeader>
  <CardContent>
    {todayStats ? (
      <div>
        <div className="text-xl font-bold text-blue-600 dark:text-blue-400">
          {todayStats.totalCredits.toFixed(2)} Credits
        </div>
        <div className="text-sm text-orange-600 dark:text-orange-400 font-medium mt-0.5">
          ${todayStats.totalCost.toFixed(4)}
        </div>
      </div>
    ) : (
      <div className="text-2xl font-bold text-muted-foreground">—</div>
    )}
  </CardContent>
</Card>
```

- [ ] **Step 6: 在凭据 tab 路由逻辑中插入 dailyView 分支**

找到（约第 602 行）：
```tsx
) : detailCredentialId !== null ? (
```

在其之前插入：
```tsx
) : dailyView === 'list' ? (
  <DailyStatsPage
    onBack={() => setDailyView(null)}
    onViewDay={(date) => setDailyView(date)}
  />
) : dailyView !== null ? (
  <DailyDetailPage
    date={dailyView}
    onBack={() => setDailyView('list')}
  />
```

- [ ] **Step 7: 在 tab 切换时重置 dailyView**

三个 tab 按钮的 onClick 各加上 `setDailyView(null)`：
```tsx
onClick={() => { setActiveTab('credentials'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) }}
onClick={() => { setActiveTab('apikeys'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) }}
onClick={() => { setActiveTab('settings'); setDetailKeyId(null); setDetailCredentialId(null); setDailyView(null) }}
```

- [ ] **Step 8: 编译并构建验证**

```bash
cd admin-ui && npm run build 2>&1 | tail -10
```

Expected: `✓ built in X.XXs`，无 error

- [ ] **Step 9: Commit**

```bash
git add admin-ui/src/components/dashboard.tsx
git commit -m "feat: add today usage card and daily stats navigation to dashboard"
```

---

### Task 8: 后端完整构建验证

- [ ] **Step 1: 完整构建**

```bash
cargo build 2>&1 | grep -E "^error"
```

Expected: 无输出（无 error）

### Task 2: 后端 — 新增两个 HTTP 处理器

**Files:**
- Modify: `src/admin/api_keys.rs`

- [ ] **Step 1: 在 api_keys.rs 末尾（get_rpm 之后）追加两个处理器**

```rust
/// GET /api/admin/usage/daily
/// 获取所有日期的用量汇总（按日期降序）
pub async fn get_daily_usage(State(state): State<AdminState>) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    Json(tracker.get_daily_summaries()).into_response()
}

/// GET /api/admin/usage/daily/{date}/records?page=1&page_size=50
/// 分页获取指定日期的原始请求记录（最多 2000 条）
pub async fn get_daily_usage_records(
    State(state): State<AdminState>,
    Path(date): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(tracker) = &state.usage_tracker else {
        let error = AdminErrorResponse::internal_error("用量追踪未启用");
        return (StatusCode::SERVICE_UNAVAILABLE, Json(error)).into_response();
    };
    let page = params.get("page").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
    let page_size = params.get("page_size").and_then(|v| v.parse::<usize>().ok()).unwrap_or(50);
    let labels = state.service.credential_labels();
    Json(tracker.get_records_paged_by_date(&date, page, page_size, &labels)).into_response()
}
```

- [ ] **Step 2: 编译验证**

```bash
cargo check 2>&1 | grep "error"
```

Expected: 无 error

- [ ] **Step 3: Commit**

```bash
git add src/admin/api_keys.rs
git commit -m "feat: add get_daily_usage and get_daily_usage_records handlers"
```

---

### Task 3: 后端 — 注册路由

**Files:**
- Modify: `src/admin/router.rs`

- [ ] **Step 1: 在 router.rs 的 import 行添加新处理器**

找到第 10-11 行：
```rust
        create_api_key, delete_api_key, get_all_usage, get_credential_usage_records,
        get_key_usage, get_key_usage_records, get_rpm, get_server_info, list_api_keys,
        reset_key_usage, update_api_key,
```

改为：
```rust
        create_api_key, delete_api_key, get_all_usage, get_credential_usage_records,
        get_daily_usage, get_daily_usage_records, get_key_usage, get_key_usage_records,
        get_rpm, get_server_info, list_api_keys, reset_key_usage, update_api_key,
```

- [ ] **Step 2: 在路由表中注册新路由**

找到 `.route("/rpm", get(get_rpm))` 这行，在其后添加：
```rust
        .route("/usage/daily", get(get_daily_usage))
        .route("/usage/daily/{date}/records", get(get_daily_usage_records))
```

- [ ] **Step 3: 编译验证**

```bash
cargo check 2>&1 | grep "error"
```

Expected: 无 error

- [ ] **Step 4: Commit**

```bash
git add src/admin/router.rs
git commit -m "feat: register /usage/daily routes in admin router"
```

---
