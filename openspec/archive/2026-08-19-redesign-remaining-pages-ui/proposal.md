# 变更提案：redesign-remaining-pages-ui

## 背景

`redesign-account-management-ui`（commit `94a8739`）完成了设计令牌系统重写与账号管理页表格化改版，但落地范围仅覆盖「令牌 + 侧栏外壳 + 账号管理页主体」。其余 11 个页面组件与 5 个对话框仍是改版前的 shadcn 卡片式布局，只被动继承了新令牌取值，形成两类断层：

1. **页头断层** —— 仅账号管理页具备「面包屑 + 19px 标题 + 同基线副标题 + 右侧动作区」的页头结构，其他页面是裸 `<h2 className="text-xl font-semibold">`
2. **组件断层** —— 其他页面仍用 `Card` / `CardContent` / `Badge` / `Progress` 原语，与新建立的 `.panel` / `.tag` / `.bar` / `.metric` 视觉语言不一致；表格无 `.tscroll` 粘性表头，列表无 `.panel-foot` 汇总条

设计稿目录 `/Users/MacBook/Downloads/kiro2ccproxy-redesign/` 提供了 `option-a-console.html`（2574 行，含 6 个非账号管理页面的完整原型与 7 组新组件类）以及 12 张双主题 PNG 对稿图。

## 目标范围

### 在范围内

**A 组 —— 有设计稿的 6 个页面（严格对稿 `option-a-console.html`）**

| 页面 | 现有组件（行数） | 设计稿段落 | 目标结构 |
|---|---|---|---|
| API Keys | `api-keys-panel.tsx`（1094） | 1084–1345 | `.conn` 连接卡 + 4 指标卡 + `.actionbar` + `.toolbar` + 10 列表格 + `.pager` |
| 每日统计 | `daily-stats-page.tsx`（90）+ `daily-credits-trend-chart.tsx`（142） | 1351–1610 | 4 指标卡 + `.chart-card` 面积折线图（零消耗区间带 + 峰值标注）+ `.toolbar` + `.panel.is-static` 表格 |
| 支持模型 | `model-list-page.tsx`（111） | 1617–2013 | 4 指标卡 + `.actionbar` + `.toolbar` + 家族分组表格（`.mult-bar` 倍率条） |
| 实时日志 | `log-viewer-page.tsx`（420） | 2019–2122 | 4 指标卡 + `.logbar`（搜索 + 级别分段 + 双开关）+ `.logstream` 等宽日志流 + `.panel-foot` |
| 更新日志 | `changelog-page.tsx`（67） | 2128–2274 | `.tl-idx` 版本索引（sticky）+ `.tl-main` 时间线，条目按「新增 / 修复 / 变更」三类分区 |
| 设置 | `settings-panel.tsx`（154） | 2281–2536 | `.set-wrap` 分区式 `.set-card` + `.set-row`（键 / 描述 / 控件三列） |

**B 组 —— 无设计稿的 6 个页面（沿用 A 组与账号管理页已建立的模式推导）**

`credential-detail-page.tsx`（316）、`api-key-detail-page.tsx`（282）、`daily-detail-page.tsx`（215）、`throttle-log-page.tsx`（182）、`failure-log-page.tsx`（182）、`login-page.tsx`（65）

推导基准：统一页头结构、`.panel` + `.tscroll` 表格、`.tag` 状态徽章、`.metric` 指标卡、`.btn` / `.iconbtn` 按钮、`.panel-foot` 汇总条。

**C 组 —— 共享层**

- `index.css`：补充无法用 Tailwind 原子类表达的组件类（`.logstream` 等宽流、图表绘图坐标层）
- ui 原语令牌对齐：`card` / `badge` / `progress` / `dialog` / `input` / `button`
- 5 个对话框内表单字段统一为设计稿 `.field`（`w-l` / `w-s` / `u` 三档）：`add-credential-dialog`、`batch-import-dialog`、`kam-import-dialog`、`batch-verify-dialog`、`balance-dialog`

