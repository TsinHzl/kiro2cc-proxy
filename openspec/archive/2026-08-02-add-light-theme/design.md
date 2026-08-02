# 设计文档:add-light-theme

## 上下文

admin-ui、user-ui 是两个独立构建的 Vite + React + Tailwind 前端,无共享包机制。两者当前 `index.css`/`tailwind.config.js` 结构几乎一致(CSS 变量 + `darkMode: 'class'`),但 `:root` 与 `.dark` 数值相同,实际上从未启用真正的主题切换,`index.html` 写死 `class="dark"`。

## 目标 / 非目标

**目标:**
- 用最小改动在现有 CSS 变量 + `darkMode: 'class'` 架构上补齐真正可用的亮/暗切换
- 白天模式配色对齐参考页面暖色系
- 保证暗色模式现状零回归

**非目标:**
- 不引入 next-themes 等第三方主题库
- 不将 admin-ui、user-ui 的主题逻辑抽成共享包
- 不实现跟随系统 `prefers-color-scheme` 的自动模式

## 决策

1. **自建 Hook 而非第三方库**:项目规模小、需求单一(手动切换 + localStorage),`use-theme.ts` 用 ~20 行代码即可实现,引入 next-themes 收益有限。符合 config.yaml 中"不新增外部 crate/依赖,能用标准库或已有依赖解决的不新增"约束(前端侧同理不新增依赖)。
2. **两端各自实现而非抽共享包**:admin-ui、user-ui 是两个独立构建产物,当前无 monorepo 共享包基础设施,为一个小功能新建共享包结构收益不成比例,接受两份重复实现(已在 proposal.md 风险中记录)。
3. **`:root` 承载亮色、`.dark` 承载暗色,而非新增 `.light` 选择器**:与 Tailwind `darkMode: 'class'` 约定保持一致 —— 默认(无 `dark` class)即亮色,添加 `dark` class 即暗色,避免额外的选择器优先级/覆盖问题。
4. **防闪烁用 `index.html` 内联脚本而非 React 层处理**:React 挂载本身有异步开销,必须在任何渲染发生前同步设置好 `<html>` class,唯一可靠位置是 HTML 文档内联 `<script>`。

## 风险 / 权衡

- 两端重复实现主题逻辑存在后续维护漂移风险,已在 proposal.md 中记录为已知风险,本次不处理。
- `neon-*`/`space-*` 硬编码色板文件数量少(6 个),采用人工审查而非自动化重构,权衡了投入产出比。
