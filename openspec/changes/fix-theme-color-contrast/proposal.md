# 变更提案：fix-theme-color-contrast

## 背景
管理后台「主题设置」页提供 36 个色彩主题，每个主题有浅色、深色两种变体，共 72 个变体。变体令牌写在 `web-ui/admin-ui/src/themes.css`，由各主题的品牌色和预览色板派生，没有按“文字色 × 背景色”成对校验过对比度。结果是部分页面里的文案色压在背景色上读不清。

只读审计结果（WCAG 2.x 相对亮度公式，35 组文字/背景令牌 × 72 变体 = 2520 项检查）：

- 扩充矩阵后的新基线（任务 3，`theme-contrast.py --check`，full 矩阵，半透明背景叠 `--surface` / `--surface-2` 取较低值）：74 个变体（72 个主题变体 + 基准浅/深 2 个）共 6956 项检查，1924 项失败，其中基准 26 项、主题变体 1898 项；ink 层级检查当前 0 项失败。旧矩阵（`--matrix legacy`）在 74 个变体上为 558 项，其中基准 7 项，主题变体 551 项，与下面的 551 对得上。
- 551 项低于 AA 的 4.5:1。最严重的变体：`noctis-lux/light` 18 项、`solarized-light/light` 18 项、`gruvbox/light` 17 项、`nord/dark` 17 项。基准主题 `default` 也有 7 项不达标（浅色 4 项、深色 3 项）。
- 失败最多的组合（括号内为不达标变体数与最低对比度）：
  - `ink-3` 压 `surface-3`（72 个、2.20）
  - `ink-3` 压 `surface`（48 个、2.51）
  - `warn` 压 `warn-soft`（36 个、3.08）
  - 用量进度徽章 `quota-mod` / `quota-warn` 文字（各 36 个，最低 2.99 / 3.34）
  - `destructive-foreground` 压 `destructive`（36 个、3.08）
  - `brand` 压 `brand-soft`（26 个、2.08）
  - `brand` 压 `surface`（15 个、2.30）
  - `brand-fg` 压 `brand`（13 个、2.58）
- 受影响面广：`text-ink-3` / `var(--ink-3)` 出现在 46 个文件，`text-brand` 在 26 个文件，`text-warn` / `text-ok` / `text-danger` 各 20 个左右。多数页面通过 CSS 变量继承，因此问题集中在令牌层。

「主题设置」页自身的实时预览也有同类问题（`theme-settings-page.tsx` 的 `ThemeLivePreview`，文字直接取预览色板值；缩略图只有装饰条，不含文字）。对 `theme-definitions.ts` 的 36 个主题 × 2 模式 = 72 个预览色板的只读计算：

- `swatch.muted` 作文字压 `swatch.surface` / `swatch.bg`：72 个变体全部不达标，对比度 1.09–1.61（该值本是表面淡色，被拿来当文字色）。
- `swatch.bg` 作文字压 `swatch.primary`（主按钮、选中项）：16 个变体低于 4.5，最低 2.30。
- `swatch.primary` 作文字压 `swatch.surface`：15 个变体低于 4.5；压 `swatch.bg`：16 个变体低于 4.5。

范围已与用户确认：(1) 各主题背景色、卡片色、品牌填充色不动，只调文字色；(2) 验收只按文字 4.5:1，不含边框、图标、焦点环的 3:1；(3) 预览区文字纳入本次修复，用按底色自动选择的可读色，不改 `theme-definitions.ts` 的色板取值。

## 目标范围
**在范围内：**
- 仅 `web-ui/admin-ui`。
- 「主题设置」页实时预览（`ThemeLivePreview` 的工作台与基础组件两个场景）里的文字改用按底色自动选择的可读色；`theme-definitions.ts` 的色板取值与缩略图装饰条保持不变。
- 36 个主题的浅色、深色全部变体，以及 `index.css` 基准令牌（`default` 主题沿用基准）。
- 以 WCAG AA 为验收标准：文字类令牌对其实际所在背景的对比度 ≥ 4.5:1（页面文字多为 9.5–13px，不适用大字号 3:1 档）。
- 新增 `--brand-ink` 文字专用品牌色令牌，使 `text-brand` 可读，同时 `bg-brand` / `border-brand` 保持各主题原有品牌色。
- 新增对比度审计脚本，作为本次修正的执行工具和后续回归检查；在 `package.json` 增加 `check:contrast` 脚本入口。
- 更新 `themes.css` / `index.css` 头部注释中过时的主题数量（34 → 36）和已不存在的生成脚本引用。
- 排查 7 个含字面量调色板类（如 `text-red-500`、`text-blue-600`、`text-white`）的组件文件，背景随主题变化且对比度不足的改为令牌。
- 排查把 `ink-*` / `ok` / `warn` / `danger` 令牌当填充色使用的位置（如 `bg-ink-3` 圆点），确认变深后仍可辨。

