## 新增需求

### 需求：更新日志查询接口

Admin 控制台需要提供一个只读接口，返回按版本号降序排列的更新日志列表，供前端"更新日志"页面渲染。

#### 场景：管理员打开更新日志页面

- **WHEN** 已登录的管理员访问 Admin 控制台"更新日志"页面
- **THEN** 前端调用 `GET /api/admin/changelog`，接口返回 `data: AdminReleaseNote[]`，按版本号从新到旧降序排列；每个 `AdminReleaseNote` 含 `version`（如 `"2.8.25"`）、`date`（`YYYY-MM-DD`）、`is_latest: bool`、`groups: ReleaseNoteGroup[]`；每个分组含 `title_zh`/`title_en` 与 `items: { zh: String, en: String }[]`

#### 场景：未登录访问该接口

- **WHEN** 未携带有效 Admin 会话/凭证的请求访问 `GET /api/admin/changelog`
- **THEN** 接口按现有 Admin API 统一鉴权中间件行为返回未授权响应，与其他 `/api/admin/*` 端点一致，不单独实现鉴权逻辑

### 需求：更新日志页面语言切换

更新日志页面需要支持中/英文内容切换，且切换范围仅限该页面。

#### 场景：用户切换语言

- **WHEN** 用户在更新日志页面点击语言切换控件（中文 ⇄ English）
- **THEN** 页面内所有版本号说明文案（分组标题、条目文案）切换为对应语言渲染；版本号、日期等非文案字段不受语言切换影响；切换状态仅存在于该页面组件内部，不影响 Admin 控制台其他页面的语言（其他页面仍为纯中文）

### 需求：最新版本标记

#### 场景：标记最新版本

- **WHEN** changelog 数据中某个版本的 `is_latest` 为 `true`
- **THEN** 该版本卡片在页面上显示 "NEW" 徽章；其余版本不显示该徽章；`is_latest` 由后端静态数据显式标记（非前端根据数组顺序或版本号大小推断），避免数据与展示逻辑耦合出错
