> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# 每日用量统计功能设计

## 概述

在凭据管理页面顶部统计卡片区新增"今日用量"卡片，点击后进入按天汇总的用量统计列表，再点击某天可查看当天所有原始请求日志（最多 2000 条）。

## 后端设计

### 新增 API 端点

**文件：** `src/admin/api_keys.rs`、`src/admin/router.rs`

```
GET /api/admin/usage/daily
→ 返回所有日期的汇总列表，按日期降序排列
响应：[{ date: "2026-05-21", totalRequests: N, totalCost: X, totalCredits: Y }]

GET /api/admin/usage/daily/{date}/records?page=1&page_size=50
→ 返回指定日期（UTC）的分页原始记录，后端硬限最多 2000 条
响应：UsageRecordsPage（复用现有结构）
```

### UsageTracker 新增方法

**文件：** `src/model/usage.rs`

- `get_daily_summaries() -> Vec<DailySummary>`
  - 遍历所有记录，按 `created_at` 的 UTC 日期（`YYYY-MM-DD`）聚合
  - 每条汇总包含：date、totalRequests、totalCost、totalCredits（优先 `credits_used`，fallback `estimated_cost / 0.72`）
  - 按日期降序返回

- `get_records_paged_by_date(date: &str, page: usize, page_size: usize, credential_labels: &HashMap<u64, String>) -> UsageRecordsPage`
  - 过滤 `created_at` UTC 日期等于 `date` 的记录
  - 硬限：`page_size` 最大 2000，总返回不超过 2000 条
  - 按 `created_at` 降序分页

### 新增类型

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySummary {
    pub date: String,           // "2026-05-21"
    pub total_requests: u64,
    pub total_cost: f64,
    pub total_credits: f64,
}
```

## 前端设计

### 新增类型（`src/types/api.ts`）

```ts
export interface DailySummary {
  date: string          // "2026-05-21"
  totalRequests: number
  totalCost: number
  totalCredits: number
}
```

### 新增 API 函数（`src/api/credentials.ts`）

```ts
getDailyUsage(): Promise<DailySummary[]>
getDailyUsageRecords(date: string, page: number, pageSize: number): Promise<UsageRecordsResponse>
```

### 新增 Hooks（`src/hooks/use-credentials.ts`）

```ts
useDailyUsage()           // useQuery, queryKey: ['dailyUsage']
useDailyUsageRecords(date, page, pageSize)
```

### 新增组件

**`src/components/daily-stats-page.tsx`**
- 返回按钮
- 表格列：日期 / 请求数 / Credits / 费用（$USD）
- 按日期降序，每行可点击进入当天详情

**`src/components/daily-detail-page.tsx`**
- 返回按钮（回到日列表）
- 顶部汇总卡片：总请求数 / Credits / 费用
- 日志表格（复用现有列：时间/IP/凭据/模型/Input/Output/费用/Credits）
- 分页控件，page_size=50，最多 2000 条

### dashboard.tsx 改动

**统计卡片区（第 610 行）：**
- 网格从 `md:grid-cols-4` 改为 `md:grid-cols-5`（或保持 4 列，今日用量卡片单独一行）
- 新增"今日用量"卡片，显示今日 Credits 和今日 $USD，可点击

**导航状态扩展：**
```ts
// 'list' = 日列表页, 'YYYY-MM-DD' = 某天详情页, null = 不显示
const [dailyView, setDailyView] = useState<string | null>(null)
```

**主内容区路由逻辑扩展：**
```
credentials tab:
  dailyView === 'list'      → <DailyStatsPage onBack onViewDay />
  dailyView === 'YYYY-MM-DD' → <DailyDetailPage date onBack />
  else                      → 现有凭据列表
```

## 数据流

```
dashboard 今日用量卡片
  ↓ useDailyUsage() → GET /api/admin/usage/daily
  ↓ 取第一条（今日）显示 Credits + $USD

点击卡片 → DailyStatsPage
  ↓ useDailyUsage() → 同上，展示全部日期列表

点击某天 → DailyDetailPage(date)
  ↓ useDailyUsageRecords(date, page, 50) → GET /api/admin/usage/daily/{date}/records
  ↓ 展示当天日志，最多 2000 条
```

## 约束

- 日期以 UTC 为准（与 `created_at` 字段一致）
- 前端日期显示格式化为本地时区（`toLocaleDateString('zh-CN')`）
- 每天日志硬限 2000 条，后端强制，前端无需处理
- 不新增持久化存储，实时从 `UsageTracker` 内存聚合
