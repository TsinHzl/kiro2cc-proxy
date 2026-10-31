## 上下文

- 主题机制：`use-theme.ts` 把 `theme-<id>` 与 `dark` 类挂在 `<html>` 上；`themes.css` 里 `:root.theme-<id>` / `:root.dark.theme-<id>` 覆盖 `index.css` 的基准令牌。`theme-default` 是空占位，直接沿用基准。
- 令牌双轨：shadcn 轨（`--foreground`、`--muted-foreground`、`--primary-foreground` 等，值是 HSL 三元组 `H S% L%`）和原生轨（`--ink-2`、`--brand`、`--ok` 等，值是完整色值或 rgba）。Tailwind 通过 `tailwind.config.js` 映射成 `text-ink-3`、`bg-brand` 这类类名。
- `themes.css` 头部提到的生成脚本 `scripts/inject-theme-semantic-colors.py` 已不在仓库，`web-ui/admin-ui/scripts/` 为空，目前 4011 行 `themes.css` 只能手改。
- 审计基线（见 proposal）：2520 项检查中 551 项不达标。多数失败来自三类：`ink-3` / `ink-2` 压浅灰表面；`ok` / `warn` / `danger` 与其 `-soft` 底；`brand` 作文字色压浅表面。
- 约束：不新增 Rust crate（本次不涉及）；不引入前端新依赖；`openspec/config.yaml` 要求改动通过 `cargo fmt` / `clippy`（本次不改 Rust）。项目注释里明确避免依赖 `color-mix`。

## 目标 / 非目标

**目标**
- 72 个变体 + 基准令牌，文字 × 背景全部 ≥ 4.5:1，可由脚本重复验证。
- 各主题背景、卡片、品牌填充色逐值不变。
- 「主题设置」页实时预览的文字对比度 ≥ 4.5:1，且不改 `theme-definitions.ts` 的色板取值（决策 10）。
- 组件改动仅限：4 处 CSS 直引 `var(--brand)`（任务 9）、字面量调色板类与 `text-primary`（任务 10）、预览区文字色表达式（任务 12）；`text-brand` 等类名本身不改，由 Tailwind 映射接管。

**非目标**
- 重新设计主题配色；非文字对比度；`user-ui`；主题管理逻辑。

## 决策

### 决策 1：在令牌层修正，而不是逐页改样式
问题在令牌取值，页面通过变量继承。逐页改会漏页、会与主题耦合，且新增页面会再次踩坑。**选令牌层。**

### 决策 2：只调整前景侧，背景与品牌填充不动
调整规则：在 HSL 空间保持色相和饱和度，沿“远离背景”的方向以 1% 明度步长移动，直到对所有相关背景 ≥ 4.5。浅色主题文字向深调，深色主题文字向亮调。这样主题的视觉身份由背景和品牌填充决定，保持不变。
- 备选：同时改背景。否决：会改变主题身份，超出“文案色和背景色搭配不好”中“修搭配”的最小解。

### 决策 3：新增 `--brand-ink`，由 Tailwind `textColor` 映射接管 `text-brand*`
`--brand` 同时承担填充（按钮、徽章）和文字（选中态标签）。亮色品牌色作填充合适，作文字压白底只有约 2.3:1；把它压到 4.5 会让所有按钮变暗，等于改了主题身份。
- 做法：`index.css` 基准加 `--brand-ink`（等于各自 `--brand` 的文字安全版本），`themes.css` 每个变体按算法给出；`tailwind.config.js` 加 `theme.extend.textColor.brand = { DEFAULT: 'var(--brand-ink)', hover, fg, soft, line }`，其中 `hover/fg/soft/line` 保持与原 `colors.brand` 一致，避免 `text-brand-fg` 等被吞掉。
- 备选 A：批量把组件里的 `text-brand` 改成 `text-brand-ink`。否决：约 26 个文件、20+ 处改动，且未来新代码容易写回 `text-brand`。
- 备选 B：直接压深 `--brand`。否决：改变按钮、徽章等所有填充的主题色。
- 备选 C：用 `color-mix()` 在运行时派生。否决：项目已明确不依赖其兼容性，且浏览器侧无法保证对比度。
- 4 处直接写 `color: var(--brand)` 的 CSS（`index.css`、`docs/docs.css`）改为 `var(--brand-ink)`；`border-color` 保持 `var(--brand)`。

