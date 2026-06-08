# Enterprise Account Stealth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 通过指纹硬化 + 行为模拟，降低代理请求被 Kiro 后台检测为自动化请求并封禁企业账号的概率。

**Architecture:** 四文件协作改动：`config.rs` 新增 4 个延迟配置字段；`machine_id.rs` 新增账号级 OS/Node 指纹派生；`token_manager.rs` 暴露账号末次使用时间；`provider.rs` 整合以上三处，修复 attempt 计数、指纹选择、请求前延迟。

**Tech Stack:** Rust, tokio, chrono (已有依赖), sha2 (已有依赖), fastrand (已有依赖)

---

## 文件改动总览

| 文件 | 操作 | 说明 |
|---|---|---|
| `src/model/config.rs` | 修改 | 新增 4 个配置字段；更新 `default_kiro_version()` |
| `src/kiro/machine_id.rs` | 修改 | 新增 `sha256_bytes`、`derive_os_fingerprint`、`derive_node_version` |
| `src/kiro/token_manager.rs` | 修改 | `CallContext` 新增 `last_used_at`；新增 `get_last_used_at` 方法 |
| `src/kiro/provider.rs` | 修改 | `build_headers`/`build_mcp_headers` 加 `attempt` 参数；加延迟逻辑 |

---

## Task 1: Config Extension

**Files:**
- Modify: `src/model/config.rs`

### 目标
在 `Config` 结构中新增 4 个可选延迟配置字段，并更新 `kiro_version` 默认值。

- [ ] **Step 1: 在 `Config` 结构末尾添加 4 个字段**

在 `src/model/config.rs` 的 `load_balancing_mode` 字段之后（约第 93 行）添加：

```rust
    /// 请求前最小延迟（毫秒），0 表示关闭
    #[serde(default = "default_request_delay_min_ms")]
    pub request_delay_min_ms: u64,

    /// 请求前最大延迟（毫秒），0 表示关闭
    #[serde(default = "default_request_delay_max_ms")]
    pub request_delay_max_ms: u64,

    /// 冷启动阈值（分钟）：账号超过此时间未使用，首次请求触发额外延迟
    #[serde(default = "default_cold_start_threshold_mins")]
    pub cold_start_threshold_mins: u64,

    /// 冷启动额外延迟上限（毫秒），实际延迟为 0..=此值 的随机数
    #[serde(default = "default_cold_start_delay_ms")]
    pub cold_start_delay_ms: u64,
```

- [ ] **Step 2: 添加对应的默认值函数**

在 `src/model/config.rs` 的其他 `fn default_*` 函数附近添加：

```rust
fn default_request_delay_min_ms() -> u64 {
    100
}

fn default_request_delay_max_ms() -> u64 {
    800
}

fn default_cold_start_threshold_mins() -> u64 {
    30
}

fn default_cold_start_delay_ms() -> u64 {
    2000
}
```

- [ ] **Step 3: 更新 `default_kiro_version()`**

找到 `fn default_kiro_version()` 函数（约第 112 行），更新返回值：

```rust
fn default_kiro_version() -> String {
    // 请在 config.json 中设置 kiroVersion 为你的 IDE 实际版本（Kiro IDE → About）
    "0.14.0".to_string()
}
```

- [ ] **Step 4: 更新 `Config::default()` 中的新字段**

在 `impl Default for Config` 的 `Self { ... }` 块中（约第 137 行），在 `load_balancing_mode` 之后添加：

```rust
            request_delay_min_ms: default_request_delay_min_ms(),
            request_delay_max_ms: default_request_delay_max_ms(),
            cold_start_threshold_mins: default_cold_start_threshold_mins(),
            cold_start_delay_ms: default_cold_start_delay_ms(),
```

- [ ] **Step 5: 验证编译**

```bash
cargo check
```

期望：无错误。若提示 `missing field`，检查 `Default` 实现是否补全了所有新字段。

- [ ] **Step 6: 提交**

```bash
git add src/model/config.rs
git commit -m "feat(stealth): 新增请求延迟与冷启动配置字段"
```

---

## Task 2: Per-Account Fingerprint Derivation

**Files:**
- Modify: `src/kiro/machine_id.rs`

### 目标
新增两个公开函数，根据账号 `refreshToken` 确定性地派生 OS 和 Node 版本字符串，保证同账号稳定、跨账号各异。

- [ ] **Step 1: 在文件顶部添加常量和 `sha256_bytes` 辅助函数**

