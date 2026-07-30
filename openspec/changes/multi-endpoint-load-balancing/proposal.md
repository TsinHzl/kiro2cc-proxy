# 变更提案：multi-endpoint-load-balancing

## 背景

Kiro 上游的多个接入端点（`ide` / `runtime` / `codewhisperer` / `amazonq`）走不同的限流桶：

- `ide` → `q.{region}.amazonaws.com`，默认 CodeWhisperer 路由，无 `x-amz-target`
- `runtime` → `runtime.{region}.kiro.dev`，与 `q.*` 域名独立，无 `x-amz-target`
- `codewhisperer` → `codewhisperer.{region}.amazonaws.com`（us-east-1 独占主机）/ `q.{region}.amazonaws.com`（其它区域），显式 `x-amz-target: AmazonCodeWhispererStreamingService.GenerateAssistantResponse`
- `amazonq` → `q.{region}.amazonaws.com` 但携带 `x-amz-target: AmazonQDeveloperStreamingService.SendMessage`

当前 `provider.rs` 硬编码单一域 `q.{region}.amazonaws.com` + 单一 `x-amz-target`，无法利用 4 个独立限流桶，单账号 429 后只能等退避窗口恢复。

**认证机制约束**：当前 provider 使用 OAuth Bearer token（`Authorization: Bearer {access_token}`，见 `provider.rs:289` / `:348`），不依赖 AWS SigV4。端点切换不涉及 service name 签名变化，4 端点共用同一 token。

参考社区实现：
- https://github.com/doitcan-oiu/kiro.rs-admin/commit/e00bc27971ee13cc40f7a503550c0372f800b8ce
- https://github.com/liuran001/kiro.rs-admin/commit/bd67fdd13a2f0e4a1d0ab576482781e825252617

## 目标范围

**在范围内：**
- 新增 `src/kiro/endpoint.rs`：`EndpointName` 枚举 + `Endpoint` 结构 + `EndpointBucketRegistry` 桶级 429 状态
- `KiroCredentials` 新增 `endpoint: Option<Vec<EndpointName>>`（首选项优先，未配置则用全集）
- `KiroProvider` 在请求层引入端点选择 + 429 切换逻辑（沿用现有 `call_api_with_retry` 重试骨架）
- 单账号 4 桶独立计数；端点切换不触发账号切换
- 端点切换与账号切换正交：单次 attempt 选 1 个端点，3 次 attempts = 最多 3 个端点，3 次后由外层重试决定是否切账号
- 默认端点顺序：`ide` → `runtime` → `codewhisperer` → `amazonq`（依据：① `ide` 与现行 provider 默认行为一致，迁移最小化；② `runtime` 域名独立，作为次选；③ `codewhisperer` / `amazonq` 走 `x-amz-target` 路由差异，作为最后均衡）
- 单元测试：端点枚举 / 桶独立计数 / 429 切换 / 优先级回退 / 端点耗尽升级切账号

**不在范围内：**
- 跨账号的多端点 LB（账号选择仍由 `token_manager` 的 priority/balanced 策略决定）
- 端点级 RPM 全局阈值（继续复用现有 `RpmTracker`，不引入第二套限流）
- MCP 端点（`mcp_url` 维持现状）
- 前端改动（admin-ui 不暴露端点配置）
- prefix cache 影响评估：未实测，作为后续 issue follow-up
- 4 端点的精确 QPS / TPM 配额：未知，作为 follow-up 文档化
- `endpointRoundRobin` 顶层配置项：撤回（首版只需按顺序 + 跳过封禁桶，轮询偏移可在后续 issue 按需引入）

## 技术方案

1. **端点定义**（`src/kiro/endpoint.rs`）：
   ```rust
   pub enum EndpointName { Ide, Runtime, Codewhisperer, Amazonq }
   pub struct Endpoint {
       pub name: EndpointName,
       pub host: String,             // 计算结果，按 region 一次性物化
       pub amz_target: Option<&'static str>,
   }
   impl Endpoint {
       pub fn by_name(name: EndpointName, region: &str) -> Self;
       pub fn all(region: &str) -> [Endpoint; 4];
       pub fn default_order() -> [EndpointName; 4];  // [Ide, Runtime, Codewhisperer, Amazonq]
   }
   ```
   桶状态：
   ```rust
   pub struct EndpointBucketRegistry {
       inner: Mutex<HashMap<(u64, EndpointName), Instant>>,  // throttled_until
   }
   impl EndpointBucketRegistry {
       pub fn new() -> Self;
       pub fn is_throttled(&self, credential_id: u64, name: EndpointName) -> bool;
       pub fn throttle(&self, credential_id: u64, name: EndpointName, duration: Duration);
   }
   ```
   封禁时长固定 30s（与现有 `throttle_delay(attempt)` 最大档对齐）。

