# 任务清单：rename-admin-psw-field

## 状态：ARCHIVED

## 任务

### 后端 Rust

- [x] 任务 1：`src/model/config.rs` — `Config.admin_api_key` 字段改名为 `admin_psw`，添加 `#[serde(alias = "adminApiKey")]`；同步更新 `Default` 实现（`admin_api_key: None,` → `admin_psw: None,`）与字段文档注释
  - spec.md 引用：config.json 字段改名（读取兼容旧字段，写入统一新字段）
- [x] 任务 2：`src/model/config.rs::apply_env_overrides` — 环境变量覆盖逻辑改为优先读取 `ADMIN_PSW`，未设置时回退读取 `ADMIN_API_KEY`（`design.md` 决策 2 的显式 `if/else if` 写法）；同步更新函数文档注释中的环境变量列表
  - spec.md 引用：环境变量改名（回退兼容旧变量）
- [x] 任务 3：`src/admin/types.rs` — `AuthKeysResponse.admin_api_key` 改名为 `admin_psw`；`SetAuthKeysRequest.admin_api_key` 改名为 `admin_psw` 并添加 `#[serde(alias = "adminApiKey")]`
  - spec.md 引用：认证密钥查询响应字段改名、认证密钥修改请求字段改名（兼容旧字段）
- [x] 任务 4：`src/admin/handlers.rs` — `get_auth_keys`/`set_auth_keys` 中的局部变量与字段引用改名为 `admin_psw`；错误提示文案 `"adminApiKey 不能为空（Admin Password）"` 改为 `"adminPsw 不能为空（Admin Password）"`
  - spec.md 引用：新字段为空字符串时拒绝
- [x] 任务 5：`src/admin/handlers.rs::persist_auth_keys` — 写入 JSON 键由 `adminApiKey` 改为 `adminPsw`；写入成功后显式移除旧键 `adminApiKey`（若存在），实现 `design.md` 决策 3 的持久化清理策略
  - spec.md 引用：通过 Admin 面板保存密码后旧字段被清理
- [x] 任务 6：`src/admin/middleware.rs` — `AdminState.admin_api_key` 字段、`AdminState::new()` 构造参数改名为 `admin_psw`；同步更新字段文档注释与鉴权比较逻辑中的引用
- [x] 任务 7：`src/admin/mod.rs` — 文档注释示例代码中的 `admin_api_key` 改名为 `admin_psw`
- [x] 任务 8：`src/admin/log_handler.rs` — `state.admin_api_key` 引用改名为 `state.admin_psw`
- [x] 任务 9：`src/main.rs` — 配置提取变量、日志警告文案（"admin_api_key 配置为空"）、`admin_api_key_shared` 局部变量、`AdminState::new()` 调用实参改名为 `admin_psw` 系列命名

### 前端 admin-ui

- [x] 任务 10：`admin-ui/src/api/credentials.ts` — `getAuthKeys()`/`setAuthKeys()` 的 TS 类型与请求体字段 `adminApiKey` 改为 `adminPsw`
- [x] 任务 11：`admin-ui/src/hooks/use-credentials.ts` — `useSetAuthKeys` mutation payload 类型字段改为 `adminPsw`
- [x] 任务 12：`admin-ui/src/components/settings-panel.tsx` — 调用 `setAuthKeysMut`/读取 `authKeysData` 处的字段引用改为 `adminPsw`；同步改名本地状态变量及其 setter 函数：`adminApiKeyDraft`/`setAdminApiKeyDraft` → `adminPswDraft`/`setAdminPswDraft`，`editingAdminApiKey`/`setEditingAdminApiKey` → `editingAdminPsw`/`setEditingAdminPsw`（共 7 处调用点需同步更新）（UI 展示文案已是 "Admin Password"，无需改动）
- [x] 任务 13：`admin-ui/src/lib/storage.ts` — `API_KEY_STORAGE_KEY` 常量值由 `'adminApiKey'` 改为 `'adminPsw'`