**不在范围内：**
- `web-ui/user-ui`。
- 各主题的背景色、卡片色、品牌填充色本身（保持主题视觉身份，只调整文字侧令牌）。
- 新增、删除或重命名主题；主题切换逻辑、动画、持久化。
- 非文字对比度（边框、图标、焦点环的 3:1 要求）。
- 页面布局、组件结构、文案内容。
- Rust 后端。
- 修复 `openspec/specs/theme/spec.md` 中已过时的描述（localStorage key、默认主题等）。
- `web-ui/user-ui` 的令牌漂移：任务 1 的 `diff` 结论是 user-ui 没有 `themes.css` 与 `theme-definitions.ts`，但 `user-ui/src/index.css` 复制了一份基准令牌（抽查的 12 个 `--ink*` / `--brand*` / `--ok` / `--warn` / `--danger` / `--surface*` 与 admin-ui 逐值相同，含 `.dark` 块）。本次修正基准令牌后，两边会产生漂移，user-ui 保持旧值。本次不改 user-ui，是否另开变更由用户决定。
- Windows 上 `python3` 命令差异：`pnpm check:contrast` 面向 macOS / Linux 开发环境（要求 Python 3.8+），Windows 用户用 `python scripts/theme-contrast.py --check`。
- 把对比度检查接入 CI 或提交钩子；本次只提供 `pnpm check:contrast` 入口。新增主题时需人工运行该命令，并在 `theme-definitions.ts` 与 `themes.css` 同步新增后确认通过。

## 技术方案
1. **令牌层修正，不逐页改样式。** 问题根源是令牌取值，不是页面用法。`themes.css` 中调整不达标的文字类令牌，页面经 CSS 变量自动受益。
2. **只动前景侧。** 背景和品牌填充保持不变，仅在 HSL 空间沿明度方向微调文字令牌（保留色相、饱和度），取最小改动使对比度 ≥ 4.5。按钮标签这类“填充上的文字”（`brand-fg`、`primary-foreground`、`destructive-foreground`）改为在“调整后的同色系值、`#FFFFFF`、近黑 `#0F1113`”三个候选中，取对比度 ≥ 4.5 且与原色相对亮度差最小者（与 design.md 决策 4 一致）。
3. **品牌色拆出文字变体。** 新增 `--brand-ink`，在 `tailwind.config.js` 用 `theme.extend.textColor.brand` 把 `text-brand*` 映射到它；`bg-brand`、`border-brand`、`outline-brand` 不变。`text-brand` 的类名不改，由 Tailwind 映射接管。`index.css` / `docs.css` 中直接写 `color: var(--brand)` 的 4 处改为 `var(--brand-ink)`。
4. **审计脚本。** 新增 `web-ui/admin-ui/scripts/theme-contrast.py`（仅标准库）：`--check` 输出失败清单并以退出码反映结果，`--fix` 批量改写 `themes.css`。`themes.css` 头部提到的生成脚本 `inject-theme-semantic-colors.py` 已不在仓库，该脚本同时承担这个空缺。
5. **预览区文字。** 在 `theme-settings-page.tsx` 的 `ThemeLivePreview` 中，文字色由一个纯函数按所在底色计算：色板色达标则保留，不达标则在同色相下沿明度步进，仍不达标则退回近黑/近白；次要文字改用中性灰阶而不是 `swatch.muted`。不改 `theme-definitions.ts`。
6. 详细决策与备选方案见 `design.md`。

