# 任务清单：theme-antigravity

## 状态：ARCHIVED

## 任务

- [x] 1. 更新 `admin-ui/index.html`：`<html class="dark">` + Google Fonts
- [x] 2. 更新 `user-ui/index.html`：`<html class="dark">` + Google Fonts
- [x] 3. 更新 `admin-ui/src/index.css`：CSS 变量替换为深空/霓虹配色 + 自定义滚动条
- [x] 4. 更新 `user-ui/src/index.css`：CSS 变量替换为深空/霓虹配色 + 自定义滚动条
- [x] 5. 更新 `admin-ui/tailwind.config.js`：新增 space/neon palette + 字体
- [x] 6. 更新 `user-ui/tailwind.config.js`：新增 space/neon palette + 字体
- [x] 7. 更新 `admin-ui/src/components/credential-card.tsx`：HEALTH_CONFIG 颜色 → neon 暗色风格
- [x] 8. 更新 `admin-ui/src/components/ui/badge.tsx`：success/warning variant → neon 暗色风格
- [x] 9. 更新 `user-ui/src/components/ui/badge.tsx`：success/warning variant → neon 暗色风格
- [x] 10. 更新 `admin-ui/src/components/ui/progress.tsx`：进度条颜色 → neon 系列
- [x] 11. 更新 `user-ui/src/components/ui/progress.tsx`：进度条颜色 → neon 系列

## 验收标准

- [ ] admin-ui 和 user-ui 背景均为近黑深空色（#09090b）
- [ ] 主按钮、焦点环为霓虹紫（#a855f7）
- [ ] 状态徽章（健康/警告/降级/不健康/已禁用）在深色背景上清晰可见
- [ ] Badge success/warning 在深色背景上清晰可见
- [ ] 页面无白色闪烁（initial load 即为暗色）
