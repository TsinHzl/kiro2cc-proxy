> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# 变更提案：credential-card-tableview

## 背景
凭据管理页当前使用 3 列网格卡片布局（`CredentialCard`），信息密度低、操作按钮分散；
API Keys 页已采用 iOS tableview 风格的单列行布局，每行水平排列信息与操作，紧凑易扫描。
用户要求将凭据列表样式统一为与 API Keys 页一致的 tableview 行布局。

## 目标范围

**在范围内：**
- 重写 `credential-card.tsx` 为 tableview 行布局（水平：左侧信息 + 右侧图标按钮）
- 修改 `dashboard.tsx` 凭据列表容器：`grid gap-4 md:grid-cols-2 lg:grid-cols-3` → `space-y-2`
- 保留所有现有功能：复选框、禁用开关、余额查看、日志、编辑、删除、重置失败、inline 优先级编辑

**不在范围内：**
- 统计卡片区域不变
- 工具栏按钮区域不变
- 分页逻辑不变
- 对话框组件不变

## 技术方案

**新行布局结构（参照 ApiKeysPanel `renderKeyCard`）：**
```
<Card py-3 px-4>
  <div flex items-center>
    [左：flex-1]
      行1: [Checkbox] [健康色点] 昵称/#id  [HealthBadge] [当前] [已禁用]
      行2: email（若有）· 最后调用
      行3: 优先级 · 失败 · 成功 · 限流 · RPM · 剩余用量（含进度条）
    [右：操作区]
      Switch · 余额按钮 · 日志按钮 · 编辑按钮 · 重置失败按钮 · 删除按钮
  </div>
</Card>
```

**优先级操作**：保留 inline 编辑（点击数值弹出 input），移除独立的「提高/降低优先级」按钮，由 inline edit 或编辑对话框替代，减少操作区宽度。

**样式参考**：`CardContent py-3 px-4`，操作按钮 `variant="ghost" size="sm" h-8 w-8 p-0`。

## 预期影响
- 视觉：从卡片网格变为紧凑行列表，与 API Keys 页风格一致
- 功能：完全向后兼容，所有传入 prop 不变
- 父组件：`dashboard.tsx` 仅改容器 className，其余逻辑不变

## 风险
- 优先级 inline 编辑在行布局中的交互空间有限 → 保留但缩短 input 宽度（`w-14`）
