# 任务清单：admin-ui-sidebar-layout

## 状态：ARCHIVED

## 任务
- [x] 重构 dashboard.tsx 外层布局：移除顶部 header，改为 flex 行布局容器
- [x] 实现左侧固定 Sidebar 组件（品牌区 + 导航区 + 底部操作区）
- [x] 实现 Sidebar 导航项：凭据管理 / API Keys / 每日统计 / 设置，带 active 左侧 accent 竖线
- [x] 更新 Main 内容区：`ml-[232px]`，padding 对齐 windsurf 风格（`px-9 py-7`）
- [x] 为凭据管理页面添加 Page Header（page-title + page-subtitle），替代原内联 h2

## 验收标准
- [ ] Sidebar 固定在左侧，不随内容滚动
- [ ] 三个主导航项 + 设置可正常切换，与原 tab 行为一致
- [ ] 活动导航项有左侧 accent 竖线高亮
- [ ] Main 内容区不被 sidebar 遮挡
- [ ] 所有原有功能（添加/删除/验活凭据、API Keys、设置、子详情页）正常工作
