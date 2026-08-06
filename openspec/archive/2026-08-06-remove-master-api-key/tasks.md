# 任务清单：remove-master-api-key

## 状态：ARCHIVED

## 任务

### 后端 Rust
- [x] 任务 1：`src/model/config.rs` — 删除 `api_key` 字段（99-100 行）、`Default` 中 `api_key: None`（216 行）、`apply_env_overrides` 中 `API_KEY` 处理块及文档注释（292、304-307 行）
- [x] 任务 2：`src/anthropic/middleware.rs` — 删除 `AppState.api_key` 字段（46-47 行）、构造函数中的赋值（66-68 行）、`auth_middleware` 主 Key 判断分支（135-142 行）及函数头/`ApiKeyContext.id` 相关注释（37-38、121-124 行），`AppState::new` 签名改为不再接收 `api_key` 参数
- [x] 任务 3：`src/admin/middleware.rs` — 删除 `AdminState.master_api_key` 字段（32-33 行）、构造函数初始化（58 行）、`with_master_api_key` builder（71-74 行）
- [x] 任务 4：`src/admin/handlers.rs` — `get_auth_keys` 删除 api_key 部分（151-163 行内相关代码，只保留 admin_api_key 逻辑）；`set_auth_keys` 删除 api_key 校验与更新分支（172-181、196-200 行）；`persist_auth_keys` 删除 api_key 相关参数与写入逻辑（222-240 行内相关代码）
- [x] 任务 5：`src/admin/api_keys.rs` — `get_server_info` 删除 `master_api_key` 字段与赋值，只保留 `version`（20-33 行）
- [x] 任务 6：`src/admin/types.rs` — 删除 `AuthKeysResponse.api_key`（394-396 行）、`SetAuthKeysRequest.api_key`（406-407 行）
- [x] 任务 7：`src/main.rs` — 删除 82-87 行读取/构建/exit(1) 逻辑、196 行传参、227-229 行 `with_master_api_key` 调用、274-278 行日志打印；同步调整 `AppState::new`/`AdminState::new` 调用点参数
- [x] 任务 8：`src/admin/router.rs` — 核对路由注册与 import，确保 `get_server_info`/`get_auth_keys`/`set_auth_keys` 签名变化后正常编译

### 前端 admin-ui
- [x] 任务 9：`admin-ui/src/api/credentials.ts` — `getServerInfo` 返回类型收窄为 `{ version: string }`；`getAuthKeys`/`setAuthKeys` 类型删除 `apiKey` 字段，只保留 `adminApiKey`
- [x] 任务 10：`admin-ui/src/hooks/use-credentials.ts` — `useSetAuthKeys` mutationFn 参数类型收窄为 `{ adminApiKey?: string }`；评估 `useServerInfo`/`version` 是否仍被其他组件消费，决定保留精简版或整体删除（结论：`useServerInfo` 仍用于展示 version，保留精简版）
- [x] 任务 11：`admin-ui/src/components/settings-panel.tsx` — 删除"主 API Key"编辑卡片（34-82 行）及对应 state（`apiKeyDraft`/`editingApiKey`，18、20 行），保留 Admin Password 卡片不变
- [x] 任务 12：`admin-ui/src/components/api-keys-panel.tsx` — 删除"服务连接信息"卡片中主 API Key 展示行（355-368 行）及关联 state/hook（`copiedMaster`、`useServerInfo`，本文件内未被其他用途消费，一并移除），保留 API Base URL 展示

### 部署/启动入口脚本（首轮提案审查发现的遗漏，本轮补全）
- [x] 任务 13：`Dockerfile:39` — 生成 `config.json` 的 `echo` 命令中删除 `\"apiKey\":\"${API_KEY}\",`，`ADMIN_API_KEY` 部分保留不变
- [x] 任务 14：`run-local-service-mac.sh` — 删除头部注释 `# export API_KEY=...`（11 行）、交互式询问 `API_KEY_INPUT`（53-54 行）、写入 `"apiKey": "$API_KEY_INPUT"`（90 行）、缺失检查提示（101-102 行）；`ADMIN_KEY_INPUT`/`ADMIN_API_KEY` 相关逻辑（57 行等）保留不变
- [x] 任务 15：`run-local-service-windows.ps1` — 对称删除 `$API_KEY_INPUT` 询问（44-46 行）、写入 `apiKey = $API_KEY_INPUT`（75 行）、缺失检查提示（93-94 行）；`$ADMIN_KEY_INPUT` 相关逻辑（49 行等）保留不变
- [x] 任务 16：`start_server.sh:104` — 删除帮助文案中的 `API_KEY=sk-your-key` 提示；`ADMIN_API_KEY` 提示（105 行）保留不变
- [x] 任务 16.1：`install_server.sh:60` — 改写"请编辑填入真实 apiKey"提示文案（第 2 轮审查发现的遗漏，本轮补全）

