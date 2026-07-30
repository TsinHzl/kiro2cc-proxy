# 规范：multi-endpoint-load-balancing

## 新增需求

### 需求：端点枚举
#### 场景：所有 4 个端点都有正确 host 和 amz_target
- **WHEN** 调用 `Endpoint::all(region="us-east-1")`
- **THEN** 返回 4 个 `Endpoint`：
  - `Ide`: host=`q.us-east-1.amazonaws.com`, amz_target=`None`
  - `Runtime`: host=`runtime.us-east-1.kiro.dev`, amz_target=`None`
  - `Codewhisperer`: host=`codewhisperer.us-east-1.amazonaws.com`, amz_target=`Some("AmazonCodeWhispererStreamingService.GenerateAssistantResponse")`
  - `Amazonq`: host=`q.us-east-1.amazonaws.com`, amz_target=`Some("AmazonQDeveloperStreamingService.SendMessage")`

#### 场景：codewhisperer 区域差异
- **WHEN** `Endpoint::by_name(Codewhisperer, region="us-east-1")`
- **THEN** host = `codewhisperer.us-east-1.amazonaws.com`（独占主机）
- **WHEN** `Endpoint::by_name(Codewhisperer, region="eu-central-1")`
- **THEN** host = `q.eu-central-1.amazonaws.com`（与 ide 同主机，靠 amz_target 区分）

#### 场景：默认顺序
- **WHEN** 调用 `Endpoint::default_order()`
- **THEN** 返回 `[Ide, Runtime, Codewhisperer, Amazonq]`（固定顺序）

### 需求：桶级 429 状态独立
#### 场景：单账号 4 桶互不影响
- **WHEN** 账号 A 的 `Ide` 桶被 `throttle(A, Ide, 30s)`
- **THEN** `is_throttled(A, Ide)` 返回 `true`
- **AND** `is_throttled(A, Runtime)` / `is_throttled(A, Codewhisperer)` / `is_throttled(A, Amazonq)` 仍返回 `false`
- **AND** 其它账号 B 的 4 个桶均返回 `false`

#### 场景：桶过期自动恢复
- **WHEN** `throttle(A, Ide, 30s)` 且时间已过 30s
- **THEN** `is_throttled(A, Ide)` 返回 `false`
- **AND** Map 中残留条目无需主动清理（过期即视为可用）

### 需求：端点选择与 429 切换
#### 场景：默认顺序轮询（无封禁）
- **WHEN** 账号未配置 `endpoint` 字段，且 4 桶均未封禁
- **THEN** `effective_endpoints(region)` 返回 `[Ide, Runtime, Codewhisperer, Amazonq]`
- **AND** `select_endpoint(ctx, attempt=0)` 返回 `Ide`，`attempt=1` 返回 `Runtime`，`attempt=2` 返回 `Codewhisperer`，`attempt=3` 回到 `Ide`

#### 场景：跳过被封禁桶
- **WHEN** 4 桶中 `Ide` 被封禁
- **THEN** `select_endpoint(ctx, attempt=0)` 返回 `Runtime`（跳过 `Ide`）
- **AND** `select_endpoint(ctx, attempt=2)` 返回 `Amazonq`（跳过 `Ide` + `Codewhisperer` 不封禁，但 attempt=2 偏移到 `Codewhisperer`）

#### 场景：账号声明首选端点
- **WHEN** 账号 `endpoint = ["Runtime", "Codewhisperer"]`
- **THEN** `effective_endpoints(region)` 返回 `[Runtime, Codewhisperer, Ide, Amazonq]`（首选在首 + 剩余按默认序去重追加）

#### 场景：账号仅声明单个端点
- **WHEN** 账号 `endpoint = ["Runtime"]`
- **THEN** `effective_endpoints(region)` 返回 `[Runtime, Ide, Codewhisperer, Amazonq]`（首选 + 兜底全集）

