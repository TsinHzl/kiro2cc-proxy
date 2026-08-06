# 变更提案：remove-master-api-key

## 背景

`config.json` 中的 `apiKey`（"主 API Key"）是 `/v1/messages` 等 Anthropic 兼容接口鉴权的第一优先级、无限制兜底凭证：请求头 Key 只要等于它就直接放行（`id=0`），完全跳过 `ApiKeyManager` 的启用/过期/额度检查。它与 Admin 后台登录密码（`adminApiKey`）、以及页面上 3 个已绑定 Kiro 账号的子 API Key（`sk-xxx`，存于独立的 `app/config/api_keys.json`）是三套完全独立的凭证。

用户确认该主 Key 已上线数月，从未设置/使用过（当前 `app/config/config.json` 中为空字符串 `""`），希望在代码层面彻底移除这一整套功能，而不是仅靠留空字符串"事实性禁用"，以便：
- 从 `config.json` 中彻底删除 `apiKey` 字段
- 消除一条无额度限制、无审计（不计入子 Key 用量统计）的认证旁路，降低攻击面
- 简化 Admin 前端 UI（去掉"主 API Key"展示卡片和编辑入口）

## 目标范围

**在范围内：**
- 后端：删除 `Config.api_key` 字段及其 env override（`API_KEY`）、`main.rs` 中的读取/构建/exit(1)/日志打印逻辑、`AppState.api_key` 字段及 `auth_middleware` 中的主 Key 判断分支、`AdminState.master_api_key` 字段及 `with_master_api_key` builder、`get_auth_keys`/`set_auth_keys`/`persist_auth_keys` 中的 apiKey 部分、`get_server_info` 中的 `masterApiKey` 字段、`AuthKeysResponse.api_key`/`SetAuthKeysRequest.api_key` 字段
- 前端：删除 `settings-panel.tsx` 中"主 API Key"编辑卡片、`api-keys-panel.tsx` 中"主 API Key"展示行、`credentials.ts`/`use-credentials.ts` 中对应的类型字段与请求
- **部署/启动入口脚本（首轮审查发现的遗漏，已补全）**：
  - `Dockerfile:39` —— 生成 `config.json` 时不再从 `API_KEY` 环境变量注入 `apiKey` 字段
  - `run-local-service-mac.sh`（11、53-54、57、90、101-102 行）与 `run-local-service-windows.ps1`（44-46、49、75、93-94 行）—— 移除交互式询问 "API Key" 并写入 `apiKey` 的逻辑、移除"缺少 apiKey"的检查提示（Admin API Key/`ADMIN_API_KEY` 询问逻辑保留不变）
  - `start_server.sh:104` —— 移除帮助文案中的 `API_KEY=sk-your-key` 提示（`ADMIN_API_KEY` 提示保留）
  - `install_server.sh:60` —— 改写提示文案"已复制示例配置到 ... 请编辑填入真实 apiKey"（该提示在字段移除后已失效）
- 文档（**概念性重写，非简单删字段**——需将"客户端使用 config.json 中的 apiKey 鉴权"改写为"客户端使用 Admin 后台创建的子 API Key 鉴权"）：`README.md`、`README.en.md`、`CLAUDE.md`、`config.example.json`、`docs/使用指南.md`、`docs/部署指南_Zeabur.md`、`docs/部署指南_VPS.md`、`docs/部署指南_NewAPI.md`、`docs/代码速查表.md` 中所有 Docker 部署指引、配置字段表格、故障排查 FAQ（如"启动报错 配置文件中未设置 apiKey"）
- 本地运行配置 `app/config/config.json`：移除 `"apiKey"` 键（征得用户确认后由 Claude 直接编辑，因该文件已 gitignore、不涉及版本控制）

**不在范围内：**
- Admin 后台登录密码（`adminApiKey`）机制 —— 完全保留，不做任何改动
- 3 个已绑定 Kiro 账号的子 API Key（`ApiKeyManager`/`api_keys.json`）—— 完全保留，鉴权/额度/统计逻辑不变
- `src/token.rs` 中 `CountTokensConfig.api_key`（外部 count_tokens 服务密钥，同名不同义字段）—— 不改动
- `src/user/handlers.rs` 中 `LoginRequest.api_key`（用户自助查询用量提交的子 Key）—— 不改动
- `admin-ui` 中变量名同为 `apiKey` 但实际存储 Admin 密码的 `localStorage`/`storage.ts`/`login-page.tsx` —— 不改动
- `test-cache.sh`、`scripts/test-cache-effectiveness.sh` 中的 `API_KEY` 变量 —— 实为测试脚本使用的子 API Key（`sk-...`），与本次移除的 `config.json` 主 `apiKey` 字段同名不同义，不改动

