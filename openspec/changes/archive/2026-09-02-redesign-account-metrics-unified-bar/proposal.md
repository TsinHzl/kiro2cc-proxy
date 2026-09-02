# 变更提案：redesign-account-metrics-unified-bar

## 背景

账号管理页（admin-ui）顶部指标条当前为「5 个等宽 cell + before 伪元素分隔」的旧版布局（`MetricsBar` grid-cols-5 / `Metric` before 伪元素），视觉密度低、主值字号 23px、环形图仅 42px 无光晕、sparkline 74×26 无终点光点。需对齐 `points_dashboard_redesign.html` 方案一「现代化一体式通栏卡片条」：单容器通栏 grid（非 5 独立卡）+ cell `::after` divider + 28px 主值 + 52px 光晕环形图 + 80×36 贝塞尔 sparkline 终点双光点 + trend-badge 环比胶囊，要求暗黑/白天双模式 100% 还原。

## 目标范围

**在范围内：**
- 重写 `admin-ui/src/components/metrics.tsx` 全部视觉原语：`MetricsBar`（5 列非等宽 fr grid `1.1fr 1.35fr 1fr 1.25fr 1.1fr` + radius-lg 16px + shadow）、`Metric`（cell `::after` divider 替代 before 伪元素，padding 20px 24px，min-height 124px，justify space-between，hover bg-card-hover）、`MetricValue`（28px/700/letter-spacing -0.02em/tabular-nums）、`MetricFoot`（12px muted gap 8px + footer-highlight secondary/500 + status-dot 6px）、新增 cell-header（title 13px/500/secondary + 15px icon muted，margin-bottom 8px）、`MetricAside`（右侧环形图/sparkline 区，cell-body justify-between 与主值并排）、`Ring`（52px viewBox 0 0 48 48 r=19 stroke-width 4.5 track stroke-border-subtle + progress 渐变 stroke + drop-shadow 光晕 + center percent 12px/700 + label 8px scale .85）、`Sparkline`（80×36 viewBox 0 0 80 36 贝塞尔 C 曲线 + 渐变区域 + 终点双 circle r=3 实心 + r=5 opacity .3）、`Delta`（trend-badge 胶囊 padding 2px 7px radius 6px 11px/600 danger/success 色）、`FootSep`（保留但改用方案1 的 `|` / `•` 文本分隔符风格可选）
- 重写 `admin-ui/src/components/account-metrics.tsx` 适配新原语结构：每卡按 cell-header / cell-body / cell-footer 三段重组；环形图中心文案改为「percent% + 充裕/紧张/告急」label；sparkline 取 emerald/teal 两色（用户已确认映射到 brand/ok token，不引入新 emerald 体系）；今日调用/今日消耗的 Delta 胶囊替换原 Delta 原语
- `index.css` 令牌双轨制扩展（仅新增，不改既有）：为环形图渐变与光晕新增 `--ring-grad-from`/`--ring-grad-to`/`--ring-glow`（light/dark 双值），映射到现有 brand/ok 体系；sparkline 渐变复用 `--brand` 与 `--ok`
- 配色策略（用户确认）：环形图与 sparkline 主色**映射到现有 brand/ok token**，不引入 emerald 变量；光晕用 `--ok` 的 rgba 软光（light .2 / dark .35）
- i18n：新增「剩余状态 label」三键（充裕/紧张/告急，对应 remainingTone 60/30 阈值的 ok/warn/danger），zh + en

**不在范围内：**
- user-ui 指标条（本变更仅 admin-ui）
- 账号行（account-row.tsx）额度单元格（上一变更已归档，不动）
- 后端 API / 数据结构
- 方案二独立微质感卡片组、方案三环形图变体（仅方案一）
- 响应式断点（1100px/680px 折叠）：本期先保证桌面端 100% 还原，移动端折叠留待后续

## 技术方案