### 决策 4：“填充上的文字”在调整值与近黑/白之间取对比度更高者
`brand-fg`、`primary-foreground`、`destructive-foreground` 的背景是主题填充色，不能改。候选为“沿明度调整后的原色”、`#FFFFFF`、近黑 `#0F1113`。在对比度 ≥ 4.5 的候选中，取与原色相对亮度差最小的一个（保留主题观感优先）。任意背景至少有一种候选 ≥ 4.5（黑白对比度的最小值约 4.58），因此必有解。

### 决策 5：用 Python 脚本批量改写并兼作回归检查
- 位置：`web-ui/admin-ui/scripts/theme-contrast.py`，仅标准库。
- `--check`：解析 `index.css` 基准和 `themes.css` 各变体，计算矩阵，打印失败清单，失败则退出码 1。
- `--fix`：按决策 2–4 改写 `themes.css`；变体块里已有的令牌就地替换，缺失的令牌追加到块末尾；不改选择器、块顺序、注释与空行。基准令牌（`index.css`）的修正由 `--fix` 打印建议值，人工写入，因为基准块带有大量设计稿注释，不适合自动改写。
- `--file <in>` / `--out <out>`：指定输入与输出路径，使 `--fix` 可在临时副本上验证，并用于幂等性检查（对输出再次运行 `--fix` 必须逐字节无变化）。
- `--selftest`：校验核心函数（黑白 21.0、`#767676` 压白约 4.54、HSL 与 hex 等价、rgba 合成）。
- 写回格式：shadcn 轨令牌写 HSL 三元组；原生轨令牌写 hex；rgba 令牌保持 rgba，仅改 RGB 通道。
- 入口：`package.json` 增加 `check:contrast`，不接 CI（见 proposal 范围）。
- 缺失令牌回退：变体块未定义 `--brand-ink` 时，`--check` 按 `--brand` 判定，所以修正前的基线可复现。
- 备选：手工编辑 `themes.css`。否决：551 项失败、约 70 个块，人工易错且无法复验。
- 备选：脚本不入库，仅在会话中运行。保留为可选项：若用户不想引入脚本，则删除脚本文件，只保留修正结果；但会失去回归检查能力。

### 决策 6：对比度判定矩阵
下列为 `--check` 的固定矩阵，全部要求 ≥ 4.5。半透明背景按决策 8 的口径合成：分别叠加在 `--surface` 与 `--surface-2` 上，取对比度较低的一种。

| 前景 | 背景 |
|---|---|
| `foreground` / `card-foreground` / `popover-foreground` | `background` / `card` / `popover` |
| `foreground` | `surface-2`、`surface-3`、`sidebar`、`muted` |
| `ink`、`ink-2` | `surface`、`surface-2`、`surface-3`、`sidebar` |
| `ink-3` | `surface`、`surface-2`、`surface-3`、`sidebar` |
| `muted-foreground` | `background`、`card`、`muted` |
| `secondary-foreground`、`accent-foreground` | `secondary`、`accent` |
| `ink`、`ink-2`、`ink-3` | `background`、`card`、`muted`、`brand-soft`、`ok-soft`、`warn-soft`、`danger-soft` |
| `brand-ink`（修正前按 `--brand` 回退） | `background`、`surface`、`surface-2`、`surface-3`、`sidebar`、`brand-soft`、`ok-soft` |
| `ok` / `warn` / `danger` | `surface`、`surface-2`、`surface-3`、`brand-soft`、`ok-soft`、`warn-soft`、`danger-soft`（共现统计中存在跨色组合，如 `text-ok` 压 `bg-warn-soft`，故四档 `-soft` 全覆盖） |
| `muted-foreground` | `surface`、`surface-2`、`surface-3`、`sidebar` |
| `brand-fg` | `brand`、`brand-hover` |
| `primary-foreground` / `destructive-foreground` | `primary` / `destructive` |
| `quota-{safe,mod,warn,danger}-badge-text` | 各自 `-badge-bg` |
| `badge-new-fg` | `badge-new-from`、`badge-new-to` |

