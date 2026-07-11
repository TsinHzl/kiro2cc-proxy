> **注：** 本文档由 **claude-sonnet-4-6** 模型自动生成。

# sub2api 网关缓存机制完整技术文档

## 目录

1. [整体缓存架构概览](#1-整体缓存架构概览)
2. [机制一：粘性会话缓存（Sticky Session）](#2-机制一粘性会话缓存sticky-session)
3. [机制二：cache_control 断点注入](#3-机制二cache_control-断点注入)
4. [机制三：cache_control 数量限制执行](#4-机制三cache_control-数量限制执行)
5. [机制四：1h TTL 注入与 Cache TTL Override](#5-机制四1h-ttl-注入与-cache-ttl-override)
6. [机制五：ForceCacheBilling（强制缓存计费）](#6-机制五forcecachebilling强制缓存计费)
7. [机制六：Billing Header 同步与 CCH 签名](#7-机制六billing-header-同步与-cch-签名)
8. [机制七：Claude Code System Prompt 缓存断点](#8-机制七claude-code-system-prompt-缓存断点)
9. [机制八：设置项缓存（SettingService 内存缓存）](#9-机制八设置项缓存settingservice-内存缓存)
10. [各机制依赖关系](#10-各机制依赖关系)
11. [配置项说明](#11-配置项说明)
12. [与 kiro2cc-proxy 的对比结论](#12-与-kiro2cc-proxy-的对比结论)

---

## 1. 整体缓存架构概览

### 两类缓存

sub2api 有两类完全不同的"缓存"：

- **会话路由缓存**：记住"这个用户上次用了哪个 Claude 账号"，下次还发给同一个账号，保证多轮对话的上下文连续性。存在 Redis 里。
- **Prompt 缓存控制**：在发给 Anthropic 的请求体里打上 `cache_control` 标记，告诉 Anthropic "这段内容可以缓存"，下次相同前缀就不重复计费 input tokens。这是 Anthropic 侧的缓存，不是本地缓存。

### 数据流全景

```
客户端请求
    │
    ▼
[ParsedRequest 解析]
    │
    ├─► [GenerateSessionHash] ──► Redis GET sticky_session:{groupID}:{hash}
    │                                  │
    │                             命中 → 复用同一账号
    │                             未命中 → 负载均衡选账号 → Redis SET
    │
    ▼
[buildUpstreamRequest / Forward]
    │
    ├─► [injectClaudeCodeSystemPrompt]        ← 注入 billing block + CC system prompt（带 cache_control）
    ├─► [applyToolsLastCacheBreakpoint]       ← tools[-1] 打 cache_control 断点
    ├─► [rewriteMessageCacheControlIfEnabled] ← strip + 重新打 messages 断点（可选开关）
    ├─► [enforceCacheControlLimit]            ← 超过 4 个断点时裁剪
    ├─► [injectAnthropicCacheControlTTL1h]   ← 全局开关：把所有 ephemeral ttl 改为 1h（可选）
    ├─► [syncBillingHeaderVersion]            ← billing block 中 cc_version 与 User-Agent 对齐
    └─► [signBillingHeaderCCH]               ← cch=00000 占位符 → xxHash64 签名
    │
    ▼
[上游 Anthropic API]
    │
    ▼
[响应处理]
    ├─► [applyCacheTTLOverride]   ← 响应中 cache_creation 5m/1h 分类重写
    └─► [recordUsageCore]         ← ForceCacheBilling：粘性切换时 input_tokens → cache_read 计费
```

---

## 2. 机制一：粘性会话缓存（Sticky Session）

### 原理

粘性会话的目标是：同一个对话的多轮请求，始终路由到同一个 Claude OAuth 账号。这样 Anthropic 侧的 prompt cache 才能命中（同一账号的 KV cache 才共享）。

### 数据结构

**Redis Key 格式**（`gateway_cache.go:24`）：

```
sticky_session:{groupID}:{sessionHash}
Value: accountID (int64)
TTL:   1 小时（stickySessionTTL）
```

### Session Hash 生成（`gateway_service.go:682`）

`GenerateSessionHash` 按优先级三级降级：

```go
// 优先级 1：metadata.user_id 中的 session_xxx 字段（最稳定）
uid := ParseMetadataUserID(parsed.MetadataUserID)
if uid.SessionID != "" { return uid.SessionID }

// 优先级 2：请求体中带 cache_control:{type:"ephemeral"} 的内容摘要
cacheableContent := s.extractCacheableContent(parsed)
if cacheableContent != "" { return s.hashContent(cacheableContent) }

// 优先级 3：ClientIP + UserAgent + APIKeyID + system + messages 全文摘要
combined := clientIP + ":" + userAgent + ":" + apiKeyID + "|" + systemText + messagesText
return s.hashContent(combined)
```

`extractCacheableContent`（`gateway_service.go:821`）的逻辑：
- 先扫 `system[]` 中带 `cache_control.type=="ephemeral"` 的 text block，拼接为字符串
- 再扫 `messages[].content[]`，一旦发现任何 block 带 ephemeral cache_control，就返回该 message 的完整 content 文本
- 这样"有 cache_control 的内容"就成为会话的稳定标识符

### Redis 操作（`gateway_cache.go`）

| 操作 | 函数 | 触发时机 |
|---|---|---|
| GET | `GetSessionAccountID` | 每次请求路由前查询 |
| SET | `SetSessionAccountID` | 首次选定账号后绑定，TTL=1h |
| EXPIRE | `RefreshSessionTTL` | 请求成功后续期 |
| DEL | `DeleteSessionAccountID` | 绑定账号不可用时解绑，触发重选 |

---

## 3. 机制二：cache_control 断点注入

Anthropic 的 prompt cache 需要在请求体中显式标记哪些内容可以缓存（`cache_control: {type: "ephemeral"}`）。网关在三个位置注入断点：

### 断点位置 1：tools[-1]（`gateway_tool_rewrite.go:258`）

```go
func applyToolsLastCacheBreakpoint(body []byte) []byte {
    arr := tools.Array()
    lastIdx := len(arr) - 1
    existingCC := arr[lastIdx].Get("cache_control")

    // 客户端已设置 ttl → 完全透传，不覆盖
    if existingCC.Exists() && existingCC.Get("ttl").String() != "" {
        return body
    }
    // 已有 cache_control 但无 ttl → 补写 ttl
    if existingCC.Exists() {
        sjson.SetBytes(body, fmt.Sprintf("tools.%d.cache_control.ttl", lastIdx), claude.DefaultCacheControlTTL)
        return body
    }
    // 无 cache_control → 注入完整块
    raw := fmt.Sprintf(`{"type":"ephemeral","ttl":%q}`, claude.DefaultCacheControlTTL)
    sjson.SetRawBytes(body, fmt.Sprintf("tools.%d.cache_control", lastIdx), []byte(raw))
    return body
}
```

### 断点位置 2：messages（`gateway_messages_cache.go:60`）

`addMessageCacheBreakpoints` 注入两个稳定断点：

```go
// 断点 A：最后一条 message 的最后一个 content block
body = injectCacheControlOnLastContentBlock(body, len(arr)-1, &arr[len(arr)-1])

// 断点 B：messages >= 4 条时，倒数第二个 role=user message 的最后一个 content block
if len(arr) >= 4 {
    for i := len(arr) - 1; i >= 0; i-- {
        if arr[i].Get("role").String() != "user" { continue }
        userCount++
        if userCount == 2 {
            body = injectCacheControlOnLastContentBlock(body, i, &arr[i])
            break
        }
    }
}
```

**为什么要先 strip 再重新注入**（`gateway_messages_cache.go:13` 注释）：

客户端（尤其是 Claude Code）会把 `cache_control` 打在"当前最后一条 user message"上。下一轮对话追加新消息后，原来的最后一条变成中间某条，`cache_control` 还挂着就导致"前缀签名变化"，破坏缓存命中。统一由代理重新打断点才能在多轮间稳定。

`injectCacheControlOnLastContentBlock` 的特殊处理：
- 若 content 是 string 类型 → 先升级为 `[{type:"text", text:..., cache_control:{...}}]` 数组
- 若 block 已有 `cache_control.ttl` → 不覆盖（尊重客户端设置）
- 若 block 已有 `cache_control` 但无 ttl → 只补写 ttl 字段

### 断点位置 3：system prompt block（`gateway_service.go:952`）

```go
func marshalAnthropicSystemTextBlock(text string, includeCacheControl bool) ([]byte, error) {
    block := anthropicSystemTextBlockPayload{Type: "text", Text: text}
    if includeCacheControl {
        block.CacheControl = &anthropicCacheControlPayload{
            Type: "ephemeral",
            TTL:  claude.DefaultCacheControlTTL,  // "5m"
        }
    }
    return json.Marshal(block)
}
```

### DefaultCacheControlTTL

```go
// constants.go:63
const DefaultCacheControlTTL = "5m"
```

网关自己生成的断点默认使用 5m TTL。客户端显式传入的 ttl 优先，不被覆盖。

---

## 4. 机制三：cache_control 数量限制执行

Anthropic API 最多允许 4 个 `cache_control` 断点（`gateway_service.go:53`）：

```go
maxCacheControlBlocks = 4
```

### collectCacheControlPaths（`gateway_service.go:4135`）

扫描整个请求体，收集所有 `cache_control` 的 JSON 路径，分为四类：

| 类别 | 说明 |
|---|---|
| `invalidThinking` | thinking 块中的 cache_control（非法，Anthropic 不支持） |
| `systemPaths` | `system[i].cache_control` |
| `messagePaths` | `messages[i].content[j].cache_control` |
| `toolPaths` | `tools[i].cache_control` |

### enforceCacheControlLimit（`gateway_service.go:4201`）

超限时的裁剪优先级（从低到高保留）：

```
1. 先清除所有 thinking 块中的非法 cache_control（无条件）
2. 超限时：优先从 tools 末尾移除
3. 再从 messages 移除
4. 最后才从 system 移除（system 断点最稳定，最后动）
```

---

## 5. 机制四：1h TTL 注入与 Cache TTL Override

### 请求侧：injectAnthropicCacheControlTTL1h（`gateway_service.go:4284`）

```go
func injectAnthropicCacheControlTTL1h(body []byte) []byte {
    return forceEphemeralCacheControlTTL(body, cacheTTLTarget1h)
}
```

`forceEphemeralCacheControlTTL` 遍历请求体中所有位置（顶层、system、messages、tools）的 `cache_control`，将 `type=="ephemeral"` 且 ttl 不等于目标值的全部改写为目标 ttl。**只修改已有断点，不新增断点。**

触发条件（`gateway_service.go:4358`）：

```go
func (s *GatewayService) shouldInjectAnthropicCacheTTL1h(ctx context.Context, account *Account) bool {
    // 必须是 Anthropic OAuth 或 SetupToken 账号
    if !account.IsAnthropicOAuthOrSetupToken() { return false }
    // 全局开关：SettingKeyEnableAnthropicCacheTTL1hInjection
    return s.settingService.IsAnthropicCacheTTL1hInjectionEnabled(ctx)
}
```

调用点（`gateway_service.go:4523`）：在模型 ID 映射之后、获取 token 之前执行。

### 响应侧：Cache TTL Override（`gateway_service.go:7785`）

这是**响应计费重写**，与请求侧注入配套使用：

```go
func (s *GatewayService) resolveCacheTTLUsageOverrideTarget(...) (string, bool) {
    // 账号级设置优先
    if account.IsCacheTTLOverrideEnabled() {
        return account.GetCacheTTLOverrideTarget(), true
    }
    // 全局 1h 注入开启时，响应计费归回 5m（避免 1h 缓存费率计费）
    if account.IsAnthropicOAuthOrSetupToken() && s.settingService.IsAnthropicCacheTTL1hInjectionEnabled(ctx) {
        return cacheTTLTarget5m, true  // 注意：返回 5m，不是 1h
    }
    return "", false
}
```

**关键设计**：请求侧注入 1h TTL（让 Anthropic 缓存更久），但响应侧把 `cache_creation_1h_input_tokens` 归回 `cache_creation_5m_input_tokens` 计费（因为 1h 缓存的计费价格更高，归回 5m 对用户更友好）。

`applyCacheTTLOverride`（`gateway_service.go:7787`）：

```go
func applyCacheTTLOverride(usage *ClaudeUsage, target string) bool {
    // Fallback：只有聚合字段无明细时，归入 5m 默认类别
    if usage.CacheCreation5mTokens == 0 && usage.CacheCreation1hTokens == 0 &&
        usage.CacheCreationInputTokens > 0 {
        usage.CacheCreation5mTokens = usage.CacheCreationInputTokens
    }
    total := usage.CacheCreation5mTokens + usage.CacheCreation1hTokens
    switch target {
    case "1h": usage.CacheCreation1hTokens = total; usage.CacheCreation5mTokens = 0
    default:   usage.CacheCreation5mTokens = total; usage.CacheCreation1hTokens = 0
    }
    return true
}
```

---

## 6. 机制五：ForceCacheBilling（强制缓存计费）

### 原理

粘性会话切换账号时，新账号没有旧账号的 KV cache，Anthropic 会按全量 input tokens 计费。但从用户视角看，这是系统内部的账号切换，不应让用户承担额外费用。`ForceCacheBilling` 机制在计费层将 `input_tokens` 强制转为 `cache_read_input_tokens`。

### 上下文传递

```go
// 定义（gateway_service.go:72）
type forceCacheBillingKeyType struct{}
var ForceCacheBillingContextKey = forceCacheBillingKeyType{}

func WithForceCacheBilling(ctx context.Context) context.Context {
    return context.WithValue(ctx, ForceCacheBillingContextKey, true)
}

func IsForceCacheBilling(ctx context.Context) bool {
    v, _ := ctx.Value(ForceCacheBillingContextKey).(bool)
    return v
}
```

### 计费转换（`gateway_service.go:8537`）

```go
if input.ForceCacheBilling && result.Usage.InputTokens > 0 {
    result.Usage.CacheReadInputTokens += result.Usage.InputTokens
    result.Usage.InputTokens = 0
}
```

---

## 7. 机制六：Billing Header 同步与 CCH 签名

### 背景

真实 Claude Code CLI 每个请求的 `system[0]` 都是一个 billing attribution block，格式为：

```
x-anthropic-billing-header: cc_version=2.1.92.c53; cc_entrypoint=cli; cch=XXXXX;
```

缺少这个 block 是 Anthropic 判定"第三方请求"的关键信号之一。

### syncBillingHeaderVersion（`gateway_billing_header.go:26`）

```go
func syncBillingHeaderVersion(body []byte, userAgent string) []byte {
    version := ExtractCLIVersion(userAgent)  // 从 User-Agent 提取版本号
    // 遍历 system[] 中以 "x-anthropic-billing-header" 开头的 text block
    // 用正则 cc_version=\d+\.\d+\.\d+ 替换为 cc_version={version}
    replacement := "cc_version=" + version
    ccVersionInBillingRe.ReplaceAllString(text.String(), replacement)
}
```

调用点（`gateway_service.go:6107`）：在所有 body 修改完成后、CCH 签名之前执行。

### signBillingHeaderCCH（`gateway_billing_header.go:60`）

```go
var cchPlaceholderRe = regexp.MustCompile(`(x-anthropic-billing-header:[^"]*?\bcch=)(00000)(;)`)
const cchSeed uint64 = 0x6E52736AC806831E

func signBillingHeaderCCH(body []byte) []byte {
    if !cchPlaceholderRe.Match(body) { return body }
    // xxHash64（带固定 seed）取低 20 bit，格式化为 5 位十六进制
    cch := fmt.Sprintf("%05x", xxHash64Seeded(body, cchSeed)&0xFFFFF)
    return cchPlaceholderRe.ReplaceAll(body, []byte("${1}"+cch+"${3}"))
}
```

**关键细节**：CCH 签名必须在所有 body 修改完成之后执行，因为签名是对整个 body 的哈希，任何后续修改都会使签名失效。

---

## 8. 机制七：Claude Code System Prompt 缓存断点

### 构造（`gateway_service.go:4072`）

非 Claude Code 客户端发来的请求，网关会替换 system prompt 为标准的 2-block 结构：

```
system[0]: billing attribution block（cc_version=X.Y.Z.fp; cc_entrypoint=cli; cch=00000;）
system[1]: "You are Claude Code, Anthropic's official CLI for Claude."（带 cache_control: ephemeral, ttl: 5m）
```

```go
billingBlock, _ := buildBillingAttributionBlockJSON(body, claude.CLICurrentVersion)
ccPromptBlock, _ := marshalAnthropicSystemTextBlock(claudeCodeSystemPrompt, true)
// true → includeCacheControl，TTL = DefaultCacheControlTTL = "5m"
out = setJSONRawBytes(body, "system", buildJSONArrayRaw([][]byte{billingBlock, ccPromptBlock}))
```

原始 system prompt 不丢弃，而是作为 `user/assistant` 消息对注入到 messages 开头：

```go
instrMsg = {role: "user", content: "[System Instructions]\n" + originalSystemText}
ackMsg   = {role: "assistant", content: "Understood. I will follow these instructions."}
// 重建 messages = [instrMsg, ackMsg, ...originalMessages]
```

### OpenCode 特殊处理（`gateway_service.go:944`）

```go
text = strings.ReplaceAll(
    text,
    "You are OpenCode, the best coding agent on the planet.",
    strings.TrimSpace(claudeCodeSystemPrompt),
)
```

OpenCode 客户端的固定身份句会被替换为标准 Claude Code banner，避免被 Anthropic 识别为非官方客户端。

---

## 9. 机制八：设置项缓存（SettingService 内存缓存）

### 架构

所有缓存相关的开关都通过 `SettingService.getGatewayForwardingSettingsCached` 统一读取，使用 `atomic.Value` + singleflight 实现进程内缓存（`setting_service.go:2134`）：

```go
type gatewayForwardingSettingsResult struct {
    fp, mp, cch, cacheTTL1h, rewriteMessageCacheControl bool
}
```

缓存 TTL 常量（`setting_service.go:117`）：

```go
const gatewayForwardingCacheTTL  = 60 * time.Second  // 正常缓存时间
const gatewayForwardingErrorTTL  = 5 * time.Second   // 读取失败时的降级缓存时间
const gatewayForwardingDBTimeout = 5 * time.Second   // DB 查询超时
```

### 错误降级默认值

| 设置项 | 错误时默认值 |
|---|---|
| fingerprintUnification | `true`（保持指纹统一） |
| metadataPassthrough | `false` |
| cchSigning | `false` |
| anthropicCacheTTL1hInjection | `false` |
| rewriteMessageCacheControl | 由 `defaultRewriteMessageCacheControl()` 决定 |

---

## 10. 各机制依赖关系

```
SettingService（内存缓存，60s TTL）
    │
    ├─► enableCCH ──────────────────────────────► signBillingHeaderCCH
    │                                                    ↑
    │                                             syncBillingHeaderVersion（先执行）
    │
    ├─► cacheTTL1h ─────────────────────────────► shouldInjectAnthropicCacheTTL1h
    │                                                    ↓
    │                                             injectAnthropicCacheControlTTL1h（请求侧）
    │                                                    +
    │                                             resolveCacheTTLUsageOverrideTarget（响应侧）
    │                                                    ↓
    │                                             applyCacheTTLOverride / rewriteCacheCreationJSON
    │
    └─► rewriteMessageCacheControl ─────────────► rewriteMessageCacheControlIfEnabled
                                                         ↓
                                                  stripMessageCacheControl
                                                         +
                                                  addMessageCacheBreakpoints

Redis（粘性会话）
    ├─► GenerateSessionHash ────────────────────► extractCacheableContent（依赖 cache_control 标记）
    └─► SetSessionAccountID / GetSessionAccountID / RefreshSessionTTL / DeleteSessionAccountID

ForceCacheBilling（Context 传递）
    └─► 粘性会话切换 ────────────────────────────► recordUsageCore（input_tokens → cache_read）

cache_control 断点注入（执行顺序）
    1. applyToolsLastCacheBreakpoint（tools[-1]）
    2. rewriteMessageCacheControlIfEnabled（messages，可选）
    3. marshalAnthropicSystemTextBlock（system[1]，Claude Code 路径）
    4. enforceCacheControlLimit（超限裁剪，最后执行）
    5. injectAnthropicCacheControlTTL1h（TTL 改写，在裁剪之后）
    6. syncBillingHeaderVersion（billing header 版本同步）
    7. signBillingHeaderCCH（CCH 签名，必须最后执行）
```

---

## 11. 配置项说明

以下配置项均存储在数据库 `settings` 表，通过 `SettingService` 读取，进程内缓存约 60s。

| SettingKey | 常量名 | 默认值 | 说明 |
|---|---|---|---|
| `enable_fingerprint_unification` | `SettingKeyEnableFingerprintUnification` | `true` | 是否统一 OAuth 账号的 X-Stainless-* 指纹头 |
| `enable_metadata_passthrough` | `SettingKeyEnableMetadataPassthrough` | `false` | 是否透传客户端原始 metadata.user_id |
| `enable_cch_signing` | `SettingKeyEnableCCHSigning` | `false` | 是否对 billing header 执行 xxHash64 CCH 签名 |
| `enable_anthropic_cache_ttl_1h_injection` | `SettingKeyEnableAnthropicCacheTTL1hInjection` | `false` | 是否将所有 ephemeral cache_control 的 ttl 改写为 1h（仅 OAuth/SetupToken 账号） |
| `rewrite_message_cache_control` | `SettingKeyRewriteMessageCacheControl` | 由代码默认值决定 | 是否 strip 客户端 messages 断点并重新注入稳定断点 |

---

## 12. 与 kiro2cc-proxy 的对比结论

| 维度 | sub2api | kiro2cc-proxy |
|---|---|---|
| 上游协议 | Anthropic API（直接控制请求体 JSON） | Kiro/AWS CodeWhisperer API（私有协议） |
| 缓存控制字段 | `cache_control: {type: "ephemeral", ttl: "5m"/"1h"}` | **无此字段**，Kiro 协议不暴露；但服务端自动执行前缀缓存 |
| TTL 延长手段 | 请求体注入 `"ttl":"1h"` | **不可行**，协议层不支持；TTL 由 Kiro 服务端决定 |
| 缓存折扣 | Anthropic 直接报告 `cache_read_input_tokens`，10% 计费 | Kiro 折扣体现在 `meteringEvent` 的 credits 用量中；2026-06-30 按 `k_ref` 反推实测确认为 **50% 全价计费**（非 10%） |
| Sticky Session | Redis，TTL=1h，基于 session hash | 内存，基于 `agentContinuationId` |
| Billing Header | 注入 + CCH 签名，伪装为官方 CLI | 不需要（走 Kiro 协议，非 Anthropic 直连） |
| 缓存命中率 | 取决于 TTL 和前缀稳定性 | 首轮 ~88%（跨会话前缀缓存），后续 ~97-99% |
| cache 模块的作用 | 无对应 | `src/cache/` 目录，四层降级链依次尝试：① metering 真值（`MeteringEvent.cache_read_input_tokens`/`cache_creation_input_tokens`，Kiro 已开始返回）② `token::count_prefix_tokens` 前缀字符估算 ③ `fingerprint.rs` 账号级前缀指纹命中（SHA-256 累积哈希追踪跨请求共享前缀，与本节"Sticky Session"及 sub2api 的 `fingerprintUnification`/HTTP 指纹统一是完全不同的概念）④ `simulation.rs` 比例模拟兜底 |

**结论**：两者底层的缓存折扣力度不同（sub2api/Anthropic 官方 cache read = 10% 全价，kiro2cc-proxy 实测为 50% 全价），控制方式也不同：
- sub2api 通过 `cache_control` 字段**显式控制**缓存行为和 TTL
- kiro2cc-proxy 通过**稳定请求前缀**（冻结 history[0] system prompt + history[2] tools）来**间接最大化** Kiro 服务端的自动前缀缓存命中率，计费展示值走上述四层降级链而非单一"反推层"

kiro2cc-proxy 无法控制缓存 TTL，但实测 Kiro 的缓存 TTL 足够长（跨会话仍能命中），且缓存命中率极高（97-99%），credits 节省效果与 sub2api 的 1h TTL 策略相当。