在 `src/kiro/machine_id.rs` 的 `use` 行之后（约第 7 行之后）插入：

```rust
const OS_VERSIONS: &[&str] = &[
    "darwin#24.6.0",
    "darwin#23.6.0",
    "win32#10.0.22631",
    "win32#10.0.19045",
];

const NODE_VERSIONS: &[&str] = &[
    "20.11.1",
    "20.18.0",
    "22.11.0",
    "22.14.0",
    "22.21.1",
];

fn sha256_bytes(input: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hasher.finalize().to_vec()
}
```

- [ ] **Step 2: 添加 `derive_os_fingerprint` 函数**

在 `generate_from_credentials` 函数之后添加：

```rust
/// 根据账号 refreshToken 确定性地派生 OS 版本字符串
///
/// 保证同账号跨会话稳定，跨账号各不相同。
/// 无 refreshToken 时回退到 config.system_version。
pub fn derive_os_fingerprint(credentials: &KiroCredentials, config: &Config) -> String {
    if let Some(ref rt) = credentials.refresh_token {
        if !rt.is_empty() {
            let hash = sha256_bytes(rt);
            let idx = (hash[0] as usize) % OS_VERSIONS.len();
            return OS_VERSIONS[idx].to_string();
        }
    }
    config.system_version.clone()
}
```

- [ ] **Step 3: 添加 `derive_node_version` 函数**

紧接 `derive_os_fingerprint` 之后添加：

```rust
/// 根据账号 refreshToken 确定性地派生 Node 版本字符串
///
/// 使用 hash[1] 与 OS 指纹使用 hash[0] 区分，保证两者独立。
/// 无 refreshToken 时回退到 config.node_version。
pub fn derive_node_version(credentials: &KiroCredentials, config: &Config) -> String {
    if let Some(ref rt) = credentials.refresh_token {
        if !rt.is_empty() {
            let hash = sha256_bytes(rt);
            let idx = (hash[1] as usize) % NODE_VERSIONS.len();
            return NODE_VERSIONS[idx].to_string();
        }
    }
    config.node_version.clone()
}
```

- [ ] **Step 4: 编写测试**

在文件末尾的 `#[cfg(test)]` 块中添加：

```rust
    #[test]
    fn test_derive_os_fingerprint_stable() {
        // 同账号两次调用结果相同
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test_token_abc".to_string());
        let config = Config::default();

        let r1 = derive_os_fingerprint(&creds, &config);
        let r2 = derive_os_fingerprint(&creds, &config);
        assert_eq!(r1, r2);
        // 结果必须在候选池内
        assert!(OS_VERSIONS.contains(&r1.as_str()));
    }

    #[test]
    fn test_derive_os_fingerprint_diverse() {
        // 不同 refreshToken 不全相同（hash 分布足够分散）
        let config = Config::default();
        let tokens = ["token_a", "token_b", "token_c", "token_d", "token_e", "token_f", "token_g", "token_h"];
        let results: Vec<String> = tokens.iter().map(|t| {
            let mut creds = KiroCredentials::default();
            creds.refresh_token = Some(t.to_string());
            derive_os_fingerprint(&creds, &config)
        }).collect();
        // 8 个不同 token 中应该至少出现 2 种不同的 OS
        let unique: std::collections::HashSet<_> = results.iter().collect();
        assert!(unique.len() >= 2, "8 个不同 token 应产生至少 2 种不同 OS，实际: {:?}", unique);
    }

    #[test]
    fn test_derive_os_fingerprint_fallback_no_token() {
        // 无 refreshToken 时回退到 config.system_version
        let creds = KiroCredentials::default();
        let mut config = Config::default();
        config.system_version = "darwin#99.0.0".to_string();

        let result = derive_os_fingerprint(&creds, &config);
        assert_eq!(result, "darwin#99.0.0");
    }

    #[test]
    fn test_derive_node_version_stable() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test_token_abc".to_string());
        let config = Config::default();

        let r1 = derive_node_version(&creds, &config);
        let r2 = derive_node_version(&creds, &config);
        assert_eq!(r1, r2);
        assert!(NODE_VERSIONS.contains(&r1.as_str()));
    }

    #[test]
    fn test_derive_node_version_fallback_no_token() {
        let creds = KiroCredentials::default();
        let mut config = Config::default();
        config.node_version = "99.0.0".to_string();

        let result = derive_node_version(&creds, &config);
        assert_eq!(result, "99.0.0");
    }

    #[test]
    fn test_os_and_node_independent() {
        // 同一 token 的 OS 和 Node 版本使用 hash 的不同字节，互不影响
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("independence_test".to_string());
        let config = Config::default();

        // 不要求它们不同，只要求它们都在合法范围内（不会因为用同一字节崩溃）
        let os = derive_os_fingerprint(&creds, &config);
        let node = derive_node_version(&creds, &config);
        assert!(OS_VERSIONS.contains(&os.as_str()));
        assert!(NODE_VERSIONS.contains(&node.as_str()));
    }
```

