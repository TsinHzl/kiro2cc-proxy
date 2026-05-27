> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# 变更提案：theme-antigravity

## 背景

kiro2cc-proxy 的 admin-ui 和 user-ui 当前使用 shadcn/ui 默认浅色主题，与 antigravity-claude-proxy 的深空赛博朋克风格差异明显。需要将两个前端的视觉风格统一到 antigravity 风格：深黑背景 + 霓虹高亮色 + 等宽字体，增强产品一致性和辨识度。

## 目标范围

**在范围内：**
- `admin-ui/index.html`：添加 `class="dark"` 到 `<html>`，引入 Google Fonts（Inter + JetBrains Mono）
- `admin-ui/src/index.css`：替换 CSS 变量为深空/霓虹配色，新增自定义深色滚动条
- `admin-ui/tailwind.config.js`：新增 `space` / `neon` 调色盘，配置 `fontFamily`
- `admin-ui/src/components/ui/badge.tsx`：`success` / `warning` variant 改为霓虹半透明风格
- `admin-ui/src/components/ui/progress.tsx`：进度条颜色改为霓虹色系
- `admin-ui/src/components/credential-card.tsx`：HEALTH_CONFIG 状态色改为霓虹风格
- `user-ui/index.html`：同 admin-ui/index.html
- `user-ui/src/index.css`：同 admin-ui/src/index.css（不含 popover 变量）
- `user-ui/tailwind.config.js`：同 admin-ui/tailwind.config.js
- `user-ui/src/components/ui/badge.tsx`：同 admin-ui badge
- `user-ui/src/components/ui/progress.tsx`：同 admin-ui progress

**不在范围内：**
- 路由、业务逻辑、API 层的任何改动
- 新增组件或页面
- 暗色模式切换按钮的行为改动（保留现有切换逻辑，两套 CSS 变量值相同）
- user-ui 无 credential-card 组件，不涉及

## 技术方案

**配色体系（来自 antigravity）：**
- 背景层：`space-950 #09090b`（body）→ `space-900 #0f0f11`（card）→ `space-800 #18181b`（secondary）
- 边框：`space-border #27272a`
- 主色：`neon-purple #a855f7`（primary / ring）
- 辅助色：`neon-cyan #06b6d4`（accent）
- 状态色：`neon-green #22c55e` / `neon-yellow #eab308` / `neon-red #ef4444`
- 文字：`#d4d4d8`（前景）/ `#a1a1aa`（muted）

**字体：**
- 正文/标签：Inter
- 数据/代码：JetBrains Mono
- 通过 Google Fonts CDN 引入

**Tailwind 扩展方式：**
- `theme.extend.colors` 追加 `space` / `neon` 对象，不替换现有 shadcn token
- `theme.extend.fontFamily` 追加 `mono` / `sans`

**shadcn/ui CSS 变量映射：**
```
--background     → 240 10% 4%   (#09090b)
--foreground     → 240 4% 85%   (#d4d4d8)
--card           → 240 10% 6%   (#0f0f11)
--card-foreground→ 240 4% 85%
--popover        → 240 10% 6%   (admin-ui only)
--popover-foreground → 240 4% 85% (admin-ui only)
--primary        → 271 91% 65%  (#a855f7)
--primary-foreground → 0 0% 100%
--secondary      → 240 4% 11%   (#18181b)
--secondary-foreground → 240 4% 85%
--muted          → 240 4% 11%
--muted-foreground → 240 2% 64% (#a1a1aa)
--accent         → 189 94% 43%  (#06b6d4)
--accent-foreground → 0 0% 100%
--destructive    → 0 84% 60%    (#ef4444)
--destructive-foreground → 0 0% 100%
--border         → 240 4% 16%   (#27272a)
--input          → 240 4% 16%
--ring           → 271 91% 65%
--radius         → 0.75rem
```

## 预期影响

- 所有页面强制深色模式（`html.dark`），不受系统主题影响
- shadcn/ui 组件（Button、Card、Dialog、Table 等）自动继承新配色，无需逐一修改
- Badge/Progress/credential-card 手动调整以适配霓虹半透明风格
- 现有暗色模式切换按钮行为不变（切换 `.dark` class），但视觉上两套配色相同

## 风险

- Google Fonts CDN 在离线/受限网络下加载失败 → 通过字体栈 fallback（`system-ui`, `monospace`）降级，功能不受影响
- 若有组件使用硬编码颜色类（如 `bg-white text-black`）则不被本次改动覆盖 → 当前审查范围内无此问题