### 不在范围内

- **任何后端改动** —— Rust 侧 `src/admin/` 零改动，不新增字段、不改 API 契约、不新增端点
- `user-ui/`（用户端前端）
- 账号管理页主体 —— 已在上一变更完成；仅在共享 ui 原语调整时被动受影响
- **对话框布局重排** —— 设计稿未提供 dialog 原型，仅改字段视觉，不动 DOM 结构与提交逻辑
- **设计稿中依赖后端新能力的交互** —— 模型倍率编辑、配置项「保存并重载」、导出 CSV / JSON、吊销所有 Key、清空统计、重置为默认配置

## 技术方案

沿用上一变更拍定的两条硬约束：**零后端改动** + **数据缺口用现有数据优雅降级**。

### 数据缺口与降级策略（已逐项核对 `admin-ui/src/types/api.ts`）

| 页面 | 设计稿展示位 | 现有字段 | 降级策略 |
|---|---|---|---|
| API Keys | 权限范围 | 无 `scope` | 改为展示 `boundCredentialIds` 绑定账号数；空数组显示「全部账号」 |
| API Keys | 日配额 | `spendingLimit` + `limitUnit` | 语义改为「消费上限」；`null` 显示「不限」 |
| API Keys | 请求 / 失败 | `UsageSummary.totalRequests`，无失败计数 | 该列保留但仅渲染请求数（失败位省略），列头改为「请求数」；**属单元格内降级，不删列，故表格仍为 10 列** |
| API Keys | RPM 上限 | `RpmSnapshot.byApiKey`（实时值，非上限） | 列头改为「实时 RPM」 |
| API Keys | 最后使用 | 无 `lastUsedAt` | 改为 `activatedAt` ?? `createdAt`，列头改为「启用时间」 |
| API Keys | Base URL | `window.location.origin` | 直接取用，无缺口 |
| 支持模型 | 上下文窗口 | 无 `context_window` | 该列不渲染，8 列降为 7 列 |
| 支持模型 | 状态「已弃用」 | 无 `deprecated` | 「已弃用」分段筛选不渲染；状态列改表达倍率同步态（「已同步」/「倍率缺失」）——后端回退本地静态表时 `rate_multiplier` 全为 `null`，恒为「可用」的列零信息量。详见 design.md 决策 9 |
| 支持模型 | 默认模型 | 无 `default` 字段 | 第 3 张指标卡改为「倍率数据覆盖」（已同步 N / 共 M + 环形图）。详见 design.md 决策 9 |
| 支持模型 | 模型家族分组 | 由 `ModelItem.id` 前缀客户端推导 | 无缺口 |
| 每日统计 | 失败 / 成功率 / 主要模型 / Token | `DailySummary` 仅 4 字段 | 表格降为 4 列（日期 / 星期 / 积分消耗 / 请求数），逐模型明细保留在每日明细页 |
| 实时日志 | PID / 运行时长 / 服务端日志级别 | 无对应接口 | 第 4 张指标卡改为「已观测级别分布」 |
| 实时日志 | 折叠重复 / 下载日志 / 清空缓冲 | 前端持有缓冲区 | 全部客户端实现（下载走 Blob） |
| 更新日志 | 归档分组「N 条」 | `ReleaseNote[]` 全量返回 | 按 minor 版本客户端聚合 |
| 设置 | 20 项配置中的 17 项 | 仅 `adminPsw` / `loadBalancingMode` / 语言 | 设计稿 20 项中，3 项有后端支撑（渲染）、**17 项无支撑（不渲染）**，20 = 3 + 17 自洽。另**追加 2 项设计稿之外的纯前端本地项**（主题、侧栏默认收起，均已在上一变更实现且状态存于 localStorage，不计入设计稿的 20 项），故落地共渲染 5 项。不渲染的 17 项连带「账号池与调度」「危险区」**2 个分区标题**与「导出配置 / 放弃更改 / 保存并重载」**3 个页头动作**一并不渲染（逐项清单见 `specs/admin-ui/spec.md` 的「设计稿中不渲染的项」场景） |

