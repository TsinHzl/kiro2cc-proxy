## 修改的需求

### 需求：认证密钥查询响应字段改名

`GET /api/admin/config/auth-keys` 的响应体字段名由 `adminApiKey` 改为 `adminPsw`，语义不变（仍为脱敏显示的 Admin Password）。

#### 场景：查询当前 Admin Password（脱敏）

- **WHEN** 客户端携带有效 Admin Password 调用 `GET /api/admin/config/auth-keys`
- **THEN** 响应体为 `{"adminPsw": "<脱敏后的值>"}`（不再包含 `adminApiKey` 字段）

### 需求：认证密钥修改请求字段改名（兼容旧字段）

`PUT /api/admin/config/auth-keys` 的请求体字段名由 `adminApiKey` 改为 `adminPsw`，同时兼容接受旧字段名 `adminApiKey` 以保证已有第三方脚本/工具不因本次改名而中断。

#### 场景：使用新字段名修改 Admin Password

- **WHEN** 客户端提交 `PUT /api/admin/config/auth-keys`，请求体为 `{"adminPsw": "<新密码>"}`
- **THEN** 运行时鉴权密码立即更新为新值，且 `config.json` 中持久化字段为 `adminPsw`

#### 场景：使用旧字段名修改 Admin Password（向后兼容）

- **WHEN** 客户端提交 `PUT /api/admin/config/auth-keys`，请求体为 `{"adminApiKey": "<新密码>"}`（旧字段名）
- **THEN** 请求被正常接受并生效，效果与使用新字段名 `adminPsw` 完全一致

#### 场景：新字段为空字符串时拒绝

- **WHEN** 客户端提交 `{"adminPsw": ""}` 或 `{"adminPsw": "   "}`
- **THEN** 返回 `400 Bad Request`，错误信息提示 `adminPsw` 不能为空

### 需求：config.json 字段改名（读取兼容旧字段，写入统一新字段）

`config.json` 中持久化 Admin Password 的字段名由 `adminApiKey` 改为 `adminPsw`。

#### 场景：读取仅含新字段的 config.json

- **WHEN** `config.json` 中存在 `"adminPsw": "<值>"`，不存在 `adminApiKey`
- **THEN** 启动时 `admin_psw` 配置项被正确加载为该值

#### 场景：读取仅含旧字段的 config.json（历史部署兼容）

- **WHEN** `config.json` 中仅存在旧字段 `"adminApiKey": "<值>"`，不存在 `adminPsw`
- **THEN** 启动时 `admin_psw` 配置项仍被正确加载为该值（通过字段别名兼容读取）

#### 场景：通过 Admin 面板保存密码后旧字段被清理

- **WHEN** `config.json` 中原本仅存在旧字段 `adminApiKey`，此时通过 `PUT /api/admin/config/auth-keys` 修改了密码
- **THEN** 持久化后的 `config.json` 只包含新字段 `adminPsw`，旧字段 `adminApiKey` 被移除

### 需求：环境变量改名（回退兼容旧变量）

容器化部署使用的环境变量由 `ADMIN_API_KEY` 改为 `ADMIN_PSW`，未设置 `ADMIN_PSW` 时回退读取 `ADMIN_API_KEY`。

#### 场景：仅设置新环境变量 ADMIN_PSW

- **WHEN** 容器环境变量设置了 `ADMIN_PSW=<值>`，未设置 `ADMIN_API_KEY`
- **THEN** 启动时 `admin_psw` 配置项被该环境变量覆盖为该值

#### 场景：仅设置旧环境变量 ADMIN_API_KEY（历史部署兼容）

- **WHEN** 容器环境变量仅设置了 `ADMIN_API_KEY=<值>`，未设置 `ADMIN_PSW`
- **THEN** 启动时 `admin_psw` 配置项仍被该环境变量覆盖为该值（回退兼容）

#### 场景：两者同时设置时新变量优先

- **WHEN** 容器环境变量同时设置了 `ADMIN_PSW=<值A>` 和 `ADMIN_API_KEY=<值B>`
- **THEN** 启动时 `admin_psw` 配置项取值为 `<值A>`（新变量优先）

## 移除的需求

### 需求：Admin REST API 字段 adminApiKey 作为唯一/首选字段名

**原因**：该字段代表的是后台管理员登录密码，而非可供第三方调用的 API Key，继续以 "adminApiKey" 作为首选/唯一字段名容易与"API Key 管理"功能（子 API Key）混淆。改为以 `adminPsw` 作为首选字段名，`adminApiKey` 仅保留为向后兼容的输入别名。

**迁移路径**：已有客户端无需立即修改即可继续工作（旧字段名仍被接受）；建议尽快切换到新字段名 `adminPsw`。