- [ ] **Step 5: 运行测试**

```bash
cargo test machine_id
```

期望：所有测试通过（包括原有的 `test_sha256_hex`、`test_normalize_*` 等）。

- [ ] **Step 6: 提交**

```bash
git add src/kiro/machine_id.rs
git commit -m "feat(stealth): 账号级 OS/Node 指纹确定性派生"
```

---

## Task 3: Expose `last_used_at` from TokenManager

**Files:**
- Modify: `src/kiro/token_manager.rs`

### 目标
向 `MultiTokenManager` 添加 `get_last_used_at` 方法，供 `provider.rs` 冷启动检测使用。

- [ ] **Step 1: 在 `MultiTokenManager` 的公开方法区添加 `get_last_used_at`**

在 `report_success` 函数（约第 1458 行）之前找一个合适位置，添加：

```rust
    /// 获取指定账号的末次使用时间（RFC3339 字符串）
    ///
    /// 用于冷启动检测：若账号超过阈值时间未使用，可在首次请求前加额外延迟。
    pub fn get_last_used_at(&self, id: u64) -> Option<String> {
        let entries = self.entries.lock();
        entries.iter().find(|e| e.id == id)?.last_used_at.clone()
    }
```

- [ ] **Step 2: 验证编译**

```bash
cargo check
```

期望：无错误。

- [ ] **Step 3: 提交**

```bash
git add src/kiro/token_manager.rs
git commit -m "feat(stealth): 暴露账号末次使用时间供冷启动检测"
```

---

## Task 4: Fix `build_headers` — Attempt Counter & Per-Account Fingerprint

**Files:**
- Modify: `src/kiro/provider.rs`

### 目标
修复 `build_headers` 和 `build_mcp_headers`：
1. 新增 `attempt` 参数，让 `amz-sdk-request` 头随重试次数递增
2. 用账号级派生函数替换全局 `config.system_version` / `config.node_version`

- [ ] **Step 1: 在 `provider.rs` 顶部补充 `chrono` 导入**

在现有的 `use` 块中添加：

```rust
use chrono::{DateTime, Utc};
```

- [ ] **Step 2: 更新 `build_headers` 签名**

找到 `fn build_headers` 定义（约第 205 行），将签名改为：

```rust
fn build_headers(&self, ctx: &CallContext, request_body: &str, attempt: usize) -> anyhow::Result<HeaderMap> {
```

- [ ] **Step 3: 在 `build_headers` 内替换 os_name / node_version 读取方式**

找到以下两行（约第 211-213 行）：

```rust
        let os_name = &config.system_version;
        let node_version = &config.node_version;
```

替换为：

```rust
        let os_name = machine_id::derive_os_fingerprint(&ctx.credentials, config);
        let node_version = machine_id::derive_node_version(&ctx.credentials, config);
```

注意：`os_name` 和 `node_version` 现在是 `String`（非引用），后续使用 `&os_name` / `&node_version`。

- [ ] **Step 4: 更新 `build_headers` 内的 User-Agent 格式化**

找到 `x_amz_user_agent` 和 `user_agent` 的格式化字符串（约第 215-220 行），将 `{}` 的引用改为对所有权值的引用：

```rust
        let x_amz_user_agent = format!("aws-sdk-js/1.0.27 KiroIDE-{}-{}", kiro_version, machine_id);

        let user_agent = format!(
            "aws-sdk-js/1.0.27 ua/2.1 os/{} lang/js md/nodejs#{} api/codewhispererstreaming#1.0.27 m/E KiroIDE-{}-{}",
            os_name, node_version, kiro_version, machine_id
        );
```

（与原代码结构一致，只是 `os_name` / `node_version` 现在是 `String` 而非 `&String`，Rust 会自动 Deref，无需额外修改格式化字符串。）

