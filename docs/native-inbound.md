# Native Kiro inbound（`/native`）— 协议直通入口

> 本文件属于 kiro2cc-proxy **fork**（`github.com/LichKing-2234/kiro2cc-proxy`）。
> 所有 kiro2cc 相关改动都必须留在该 fork 仓库；不要改上游 `TsinHzl/kiro2cc-proxy`。
> 本次变更仅新增能力，不改变既有 Anthropic / OpenAI / Admin 路由行为。

## 1. 目标链路

```text
Kiro CLI / Kiro IDE（本地客户端）
        │  原生 Kiro 协议：POST + x-amz-target（AWS JSON 1.0）
        │  推理响应：application/vnd.amazon.eventstream（AWS EventStream）
        ▼
sub2api 薄原生转发路由（不改协议，仅鉴权/转发）
        │  POST http://kiro2cc-proxy:5678/native
        │  x-amz-target: <客户端原始 target>
        │  X-Kiro-Native-Token: <KIRO_NATIVE_TOKEN>
        ▼
kiro2cc-proxy  /native  （本模块）
        │  KiroProvider::call_api_stream / call_mcp
        ▼
KiroProvider 账号池（OAuth 刷新 / 多账号故障转移 / 限流 / 并发控制）
        ▼
Kiro / AWS（kiro2cc 直接持有账号，无需 Anthropic 协议翻译）
```

与既有 Anthropic 兼容链路（`/v1/messages` → converter → Kiro）相比，`/native` 是
**协议直通**：

- 不做 Anthropic⇄Kiro 任何字段转换；
- 上游 AWS EventStream 按原始字节逐块转发，**不解析、不缓冲、不重新编码**；
- 因此 tools / MCP / images / thinking / metering / contextUsage / 代码引用等全部
  原生事件与客户端直连 Kiro 时完全一致（这也是 Option A 的全部意义）。

## 2. 端点契约

| 项目 | 值 |
| --- | --- |
| 方法/路径 | `POST /native`（也接受 `/native/<任意子路径>`，仅按 header 分发） |
| 健康检查 | `GET /native/healthz` → `200 {"ok":true,"service":"kiro-native-inbound"}`（不校验令牌，供 sub2api/探针使用） |
| 请求 Content-Type | `application/x-amz-json-1.0`（`application/json` 亦可；本服务不校验） |
| 分发头 | `x-amz-target: <Service>.<Operation>`（缺失 → `400`） |
| 内部认证头 | `X-Kiro-Native-Token: <KIRO_NATIVE_TOKEN>`（不匹配 → `403`） |
| 请求体 | 客户端原生 JSON，**原样**交给 KiroProvider（推理/MCP）或仅用于分发（控制面） |
| 请求体上限 | 默认 64 MiB，`KIRO_NATIVE_MAX_BODY_BYTES` 可覆盖；超限由 axum 返回 `413` |
| 响应 | 推理/MCP：上游状态码+响应头+原始流；控制面：`application/json` |

注意 `/native` **不校验终端用户 API Key**：它信任调用方（sub2api 薄转发层）。
sub2api 负责校验本地 Kiro 客户端携带的凭据（sub2api `sk-` key），kiro2cc 只使用
自己的账号池。因此 `/native` 必须是集群内部入口（ClusterIP + 令牌 + 网络策略），
绝不能直接暴露到公网。

### 2.1 内部认证

- 启动时读取环境变量 `KIRO_NATIVE_TOKEN`（trim 后非空才生效）。
- 已设置：所有 POST 请求必须带 `X-Kiro-Native-Token`，且与配置值常量时间相等
  （`common::auth::constant_time_eq`）；否则 `403 {"__type":"AccessDeniedException", ...}`。
- 未设置：POC/本地模式放行，启动日志打印 WARN。**生产环境必须设置**，且只允许
  sub2api 访问本端口。

### 2.2 计费 token（`metadataEvent.tokenUsage` 注入）

原生 Kiro EventStream **没有 tokenUsage**（只有 `meteringEvent` credits 与
`contextUsageEvent` 百分比），sub2api 无法按 token 记账。`/native` 推理路径
（`*.GenerateAssistantResponse` / `*.SendMessage`）会在流式转发时做两件事：

1. **输入 tokens 估算**（请求转发前，`crate::token::count_tokens` tiktoken
   cl100k_base）：对 `conversationState.systemPrompt` / `history[].userInputMessage.content` /
   `history[].assistantResponseMessage.content` / `currentMessage.userInputMessage.content` /
   `toolResults[].content[].text|json` / `tools[].toolSpecification(name+description+inputSchema.json)` /
   `assistantResponseMessage.toolUses[].input` / `additionalModelRequestFields` 逐段求和，
   上限 1,000,000；图片 base64 不参与估算（避免严重高估）。估算失败/字段缺失按 0 处理。
