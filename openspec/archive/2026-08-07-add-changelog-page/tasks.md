# 任务清单：add-changelog-page

## 状态：DONE

## 任务

- [x] 后端：新增 `src/admin/changelog_data.rs`，定义 `ReleaseNote`/`ReleaseNoteGroup`/`Bilingual` 数据结构，硬编码 `v2.8.20`~`v2.8.25` 共 6 条中英双语数据；分类分组固定使用「新功能」「优化」「修复」三类，无对应改动的类别不出现（`spec.md 引用：更新日志查询接口`）
- [x] 后端：`src/admin/types.rs` 新增 `AdminReleaseNote`/`AdminReleaseNotesResponse` 序列化类型（`spec.md 引用：更新日志查询接口`）
- [x] 后端：新增 `src/admin/changelog.rs`，实现 `GET /api/admin/changelog` handler，注册路由到 `src/admin/router.rs`，复用现有 Admin 鉴权中间件（`spec.md 引用：更新日志查询接口、未登录访问该接口`）
- [x] 后端：编写单元测试验证 `changelog_data.rs` 数据完整性（版本号格式、仅一条 `is_latest=true`、每条目 `zh`/`en` 字段均非空）与 handler 返回结构（`spec.md 引用：最新版本标记`）
- [x] 前端：`admin-ui/src/api/credentials.ts` 新增 `getChangelog()`；`admin-ui/src/hooks/use-credentials.ts` 新增 `useChangelog()`
- [x] 前端：新增 `admin-ui/src/components/changelog-page.tsx`，实现版本卡片列表、NEW 徽章、分类条目渲染、中英文切换（`spec.md 引用：更新日志页面语言切换、最新版本标记`）
- [x] 前端：`admin-ui/src/components/dashboard.tsx` 接入新导航项（"系统"分组）与 `activeTab` 页面分支
- [x] 验证：`cargo test`/`cargo fmt`/`cargo clippy` 通过；本地构建 admin-ui 并手动核对页面展示与语言切换效果

## 验收标准

- [x] Admin 控制台侧边栏"系统"分组下出现"更新日志"入口，点击可正常打开页面
- [x] 页面展示 `v2.8.20`~`v2.8.25` 共 6 个版本，按版本号降序排列，仅最新版本（`v2.8.25`）带 NEW 徽章
- [x] 页面内可切换中/英文，切换后所有条目文案对应更新，不影响其他 Admin 页面语言
- [x] `GET /api/admin/changelog` 未携带有效 Admin 凭证访问时按现有鉴权中间件行为拒绝
- [x] `cargo test`、`cargo fmt --check`、`cargo clippy -- -D warnings`（本次改动涉及文件范围内）均通过