- [ ] **Step 5: 在 `build_headers` 内修复 `amz-sdk-request` 头**

找到：

```rust
        headers.insert(
            "amz-sdk-request",
            HeaderValue::from_static("attempt=1; max=3"),
        );
```

替换为：

```rust
        let attempt_str = format!("attempt={}; max=3", attempt + 1);
        headers.insert(
            "amz-sdk-request",
            HeaderValue::from_str(&attempt_str).unwrap(),
        );
```

- [ ] **Step 6: 同样更新 `build_mcp_headers`**

`build_mcp_headers` 有与 `build_headers` 相同的三处问题，同步修改：

a) 签名改为：
```rust
fn build_mcp_headers(&self, ctx: &CallContext, attempt: usize) -> anyhow::Result<HeaderMap> {
```

b) 替换 `os_name` / `node_version` 读取：
```rust
        let os_name = machine_id::derive_os_fingerprint(&ctx.credentials, config);
        let node_version = machine_id::derive_node_version(&ctx.credentials, config);
```

c) 修复 `amz-sdk-request`：
```rust
        let attempt_str = format!("attempt={}; max=3", attempt + 1);
        headers.insert(
            "amz-sdk-request",
            HeaderValue::from_str(&attempt_str).unwrap(),
        );
```

- [ ] **Step 7: 更新 `call_api_with_retry` 中的 `build_headers` 调用**

找到（约第 531 行）：

```rust
            let headers = match self.build_headers(&ctx, request_body) {
```

改为：

```rust
            let headers = match self.build_headers(&ctx, request_body, attempt) {
```

- [ ] **Step 8: 更新 `call_mcp_with_retry` 中的 `build_mcp_headers` 调用**

找到（约第 366 行）：

```rust
            let headers = match self.build_mcp_headers(&ctx) {
```

改为：

```rust
            let headers = match self.build_mcp_headers(&ctx, attempt) {
```

- [ ] **Step 9: 验证编译**

```bash
cargo check
```

期望：无错误。常见问题：`os_name` 是 `String` 后再次被格式化用 `&os_name`（OK）；若提示 lifetime 相关错误，检查是否误用了 `&config.system_version` 的引用。

- [ ] **Step 10: 运行全量测试**

```bash
cargo test
```

期望：全部通过。

- [ ] **Step 11: 提交**

```bash
git add src/kiro/provider.rs
git commit -m "feat(stealth): 修复 attempt 计数与账号级 OS/Node 指纹"
```

---

## Task 5: Request Delays — Cold Start & Per-Request Jitter

**Files:**
- Modify: `src/kiro/provider.rs`

### 目标
在 `call_api_with_retry` 和 `call_mcp_with_retry` 中插入冷启动延迟和每次请求前随机延迟。

- [ ] **Step 1: 在 `call_api_with_retry` 的循环体内插入延迟逻辑**

找到 `call_api_with_retry` 的循环体（约第 520 行），在成功获取 `ctx` 之后、`build_headers` 之前插入：

```rust
            // ── 行为模拟延迟 ──────────────────────────────────────────────
            let config = self.token_manager.config();

            // 冷启动延迟：账号超过阈值未活动时，首次请求额外等待（仅第 1 次尝试）
            if attempt == 0
                && config.cold_start_threshold_mins > 0
                && config.cold_start_delay_ms > 0
            {
                let threshold_secs = config.cold_start_threshold_mins * 60;
                let is_cold = self
                    .token_manager
                    .get_last_used_at(ctx.id)
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|t| {
                        Utc::now()
                            .signed_duration_since(t.with_timezone(&Utc))
                            .num_seconds()
                            > threshold_secs as i64
                    })
                    .unwrap_or(true); // 从未使用过也视为冷启动
                if is_cold {
                    let extra_ms = fastrand::u64(0..=config.cold_start_delay_ms);
                    sleep(Duration::from_millis(extra_ms)).await;
                }
            }

            // 请求前随机延迟：每次尝试均执行
            if config.request_delay_max_ms > 0 {
                let lo = config.request_delay_min_ms;
                let hi = config.request_delay_max_ms;
                let delay_ms = if lo < hi {
                    fastrand::u64(lo..=hi)
                } else {
                    lo
                };
                if delay_ms > 0 {
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
            // ─────────────────────────────────────────────────────────────
```

