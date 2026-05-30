# 任务清单：ui-text-credential-to-account

## 状态：ARCHIVED

## 任务
- [x] 替换 `credential-card.tsx` 中的凭据文案
- [x] 替换 `dashboard.tsx` 中的凭据文案
- [x] 替换 `add-credential-dialog.tsx` 中的凭据文案
- [x] 替换 `edit-credential-dialog.tsx` 中的凭据文案
- [x] 替换 `batch-import-dialog.tsx` 中的凭据文案
- [x] 替换 `kam-import-dialog.tsx` 中的凭据文案
- [x] 替换 `batch-verify-dialog.tsx` 中的凭据文案
- [x] 替换 `balance-dialog.tsx` 中的凭据文案
- [x] 替换 `credential-detail-page.tsx` 中的凭据文案
- [x] 替换 `daily-detail-page.tsx` 中的凭据文案
- [x] 替换 `api-keys-panel.tsx` 中的凭据文案
- [x] 替换 `api-key-detail-page.tsx` 中的凭据文案

## 验收标准
- [x] `grep -rn "凭据" admin-ui/src --include="*.tsx" | grep -v "^\s*//" | grep -v "{/\*"` 无命中（剩余均为注释）
- [x] `npm run build` 无编译错误