## 预期影响
- 视觉：浅色主题的次要文字（`ink-3`）、状态色文字（`ok` / `warn` / `danger`）和品牌色文字整体略深；深色主题个别变体文字略亮。填充色、背景色、布局不变。
- 代码：`themes.css` 约 70 个变体块内的颜色行被改写，并新增约 70 行 `--brand-ink` 及少量 `--ink-*` 覆盖，预计净变更 250–400 行（均为 CSS 令牌行）；`index.css` 基准令牌若干行；`tailwind.config.js` 约 10 行；`docs.css` 2 行；新增 1 个 Python 脚本（约 250 行）。
- 预览区：`theme-settings-page.tsx` 约 40 行文字色表达式改为调用新的颜色辅助函数（新增 1 个约 60 行的 TS 文件）；预览看起来会更清晰，主题身份（背景、主色）不变。
- 无后端、API、数据格式变化；无新增运行时依赖；构建产物体积基本不变。

## 风险
- **`ok` / `warn` / `danger` / `ink-3` 兼作圆点、分隔或轨道填充。** 例如 `bg-ink-3`（凭据筛选圆点、日志级别点）、`index.css:197` 的 `background: var(--ink-3)`、`bg-ok` / `bg-warn`。令牌变深后，这些填充在个别主题里会略暗。应对：审计脚本只做最小明度调整；任务 11 列出全部填充用法并逐处确认；若某处明显变暗，再单独拆填充令牌。
- **对比度基线数字会随矩阵扩大而变化。** 当前 551 项基于 35 组文字/背景令牌；任务 1 按真实共现扩充矩阵后，基线会增加。应对：任务 3 在矩阵定稿后把最终基线数字写回本文件。`--brand-ink` 在修正前不存在，审计时按 `--brand` 回退判定。
- **对比度口径。** HSL 三元组按标准公式转 RGB，半透明背景分别叠加到 `--surface` 与 `--surface-2` 后取对比度较低者，比较使用未四舍五入的 `>= 4.5`。这与浏览器实际合成存在极小偏差（渐变、毛玻璃背景不在矩阵内），由浏览器抽检补充。
- **`text-brand` 与同行 `bg-brand` 元素的色差。** 文字比填充更深（浅色）或更亮（深色）是预期行为；抽检主题设置页和凭据对话框确认无突兀。
- **Tailwind `extend.textColor.brand` 的覆盖粒度。** 写成字符串会吞掉 `text-brand-fg` / `text-brand-hover`。应对：写成含 `DEFAULT/hover/fg/soft/line` 的完整对象，并检查构建产物里这些类仍存在。
- **改动量大（预计净变更超过 500 行时需拆分评估）。** 绝大部分是机械的令牌行替换，由脚本生成并可重复校验；CR 重点看脚本逻辑和 tailwind 映射，不逐行看令牌值。
- **无法在真实后端上完整点验。** 管理后台需登录；若本地没有可用后端，浏览器抽检会受限，验收时会明说未验证的部分。
- **预览区算法与审计脚本是两套实现。** TS 辅助函数与 Python 脚本的步进规则相同但互不共享代码，admin-ui 又没有 JS 单元测试框架。应对：预览区用浏览器控制台脚本对全部 36 主题 × 2 模式 × 2 场景逐个计算并记录失败数，脚本不可用时退到抽样并明说。
- **`brand-hover` 例外（任务 4 实测发现，用户已确认）。** 10 个浅色变体的 `brand` / `brand-hover` 亮度处在中间区间，白字与近黑字都无法同时在两者上达到 4.5。处理方式：只把这 10 个变体的 `--brand-hover` 向更亮方向微调，`--brand` 不动；hover 态会比默认态略亮。逐变体的新旧值由 `--compare-fills` 输出留存。
- **`badge-new-to` 例外（任务 4 实测发现，用户已确认）。** catppuccin、dracula、stripe 三个深色变体的 NEW 徽章渐变较暗一端对纯黑文字也只有 3.19–4.25。处理方式：只把这 3 个变体的 `--badge-new-to` 向更亮方向微调，`--badge-new-from` 不动；渐变会略浅。逐变体新旧值由 `--compare-fills` 输出留存。
- **回滚。** 纯前端 CSS/配置变更，`git revert` 该提交即可恢复，不涉及数据迁移。