插入位置（上下文参考）：
```rust
            // 成功获取 ctx 之后 ↓
            let ctx = match self.token_manager.acquire_context_sticky(...).await { ... };

            // ← 插入延迟逻辑

            let url = self.base_url_for(&ctx.credentials);
            let headers = match self.build_headers(&ctx, request_body, attempt) { ... };
```

- [ ] **Step 2: 在 `call_mcp_with_retry` 的循环体内同样插入延迟逻辑**

定位 `call_mcp_with_retry` 循环体（约第 357 行），在成功获取 `ctx` 之后、`build_mcp_headers` 之前插入完全相同的延迟块：

```rust
            // ── 行为模拟延迟 ──────────────────────────────────────────────
            let config = self.token_manager.config();

            if attempt == 0
                && config.cold_start_threshold_mins > 0
                && config.cold_start_delay_ms > 0
            {
                let threshold_secs = config.cold_start_threshold_mins * 60;
                let is_cold = self
                    .token_manager
                    .get_last_used_at(ctx.id)
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|t| {
                        Utc::now()
                            .signed_duration_since(t.with_timezone(&Utc))
                            .num_seconds()
                            > threshold_secs as i64
                    })
                    .unwrap_or(true);
                if is_cold {
                    let extra_ms = fastrand::u64(0..=config.cold_start_delay_ms);
                    sleep(Duration::from_millis(extra_ms)).await;
                }
            }

            if config.request_delay_max_ms > 0 {
                let lo = config.request_delay_min_ms;
                let hi = config.request_delay_max_ms;
                let delay_ms = if lo < hi {
                    fastrand::u64(lo..=hi)
                } else {
                    lo
                };
                if delay_ms > 0 {
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
            // ─────────────────────────────────────────────────────────────
```

- [ ] **Step 3: 验证编译**

```bash
cargo check
```

期望：无错误。若提示 `DateTime` 未找到，确认 Step 1（Task 4）中的 `use chrono::{DateTime, Utc};` 已加入。

- [ ] **Step 4: 运行全量测试**

```bash
cargo test
```

期望：全部通过。

- [ ] **Step 5: 提交**

```bash
git add src/kiro/provider.rs
git commit -m "feat(stealth): 请求前随机延迟与账号冷启动检测"
```

---

## Task 6: Smoke Test & Configuration

### 目标
本地运行验证延迟生效、指纹多样化，并说明如何调整配置。

- [ ] **Step 1: 全量构建**

```bash
cargo build --release
```

期望：零错误、零 warning（clippy 相关除外）。

- [ ] **Step 2: 验证默认行为（RUST_LOG 观察延迟日志）**

由于延迟是 `sleep`，不会打印日志。可临时在 Task 5 的延迟块前后各加一行 debug 日志来验证：

```rust
tracing::debug!("冷启动延迟 {}ms", extra_ms);
tracing::debug!("请求前随机延迟 {}ms", delay_ms);
```

运行：
```bash
RUST_LOG=debug cargo run -- --config app/config/config.json 2>&1 | grep -E "延迟|stealth"
```

验证后删除临时日志行。

- [ ] **Step 3: 验证指纹多样化**

如果有多个账号，在 Admin UI 的 Credentials 列表中查看不同账号，通过请求日志或调试确认不同账号的 User-Agent 中 `os/` 后面的值不同。

- [ ] **Step 4: 配置说明（按需调整 config.json）**

完整配置示例：

```json
{
  "kiroVersion": "0.14.0",
  "requestDelayMinMs": 100,
  "requestDelayMaxMs": 800,
  "coldStartThresholdMins": 30,
  "coldStartDelayMs": 2000
}
```

关闭延迟（高吞吐场景）：
```json
{
  "requestDelayMinMs": 0,
  "requestDelayMaxMs": 0,
  "coldStartThresholdMins": 0,
  "coldStartDelayMs": 0
}
```

**重要：** `kiroVersion` 请设置为你 Kiro IDE 实际版本（Help → About Kiro）。

- [ ] **Step 5: 最终提交**

```bash
git add -A
git commit -m "chore(stealth): 烟雾测试确认，清理临时调试日志"
```

---

## 验收标准

- [ ] `cargo test` 全部通过
- [ ] `cargo build --release` 零错误
- [ ] 两个账号的 `User-Agent` 头中 `os/` 部分不同
- [ ] 重试请求中 `amz-sdk-request` 的 `attempt` 值随次数递增
- [ ] 本地运行时可观察到每次请求有 ~100-800ms 的首字延迟
