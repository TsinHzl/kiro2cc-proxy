> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# 任务清单：credential-card-tableview

## 状态：ARCHIVED

## 任务
- [x] 重写 `credential-card.tsx`：改为 tableview 行布局，保留所有功能
- [x] 修改 `dashboard.tsx`：凭据列表容器改为 `space-y-2` 单列

## 验收标准
- [ ] 凭据列表呈单列行布局，视觉与 API Keys 页一致
- [ ] 所有功能正常：复选框选择、禁用开关、余额查看、日志、编辑、删除、重置失败
- [ ] inline 优先级编辑可用
- [ ] TypeScript 编译无错误（`npx tsc --noEmit`）
