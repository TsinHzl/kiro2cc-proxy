# 变更提案：sticky-cache-routing

## 背景

kiro2cc-proxy 支持多账号轮转，但当前每次请求均按 priority/balanced 策略独立选择凭据，
不感知"同一会话的连续请求"。

Kiro 后端的 prompt cache 是**凭据维度**的：只有同一 `agentContinuationId` 持续打到
同一账号，Kiro 才能命中 conversation history 缓存，显著降低上游计费 token 数。

多轮对话中若账号轮转，每轮都是 cache miss，缓存形同虚设。

## 目标范围

**在范围内：**
- 在 `MultiTokenManager` 中维护 `agentContinuationId → credential_id` 内存映射表（sticky cache）
- `KiroProvider` 从 request_body 提取 `agentContinuationId`，优先将请求路由到缓存中的同一账号
- 缓存条目 TTL 30 分钟，懒惰淘汰
- 缓存账号不健康时自动驱逐并重选，保证故障转移不受影响

**不在范围内：**
- 持久化 sticky cache 到磁盘或 Redis
- 修改 converter.rs / handlers.rs / stream.rs
- 修改负载均衡模式本身（priority / balanced 逻辑不变）
- 向 config 新增可配置项（TTL 硬编码）

## 技术方案

### 数据结构（token_manager.rs）

```rust
struct StickyCacheEntry {
    credential_id: u64,
    inserted_at: Instant,
}

// 新增到 MultiTokenManager struct
sticky_cache: Mutex<HashMap<String, StickyCacheEntry>>,
```

常量：`const STICKY_CACHE_TTL: StdDuration = StdDuration::from_secs(30 * 60);`

### 新增方法（token_manager.rs）

```rust
pub async fn acquire_context_sticky(
    &self,
    model: Option<&str>,
    allowed_ids: &[u64],
    continuation_id: Option<&str>,
) -> anyhow::Result<CallContext>
```

执行逻辑：
1. `continuation_id` 有值 → 查 sticky_cache
2. 命中且未过期且凭据健康且在 allowed_ids 范围内 → 直接使用，更新 `inserted_at`
3. 未命中/过期/不健康 → 驱逐缓存条目，按原有策略（`acquire_context_filtered`）选凭据
4. 正常选完后，若 `continuation_id` 有值 → 写入/更新 sticky_cache
5. 每次写入时顺手清理 sticky_cache 中所有过期条目（懒惰 GC）

### 辅助函数（provider.rs）

```rust
fn extract_continuation_id_from_request(request_body: &str) -> Option<String>
// 提取 conversationState.agentContinuationId
```

`call_api_with_retry` 和 `call_mcp_with_retry` 均改用 `acquire_context_sticky`。

## 预期影响

- 同一会话的多轮请求路由到同一账号，prompt cache 命中率从接近 0 提升至接近 100%
- 账号故障时自动回退到正常轮转，无降级风险
- 内存占用可忽略（每条记录 ~80 字节）
- 对单账号场景透明（sticky_cache 只有一条记录，始终命中同一账号）

## 风险

| 风险 | 应对 |
|------|------|
| 账号被禁用后仍路由过去 | 写入前检查 `!e.disabled && health != Unhealthy`；命中后若 token 刷新失败则驱逐并重选 |
| 内存泄漏（无限写入） | 每次写入时清理过期条目；30 分钟 TTL 自然淘汰 |
| agentContinuationId 缺失 | `continuation_id = None` 时直接走原有逻辑，无任何影响 |
