# 变更提案:add-light-theme

## 背景

admin-ui 和 user-ui 当前均无主题切换机制:`index.html` 写死 `<html class="dark">`,`index.css` 中 `:root` 与 `.dark` 两套 CSS 变量数值完全相同(均为暗色),Tailwind `darkMode: 'class'` 配置存在但从未被实际切换过。用户希望新增一套白天模式,配色风格与本地参考页面 `prompt_engineering_guide.html` 一致(米黄背景 `#fdf6ec` + 白色卡片 + 橙色主色调 `#c75a1f` 的暖色系)。

## 目标范围

**在范围内:**
- admin-ui、user-ui 两个前端各自新增主题切换能力(手动切换按钮 + localStorage 持久化)
- 新增一套白天模式 CSS 变量,配色完全采用参考页面的暖色系(背景/文字/卡片/边框颜色),替换现有 `:root` 默认值
- 现有暗色视觉效果原样保留,迁移至 `.dark` 选择器下
- 页面加载时依据 localStorage 记录提前应用主题 class,避免闪烁(FOUC)
- 新增用户首次访问(无 localStorage 记录)时默认仍为暗色,与现状保持一致
- 审计并适配两端共 6 个使用硬编码 `neon-*`/`space-*` 色板字面量的文件(`admin-ui`: `credential-card.tsx`、`ui/progress.tsx`、`ui/badge.tsx`、`lib/utils.ts`;`user-ui`: `ui/progress.tsx`、`ui/badge.tsx`),确保在白天模式下可读、协调

**不在范围内:**
- 不跟随系统 `prefers-color-scheme` 自动切换(已确认仅手动按钮)
- 不改变 CSS 变量的语义角色/命名结构(如 `primary`/`accent`/`muted` 等变量名保持不变);但白天模式下这些变量对应的**具体色值**会替换为参考页面暖色系取值(`primary` 由紫色改为橙色系,`accent` 由青色改为蓝色系),详见下方色值映射表 —— 暗色模式(`.dark`)下的原有紫色/青色数值保持不变,不受影响
- 不引入第三方主题库(如 next-themes),使用项目已有的 React + Tailwind CSS 变量方案自行实现
- 不改动后端 `src/admin/` 及配置结构

## 技术方案

- **状态管理**:各自新增 `src/hooks/use-theme.ts`(admin-ui、user-ui 各一份,逻辑一致,因两项目为独立构建产物无共享包机制),对外提供 `theme: 'light' | 'dark'` 与 `toggleTheme()`,内部读写 `localStorage`,并同步给 `document.documentElement.classList`。**两端 `localStorage` key 必须不同**(`admin-ui-theme` / `user-ui-theme`):admin-ui(`/admin`)与 user-ui(`/user`)通过同一个 axum `Router` 挂载在同一 host:port 下(`src/main.rs` `.nest("/admin", ...)` / `.nest("/user", ...)`),属于同源不同路径,而浏览器 `localStorage` 按源(origin)隔离、不按路径隔离 —— 若使用同名 key,两端主题选择会相互覆盖
- **防闪烁**:在 `index.html` `<head>` 内联一段同步执行的 `<script>`,在 React 挂载前依据对应的 `localStorage` key(admin-ui 用 `admin-ui-theme`,user-ui 用 `user-ui-theme`;缺省值均为 `'dark'`)提前设置 `<html>` 的 `dark` class,避免首屏闪烁
- **CSS 变量**:`index.css` 的 `:root` 替换为白天模式暖色系 HSL 变量,`.dark` 选择器保留当前数值不变(即现状的暗色体验)。具体色值映射表(由参考页面对应 Hex 换算得出,单位 `H S% L%`,实施时允许 ±5% 范围内微调以适配组件实际观感,以浏览器实测为准):

  | 变量 | 参考页面来源色 | 目标 HSL 值 |
  |---|---|---|
  | `--background` | 页面背景 `#fdf6ec` | `35 81% 96%` |
  | `--foreground` | 正文文字 `#1a1a1a` | `0 0% 10%` |
  | `--card` / `--popover` | 卡片背景 `#ffffff` | `0 0% 100%` |
  | `--card-foreground` / `--popover-foreground` | 同 `--foreground` | `0 0% 10%` |
  | `--secondary` / `--muted` | 卡片内层背景 `#faf7f3` | `34 41% 97%` |
  | `--secondary-foreground` | 同 `--foreground` | `0 0% 10%` |
  | `--muted-foreground` | 次要文字 `#98886e`,原始 HSL 亮度(L 51%)对背景/卡片的对比度仅约 3.2:1/3.5:1,未达 WCAG AA 4.5:1,已下调 L 至 38% 预先修正 | `37 17% 38%` |
  | `--primary` | 主色调橙 `#c75a1f` | `21 73% 45%` |
  | `--primary-foreground` | 白字 | `0 0% 100%` |
  | `--accent` | 强调蓝 `#2c5d8c` | `209 52% 36%` |
  | `--accent-foreground` | 白字 | `0 0% 100%` |
  | `--destructive` | 警示红 `#dc2626` | `0 72% 51%` |
  | `--destructive-foreground` | 白字 | `0 0% 100%` |
  | `--border` / `--input` | 浅边框 `#f1e3d0` | `35 54% 88%` |
  | `--ring` | 同 `--primary` | `21 73% 45%` |
