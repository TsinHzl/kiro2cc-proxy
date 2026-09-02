# 任务清单：redesign-account-metrics-unified-bar

## 状态：ARCHIVED

## 任务

- [x] 任务 1：index.css 令牌双轨制扩展 —— 新增 `--ring-grad-from` / `--ring-grad-to` / `--ring-glow` 三个 CSS 变量（:root 与 .dark 双值），映射到现有 brand/ok 体系（light: brand 系 / ok 光晕 .2；dark: brand 系 / ok 光晕 .35）。验证：`grep --ring-grad index.css` 命中 6 行（3 变量 × 2 主题），值不引用 emerald 字面量
- [x] 任务 2：重写 `metrics.tsx` 视觉原语 —— `MetricsBar`（5 列非等宽 fr 内联 style + rounded-[16px] border shadow-hair）、`Metric`（cell flex-col justify-between min-h-[124px] px-6 py-5 + `divider` prop 控制右侧 `::after` 等价绝对定位 span + `icon` prop 渲染 cell-header）、`MetricValue`（28px/700/ls -0.02em/tabular-nums）、`MetricFoot`（12px muted gap-2 + footer-highlight）、`MetricAside`（cell-body 内右侧并排，非 absolute）、`Ring`（52px viewBox 0 0 48 48 r=19 sw 4.5 track stroke-track + progress 渐变 stroke + drop-shadow 光晕 + center percent 12px/700 + label 8px）、`Sparkline`（80×36 Catmull-Rom 平滑贝塞尔 + 渐变区域 + 终点双 circle r=3/r=5）、`Delta`（trend-badge 胶囊 padding 2px 7px radius 6px 11px/600）、`FootSep`（保留）、新增 `StatusDot`（6px 圆点 success/muted）。验证：`cargo` 不涉及；`cd admin-ui && npx tsc --noEmit` 无类型错误；导出签名向后兼容（Metric 新增可选 icon/divider）
- [x] 任务 3：重写 `account-metrics.tsx` 适配新原语 —— 5 卡按 cell-header（lucide icon + i18n title）/ cell-body（主值 + MetricAside 环形图或 sparkline）/ cell-footer（status-dot + footer-highlight + 分隔符）三段重组；环形图中心文案改为「Math.round(avg)% + 充裕/紧张/告急 label」（remainingTone 60/30 阈值映射）；sparkline 取 brand/ok 双色（今日调用 teal→brand，今日消耗 emerald→ok）；Delta 胶囊替换原 Delta；footer 分隔符用 `|` / `•` 文本（对齐方案1）。验证：`npx tsc --noEmit` 通过；5 卡数据流不变（props 签名不动）
- [x] 任务 4：i18n 新增「剩余状态 label」三键 —— zh.json / en.json 在 credentials 段新增 `metricRemainingStatusAbundant`（充裕 / Abundant）、`metricRemainingStatusTight`（紧张 / Tight）、`metricRemainingStatusCritical`（告急 / Critical），对应 remainingTone ok/warn/danger。验证：`grep metricRemainingStatus zh.json en.json` 各命中 3 键
- [x] 任务 5：构建验证 —— `cd admin-ui && npm run build`（或 pnpm/yarn 对等命令，读 package.json 确认）通过，无 TS / Tailwind purge 报错；手动核对方案1 截图对比（主值字号、环形图尺寸+光晕、sparkline 终点光点、trend-badge 胶囊、cell divider）

## 验收标准
- [ ] metrics.tsx 全部原语视觉与方案1 HTML 像素级对齐（28px 主值、52px 光晕环形图、80×36 终点双光点 sparkline、trend-badge 胶囊、cell ::after divider）
- [ ] 暗黑与白天双模式均还原（index.css 双轨变量生效）
- [ ] 配色映射到 brand/ok token，无 emerald 字面量硬编码
- [ ] account-metrics.tsx 5 卡数据流与 props 签名不变，仅视觉层重组
- [ ] i18n 三状态 label 键落地（zh + en）
- [ ] `npx tsc --noEmit` + 前端构建均通过
