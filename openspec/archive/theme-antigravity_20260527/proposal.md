# 变更提案：theme-antigravity

## 背景

将 kiro2cc-proxy（admin-ui + user-ui）的网页端主题从默认浅色 shadcn/ui 风格，改为与 antigravity-claude-proxy 相同的赛博朋克深空风格：近黑背景、霓虹强调色、JetBrains Mono 字体、自定义滚动条。

## 目标范围

**在范围内：**
- CSS 变量替换（shadcn token → 深空/霓虹配色）
- Tailwind 配置扩展（新增 `space` + `neon` 颜色 palette，`JetBrains Mono` + `Inter` 字体）
- HTML 入口添加 `class="dark"` + Google Fonts 链接
- 亮色专用组件颜色修复（`HEALTH_CONFIG` 状态徽章、Badge `success`/`warning` variant）

**不在范围内：**
- 任何后端 Rust 代码修改
- 组件布局/结构重构
- 删除 admin-ui 的暗色模式切换按钮（按钮保留，但主题已是深色默认）
- 引入 DaisyUI 或 Alpine.js

## 技术方案

**CSS 变量映射（shadcn token → antigravity 颜色）：**

| shadcn token | 新值（HSL） | 对应颜色 |
|---|---|---|
| `--background` | `240 10% 4%` | `#09090b` space-950 |
| `--foreground` | `240 4% 85%` | `#d4d4d8` text-secondary |
| `--card` | `240 10% 6%` | `#0f0f11` space-900 |
| `--primary` | `271 91% 65%` | `#a855f7` neon-purple |
| `--secondary` | `240 4% 11%` | `#18181b` space-800 |
| `--muted-foreground` | `240 2% 64%` | `#a1a1aa` text-tertiary |
| `--accent` | `189 94% 43%` | `#06b6d4` neon-cyan |
| `--destructive` | `0 84% 60%` | `#ef4444` neon-red |
| `--border` / `--input` | `240 4% 16%` | `#27272a` space-border |
| `--ring` | `271 91% 65%` | `#a855f7` neon-purple |
| `--radius` | `0.75rem` | 12px |

**`:root` 改为暗色，`.dark` 保留相同值**（保证 admin-ui 的 dark 切换按钮不破坏页面显示，同时所有 `dark:` Tailwind variant 正常工作）。

**Tailwind 新增 palette：**
```js
space: { 950: '#09090b', 900: '#0f0f11', 850: '#121214', 800: '#18181b', border: '#27272a' }
neon:  { purple: '#a855f7', cyan: '#06b6d4', green: '#22c55e', yellow: '#eab308', red: '#ef4444' }
fontFamily: { mono: ['"JetBrains Mono"', ...], sans: ['Inter', ...] }
```

**组件颜色修复：**
- `HEALTH_CONFIG`（credential-card.tsx）：`bg-green-100 text-green-800` → `bg-neon-green/10 text-neon-green border-neon-green/30`（其余状态同理）
- Badge `success`：→ `bg-neon-green/10 text-neon-green border-neon-green/30`
- Badge `warning`：→ `bg-neon-yellow/10 text-neon-yellow border-neon-yellow/30`

## 预期影响

- 所有页面背景变为近黑深空风格，立即生效
- 主按钮、焦点环、链接色变为霓虹紫
- 数字/代码区域可选用 `font-mono` 渲染为 JetBrains Mono
- 滚动条改为深色+紫色 hover 样式
- 无 API 调用路径变化，纯前端改动

## 风险

- Google Fonts 需网络可用，离线环境降级为系统字体（可接受）
- admin-ui 的 dark/light 切换按钮在视觉上效果消失（两套 token 相同），但功能不报错