2. **流内累积输出 tokens**：`assistantResponseEvent.content` +
   `reasoningContentEvent.text` + `toolUseEvent.input` 片段拼接后一次性 tiktoken 估算；
   `meteringEvent.cacheReadInputTokens` / `cacheCreationInputTokens`
   （camelCase 与 snake_case 均兼容）若出现则采集，否则 cache 字段记 0。

在 `metadataEvent` 帧到达时，若 payload 已带 `tokenUsage`（上游未来版本可能提供）
则**原样转发**；否则注入：

```json
"tokenUsage": {
  "uncachedInputTokens": "max(0, input - cache_read - cache_write)",
  "outputTokens": "out",
  "totalTokens": "input + out",
  "cacheReadInputTokens": "cache_read",
  "cacheWriteInputTokens": "cache_write"
}
```

注入只重建该帧（复用原 header 字节、重算长度与 CRC），其余帧逐字节原样转发。
实现上使用增量 AWS EventStream 解析（`src/native/billing.rs`）：半帧留在 buffer、
按 chunk flush、不缓冲整条响应；帧结构/CRC 异常时该帧原样转发并继续，长度非法则
退化为整条流原样透传，绝不丢字节。若上游**没有** `metadataEvent`，整条流原样转发，
sub2api 仍会看到 `kiro_native.usage_missing`。

> 口径提醒：cl100k_base 与 Kiro 实际 tokenizer 存在系统性偏差（通常 cl100k 偏多/偏少
> 视内容而定），因此数值是**估算**；上游 metering credits 仍是真实计费值。cache
> 字段只有在上游 meteringEvent 提供时才有真实值，否则为 0。

## 3. 支持的 `x-amz-target`

分类规则按操作名（最后一段）匹配，兼容 Kiro 2.36 的旧/新两代 service 前缀
（`AmazonCodeWhispererService` / `AmazonCodeWhispererStreamingService` /
`AmazonQDeveloperStreamingService` / `KiroRuntimeService` /
`KiroControlPlaneBearerService` / `CodeWhispererRuntime`）。

### 3.1 推理（原样直通）

| `x-amz-target`（操作名） | 行为 |
| --- | --- |
| `*.GenerateAssistantResponse` | 请求体 → `KiroProvider::call_api_stream(body, false, false, &[])`；上游 EventStream 原样流式转发 |
| `*.SendMessage` | 同上（Amazon Q Developer Streaming 旧协议） |

上游 HTTP 状态码原样返回（包括非 2xx 的错误体）；响应头按黑名单过滤后转发
（透传 `content-type`、`x-amzn-requestid`、`x-amzn-codewhisperer-conversation-id`、
`x-amzn-kiro-*`、`retry-after` 等；丢弃 hop-by-hop、`content-length`、
`content-encoding`、`transfer-encoding`）。客户端断开时 axum 丢弃 body stream，
reqwest 随之关闭上游连接（取消传播）。

### 3.2 MCP（原样直通）

| `x-amz-target`（操作名） | 行为 |
| --- | --- |
| `*.InvokeMCP` / `*.InvokeMCPStream` | 请求体（JSON-RPC）→ `KiroProvider::call_mcp(body, &[])`；上游 JSON 或流式响应原样转发 |

### 3.3 控制面（映射到 kiro2cc 已有能力）

| 操作名 | 实现 | 数据来源 |
| --- | --- | --- |
| `GetProfile` | 合成 `{"profile": {...}, "profileArn": ..., "downloadEnabled": true}` | 选中账号凭据（profileArn / email / nickname / authMethod / provider） |
| `ListAvailableModels` | 真实上游模型目录 → `{"models":[...],"defaultModel":{...},"nextToken":null}` | `MultiTokenManager::list_available_models()`（真实 Kiro 控制面调用） |
| `GetUsageLimits` | 真实额度 → `{limits:[], nextDateReset, daysUntilReset, usageBreakdownList, subscriptionInfo, overageConfiguration, userInfo}` | `MultiTokenManager::get_usage_limits_for(id)`（真实 Kiro 控制面调用） |
| `ListAvailableProfiles` | 选中账号 profile 列表（缺失时先真实 discovery） | 凭据 + `discover_profile_arn()`（`ListAvailableProfiles` 真实调用） |
| `SendTelemetryEvent` | `200 {}`（吞掉） | — |
| `GetEffectiveEnterpriseManagementResourceWithToken` | `200 {}` | **POC 限制**：kiro2cc 无企业管控资源数据源 |
| `GetFeatureConfiguration` | `200 {}` | **POC 限制**：无功能开关数据源（仅影响客户端 UI 特性） |
| 其它未知 target | `200 {}` + debug 日志 | POC 兜底，保证客户端启动/交互不因未实现 op 失败 |