### 决策 7：文字侧白名单，其余令牌一律逐值不变
`--fix` 只允许改写下列令牌，并新增 `--brand-ink`；白名单之外的任何令牌（含 `--primary`、`--ring`、`--secondary`、`--muted`、`--accent`、`--border`、`--input`、`--hairline*`、`--surface*`、`--sidebar`、`--brand*`（除新增的 `--brand-ink` 与下一条的 `--brand-hover` 例外）、`--*-soft`、`--*-line`、`--quota-*-badge-bg`、`--quota-*-grad`、`--badge-new-from/to/line/shadow`、`--track`、`--code-bg`、阴影与环形渐变）都要求改动前后逐值一致。
- 白名单：`--foreground`、`--card-foreground`、`--popover-foreground`、`--secondary-foreground`、`--accent-foreground`、`--muted-foreground`、`--primary-foreground`、`--destructive-foreground`、`--ink`、`--ink-2`、`--ink-3`、`--brand-fg`、`--ok`、`--warn`、`--danger`、`--quota-{safe,mod,warn,danger}-badge-text`、`--badge-new-fg`。
- 例外（任务 4 实测发现，用户已确认）：10 个浅色变体（aurora、chatgpt、claude、github、graphite、gruvbox、monokai-pro、nord、solarized-light、tokyo-night）里，`brand` 与 `brand-hover` 的亮度处在白字和近黑字都无法同时达标的区间。`--fix` 对这类变体只把 `--brand-hover` 沿明度向更亮方向微调到近黑 `brand-fg` 能同时满足两者为止；`--brand` 本身不动。`--compare-fills` 允许 `--brand-hover` 出现差异，但必须逐项打印变体名与新旧值，随交付说明留存。
- 例外二（任务 4 实测发现，用户已确认）：3 个深色变体（catppuccin、dracula、stripe）的 NEW 徽章渐变较暗一端 `--badge-new-to` 太暗，纯黑与近黑文字都压不到 4.5（最低 3.19–4.25）。`--fix` 只把这 3 个变体的 `--badge-new-to` 沿明度向更亮方向微调到文字可同时压住渐变两端；`--badge-new-from` 不动。`--compare-fills` 同样允许 `--badge-new-to` 出现差异，并逐项打印变体名与新旧值。
- 脚本提供 `--compare-fills <原文件> <新文件>`：按令牌名比对白名单之外的行，有差异即退出码 1。

### 决策 8：对比度口径与边界条件
- 全程浮点计算，不在合成与亮度计算中取整。`--check` 与浏览器抽检使用同一公式。
- 半透明背景（`-soft`、徽章底色）的合成基底取 `--surface` 与 `--surface-2` 两种，取对比度更低（更严）的一种。
- 个别背景另有真实叠加基底（任务 14 浏览器抽检发现）：`brand-soft` 另叠 `background`、`surface-3`（文档页 tabs 与 callout），`ok-soft` 另叠 `background`（页头状态徽章）。脚本以 `EXTRA_BASES` 常量表达，仅对这两个背景追加基底，其余背景口径不变。
- 一个文字令牌要满足多个背景时取最严者：沿明度步进直到对矩阵中该令牌的全部背景 ≥ 4.5。
- 写回量化：令牌写回 HSL 三元组（整数）或 hex 后，必须用写回值重新计算并断言 ≥ 4.5；不达标则继续步进一格。
- 修不到 4.5（步进到明度极值仍不够）时脚本报错并以非零退出，不静默跳过。
- ink 层级：要求 `contrast(ink-2, surface) ≥ contrast(ink-3, surface) + 0.2`。`--fix` 先修 `ink-3`，若层级被压缩，再把 `ink-2` 向远离背景方向多调，直到满足。该规则同时作为 `--check` 的一项检查。