### 部署 / 脚本 / 示例配置

- [x] 任务 14：`Dockerfile` — 生成 config.json 的 shell 逻辑中，JSON 键 `adminApiKey` 改为 `adminPsw`，读取的环境变量由 `ADMIN_API_KEY` 改为 `${ADMIN_PSW:-$ADMIN_API_KEY}`（新变量优先、回退旧变量）
- [x] 任务 15：`start_server.sh` — 提示文案 `ADMIN_API_KEY=sk-admin-key` 改为 `ADMIN_PSW=sk-admin-key`
- [x] 任务 16：`run-local-service-mac.sh` — 交互式提示、生成的 config.json 模板字段（`"adminApiKey"`）、后续 grep 检测逻辑（`grep -q '"adminApiKey"'`）均改为 `adminPsw`；注释中的 `ADMIN_API_KEY` 示例改为 `ADMIN_PSW`
- [x] 任务 17：`run-local-service-windows.ps1` — 同任务 16，PowerShell 版本的交互提示、`$config.adminApiKey`、`Select-String -Pattern '"adminApiKey"'` 均改为 `adminPsw`
- [x] 任务 18：`install_server.sh` — 提示文案中的 `adminApiKey` 改为 `adminPsw`
- [x] 任务 19：`config.example.json` — 示例字段 `"adminApiKey"` 改为 `"adminPsw"`
- [x] 任务 20：`app/config/config.json`（本地开发实例）— 手动同步改为 `adminPsw` 字段名，验证向后兼容 alias 生效后本地服务仍可正常启动
- [x] 任务 21：`server.env.example` — 第 8 行注释示例 `# ADMIN_API_KEY=sk-admin-your-key` 改为 `# ADMIN_PSW=sk-admin-your-key`

### 文档

- [x] 任务 22：`README.md` / `README.en.md` — 涉及 `adminApiKey`/`ADMIN_API_KEY`/"Admin API Key" 的示例与说明文字改为 `adminPsw`/`ADMIN_PSW`/"Admin Password"
- [x] 任务 23：`CLAUDE.md` — 涉及 `adminApiKey` 的引用同步改名（若为架构说明性文字则同时更新措辞）
- [x] 任务 24：`docs/使用指南.md` — 交互式安装提示文案、"Admin API Key" 措辞、`adminApiKey` 字段引用全部改为 `adminPsw`/"Admin Password"
- [x] 任务 25：`docs/代码速查表.md` — `adminApiKey`/`admin_api_key` 引用同步改为新命名，确保代码定位速查表准确
- [x] 任务 26：`docs/部署指南_Zeabur.md` — 环境变量/字段示例改为 `ADMIN_PSW`/`adminPsw`，可附带一句向后兼容说明（旧变量仍可用）
- [x] 任务 27：`docs/部署指南_VPS.md` — 同任务 26

### 归档前置处理（重要 — 避免新旧需求块并存混淆）

- [x] 任务 28：已直接编辑 `openspec/specs/admin-api/spec.md`（未等到归档步骤，提前完成）：
  - "Admin 只读模型列表端点"需求两处场景的 `x-api-key（等于 adminApiKey）`/`不等于 adminApiKey` 已改为 `adminPsw`（前者附带"历史部署兼容旧字段名 adminApiKey"说明）
  - "`GET /api/admin/config/auth-keys` 响应体不再包含主 API Key"需求块标题与响应体示例已改为 `adminPsw`
  - "`PUT /api/admin/config/auth-keys` 不再支持修改主 API Key"需求块已改名为 `adminPsw`/`admin_psw`，并新增"使用旧字段名修改（向后兼容）"与"新字段为空字符串时拒绝"两个场景
  - 新增两个此前主 spec.md 未覆盖的需求块："`config.json` 字段改名（读取兼容旧字段，写入统一新字段）"与"环境变量改名（回退兼容旧变量）"，内容取自本次变更自身 `specs/admin-api/spec.md`，避免与已就地更新的"响应字段改名"需求重复描述
  - 本次变更自身 `specs/admin-api/spec.md` 后续归档合并时，"认证密钥查询响应字段改名"/"认证密钥修改请求字段改名"两块无需再次合并（已通过就地编辑体现），仅需合并"config.json 字段改名"/"环境变量改名"两块（本任务已提前手动合并，归档步骤 3 可直接跳过对应部分）
  - 本任务编辑经独立 CR（PASS），修复 2 条 Medium 建议：消除"PUT"需求与"config.json 字段改名"需求间的重复持久化清理描述（改为交叉引用），并将"空字符串拒绝"场景扩展为新旧字段名均覆盖