控制面账号选择：`acquire_context(None)`，与推理共用同一账号池与负载均衡策略，
但不写 sticky 绑定，因此不会影响推理会话粘性；每次控制面调用会占用一次账号选择
（可能触发 token 刷新），但不会上报成功/失败计数。

`GetUsageLimits` 的 `limits` 字段固定返回空数组：kiro2cc 的额度模型以
`usageBreakdownList` 为主，客户端会回退到该列表展示。`daysUntilReset` 由
`nextDateReset`（Unix 秒）向上取整计算，最小 0。

### 3.4 错误映射

| 上游错误特征 | 原生响应 |
| --- | --- |
| 额度耗尽（`QUOTA_EXHAUSTED_ALL_MARKER` / `MONTHLY_REQUEST_COUNT`） | `402 QuotaExhaustedException` |
| `429` / 限流 / throttling | `429 ThrottlingException` |
| `400 Bad Request` / `ValidationException` | `400 ValidationException` |
| `401/403`（账号侧鉴权失败，网关上游问题） | `502 UpstreamAuthException` |
| 其它网络/上游异常 | `502 UpstreamException` |

错误体为 AWS JSON 1.0 形状：`{"__type":"...","message":"..."}`；message 最多
1000 字符（避免回显超长上游响应体）。

## 4. 环境变量

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `KIRO_NATIVE_TOKEN` | 空（POC 放行） | 内部令牌；生产必须设置 |
| `KIRO_NATIVE_MAX_BODY_BYTES` | `67108864`（64 MiB） | 原生请求体上限 |

其余配置（账号、代理、超时、并发、限流）完全复用现有 `config.json` / 环境变量，
无需为原生入口新增配置项。

## 5. 日志与可观测性

- 请求：`target`、分类、body 字节数（**不记录请求体/凭据/prompt**）。
- 上游：账号 id、HTTP 状态、估算输入 tokens（`estimated_input_tokens`）、首包耗时
  （`elapsed_ms`）；流式内容本身不落日志。
- 控制面：账号 id、模型数量、额度查询结果只记录计数，不记录数值明细。
- 失败：`warn` 级别的顶层错误（不含完整上游 body）。

## 6. sub2api 如何调用

sub2api 原生转发层（另一任务）应：

1. 校验本地 Kiro 客户端凭据（sub2api `sk-` key，现有 sub2api 鉴权/配额/计费）；
2. 把客户端请求体、`x-amz-target` **原样**转发到
   `http://kiro2cc-proxy:5678/native`（同 namespace 可用 Service DNS；
   生产通过 Helm values 注入实际 Service 地址）；
3. 附加 `X-Kiro-Native-Token: <KIRO_NATIVE_TOKEN>`（与 kiro2cc 部署共享的
   Secret/环境变量）；
4. 响应状态码、响应头、响应体（含 EventStream 二进制流）原样回传客户端，
   不要缓冲整包（逐块转发）。

curl 冒烟示例（控制面，无需真实 Kiro 凭据即可验证路由/认证）：

```bash
# 健康检查（不需要令牌）
curl -fsS http://127.0.0.1:5678/native/healthz

# 缺少 x-amz-target → 400
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:5678/native

# 令牌错误 → 403（KIRO_NATIVE_TOKEN 已设置时）
curl -s -o /dev/null -w '%{http_code}\n' -X POST http://127.0.0.1:5678/native \
  -H 'x-amz-target: KiroControlPlaneBearerService.GetProfile' \
  -H 'X-Kiro-Native-Token: wrong' -d '{}'

# 控制面（真实账号池可用时返回 200 JSON）
curl -sS -X POST http://127.0.0.1:5678/native \
  -H 'content-type: application/x-amz-json-1.0' \
  -H 'x-amz-target: KiroControlPlaneBearerService.ListAvailableModels' \
  -H "X-Kiro-Native-Token: $KIRO_NATIVE_TOKEN" -d '{"origin":"KIRO_CLI"}'
```

## 7. 已知限制与后续项（POC → 生产）

1. **企业管控/功能开关**：`GetEffectiveEnterpriseManagementResourceWithToken`、
   `GetFeatureConfiguration` 无数据源，返回 `{}`。如企业版客户端依赖企业策略
   （如禁用某些功能），需要后续直连真实 Kiro 控制面转发。
