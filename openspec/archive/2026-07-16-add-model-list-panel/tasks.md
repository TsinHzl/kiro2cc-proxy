# 任务清单：add-model-list-panel

## 状态：ARCHIVED

## 任务

- [x] 将 `src/anthropic/handlers.rs` 中的 `build_model_list()` 提升为 `pub(crate) fn`；`cargo check` 通过
- [x] 在 `src/admin/api_keys.rs` 新增 `pub async fn get_admin_models(...)` handler（与现有 `get_server_info`/`get_daily_usage` 同类只读 handler 并列；命名避免与 `src/anthropic/handlers.rs` 中 `/v1/models` 端点已存在的 `get_models` 混淆），调用 `build_model_list()` 并返回 `{ object: "list", data: [...] }`
- [x] 在 `src/admin/router.rs` 的 `protected` 路由组新增 `.route("/models", get(get_admin_models))`
- [x] 为新端点补充单元测试：验证返回列表包含当前全部模型 ID（含 gpt-5.6-sol/terra/luna、claude-fable-5、claude-sonnet-5），且与 `build_model_list()` 集合完全一致（`src/admin/api_keys.rs` 新增 `mod tests`）。401 行为由已有的通用 `admin_auth_middleware` 保证（该中间件已应用于 `protected` 分组下全部路由，本次未新增鉴权逻辑；HTTP 级集成测试需引入 `tower` dev-dependency 做 `oneshot` 调用，与 `openspec/config.yaml` 的"不新增外部 crate"约束冲突，且本仓库现有 admin 路由均无此类集成测试先例，故未新增，与之保持一致）
- [x] 在 `admin-ui/src/api/credentials.ts` 新增 `getModels()` 函数，复用文件内已有的模块私有 axios 实例
- [x] 在 `admin-ui/src/hooks/use-credentials.ts` 新增 `useModels()` react-query hook
- [x] 新建 `admin-ui/src/components/model-list-page.tsx`，参照 `daily-stats-page.tsx` 的 `Card`/`CardContent` + 原生 `<table>` 结构，展示 `id/display_name/owned_by/max_tokens`
- [x] 在 `admin-ui/src/components/dashboard.tsx` 中：扩展 `activeTab` 的 `useState` 联合类型（当前为 `'credentials' | 'apikeys' | 'settings' | 'logs'`）新增 `'models'`（同步扩展 `prevTabRef` 的联合类型）；在导航数组"主要"分组下新增"支持模型"项；在 `activeTab` 渲染分支中接入 `ModelListPage`
- [x] `cd admin-ui && npm run build`（或项目既有构建脚本）确认前端编译通过；`cargo check` + `cargo test` 确认后端无回归

## 验收标准

- [ ] admin-ui 侧边栏"主要"分组出现"支持模型"导航项，点击后展示当前 `build_model_list()` 暴露的全部模型（含最新 gpt-5.6 系列）
- [ ] 页面数据来自新端点 `GET /api/admin/models`，使用 admin key 鉴权，与 `/v1/models`（普通 API Key 鉴权）互不影响
- [ ] 未携带 admin key 访问 `/api/admin/models` 返回 401
- [ ] `cargo test` 全量通过；admin-ui 构建无报错