- **grid 非等宽列**：Tailwind 无 `grid-template-columns: 1.1fr 1.35fr ...` 原子类，改用 `MetricsBar` 内联 `style={{ gridTemplateColumns: '1.1fr 1.35fr 1fr 1.25fr 1.1fr' }}`，外壳仍走 Tailwind（rounded-[16px] border shadow）。`cols` prop 弃用（固定 5 列），保留签名以减小调用方改动，内部忽略
- **cell divider**：方案1 用 `.metric-cell:not(:last-child)::after`（right:0 top:18px bottom:18px width:1px）。React 内联伪元素不可行，改 `@layer components` 白名单新增 `.metric-cell` 类（与现有 logrow/.lv 同级策略），定义 `::after`；或退化为给除最后一个外的 Metric 注入一个 `<span aria-hidden className="cell-divider">` 绝对定位元素。**选后者**：避免再扩 CSS 白名单，纯 Tailwind 原子类实现 `absolute right-0 top-[18px] bottom-[18px] w-px bg-hairline-2`，`Metric` 接收 `divider` bool（默认 true，最后一个传 false 或由 MetricsBar 用 React.Children 自动判断 last）
- **环形图渐变 + 光晕**：SVG `<defs>` 内 `<linearGradient id="ring-grad">` stops 用 `var(--ring-grad-from)`/`var(--ring-grad-to)`；progress circle `style={{ filter: 'drop-shadow(0 0 3px var(--ring-glow))' }}`。渐变 id 用 `useId` 去冒号（同 Sparkline 现有做法）
- **sparkline 贝塞尔**：方案1 用手写 `C` 三次贝塞尔。数据驱动无法精确复刻其手调控制点，改为**平滑插值**：对相邻点取中点作控制点生成单调三次贝塞尔（Catmull-Rom 转 Bezier），视觉接近「平滑曲线」目标；终点双 circle r=3 + r=5 opacity .3
- **cell 三段布局**：`Metric` 内部由 `cell-header`（title+icon）→ `cell-body`（主值 + MetricAside 并排 justify-between）→ `cell-footer`（MetricFoot）三段，`flex flex-col justify-between min-h-[124px]`。label 现为小写语义标题，需从纯文本升级为含 icon 的 `cell-header`——`Metric` 新增可选 `icon?: ReactNode` 参数（lucide icon 由 account-metrics 传入）
- **trend-badge（Delta）**：方案1 danger/success 二色（无 flat 中性态）。现有 Delta 有 flat 态。保留 flat 态但视觉对齐方案1 胶囊尺寸（padding 2px 7px radius 6px 11px/600），flat 用 muted 色
- **状态点 status-dot**：footer 内 6px 圆点，success/muted 二态，新建 `StatusDot` 原语或内联 span

## 预期影响

- `metrics.tsx` 全量重写（6 个原语 + 新增 cell-header/StatusDot），调用方 `account-metrics.tsx` 同步重构
- 视觉密度提升，主值 23→28px，环形图 42→52px 带光晕，sparkline 74×26→80×36 带终点光点
- 既有 `MetricsBar`/`Metric`/`MetricValue`/`MetricFoot`/`MetricAside`/`Ring`/`Sparkline`/`Delta`/`FootSep` 导出签名保留（`Metric` 新增可选 `icon`、`divider` prop），其他 6 页若引用 `metrics.tsx` 需排查（预计仅 account-metrics 引用）
- index.css 新增 3 个 CSS 变量（ring-grad-from/to/glow），双模式
- i18n 新增 3 键

## 风险

- **贝塞尔平滑度**：数据驱动平滑插值与方案1 手调控制点视觉不完全一致，但同属「平滑曲线」语义，可接受（无法逐像素复刻手调路径）
- **非等宽 grid 内联 style**：与项目「组件类白名单 + Tailwind 原子类」策略略有出入，但 fr 单位无原子类等价物，内联 style 是唯一可行方案，注释说明
- **cell divider 由 React.Children 判断 last**：若 cell 数量动态变化需保证 last 判断正确；account-metrics 固定 5 卡，风险低
- **min-height 124px**：在内容不足时撑高，可能与其他页指标条高度不一致——本变更仅 admin-ui account-metrics，不影响其他页
