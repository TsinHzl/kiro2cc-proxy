## 上下文

admin-ui 需要展示当前支持的模型列表。后端已有 `GET /v1/models`（`src/anthropic/handlers.rs`），但它挂载在面向客户端的 `/v1` 路由分组下，鉴权是普通 API Key（`auth_middleware`），而 admin-ui 的其他所有数据请求都使用独立的 admin key 鉴权（`/api/admin/*`）。若前端直接调用 `/v1/models`，需要额外处理与 admin key 不同的鉴权凭据，体验和安全边界都不清晰。

## 目标 / 非目标

**目标：**
- admin-ui 展示模型列表时，鉴权方式与其他 admin 页面保持一致（admin key）
- 模型数据源单一，避免前端维护一份独立的模型清单造成后续新增模型时的双写遗漏

**非目标：**
- 不为模型列表设计独立的数据模型或元数据扩展（分类、上下文窗口等）
- 不改变 `/v1/models` 对外客户端的行为

## 决策

新增 `GET /api/admin/models`，handler 直接调用提升为 `pub(crate)` 的 `build_model_list()`，与 `/v1/models` 共享同一数据源，只是换了一层鉴权中间件。放弃"前端直连 `/v1/models`"方案，因为那需要 admin-ui 额外持有/输入一个普通 API Key，与现有"只需 admin key 即可使用全部管理功能"的心智模型不一致。

## 风险 / 权衡

- `build_model_list()` 从模块私有提升为 `pub(crate)`，扩大了可见性范围，但仍限制在 crate 内部，不构成对外 API 面的扩大。
- 两个端点（`/v1/models` 与 `/api/admin/models`）返回同一份数据但走不同鉴权路径，需在代码注释中说明二者关系，避免后续维护者误以为是重复实现。