## 技术方案

1. **`auth_middleware` 简化**：移除"主 Key 优先放行"分支（`src/anthropic/middleware.rs:135-142`），鉴权完全收敛为"仅子 Key 校验"。`ApiKeyContext.id` 不再有 `0 = 主密钥` 的特殊语义，同步删除相关注释。
2. **启动流程**：`main.rs` 不再读取/校验 `apiKey`，不再因缺失而 `exit(1)`；`AppState::new`/`AdminState` 构造函数签名相应简化（移除 `api_key`/`master_api_key` 参数与 builder）。
3. **Admin 接口收窄而非整体删除**：`GET/PUT /api/admin/config/auth-keys` 路由保留，只服务 `adminApiKey`（因为该接口本身承载了 Admin 密码修改功能，不能整体删除）；`GET /api/admin/server-info` 保留但响应体只剩 `version`（前端 `useServerInfo` 若确认无其他消费者一并删除，否则精简返回类型）。
4. **serde 兼容性**：`Config` 无 `deny_unknown_fields`，因此即便旧 `config.json` 残留 `"apiKey"` 键，反序列化也会静默忽略，不会报错——但仍会主动清理 `app/config/config.json` 和 `config.example.json` 中的该字段，避免歧义。
5. **前端**：`settings-panel.tsx`/`api-keys-panel.tsx` 中主 Key 与 Admin 密码的 UI 代码块彼此独立（各自单独的 `<div>`/state），可以做到只删主 Key 部分、不影响 Admin 密码部分。

## 预期影响

- **行为变更（需用户明确知悉）**：移除后，`/v1/messages`、`/cc/v1/messages` 等 Anthropic 接口将**仅**接受已在 Admin 后台创建并启用的子 API Key；若某天 Admin 被禁用且所有子 Key 被删除/禁用，代理将对所有请求返回 401（此前主 Key 可作为紧急兜底通道）。当前环境已有 3 个启用中的子 Key 且 Admin 已在使用，无实际可用性风险。
- **对现有 3 个子 Key 无影响**：鉴权、额度、绑定账号、用量统计逻辑完全独立，不经过本次改动的任何代码路径。
- **对 Admin 登录无影响**：`adminApiKey`/`admin_auth_middleware` 是独立字段与中间件，不涉及。
- **接口变更**：`GET /api/admin/config/auth-keys` 响应体、`PUT /api/admin/config/auth-keys` 请求体、`GET /api/admin/server-info` 响应体均减少 `apiKey`/`masterApiKey` 字段——纯字段删除，非破坏性重命名，前后端同步修改后无兼容性问题。

## 风险

| 风险 | 应对 |
|---|---|
| 移除主 Key 后，若未来 Admin 被误关闭且子 Key 全部失效，代理将完全不可访问（无兜底通道） | 已确认当前 3 个子 Key 均启用、Admin 后台正常使用，属于可接受的既有工作模式变更；不额外引入新的兜底机制（用户明确要求"几乎没有任何作用"应删除，不做过度设计） |
| 遗漏引用点导致编译失败或前端类型报错 | 已通过代码符号全局搜索（`config.api_key`/`AppState.api_key`/`masterApiKey`）核对 Rust/TS 引用点；**首轮 sub-agent 审查额外发现该方法论未覆盖 Dockerfile/shell/ps1 脚本及文档中的字符串字面量与环境变量引用，已补充排查并纳入 tasks.md**（Dockerfile、run-local-service-mac.sh、run-local-service-windows.ps1、start_server.sh）。逐项处理后以 `cargo check`/`cargo clippy`/`cargo test`/前端构建验证 |
| `src/token.rs:156`（`CountTokensConfig.api_key`）、`src/user/handlers.rs`（`LoginRequest.api_key`）、前端 Admin 密码相关代码同名易混淆 | 已在调研阶段逐一排除，proposal 中明确列入"不在范围内"，实施时逐文件核对字段来源 struct，不做批量文本替换 |
| 本地 `app/config/config.json` 是用户正在使用的真实运行配置 | 修改前通过 `AskUserQuestion` 单独确认，且改动仅为删除一个空字符串键，不影响其余字段（`adminApiKey`、账号凭据等） |