### 配置与文档
- [x] 任务 17：`config.example.json` — 删除 `"apiKey"` 示例字段
- [x] 任务 18：`app/config/config.json`（本地运行配置）— 删除 `"apiKey": ""` 键（已经 AskUserQuestion 单独确认后执行）
- [x] 任务 19：**概念性重写**（非简单删字段）`README.md`（296、305、353、499、520、722 行附近）、`README.en.md`（对应英文段落）—— 将"客户端使用 config.json 中的 apiKey 鉴权"改写为"客户端使用 Admin 后台创建并启用的子 API Key 鉴权"，删除相关配置字段表格行与示例
- [x] 任务 20：`CLAUDE.md:73` — 更新 `app/config/config.json` 说明，去掉 `apiKey` 提及
- [x] 任务 21：**概念性重写** `docs/使用指南.md`（90、99、201、208、220、231、241 行附近）—— 尤其是 FAQ "启动报错 配置文件中未设置 apiKey" 需整段改写或删除（该报错本身在本次改动后不再存在）
- [x] 任务 22：`docs/部署指南_Zeabur.md:27`、`docs/部署指南_VPS.md:53`、`docs/部署指南_NewAPI.md:99` — 删除/改写 `apiKey` 相关配置示例
- [x] 任务 23：`docs/代码速查表.md` — 更新涉及 `api_key` 字段的行号索引与描述（432-434、484 行附近）

### 验证
- [x] 任务 24：`cargo check` + `cargo clippy` + `cargo test` + `cargo fmt --check` 全部通过（`cargo fmt --check` 发现 `src/model/usage.rs:378` 一处格式差异，经 `git log`/`git diff` 核实为本次改动之前已存在、与本次改动无关的历史遗留格式问题，不属于本次变更引入，未修复）
- [x] 任务 25：前端 `admin-ui` 执行类型检查/构建（`tsc`/`vite build` 或项目实际构建命令）通过（`npm run build` = `tsc -b && vite build`，无类型错误，构建成功）
- [x] 任务 26：手动验证：现有子 API Key 正常鉴权可用（有效 Key → 200，无效/缺失 → 401）、Admin 后台鉴权正常（`adminApiKey` Bearer → 200，无鉴权 → 401）、`/api/admin/server-info` 仅返回 `{"version":...}`、`/api/admin/config/auth-keys` 仅返回 `{"adminApiKey":...}`（无 `apiKey` 字段），均实际启动 release 二进制验证通过
- [x] 任务 27：人工走读 `run-local-service-mac.sh` 全文（逐行 Read 核对）确认 `setup_config()` 仅询问 Admin API Key/端口/Region/代理，生成的 config.json 模板无 `apiKey` 字段；`grep -in "apikey"` 复查全文件仅剩 `adminApiKey`/`ADMIN_API_KEY` 匹配；并结合任务 26 中已用真实（无 `apiKey`）config.json 实际启动 release 二进制成功、无任何 API Key 相关报错的结果，确认脚本行为符合预期

## 执行约束
- 每个任务实施前需重新 `Read` 目标文件确认当前实际行号（同文件内多任务顺序执行会因前序删除导致行号偏移，禁止凭本清单中的行号直接盲改）

## 验收标准
- [x] 全仓库不再存在 `Config.api_key`、`AppState.api_key`、`AdminState.master_api_key`、`AuthKeysResponse.api_key`、`SetAuthKeysRequest.api_key`、前端 `masterApiKey`/主 API Key UI 相关代码（已 grep 全仓库复核，残留 `api_key` 均为范围外字段：sub API Key、`CountTokensConfig.api_key`、用户登录子 Key、RPM 按子 Key 统计）
- [x] `Dockerfile`/`run-local-service-mac.sh`/`run-local-service-windows.ps1`/`start_server.sh` 均不再询问、生成或提示 `apiKey`/`API_KEY`（`adminApiKey`/`ADMIN_API_KEY` 不受影响）
- [x] `cargo check`/`cargo clippy`/`cargo test`/`cargo fmt --check` 全部无错误无警告新增（`cargo fmt --check` 中 `src/model/usage.rs` 一处格式差异经核实为本次改动之前已存在，非本次引入）
- [x] 前端构建通过，无 TypeScript 类型错误
- [x] 现有子 API Key 鉴权、额度、绑定账号功能不受影响（实际启动 release 二进制验证：有效 Key → 200，无效/缺失 → 401）
- [x] Admin 后台登录（`adminApiKey`）功能不受影响（实际验证：Bearer adminApiKey → 200，无鉴权 → 401）
- [x] `app/config/config.json` 中不再含 `"apiKey"` 键，服务正常启动（额外发现该文件历史提交含泄露的明文 `adminApiKey`，已按用户确认执行 `git rm --cached` + `.gitignore`，密码轮换需用户自行处理，详见已知问题记录）