#### 场景：账号声明非法端点名（serde 忽略）
- **WHEN** `credentials.json` 的 `endpoint` 字段包含 `"invalid_endpoint"`
- **THEN** serde 反序列化忽略该项（启动**不报错**）
- **AND** `effective_endpoints(region)` 回退到全集 4 桶

#### 场景：4 桶全 429 返回明确错误
- **WHEN** 账号 A 的 4 桶在 3 次 attempts 内全部 429
- **THEN** 返回 `Err(ProviderError::AllEndpointsThrottled { credential_id: A, endpoints: [Ide, Runtime, Codewhisperer, Amazonq] })`
- **AND** 对外 HTTP 响应 502 + JSON 错误体含 `credential_id` 与 `endpoints` 列表
- **AND** `tracing::error!` 输出账号 id + 端点名

### 需求：请求头注入 amz_target
- **WHEN** `endpoint = Ide` 或 `Runtime` → `build_headers` 不包含 `x-amz-target`
- **WHEN** `endpoint = Codewhisperer` → `x-amz-target = AmazonCodeWhispererStreamingService.GenerateAssistantResponse`
- **WHEN** `endpoint = Amazonq` → `x-amz-target = AmazonQDeveloperStreamingService.SendMessage`

### 需求：与账号维度正交
#### 场景：单端点 429 不切换账号
- **WHEN** 账号 A 的 `Ide` 桶 429
- **THEN** 下次 attempt 切换到 A 的 `Runtime` 桶，**不切换**到账号 B
- **AND** 端点切换不消耗外层切账号配额（仍可在 A 的 3 次 attempts 内尝试 `Runtime` / `Codewhisperer`）

#### 场景：单账号 4 桶全 429 → 升级切账号
- **WHEN** 账号 A 的 4 桶在 3 次 attempts 内全部 429
- **THEN** 返回 `AllEndpointsThrottled` 错误（**不静默**切换到账号 B；调用方/外层按现有重试逻辑决定是否切账号）

### 需求：可观测性
#### 场景：tracing 输出端点名
- **WHEN** 每次 attempt 选择端点
- **THEN** `tracing::debug!` 输出 `endpoint={:?} host={}` （region 脱敏为 `***`）

#### 场景：throttle_log 记录端点维度（向后兼容）
- **WHEN** 账号 A 的 `Codewhisperer` 桶 429
- **THEN** `ThrottleLogStore` 新增记录 payload：`{ credential_id, endpoint: Some("codewhisperer"), timestamp, ... }`
- **AND** 旧调用点（`endpoint: None`）序列化时不写出 `endpoint` 字段（向后兼容）

### 需求：接口差异（/v1 vs /cc/v1）
- **WHEN** 客户端调用 `/v1/messages` 或 `/cc/v1/messages`
- **THEN** 两者都走 `provider.call_api*`，端点 LB 对调用方透明

### 需求：配置向后兼容
#### 场景：未配置 endpoint 字段
- **WHEN** `credentials.json` 不含 `endpoint` 字段（现有所有账号配置）
- **THEN** `effective_endpoints(region)` 返回全集 4 桶（`Ide` 在最前）
- **AND** `select_endpoint` 第一选择 = `Ide`（与现行 `q.{region}.amazonaws.com` 单端点行为完全一致）

#### 场景：上线回滚
- **WHEN** 某端点生产异常，运维修改 `credentials.json` 的 `endpoint` 字段移除该端点
- **THEN** **不需要**代码改动 / 重发版本即可回滚；重启或热加载配置生效

### 需求：复用约束
#### 场景：不引入新 crate
- **WHEN** 实施完成
- **THEN** `Cargo.toml` 无新增依赖（端点枚举 / 桶注册表全部使用标准库 + `parking_lot` 现有依赖）

#### 场景：状态共享通过 Arc
- **WHEN** `EndpointBucketRegistry` 在多 provider / 多任务间共享
- **THEN** 通过 `Arc<EndpointBucketRegistry>` 传递，禁止 `static mut`

#### 场景：认证机制不变
- **WHEN** 端点切换
- **THEN** 仍使用 OAuth Bearer token；**不涉及** SigV4 service name 签名变化