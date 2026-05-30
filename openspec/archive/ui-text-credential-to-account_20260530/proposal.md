# 变更提案：ui-text-credential-to-account

## 背景
Admin UI 中所有面向用户的文案使用"凭据"一词，用户习惯使用"账号"，需统一替换以提升可读性。

## 目标范围
**在范围内：**
- 所有 `.tsx` / `.ts` 文件中 JSX 字符串、toast 消息、dialog 标题/描述、placeholder、button label 等用户可见文案
- 涉及文件：`credential-card.tsx`、`dashboard.tsx`、`add-credential-dialog.tsx`、`edit-credential-dialog.tsx`、`batch-import-dialog.tsx`、`kam-import-dialog.tsx`、`batch-verify-dialog.tsx`、`balance-dialog.tsx`、`credential-detail-page.tsx`、`daily-detail-page.tsx`、`api-keys-panel.tsx`、`api-key-detail-page.tsx`

**不在范围内：**
- 代码注释（`//` 单行注释 和 `{/* */}` JSX 注释）
- 变量名、函数名、类型名（代码标识符）
- 后端 API 字段名
- `.ts` 文件（`use-credentials.ts`、`api/credentials.ts`、`types/api.ts` 中的"凭据"均为注释）

**已知副作用（明确确认）：**
- `凭据 #${id}` → `账号 #${id}`（9 处 ID 引用，语义可接受）
- `凭据编号` → `账号编号`（`api-keys-panel.tsx:1040` placeholder，语义可接受）
- `凭据管理`（`dashboard.tsx:663` h1 标题）→ `账号管理`，符合预期

## 技术方案
纯文本替换，无逻辑变更。替换规则（按优先级顺序）：
1. 特例优先：`账号凭据` → `账号`（`dashboard.tsx:664`，原文"管理 Kiro 账号凭据与负载均衡"→"管理 Kiro 账号与负载均衡"）
2. 通用规则：`凭据` → `账号`（所有其余 UI 文案，不含注释）

## 预期影响
纯文案变更，不影响任何逻辑、API 调用、状态管理。

## 风险
无。
