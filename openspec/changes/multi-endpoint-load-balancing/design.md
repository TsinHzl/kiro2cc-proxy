# 设计文档：multi-endpoint-load-balancing

## 上下文

`src/kiro/provider.rs` 当前硬编码单一域 `q.{region}.amazonaws.com` + 单一 `x-amz-target`（`AmazonCodeWhispererStreamingService.GenerateAssistantResponse`）。Kiro 上游 4 个端点（`ide` / `runtime` / `codewhisperer` / `amazonq`）有独立限流桶，单账号单端点 429 后只能等 `throttle_delay` 退避窗口。本设计在 `provider.rs` 内引入端点维度，与账号维度的故障转移正交。

**认证机制**：当前使用 OAuth Bearer token（`Authorization: Bearer {access_token}`），不依赖 AWS SigV4。端点切换不涉及 service name 签名变化。

## 目标 / 非目标

**目标：**
- 单账号在 4 个独立限流桶间自动轮询
- 429 时只封禁命中桶，不切换账号
- 端点首选项由账号配置声明，未声明用全集默认序
- 桶封禁到期自动恢复
- 端点切换与账号切换正交

**非目标：**
- 跨账号端点 LB
- 端点级 RPM 全局阈值
- MCP / Admin API 端点
- `endpointRoundRobin` 顶层配置（首版不需要）

## 决策

### 决策 1：端点切换不消耗账号切换配额

`call_api_with_retry` 单账号内 3 次 attempts：每次 attempt = 1 个端点；429/5xx/网络失败 → 下次 attempt 换端点（仍同账号）；3 次 attempts 后 → 外层重试切账号（沿用 `MAX_ACCOUNTS = 3`）。

理由：
- 外层重试已能切账号，端点 LB 与之正交
- 端点切换不消耗账号切换配额 → 最多 4 个端点 × 3 个账号 = 12 次 attempts 才彻底失败
- 单账号内端点切换与现有重试骨架对齐，不引入新的循环维度

### 决策 2：桶状态独立于账号 cooldown

```rust
pub struct EndpointBucketRegistry {
    inner: Mutex<HashMap<(credential_id, EndpointName), Instant>>,
}
```

不放进 `MultiTokenManager`（避免膨胀）；不放进 `KiroProvider` 单实例（多 provider 共享时通过 `Arc`）。

账号级 cooldown 仍由现有 `report_throttled` / `report_throttled_for_rotation` 触发，**不引入**端点级 → 账号级的聚合逻辑，避免误锁死账号。

### 决策 3：桶封禁时长固定 30s

与现有 `throttle_delay(attempt)` 最大档对齐（`provider.rs::throttle_delay`）。不引入自适应窗口。后续 issue 可调参。

### 决策 4：默认端点顺序 `ide → runtime → codewhisperer → amazonq`

依据：
- `ide` 与现行 provider 默认行为完全一致（`q.{region}.amazonaws.com` 无 `x-amz-target`）—— 最小迁移风险
- `runtime` 域名独立，是天然次选
- `codewhisperer` / `amazonq` 走 `x-amz-target` 路由差异，作为兜底

### 决策 5：`Endpoint` 物化 host（不存 host_pattern 函数指针）

```rust
pub struct Endpoint {
    pub name: EndpointName,
    pub host: String,             // 构造时一次性计算
    pub amz_target: Option<&'static str>,
}
```

理由：host 计算简单（region → 字符串拼接），运行时只读一次，物化避免闭包调用开销 + 简化序列化。

### 决策 6：`build_headers` 接收 `&Endpoint` 参数

每次 attempt 根据当前 `endpoint.amz_target` 决定是否注入 `x-amz-target` 头。

### 决策 7：端点耗尽作为新错误类型

```rust
pub enum ProviderError {
    // 既有变体...
    AllEndpointsThrottled {
        credential_id: u64,
        endpoints: Vec<EndpointName>,
    },
}
```

对外 HTTP 响应 502 + JSON 错误体含端点名列表。日志含账号 id + 端点名。

### 决策 8：回滚策略依赖配置而非代码

上线后若某端点异常，修改 `credentials.json` 的 `endpoint` 字段移除异常端点即可（**不依赖**代码改动 / 重发版本）。

## 风险 / 权衡

| 风险 | 缓解 |
|---|---|
| `codewhisperer` 区域差异（us-east-1 独占主机） | `Endpoint::by_name` 内分支 + 单测覆盖 |
| `amazonq` / `codewhisperer` 响应 frame 协议同构性 | 单测覆盖 host_pattern + amz_target；生产灰度时记录首字节 |
| 桶封禁 30s 锁死有效端点 | 到期自动恢复；可调参 |
| 4 桶分散 → prefix cache 命中率下降 | 文档化；账号主可用 `endpoint` 字段固定首选 |
| `amazonq` 实际不独立限流 | 失败时降级为 3 桶 |
| 端点异常紧急禁用 | 改 `credentials.json` 即可 |

## 数据流

```
call_api_with_retry(ctx, body):
  for attempt in 0..MAX_RETRIES_PER_ACCOUNT (3):
    endpoint = select_endpoint(ctx, attempt)?
      // skip is_throttled, return effective_endpoints[attempt % len]
    url = base_url_for(ctx.creds, &endpoint)
    headers = build_headers(ctx, body, &endpoint)
    response = reqwest POST url headers body
    
    if 429:
      registry.throttle(ctx.id, endpoint.name, 30s)
      continue
    if 5xx / network error:
      continue
    return Ok(response)
  
  return Err(AllEndpointsThrottled { credential_id: ctx.id, endpoints: vec![...] })
```

## 关键文件

| 文件 | 改动 |
|---|---|
| `src/kiro/endpoint.rs`（新增） | 枚举 / 结构 / 桶注册表 |
| `src/kiro/mod.rs` | `pub mod endpoint;` |
| `src/kiro/model/credentials.rs` | `endpoint` 字段 + `effective_endpoints` |
| `src/kiro/provider.rs` | 引入 endpoint_registry；重构 3 个 method + call_api_with_retry；新增 ProviderError 变体 |
| `src/model/throttle_log.rs` | payload 增加 `endpoint` 字段 |
| `src/test.rs` 或 `provider.rs::tests` | ≥ 6 个新单测 |
| `docs/代码速查表.md` | 新章节 |
| `README.md` | 同步 |