### 决策 9：矩阵单一来源与 shadcn 轨文字令牌
- 矩阵唯一来源是 `theme-contrast.py` 里的 `MATRIX` 常量；design.md 决策 6 的表格是它的镜像，任务 1 定稿后两者同步。脚本提供 `--matrix legacy`，使用提案审计时的 35 组旧矩阵，用来复现 551 项基线；扩充后的新基线数字单独记录。
- `muted-foreground` 已在矩阵内。`text-primary`（`hsl(var(--primary))`）全仓库仅 `lib/utils.ts:113` 一处使用。任务 3 实测 `--primary` 作文字色压 `background` / `card` 在 29 项检查中不达标（最低 2.30），而 `--primary` 兼作填充不能动，所以该处改为 `text-brand`（映射到 `--brand-ink`），`primary` 不进 `--check` 矩阵。
- 带透明度修饰符的写法（`text-ink-3/70`）：`tailwind.config.js` 注释已说明整值 `var()` 令牌不支持透明度修饰符，会静默生成空规则；任务 9、11 用 grep 确认没有此类用法。

### 决策 10：预览区文字用按底色计算的可读色，不改色板
`ThemeLivePreview` 把色板值直接当文字色：`swatch.muted`（表面淡色）作次要文字，对比度只有 1.09–1.61；`swatch.bg` 压 `swatch.primary`、`swatch.primary` 压表面在部分主题低于 4.5。用户已确认纳入，且不改 `theme-definitions.ts`。
- 新增 `web-ui/admin-ui/src/lib/preview-contrast.ts`，导出纯函数 `readableText(preferred: string, background: string): string`：解析 `hsl(H S% L%)`（色板固定格式），`preferred` 对 `background` ≥ 4.5 则原样返回；否则保持色相与饱和度，沿“远离背景”方向以 1% 明度步进，直到 ≥ 4.5；步进到极值仍不够则退回 `#0F1113` 与 `#FFFFFF` 中对比度更高者。
- 次要文字的 `preferred` 不再用 `swatch.muted`，改为中性灰阶常量：背景偏亮取 `#5F646B`，背景偏暗取 `#9AA1A9`（亮度阈值取相对亮度 0.4）。`swatch.muted` 继续只用于描边和淡色填充（徽标底色、分隔线），含义不变。
- 主色填充上的文字：`preferred = swatch.bg`，不达标时同样走上述规则，背景为 `swatch.primary`。
- 缩略图（`ThemeThumbnail`）只有装饰条，不含文字，不改。
- 备选：在 `theme-definitions.ts` 给 36 个主题新增 `text` / `textMuted` 字段。否决：用户选择不改色板定义，且要改 36 处数据；运行时计算保证未来新增主题不会再踩坑。
- 对比度公式与 `theme-contrast.py` 相同；两处实现互不共享代码。TS 辅助函数用浏览器控制台脚本对全部主题验证（见 tasks）。

## 风险 / 权衡

- `ok` / `warn` / `danger` 令牌变深会同时影响把它们当填充用的圆点和进度条，略暗但仍可辨。权衡：不为此再拆一套填充令牌，避免令牌数量翻倍；抽检发现问题再拆。
- `--brand-ink` 在亮色主题里比 `--brand` 深，选中态文字会比同屏按钮底色深一级。这是为可读性接受的差异。
- 脚本对 `themes.css` 做文本级改写，格式依赖现有的“每行一个令牌”写法；`--fix` 后必须再跑 `--check` 并检查 diff 只含颜色行。
- 对比度只覆盖令牌矩阵，不覆盖“组件里用了矩阵之外的组合”（例如 `text-ink-3` 压 `bg-brand-soft`）。抽检阶段会按实际页面补测，发现遗漏再扩矩阵。
- 修正后部分主题的 `ink-3` 会比设计稿更深，视觉层次（`ink-2` 与 `ink-3` 的区分）略收窄。脚本会保证 `ink-3` 不深于 `ink-2`。

## 迁移方案

无数据迁移。回滚方式：`git revert` 本次提交。无功能开关（纯 CSS 令牌变更，开关会让两套令牌并存）。

## 待决问题

- 审计脚本是否入库：默认入库，用户可在确认提案时要求删除。该选择不影响 spec 和任务拆分以外的修正结果。