- **UI 入口**:admin-ui 在 `dashboard.tsx` 侧边栏底部图标按钮区(刷新/退出登录旁)新增 Sun/Moon 图标切换按钮(`lucide-react` 已有依赖);user-ui 在 `dashboard.tsx` 顶部 header 按钮区(查看日志/刷新/退出旁)新增同类按钮
- **硬编码色板审计**:对 6 个直接使用 `neon-*`/`space-*` 字面量类名(而非 CSS 变量驱动)的文件逐一检查渲染效果,必要时调整为亮色模式下可读的组合(如降低透明度、改用 `dark:` 变体区分明暗两态的取值)

## 预期影响

- 两个前端的默认视觉体验(暗色)保持不变,新增功能为纯增量,不影响现有用户
- 涉及文件:每端 `index.html`、`index.css`、新增 `use-theme.ts`、`dashboard.tsx`、以及各自 2-4 个硬编码色板组件文件,共约 14 个文件
- 无后端改动,无需数据库/API 变更,无兼容性风险

## 风险

- **FOUC 风险**:防闪烁脚本若遗漏或时机不对,白天模式用户刷新页面时会先闪一下暗色再切换 —— 通过 `index.html` 内联同步脚本(先于任何 CSS/JS 资源加载)规避
- **硬编码色板协调性风险**:`neon-*` 系列色值原为暗色霓虹风格设计,直接套用到暖色调背景上可能显得突兀 —— 通过逐文件人工审查 + 浏览器实测确认视觉效果
- **两端实现漂移风险**:admin-ui、user-ui 各自独立实现 `use-theme.ts`,后续若其中一端修改主题逻辑另一端未同步,会导致行为不一致 —— 已知风险,本次按现状架构(两个独立前端无共享包)接受此重复,不在本次范围内引入共享包重构
- **同源跨路径存储冲突风险**:admin-ui、user-ui 同源不同路径部署,若沿用同名 `localStorage` key 会导致两端主题状态相互覆盖 —— 已通过采用不同 key(`admin-ui-theme` / `user-ui-theme`)在设计阶段规避,验证阶段(任务 11/12)需实测确认互不干扰
- **可访问性/对比度风险**:白天模式背景整体偏浅(`--background` L≈96%)。原始 `--muted-foreground`(L 51%)对背景/卡片对比度仅约 3.2:1/3.5:1,未达 WCAG AA 4.5:1,已在色值映射表中下调至 L 38%(预估对比度 ≥5.3:1)—— 任务 13 需实测复核该值及其余关键文字/背景组合,若仍不达标需进一步下调