2. **请求路径**（`provider.rs::call_api_with_retry`）：
   ```
   attempt = 0..MAX_RETRIES_PER_ACCOUNT (默认 3):
     endpoint = select_endpoint(ctx, attempt)
       → 跳过 is_throttled 的桶
       → 按 effective_endpoints 顺序 + attempt 偏移返回第一个可用
     url = base_url_for(ctx.creds, &endpoint)
     headers = build_headers(ctx, body, &endpoint)  // amz_target 注入
     → reqwest POST
     if 429:
       registry.throttle(ctx.id, endpoint.name, 30s)
       attempt += 1
       continue  // 切端点
     if 5xx / network:
       attempt += 1
       continue  // 切端点（与现行行为一致：同 attempt 不切账号）
     success → return
   3 次 attempts 后 → return EndpointExhausted（外层决定切账号）
   ```
   **端点切换 vs 账号切换正交**：
   - 单 attempt 失败（429/5xx/网络）→ 下 attempt 同账号内换端点（**不消耗**外层切账号配额）
   - 3 attempts 后仍失败 → 外层重试切账号（沿用现有 `MAX_ACCOUNTS = 3`，总共 9 次 attempts）

3. **配置**（`KiroCredentials`）：
   - 新增 `pub endpoint: Option<Vec<EndpointName>>`，camelCase JSON `endpoint`，`skip_serializing_if = Option::is_none`
   - 实现 `effective_endpoints(&Config) -> Vec<Endpoint>`：未配置返回全集；配置了则首选在首，剩余端点按默认顺序去重追加
   - 非法端点名（如 `"invalid_endpoint"`）：serde 忽略该项，回退到全集（不报错，避免启动失败）

4. **账号级 throttle 兼容**：
   - 现有 `MultiTokenManager.report_throttled` / `report_throttled_for_rotation` 触发时机不变
   - 端点级 throttle 是**更细粒度**的子状态；账号级 cooldown 仍由外层重试时已 retry-exhausted 触发（不引入新聚合逻辑，避免误锁死账号）

5. **env 覆盖**：不新增顶层配置；账号级 `endpoint` 字段由 JSON 加载足够。

## 预期影响

| 模块 | 改动 | 兼容性 |
|---|---|---|
| `src/kiro/endpoint.rs`（新增） | 枚举 / 结构 / 桶注册表 | 无影响（新模块） |
| `src/kiro/mod.rs` | `pub mod endpoint;` | 无影响（新增模块声明） |
| `src/kiro/provider.rs` | `base_url_for/base_domain_for/build_headers` 增加 `endpoint: &Endpoint` 参数；`call_api_with_retry` 加端点轮询；新增 `endpoint_registry` 字段 | 私有方法签名变化；公开 `call_api` / `call_api_stream` / `call_mcp` 不变 |
| `src/kiro/token_manager.rs` | 无改动（仅依赖既有 `report_throttled*` 触发时机） | 无影响 |
| `src/kiro/model/credentials.rs` | 新字段 `endpoint` | 向后兼容（Option，缺省等价于全集默认序） |
| `src/model/throttle_log.rs` | payload 增加 `endpoint: Option<EndpointName>` 字段 | 向后兼容（Option，缺省无值） |

`/v1/messages` 与 `/cc/v1/messages` 均走 `provider.call_api_*`，端点 LB 对两者透明。

## 风险

| 风险 | 级别 | 缓解 |
|---|---|---|
| `codewhisperer` 区域语义差异（us-east-1 独占主机） | medium | `Endpoint::by_name` 内分支，单元测试覆盖 us-east-1 与 eu-central-1 两种 region |
| `amazonq` / `codewhisperer` 响应二进制帧与 `ide` 同构性未实测 | high | 单测覆盖 4 个端点的 host_pattern + amz_target；生产灰度时记录首字节十六进制对比；不通过则将端点从默认序移除 |
| 桶封禁时长 30s 锁死有效端点 | medium | 到期自动恢复；后续 issue 提供可配置参数 |
| 4 桶分散请求 → prefix cache 命中率下降（参考 #18） | low | 文档化为已知 trade-off；账号主可通过 `endpoint` 字段固定首选端点收敛 |
| `amazonq` 实际不独立限流（仅 `x-amz-target` 路由差异） | medium | 失败时 4 桶退化为 3 桶不影响可用性；观测发现后再调整 |
| 上线后某端点异常需要紧急禁用 | medium | **回滚策略**：修改 `credentials.json` 的 `endpoint` 字段移除异常端点 → 重启 / 热加载即可（**不依赖**代码改动） |

## 验收标准

- [ ] `cargo check` 通过
- [ ] `cargo clippy -- -D warnings` 通过
- [ ] `cargo fmt --check` 通过
- [ ] `cargo test` 全绿：既有测试**零回归**（按当前 master HEAD `1eb6212` 为基线，不锁定历史数量）
- [ ] 新增 ≥ 6 个单元测试覆盖：
  1. `Endpoint::all` 4 端点 host + amz_target 正确
  2. `Endpoint::by_name(Codewhisperer, region)` 区域分支（us-east-1 vs eu-central-1）
  3. `BucketState` 桶独立 key（账号 A 与 B 互不影响）
  4. `select_endpoint` 跳过被封禁桶
  5. 429 后下次 attempt 换桶（端点切换不消耗账号配额）
  6. 4 桶全封 → 返回 `EndpointExhausted` 错误（错误信息含 `credential_id` + 端点名列表，HTTP 502 透传上游）
- [ ] `credentials.json` 缺省 `endpoint` 字段时行为与现行完全一致（默认 `Ide` 单端点）
- [ ] 不引入新外部 crate
- [ ] `docs/代码速查表.md` + `README.md` 同步端点 LB 章节