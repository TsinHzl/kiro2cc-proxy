# 变更提案：rename-admin-psw-field

## 背景

后台管理员登录凭证目前在代码、配置文件和 API 中统一使用 "Admin API Key" / `adminApiKey` 命名。但该凭证本质上是一个**登录密码**（用于登录 Admin 控制台，而非像"API Key 管理"页面里那样代表可被第三方程序调用的密钥），继续沿用 "API Key" 命名容易与"API Key 管理"功能（`src/admin/api_keys.rs`，管理面向第三方调用方签发的子 API Key）混淆。

前端 UI 层面此前已局部改用 "Admin Password" 文案（`settings-panel.tsx` 的标签、`AdminErrorResponse::authentication_error()` 的错误文案），但底层数据字段名（`config.json` 的 `adminApiKey`、Admin REST API 请求/响应体的 `adminApiKey`、环境变量 `ADMIN_API_KEY`、Rust 内部标识符 `admin_api_key`）仍是旧命名，导致概念名称不一致。

## 目标范围

**在范围内：**
- `config.json` 持久化字段：`adminApiKey` → `adminPsw`（读取时兼容旧字段名）
- 环境变量：`ADMIN_API_KEY` → `ADMIN_PSW`（未设置 `ADMIN_PSW` 时回退读取 `ADMIN_API_KEY`）
- Admin REST API 协议字段：
  - `GET /api/admin/config/auth-keys` 响应体 `adminApiKey` → `adminPsw`
  - `PUT /api/admin/config/auth-keys` 请求体 `adminApiKey` → `adminPsw`（同时兼容接受旧字段名 `adminApiKey`）
- Rust 内部标识符同步改名：`admin_api_key` → `admin_psw`（`src/model/config.rs`、`src/admin/types.rs`、`src/admin/handlers.rs`、`src/admin/middleware.rs`、`src/admin/mod.rs`、`src/admin/log_handler.rs`、`src/main.rs`）
- 前端同步适配：`admin-ui/src/api/credentials.ts`、`admin-ui/src/hooks/use-credentials.ts`、`admin-ui/src/components/settings-panel.tsx`、`admin-ui/src/lib/storage.ts`（localStorage key 名）
- 部署/脚本层同步改名：`Dockerfile`、`start_server.sh`、`run-local-service-mac.sh`、`run-local-service-windows.ps1`、`install_server.sh`、`config.example.json`
- 文档同步更新：`README.md`、`README.en.md`、`CLAUDE.md`、`docs/使用指南.md`、`docs/代码速查表.md`、`docs/部署指南_Zeabur.md`、`docs/部署指南_VPS.md`

**不在范围内：**
- `src/admin/api_keys.rs` 及其对应的"API Key 管理"功能（子 API Key 的创建/列表/删除，供第三方调用方使用）—— 这是完全不同的概念，不做任何改动
- `docs/superpowers/plans/*`、`docs/superpowers/specs/*` 下的历史归档规划文档 —— 属于带日期的历史快照，不做回溯更新
- `.claude/worktrees/` 下其他并行 worktree 的文件 —— 不属于本次改动的工作树

## 技术方案

- **向后兼容（已确认）**：`config.json` 字段与 Admin REST API 请求体字段均通过 serde `#[serde(alias = "adminApiKey")]` 兼容旧字段名读取；序列化/持久化统一只写出新字段名 `adminPsw`。
- **环境变量（已确认）**：新增 `ADMIN_PSW`，未设置时回退读取旧的 `ADMIN_API_KEY`，保证已有 Docker/Zeabur 部署无需立即修改环境变量。
- **内部标识符（已确认）**：Rust 侧所有 `admin_api_key` 标识符同步改名为 `admin_psw`，保持代码内命名与新协议字段一致。
- **持久化清理**：`src/admin/handlers.rs::persist_auth_keys` 直接操作 `serde_json::Value`，写入时会顺带移除 JSON 中残留的旧键 `adminApiKey`（如果存在），避免 config.json 长期累积重复/过期字段。
- **前端 localStorage key**：`admin-ui/src/lib/storage.ts` 中的 key 常量由 `'adminApiKey'` 改为 `'adminPsw'`。改名后已登录用户浏览器中保存的旧值失效，需重新在登录页输入一次密码 —— 影响面很小（仅本地存储，不涉及数据丢失）。

## 预期影响

- **兼容性**：已部署实例的 `config.json`（旧字段名 `adminApiKey`）和环境变量（`ADMIN_API_KEY`）无需手动迁移即可继续工作；下次通过 Admin 面板保存密码后，`config.json` 会自动升级为新字段名并清除旧字段。
- **前端**：管理员登录状态会因 localStorage key 改名而失效一次，需要重新登录；属预期内的一次性影响。
- **文档/脚本**：不影响运行时行为，仅同步文案与新字段名，避免误导新用户。

## 风险

- **遗漏某个引用点**：字段名分散在 Rust/TS/Shell/PowerShell/Markdown 多处，存在遗漏风险 —— 通过 tasks.md 逐文件枚举 + 最终全仓库 grep 复查兜底。
- **安全敏感面**：改动涉及认证相关代码（`admin/middleware.rs` 的鉴权比较逻辑），按 `00-change-gate.md` 规则将触发强制第二轮 Code Review。
- **持久化清理逻辑引入 bug**：`persist_auth_keys` 新增"删除旧键"逻辑，需确保只在成功写入新键之后才删除旧键，避免异常情况下密钥丢失。
- **归档时新旧需求块并存混淆**：`openspec/specs/admin-api/spec.md` 中已存在 3 处引用旧字段名 `adminApiKey`/`admin_api_key` 的历史需求块（"Admin 只读模型列表端点"鉴权说明、"`GET /api/admin/config/auth-keys` 响应体不再包含主 API Key"、"`PUT /api/admin/config/auth-keys` 不再支持修改主 API Key"），Phase 4 归档时不能简单追加合并，必须手动替换这些旧措辞（见 tasks.md 任务 27），否则主 spec 会同时存在新旧字段名描述同一端点。
