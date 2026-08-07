# 变更提案：add-changelog-page

## 背景

Admin 控制台目前没有面向管理员的"版本更新历史"展示入口。版本变更信息目前只存在于 git 提交历史与 tag 中（如 `v2.8.25`、`v2.8.24` ...），管理员无法在控制台内直接查阅每个版本做了哪些改动。参照用户提供的 DCC 产品"更新日志"页面设计（版本号 + 日期 + NEW 徽章 + 分类条目列表），在本项目 Admin 控制台新增等价的"更新日志"页面。

## 目标范围

**在范围内：**
- 后端新增一份 changelog 静态数据（编译期硬编码，模式与现有 `build_model_list()` 一致），覆盖 `v2.8.20` ~ `v2.8.25` 共 6 个版本，每条文案携带中/英文双语字段
- 后端新增 `GET /api/admin/changelog` 接口，返回按版本号降序排列的版本列表
- 前端在侧边栏"系统"分组下新增"更新日志"导航项（与"查看日志""设置"的实现方式一致，独立按钮，非数组项）
- 新增"更新日志"页面组件：展示版本号、发布日期、最新版本 NEW 徽章、按分类分组的条目列表
- 页面内提供中/英文切换开关，仅作用于该页面内容
- 后续新版本发布时，通过在后端静态数据中手动追加一条记录来更新页面内容

**不在范围内：**
- 不引入全局 i18n 框架（react-i18next 等），不改造其他 Admin 页面的语言
- 不做"从 git log/tag 自动生成 changelog"的自动化脚本或 CI 集成，本次为人工整理 + 后续手动维护
- 不追溯 `v2.8.20` 之前的历史版本
- 不改动 `user-ui`（面向最终用户的界面）
- 不新增运行时可编辑的 changelog 配置文件（不做类似 `credentials.json` 的运行时读写/热加载能力）

## 技术方案

- **数据存储**：新增 `src/admin/changelog_data.rs`，以 Rust 静态函数（如 `build_release_notes()`）硬编码 `Vec<ReleaseNote>`，每个 `ReleaseNote` 含版本号、发布日期、是否最新、若干分类分组（每个分组含标题 + 若干条目，条目文案为 `{ zh: String, en: String }` 双语结构）。选择编译期硬编码而非运行时配置文件，理由：与项目现有 `build_model_list()` 的模式保持一致，避免引入额外的文件读写/校验/热加载复杂度，且 changelog 内容本就应随代码发布同步更新。分类分组固定使用「新功能」「优化」「修复」三类（对应 `feat`/`refactor`/`fix` 提交类型），某版本若无对应类别的改动则不出现该分组。
- **API**：新增 `AdminReleaseNote` / `AdminReleaseNotesResponse` 类型（`src/admin/types.rs`），新增 `src/admin/changelog.rs` 存放 handler（不复用 `api_keys.rs`，保持单文件单职责，与 `types.rs`/`router.rs` 的既有拆分粒度一致），路由注册在 `src/admin/router.rs`，复用现有 Admin 鉴权中间件，无需新增权限体系
- **前端**：
  - `admin-ui/src/api/credentials.ts` 新增 `getChangelog()`
  - `admin-ui/src/hooks/use-credentials.ts` 新增 `useChangelog()`（react-query）
  - 新增 `admin-ui/src/components/changelog-page.tsx`：版本卡片列表 + NEW 徽章 + 分类条目 + 中英文切换（页面内 `useState<'zh' | 'en'>`，不引入全局 i18n 库）
  - `admin-ui/src/components/dashboard.tsx`：`activeTab` 联合类型新增 `'changelog'`，"系统"分组新增按钮，`<main>` 条件渲染新增分支
- **初始数据来源**：整理 `v2.8.20..v2.8.25` 区间内每个版本的实际提交（已核对 git log），转换为面向管理员的中英文双语说明，过滤掉纯 `chore: bump version` 提交

## 预期影响

- 新增一个只读展示页面，不影响现有账号管理/API Keys/统计/模型/日志/设置等功能
- 新增一个 `GET` 端点，鉴权方式与其他 Admin API 一致，无新增攻击面
- 前端新增文件不影响现有页面的构建产物体积（增量很小）

## 风险

- 双语文案需要人工整理/翻译，初期 6 个版本工作量可控；后续每次发版需要开发者记得手动在 `changelog_data.rs` 追加一条，属于流程习惯风险而非技术风险，不做强制校验（不阻塞发版）
- 若未来版本发布频率很高（如同一天内多次 patch bump），需要人工判断哪些版本值得单独出现在 changelog 中（参考截图 DCC 页面本身也不是每个 patch 版本都出现），本次数据整理已按此原则合并纯 chore 提交
- `is_latest` 由人工在静态数据中显式维护，新增版本时若忘记把旧版本的 `is_latest` 改回 `false`，会导致页面同时出现多个 NEW 徽章；已通过单元测试校验「有且仅有一条 `is_latest=true`」拦截该错误，发布前 `cargo test` 未过即会暴露
