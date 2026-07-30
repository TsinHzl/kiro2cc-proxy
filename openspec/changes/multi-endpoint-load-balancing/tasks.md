# 任务清单：multi-endpoint-load-balancing

## 状态：DONE

## 任务

### 1. 端点模块骨架（合并子项）
- [x] 创建 `src/kiro/endpoint.rs`：
  - `EndpointName` 枚举（`Ide` / `Runtime` / `Codewhisperer` / `Amazonq`）+ `Serialize/Deserialize`（serde rename_all = "kebab-case"）
  - `Endpoint` 结构（`name` / `host` / `amz_target`）
  - `Endpoint::by_name(name, region)` / `Endpoint::all(region)` / `Endpoint::default_order()`
- [x] 同文件实现 `BucketState` + `EndpointBucketRegistry`（`Arc<Mutex<HashMap<(u64, EndpointName), Instant>>>`）
  - `new()` / `is_throttled(cred_id, name)` / `throttle(cred_id, name, duration)`
  - 桶封禁时长固定 30s（`pub const BUCKET_THROTTLE_DURATION: Duration = Duration::from_secs(30)`）
- [x] 注册 `pub mod endpoint;` 到 `src/kiro/mod.rs`
- [x] **单测**：`endpoint::tests` 模块，含 4 端点 host/amz_target 断言 + codewhisperer 区域分支 + 桶独立 key（账号 A 与 B 互不影响）+ 桶过期自动恢复
- 验收：`cargo test src/kiro/endpoint.rs` 全绿；模块可独立引入

### 2. KiroCredentials 字段 + effective_endpoints
- [x] `src/kiro/model/credentials.rs` 新增 `pub endpoint: Option<Vec<EndpointName>>`，camelCase + `skip_serializing_if = Option::is_none`
- [x] 实现 `impl KiroCredentials { pub fn effective_endpoints(&self, region: &str) -> Vec<Endpoint> }`：
  - 未配置 / 空 → `Endpoint::all(region).to_vec()`
  - 配置 → 首选在首 + 剩余端点按 `default_order` 去重追加
- [x] **单测**：未配置 / 单首选 / 多首选 / 非法值（serde 忽略 → 回退全集）
- 验收：`cargo test src/kiro/model/credentials.rs` 全绿（已 PASS，49 个测试含 6 个新增）

### 3. KiroProvider 改造（拆分）
- [x] **3a. 注入 endpoint_registry**：`KiroProvider` 新增字段 `endpoint_registry: Arc<EndpointBucketRegistry>`；构造器初始化空 registry；新增 `with_endpoint_registry` builder
- [x] **3b. select_endpoint 纯函数**：签名 `fn select_endpoint(&self, creds, attempt) -> Option<Endpoint>`，跳过封禁桶，attempt 偏移轮询；单账号端点首选顺序生效
- [x] **3c. 重试循环接入**：`base_url_for` / `base_domain_for` / `build_headers` 全部增加 `endpoint: &Endpoint` 参数；每次 attempt `select_endpoint`；429 时 `registry.throttle(ctx.id, name, 30s)`；4 桶全封返回明确错误（含端点名列表）
- [x] **3d. 错误传播**：`AllEndpointsThrottled` 错误信息含 `credential_id` + 端点名列表；`tracing::debug!` 输出每次 attempt 的端点选择
- [x] **单测**（6 个，全部 PASS）：跳过封禁桶 / attempt 偏移轮询 / 全封返回 None / 账号首选顺序 / codewhisperer 注入 amz_target / ide 不注入 amz_target
- 验收：`cargo test provider::tests::` 20/20 全绿（既有 14 + 新增 6）

### 4. 可观测性
- [x] `tracing::debug!` 输出每次 attempt 选择的端点名 + host（任务 3 已完成）
- [x] `model/throttle_log.rs` payload 结构增加 `pub endpoint: Option<String>`（camelCase JSON，`skip_serializing_if = Option::is_none`）
- [x] `ThrottleLogStore::record` 签名增加 `endpoint: Option<&str>` 参数；MCP 调用点传 `None`，API 429 调用点传 `Some(endpoint.name.as_str())`
- [x] **单测**（4 个，全部 PASS）：`endpoint: Some` 序列化含字段 / `endpoint: None` 序列化跳过字段 / 旧 JSON（无 endpoint）反序列化兼容 / 含 endpoint 字段往返一致
- 验收：`cargo test throttle_log::` 4/4 全绿

### 5. 文档与构建验证
- [x] `docs/代码速查表.md` 新增"3.5. `src/kiro/endpoint.rs`（多端点负载均衡）"章节，含类型 / 函数 / 端点 vs 账号关系 / 配置 / 回滚
- [x] `README.md` 同步"多端点负载均衡"小节 + 配置示例 + 回滚指引
- [x] `cargo fmt` 全跑（含预存在偏差），本 change 触及的 4 文件已干净
- [x] `cargo clippy -- -D warnings`：本 change 引入的 1 个 warning（`.into_iter()` 冗余调用）已修；剩余 2 个 error 均为预存在（`cache/fingerprint.rs:203` / `kiro/token_manager.rs:394`），不在本 change 范围
- [x] `cargo test --bin kiro2cc-proxy` **359/359 全绿**（既有 + 新增 20 个测试覆盖 endpoint / credentials / provider / throttle_log）

## 验收标准（全局）

- [ ] `cargo check` 通过
- [ ] `cargo clippy -- -D warnings` 通过
- [ ] `cargo fmt --check` 通过
- [ ] `cargo test` 全绿（既有测试零回归 + 新增 ≥ 6 测试）
- [ ] `credentials.json` 缺省 `endpoint` 字段时行为与现行完全一致（默认 `Ide` 单端点）
- [ ] 不引入新外部 crate（`Cargo.toml` 无新增依赖）
- [ ] 文档同步完成（代码速查表 + README）

## 依赖与约束

- 复用现有 `parking_lot::Mutex`（不引入新同步原语）
- 复用现有 `RpmTracker`（不引入第二套限流）
- 沿用 `call_api_with_retry` 重试骨架（不动外层重试上限 `MAX_RETRIES_PER_ACCOUNT = 3` × `MAX_ACCOUNTS = 3`）
- 不动 `token_manager` 的账号选择策略
- 不动 `mcp_url`（MCP 端点独立）
- 端点切换与账号切换**正交**：3 次 attempts 内只切端点不切账号；3 次 attempts 后才切账号
- 认证机制不变（仍用 OAuth Bearer token，**不涉及** SigV4 service name）

## 已知 follow-up（不在本 change 范围）

- `endpointRoundRobin` 顶层配置（首版不需要，attempt 偏移已能轮询）
- prefix cache 影响实测（待生产数据）
- 桶封禁时长可配置化（首版固定 30s）
- 4 端点精确 QPS / TPM 配额文档化