2. **GetProfile 为合成**：profileArn 优先真实凭据；social/idc 回退到 Kiro IDE
   固定占位 ARN；API Key 账号无 ARN 语义则为空。`profileType` 为启发式
   （ENTERPRISE / KIRO / BUILDER_ID / SOCIAL），未经真实企业账号全量验证。
3. **ListAvailableProfiles 只覆盖当前选中账号**，不聚合账号池里的全部账号；
   多账号场景下客户端只能看到本次选中账号的 profile。
4. **原生入口不做终端用户鉴权/计费/用量落库**：这些由前置 sub2api 负责；
   kiro2cc 的 Admin 用量统计只覆盖 Anthropic/OpenAI 路由。若需要 kiro2cc 侧
   计量，后续可在 native 模块内复用 `UsageTracker`（需要 key 映射，POC 不做）。
5. **速率限制**：复用 KiroProvider 的并发/账号级限制与上游 429 处理；native 入口
   自身没有独立 per-client 限流（前置 sub2api 应有）。
6. **未知 target 返回 `{}`**：便于客户端兼容，但也会掩盖拼写错误；生产可改为
   可配置的严格模式。
7. **EventStream 帧不被校验**：本模块对上游流不做 CRC/帧解析，完全依赖 Kiro
   上游与客户端自身校验（这正是"原样直通"的取舍）。
8. **v3/KAS 客户端 target 与端点差异**：`KiroRuntimeService.*` 走同一上游账号池
   （kiro2cc 的 4 端点 LB）；若上游对 KAS 新表面有额外要求，需以真实 CLI 联调
   验证（Lead 计划用真实 CLI 验证 tools/MCP/images/thinking）。
9. **请求体不是 100% 原样**：KiroProvider 会按选中账号做两处既有改写
   （`provider/errors.rs::rewrite_request_body`）：API Key 账号移除 `profileArn`、
   其它账号注入正确 `profileArn`；并按账号开关注入/剥离 `additionalModelRequestFields.thinking`。
   这两处改写与 Anthropic 路由共用同一逻辑，是"同一账号池"的必要行为；其余字段
   （conversationState/history/tools/toolResults/images/documents 等）保持不变。
10. **客户端请求头不转发**：上游请求头由 KiroProvider 统一构造（UA、agent-mode
    由请求体推导），客户端自带的 `x-amzn-kiro-*` 透传头不会带到 Kiro；如需
    客户端级归因，后续可在 native 模块内显式透传白名单头。
11. **计费 token 是估算而非上游真值**：`metadataEvent.tokenUsage` 由 tiktoken
    cl100k_base 估算输入/输出 token；cacheRead/cacheWrite 仅当上游 meteringEvent
    同时给出才非 0。若 meteringEvent 出现在 metadataEvent **之后**，该请求的
    cache 字段会记为 0（帧顺序由上游决定，不会为计费而延迟 metadataEvent）。
12. **未注入场景**：上游完全没有 `metadataEvent` 时无法注入（流原样转发，sub2api
    继续记录 `kiro_native.usage_missing`）；metadataEvent payload 非 JSON 或帧 CRC
    异常时同样跳过注入（只记 warn，不破坏客户端流）。

## 8. 测试

```bash
# 仅本模块单元测试
cargo test native::

# 全量（CI 同款；首次编译较慢）
cargo test

# 编译检查
cargo check --all-targets
```

单元测试覆盖（`src/native/` 内 `#[cfg(test)]`）：

- `classify_target`：旧/新两代推理、MCP、控制面 target 与未知 fallback；
- `check_native_token`：未配置放行、配置后必须精确匹配（大小写/空值/缺失）；
- `aws_json`：响应头黑名单、null 裁剪、错误分类、UTF-8 安全截断；
- `control_plane`：模型/额度/Profile 合成、`daysUntilReset` 计算、空上游容错；
- `billing`：输入估算（history/tools/system/toolResults/额外字段、图片跳过、1M 截断）、
  EventStream 分帧解析（任意 chunk 边界/半帧/flush/垃圾流透传）、metadataEvent
  tokenUsage 注入（CRC 合法、其余帧逐字节一致、已有 tokenUsage 不覆盖）、
  metering cache 采集（camelCase/snake_case/负值）、uncached 下限 0。

## 9. 变更范围

- 新增：`src/native/`（`mod.rs` / `aws_json.rs` / `control_plane.rs` / `billing.rs`）；
- 最小改动：`src/main.rs`（`mod native`、共享 `Arc<KiroProvider>`、`nest("/native")`）、
  `src/anthropic/middleware.rs` + `router.rs` + `mod.rs`（新增共享 provider 构造函数）；
- 不修改：Anthropic/OpenAI/Admin 路由行为、converter、provider 既有逻辑、前端。
