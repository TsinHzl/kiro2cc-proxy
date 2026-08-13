# 任务清单：sidebar-collapse-toggle

## 状态：ARCHIVED

## 任务
- [x] 从 `lucide-react` 新增导入 `PanelLeftClose`/`PanelLeftOpen`；新增 `sidebarCollapsed` state（沿用 `use-theme.ts` 的 `readStoredXxx()` 初始化 + 切换时 `localStorage.setItem` 模式，key: `sidebar-collapsed`）
- [x] 新增折叠/展开切换按钮（header 区域，图标随状态切换）
- [x] Header 区域收起态隐藏 Logo 文案/副标题，Logout 按钮仅保留图标
- [x] Nav 分组标题 + 按钮 label 收起态隐藏，图标居中并同时添加 `title` + `aria-label`
- [x] Footer 区域收起态隐藏 GitHub 文字/版本号，保留 GitHub 图标（点击跳转不变）与主题切换图标
- [x] `<aside>` 宽度与 `<main>` 左边距随折叠状态联动（`232px`/`64px`）
- [x] 手动验证：展开态与折叠前行为一致（无回归）；收起态各图标可点击切换页面；刷新页面折叠状态保持

## 验收标准
- [x] 展开状态下侧边栏宽度保持 232px，所有文字/label/图标正常展示，与折叠功能上线前行为一致（无回归）
- [x] 点击折叠按钮后，侧边栏收起为仅图标（无文字），宽度收窄为 64px，footer 的 GitHub 图标与主题图标仍可见
- [x] 收起状态下点击任意图标按钮可正确切换到对应页面（功能不变）
- [x] 折叠状态刷新页面后保持
- [x] `npm run build`（admin-ui）无报错