### 验证

- [x] 任务 29：`cargo check` + `cargo clippy` + `cargo fmt --check` 全部通过（本次改动涉及文件均通过；`src/model/usage.rs` 存在与本次改动无关的预存在 fmt 格式问题，未处理）
- [x] 任务 30：`cargo test` 全部通过（如现有测试覆盖 `admin_api_key` 相关逻辑，需同步改名而非删除）
- [x] 任务 31：`cd admin-ui && npx tsc -b --noEmit` 通过；`npm run build`（或仓库既有构建命令）成功
- [x] 任务 32：全仓库复查 —— `grep -rn "adminApiKey\|admin_api_key\|ADMIN_API_KEY" src/ admin-ui/src/ *.sh *.ps1 *.env.example Dockerfile config.example.json README*.md CLAUDE.md docs/使用指南.md docs/代码速查表.md docs/部署指南_*.md openspec/specs/`，确认仅剩"作为向后兼容 alias/回退值"出现的必要引用（`src/admin/handlers.rs:209` 旧键清理、`src/admin/types.rs:404`/`src/model/config.rs:134` 的 alias 声明、`src/model/config.rs:293,319` 的 env 回退分支与文档注释、`Dockerfile:39` 的 `$ADMIN_API_KEY` 回退），发现并修正了 1 处遗漏的过期注释（`run-local-service-mac.sh:117`，功能代码已改名但注释未同步），修正后已无遗漏；`openspec/specs/admin-api/spec.md` 中任务 28 所列旧措辞按计划延后至 Phase 4 归档前处理，本任务范围内不含该项
- [x] 任务 33：本地手工验证向后兼容 —— 用仅含旧字段 `adminApiKey` 的 config.json 启动服务，确认登录正常；再通过 Admin 面板修改一次密码，确认 config.json 收敛为仅含 `adminPsw`（已用调试二进制 + 临时配置实测通过）
- [x] 任务 34：本地手工验证 Admin API 向后兼容 —— 用旧字段名 `{"adminApiKey": "..."}` 调用 `PUT /api/admin/config/auth-keys`，确认返回成功且密码生效（已实测通过，返回 200 且新密码鉴权成功）

## 验收标准

- [x] Admin REST API 新字段 `adminPsw` 生效，旧字段名 `adminApiKey` 仍可用于请求体输入（向后兼容）
- [x] `config.json` 新增/修改密码后统一收敛为 `adminPsw` 字段，旧字段 `adminApiKey` 被自动清理
- [x] 环境变量 `ADMIN_PSW` 生效，未设置时回退到 `ADMIN_API_KEY` 仍可正常工作
- [x] Rust 内部标识符、前端字段引用、部署脚本、文档均已同步改名，全仓库复查（任务 32）无遗漏，`openspec/specs/admin-api/spec.md` 中的历史需求块已同步替换（任务 28）
- [x] `cargo check`/`clippy`/`fmt`/`test` 与前端 `tsc`/构建均通过
- [x] `src/admin/api_keys.rs`（子 API Key 管理功能）未被本次改动影响
