# 变更提案：add-model-list-panel

## 背景

admin-ui 目前提供"账号管理""API Keys""每日统计""查看日志""设置"等栏目，但管理员无法在面板内直接查看当前代理支持的完整模型列表（Claude Sonnet 5/4.5/4.6、Opus 4.5/4.6/4.7/4.8、Fable 5、Haiku 4.5、DeepSeek 3.2、GLM-5、MiniMax M2.1/M2.5、Qwen3-Coder、GPT-5.6 Sol/Terra/Luna 等），只能查阅 README 或直接调用 `/v1/models`。需要在 admin-ui 新增一个只读的"支持模型"栏目，直接在面板内展示该列表。

## 目标范围

**在范围内：**
- 新增后端只读端点 `GET /api/admin/models`，复用现有 `build_model_list()` 生成的数据（`src/anthropic/handlers.rs`），挂载在 `/api/admin` 路由分组下，使用现有 admin 鉴权（`x-api-key` + adminApiKey），与 `/v1/models` 的普通 API Key 鉴权隔离。
- 将 `build_model_list()` 提升为 `pub(crate)`，供 `src/admin/api_keys.rs`（该文件承载现有只读信息类 handler，如 `get_server_info`/`get_daily_usage`）中新增的 `get_admin_models` handler 调用，不修改其内部逻辑与返回的模型集合。
- admin-ui 侧边栏"主要"分组下新增导航项"支持模型"，新建只读页面组件，沿用 `daily-stats-page.tsx` 的既有模式（`Card`/`CardContent` 内嵌原生 `<table>`，非 shadcn `Table` 组件——本项目 `components/ui/` 下未引入该组件），展示 `id / display_name / owned_by / max_tokens` 四个现有字段。
- 复用 admin-ui 现有的 axios 实例（`admin-ui/src/api/credentials.ts` 内的模块私有 `api`，`baseURL: '/api/admin'` + `x-api-key`）与 `@tanstack/react-query` hooks 约定（`admin-ui/src/hooks/use-credentials.ts`）、现有 tab 切换模式（`dashboard.tsx` 内的 `activeTab` 三元渲染）。

**不在范围内：**
- 不新增模型元数据（分类、真实上下文窗口、是否支持 thinking 等），字段严格复用后端现有返回结构。
- 不修改 `map_model()` 的映射逻辑或 `build_model_list()` 已暴露的模型集合。
- 不引入前端路由库（继续沿用现有的 `activeTab` 手写切换模式）。
- 不支持在面板内编辑/禁用模型（纯只读展示）。

## 技术方案

- 后端：在 `src/admin/api_keys.rs` 新增 `pub async fn get_admin_models(...)` handler（命名区分于 `src/anthropic/handlers.rs` 中已存在的 `/v1/models` 端点 handler `get_models`，避免同名混淆），调用提升后的 `build_model_list()`，返回结构与 `/v1/models` 一致的 `{ object: "list", data: [...] }`；在 `src/admin/router.rs` 新增 `.route("/models", get(get_admin_models))`（挂载后即 `/api/admin/models`）。
- 鉴权：复用 admin 路由分组既有的 admin key 鉴权中间件，不新增鉴权逻辑。
- 前端：在 `admin-ui/src/api/credentials.ts` 新增 `getModels()` 函数（复用文件内已有的模块私有 axios 实例），在 `admin-ui/src/hooks/use-credentials.ts` 新增 `useModels()` react-query hook；新建 `admin-ui/src/components/model-list-page.tsx`，参照 `daily-stats-page.tsx` 的 `Card`/`CardContent` + 原生 `<table>` 结构渲染只读列表；在 `dashboard.tsx` 的导航数组与 `activeTab` 渲染分支中各加一项。

## 预期影响

- 新增一个只读 GET 端点与一个只读前端页面，不影响现有账号管理、API Keys、统计、日志、设置功能。
- 不改变 `/v1/models` 端点行为，Claude Code 等客户端侧无感知。
- `build_model_list()` 从 `handlers.rs` 私有函数变为 `pub(crate)`，需确认无命名冲突、无需修改其现有单元测试。

## 风险

- `build_model_list()` 提升可见性范围时可能与模块内其他同名项冲突 —— 通过 `cargo check` 验证。
- 前端新增页面若未正确复用现有 admin 鉴权，会导致 401 —— 通过手动验证 admin key 场景排除。