### 图表实现

每日统计的面积折线图用内联 SVG（`viewBox="0 0 100 100"` + `preserveAspectRatio="none"` + `vector-effect="non-scaling-stroke"`），与设计稿一致，**不引入图表库**。零消耗区间带与峰值点标注用绝对定位百分比坐标。

### 沿用上一变更的三条 Tailwind 3.4 实证限制

1. 对整数 `var()` 颜色用 opacity 修饰符（`bg-brand/50`）不生成 CSS → 一律用 `-soft` / `-line` 变体
2. `tailwindcss-animate` 未安装 → 不使用 `animate-in` / `fade-in-0` / `zoom-in-95`
3. 父级 `opacity` 不可被子元素撤销 → 降权用 `[&>td:not(:last-child)]` 而非父级 opacity

## 预期影响

- **受影响文件**：12 个页面组件、5 个对话框、6 个 ui 原语、`index.css`、`i18n/locales/{zh,en}.json`、新增 `components/page-head.tsx`、`dashboard.tsx`（`:1082-1105` 内联页头替换为 `PageHead`，属上一变更范围内的受控改动）
- **能力保留要求**：每页现有交互能力必须原样保留。各页面能力清单在对应任务实施前清点并写入 `tasks.md`，实施后逐条核对
- **构建产物**：预计 CSS +3~5 kB、JS +15~25 kB（新增 SVG 图表与时间线结构）
- **后端**：零改动，无需重新验证 Rust 侧
- **i18n**：预计新增 150+ 键，zh / en 必须键集合对称

## 风险

| 风险 | 应对策略 |
|---|---|
| 逐页重建可能丢失现有交互能力（`api-keys-panel.tsx` 1094 行内含生成 Key、编辑、绑定账号、显示完整 Key、清除已停用等多项能力） | 每页任务的第一步是清点该页现有能力清单并写入 `tasks.md`；实施后逐条核对，核对结果写入实施记录 |
| 设置页大幅裁剪设计稿内容（20 项仅落地 5 项），易被误判为实现不完整 | 在 `spec.md` 显式登记「不渲染项」清单，标注为预期行为 |
| 12 个页面一次性改动量大，单次 CR 无法有效覆盖 | 按页拆分任务，每个任务独立跑增量 CR（沿用上一变更的分片规则） |
| i18n 新增键量大，zh / en 易不对称 | 每页任务收尾用 node 脚本比对两份 locale 的键集合，不对称即视为任务未完成 |
| 每日统计的 SVG 折线图在数据全为 0 或仅 1 个数据点时可能除零 / 路径退化 | 在 `spec.md` 写明零数据与单点场景的 WHEN/THEN；实施时加守卫 |
| B 组 6 个无稿页面的推导缺少像素基准，可能与 A 组产生新的不一致 | 推导基准限定为 A 组已实现的组件类，不自创新组件；B 组任务的 CR 增加「与 A 组模式一致性」核对项 |
| **T2（ui 原语令牌对齐）是 T3–T16 共 14 个下游任务的单点故障** —— 原语取色 / 尺寸一旦有偏差，全部下游页面同步偏，且下游任务的增量 CR 只看该页 diff，无法反向暴露 T2 缺陷 | 三重措施：① T2 完成后立即单独跑一次增量 CR，不与任何页面任务合并；② T2 任务内强制清点全部调用点（含已验收的账号管理页与编辑对话框），逐处确认无视觉回退；③ T3（首个下游任务）的实施记录中必须回溯记录原语在真实页面中的实际表现，发现偏差立即回到 T2 修正而非在页面侧打补丁 |
