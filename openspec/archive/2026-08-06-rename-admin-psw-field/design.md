## 上下文

后台管理密码字段当前命名为 `adminApiKey`（JSON/API）/ `admin_api_key`（Rust）/ `ADMIN_API_KEY`（环境变量），需统一改名为 `adminPsw` / `admin_psw` / `ADMIN_PSW`，同时保证已部署实例（旧 `config.json`、旧环境变量、可能存在的第三方脚本调用 Admin API）不因改名而中断。

`Config` 结构体（`src/model/config.rs`）与 `AuthKeysResponse`/`SetAuthKeysRequest`（`src/admin/types.rs`）均已带 `#[serde(rename_all = "camelCase")]`，因此 Rust 字段改名为 `admin_psw` 会自动对应 JSON 键 `adminPsw`，无需额外 `#[serde(rename = ...)]`。

## 目标 / 非目标

**目标：**
- 新字段名 `adminPsw` / `admin_psw` / `ADMIN_PSW` 成为唯一的"写出"形式（响应体、持久化文件、新脚本模板）
- 旧字段名 `adminApiKey` / `ADMIN_API_KEY` 仅作为"读入兼容"保留，不在任何新生成内容中出现
- 已部署实例无需人工迁移即可继续正常工作

**非目标：**
- 不做旧字段名的强制废弃时间表（不设过期时间，长期保留读入兼容）
- 不改动"API Key 管理"（子 API Key）功能的任何字段命名
- 不迁移已在用户浏览器 localStorage 中缓存的旧登录态数据（改名后需重新登录，视为可接受的一次性影响）

## 决策

### 1. JSON 字段兼容 — serde `#[serde(alias = "adminApiKey")]`

在 `Config.admin_psw`（`src/model/config.rs`）与 `SetAuthKeysRequest.admin_psw`（`src/admin/types.rs`）上添加 `#[serde(alias = "adminApiKey")]`。序列化仍按 `rename_all = "camelCase"` 规则统一输出 `adminPsw`，反序列化时新旧字段名均可被识别（新字段名优先，若请求体中两个字段同时出现则以标准 serde 别名冲突规则处理，属正常边界情况，不做特殊校验）。

`AuthKeysResponse` 是只读响应结构，不需要 alias（只需改名即可）。

### 2. 环境变量兼容 — 显式回退读取，而非 serde 机制

环境变量覆盖逻辑（`Config::apply_env_overrides`）是手写的命令式代码，不经过 serde，因此不能用 `#[serde(alias)]`。改为显式回退：

```rust
if let Ok(v) = env::var("ADMIN_PSW") {
    self.admin_psw = Some(v);
} else if let Ok(v) = env::var("ADMIN_API_KEY") {
    self.admin_psw = Some(v);
}
```

新变量优先，未设置时回退旧变量；两者都未设置则不覆盖（维持 `config.json` 中的值）。

### 3. 持久化清理策略 — 写入时移除旧键

`persist_auth_keys`（`src/admin/handlers.rs`）直接操作 `serde_json::Value`，属于手写 JSON patch 逻辑。决策：写入新密码时，先设置 `json["adminPsw"]`，成功后再显式 `json.as_object_mut().map(|m| m.remove("adminApiKey"))`，确保：
- 顺序上"先写新值成功、再删旧值"，避免中途失败导致密钥彻底丢失
- 长期运行的实例在下一次修改密码后，`config.json` 会自然收敛为只含新字段
- 不需要额外的"启动时自动迁移改写 config.json"逻辑（该逻辑会增加复杂度且非必需 —— 读取时的 alias 兼容已经足够保证功能正确，文件内容的"整洁度"通过下一次写操作顺带解决即可）

### 4. Rust 内部标识符 — 全量同步改名

`admin_api_key` → `admin_psw` 的改名覆盖以下位置（均为内部标识符，不涉及外部协议，改动风险低）：
- `src/model/config.rs`：字段、Default 实现、文档注释
- `src/admin/types.rs`：两个结构体字段
- `src/admin/handlers.rs`：局部变量、函数内引用
- `src/admin/middleware.rs`：`AdminState.admin_api_key` 字段与构造函数参数
- `src/admin/mod.rs`：文档注释中的示例代码
- `src/admin/log_handler.rs`：字段引用
- `src/main.rs`：配置提取、`AdminState::new()` 调用、日志文案

### 5. 前端 localStorage key 改名 — 直接改名，不做兼容读取

`admin-ui/src/lib/storage.ts` 的 `API_KEY_STORAGE_KEY` 常量值由 `'adminApiKey'` 改为 `'adminPsw'`。不读取旧 key 做迁移，理由：
- localStorage 只是浏览器本地缓存的登录态，不是持久业务数据，丢失后只需重新输入密码登录，成本极低
- 若做兼容读取需要额外读旧 key + 写新 key + 删旧 key 的逻辑，收益不成比例

## 风险 / 权衡

| 决策 | 权衡 |
|---|---|
| 环境变量用显式 `if/else if` 而非 serde alias | 更啰嗦，但环境变量覆盖本就是命令式代码，无法复用 serde 机制，属唯一可行方案 |
| 持久化清理选择"下次写入时收敛"而非"启动时主动迁移" | 已有旧 config.json 会在被动态修改密码前一直保留旧字段名（不影响功能，只是文件内容不够"干净"）；换取不引入额外启动时迁移逻辑的复杂度 |
| localStorage key 不做迁移 | 改名上线后所有已登录管理员会被强制登出一次；影响面小且用户可预期（本身就是敏感操作后的常见行为） |
