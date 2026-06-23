# 设计文档：cache-fingerprint-and-ephemeral

## 上下文

当前 prompt cache token 模拟在 metering 真值缺失时退化为三角分布随机采样（`PromptCacheUsage::from_ratio_config`），与请求历史无关；下游计费按聚合的 `cache_creation_input_tokens` 统计，无法区分 5m / 1h TTL；字符估算二分类导致不同字符类型偏差扩散到 message_start 早期上报；`contextUsage` 依赖 Kiro 上游事件 `contextUsagePercentage`，其窗口口径对部分模型与官方公开值不一致且事件到达晚。

参考 `Kiro-Go-main/proxy/cache_tracker.go` 的成熟实现引入指纹追踪，但与现有 metering 真值 + credits 反推降级链共存而非替换。同步把 contextUsage 完全本地化（统一 1M 窗口）以脱离上游口径不一致。

## 目标 / 非目标

**目标：**
- 真值缺失场景下 `cache_read_input_tokens` 反映真实跨请求复用度
- 补齐 Anthropic 协议 `ephemeral_5m_input_tokens` / `ephemeral_1h_input_tokens` 拆分
- 字符估算更贴近 tiktoken 真实分布（误差从 ~30% 降至 ~10%）
- contextUsage 完全本地化（窗口、口径、时序均不依赖 Kiro 上游事件）

**非目标：**
- 不替换 `infer_cache_read_tokens` credits 反推（基于平台计费公式，比指纹可靠）
- 不引入持久化（与 Anthropic 5min TTL 对齐，重启可接受）
- 不完全删除 `Event::ContextUsage` 事件类型（保留兜底触发 `model_context_window_exceeded`）
- 不支持 Layer 2 与 Layer 3 互补（降级是串行的）

## 决策

### 决策 1：指纹存储 — 内存 only

| 方案 | 优势 | 劣势 |
|------|------|------|
| A. 内存 `Arc<RwLock<HashMap>>` ✅ | 零依赖、零 I/O；与 Anthropic 5min TTL 对齐 | 重启丢失 5 分钟内的命中率 |
| B. SQLite/sled 持久化 | 跨重启保留 | 引入依赖；schema 演化负担；与服务端 TTL 不对称 |

选 A。

### 决策 2：指纹粒度 — 账号级，不区分模型

prompt cache 是 Anthropic 账号维度生效（每个 API key 独立缓存池），账号级匹配上游行为。不区分模型（同账号跨模型可能共享缓存边界，无观测优势）。

### 决策 3：指纹算法 — 累积 SHA-256

```
seg[0]   = "S:" + canonicalize(system_text)
seg[i+1] = "M:" + msg[i].role + ":" + canonicalize(msg[i].content)

hash[k] = SHA-256(seg[0] || seg[1] || ... || seg[k])  // 累积哈希
cumulative_tokens[k] = sum(count_tokens(seg[0..=k]))
```

**累积哈希** 保证"前缀单调性"：若 hash[k] 命中，则 hash[0..k] 必定命中。这与 Anthropic 缓存按消息前缀生效的语义一致。

### 决策 4：canonicalize 规则（处理非文本内容块）

| content 块类型 | 规范化形式 |
|---|---|
| `text` 块或纯 `Value::String` | trim 后原文 |
| `tool_use` | `"tool_use:" + name + ":" + json(input, sorted_keys)` |
| `tool_result` | `"tool_result:" + tool_use_id + ":" + canonicalize(content)` |
| `image` | `"image:" + source.media_type + ":" + hex(sha256(source.data)[..8])` |
| `document` | `"document:" + source.media_type + ":" + hex(sha256(source.data)[..8])` |
| 其他未知 type | type 字符串本身 |

**理由**：
- tool_use input 必须纳入 hash 以避免 "不同参数同函数" 误判命中
- image / document 大字段不能整体 hash（性能），用 short hash 替代
- 字段排序通过 `serde_json::to_value` → `BTreeMap` 转换实现

### 决策 5：命中计算 + 截断不变性

```
profile = build_profile(system, messages)
table   = tracker.get(account_id)

matched_len = 0
for k in 0..min(profile.len(), table.len()):
    if profile[k].hash == table[k].hash:
        matched_len = k + 1
        table[k].last_hit_at = now()
    else:
        break

cache_read = min(profile[matched_len-1].cum_tokens, total_input * 0.85)
new_segs   = profile[matched_len..]
cache_creation_raw = sum(seg.tokens for seg in new_segs)

usage = PromptCacheUsage {
    input_tokens: ...,
    cache_creation_input_tokens: cache_creation_raw,
    cache_read_input_tokens: cache_read,
    cache_creation_5m / 1h: split_by_ephemeral_1h_ratio(cache_creation_raw, config.ephemeral1hRatio),
}.clamp_to_total(total_input)
```

`clamp_to_total` 强制：
- `cache_read = min(cache_read, total_input)`
- `cache_creation = min(cache_creation, total_input - cache_read)`
- 5m/1h 按原比例缩放并保持 `5m + 1h == cache_creation`
- `input_tokens = total_input - cache_read - cache_creation`（保证 ≥ 0）

### 决策 6：ephemeral 5m / 1h 拆分策略

**确定性分配**（避免小样本概率方差）：
```
cache_creation_1h = floor(cache_creation × ephemeral1hRatio + 0.5)
cache_creation_5m = cache_creation - cache_creation_1h
```

默认 `ephemeral1hRatio = 0.0`，全部 5m。

`scale_to` 时按原比例同步缩放两字段：
```
new_5m = floor(old_5m × scale + 0.5)
new_1h = new_creation - new_5m  // 保证不变性
```

### 决策 7：降级链层次（更新）

```
Layer 1 (优先):  meteringEvent 真值齐全 → 直接采用
Layer 2:         infer_cache_read_tokens 反推（cache_read 真值，creation = 0）
Layer 3:         FingerprintTracker.compute（双字段，需 credential_id 已确定）
Layer 4 (兜底):  PromptCacheUsage::from_ratio_config 随机模拟
```

**关键时序约束**：Layer 3 必须延后到 `provider.call_api_stream` 返回 `credential_id` 之后。message_start 阶段（credential_id 尚未确定）仅能走 Layer 4 给早期粗估值；message_delta（终值）才使用完整降级链。

**变体讨论**：Layer 2 与 Layer 3 互补？决定不支持。复杂度增加但收益不显著；Layer 3 自身已能产出 cache_creation 字段。

### 决策 8：字符估算系数

| 字符类 | 系数 | 依据 |
|--------|------|------|
| ASCII 字母 (a-z, A-Z) | / 4.5 | tiktoken cl100k_base 英文平均 ~4.5 chars/token |
| 数字 (0-9) | / 2.0 | 数字 BPE 拆分粒度细，约 2 字符/token |
| 其他 ASCII (符号、空白) | / 1.5 | 符号通常单独成 token |
| 非 ASCII (CJK 等) | / 1.5 | 中文 BPE 平均 ~1.5 字符/token |

与 `Kiro-Go-main/proxy/token_estimator.go:36` 完全一致。计算后向上取整，最少 1 token。

### 决策 9：模块组织 — `src/cache.rs` 转目录

```
src/cache/
├── mod.rs            // 重导出 PromptCacheUsage、CacheSimulationRatioConfig
├── simulation.rs     // 原 cache.rs 内容（三角分布、比例模拟）
└── fingerprint.rs    // 新增：FingerprintTracker、Breakpoint、FingerprintTable
```

公共 API 路径保持不变（`crate::cache::PromptCacheUsage`）。

### 决策 10：contextUsage 本地化（核心）

**变更前**：
```
Kiro Event::ContextUsage(percentage)
  → actual_input_tokens = percentage × context_window_for_model(model) / 100
  → 覆盖 context_input_tokens
  → final_input_tokens = context_input_tokens.unwrap_or(local_estimate)
```

**变更后**：
```
final_input_tokens = metering.inputTokens.unwrap_or(local_estimate)
contextUsage% = local_estimate / 1_000_000 × 100  (如下游需要)
model_context_window_exceeded ← (local_estimate ≥ 1_000_000) OR (Kiro percentage == 100)
```

`Event::ContextUsage` 事件接收保留，但**仅用于触发 stop_reason 兜底判定**，不再写入 `context_input_tokens`。

`context_window_for_model` 统一返回 `1_000_000`：
- 这是 Anthropic 官方公开的 Claude 4.x 系列窗口
- 旧版 200K 分支（haiku、未知模型兜底）删除
- 后续若需差异化，仍可恢复 match 分支

### 决策 11：账号 ID 来源与故障转移

写入和查表都用 **provider 返回的实际成功 credential_id**：
- provider 内部可能 A→B→C 故障转移
- 仅当一个 credential_id 成功响应时，调用 `tracker.update(credential_id, profile, total_input)`
- 失败链路（无 credential_id）跳过 fingerprint 写入
- 这与 Anthropic 服务端的真实缓存归属一致

### 决策 12：并发与锁

```
FingerprintTracker {
    tables: Arc<parking_lot::RwLock<HashMap<String, parking_lot::Mutex<FingerprintTable>>>>,
    config: CacheSimulationConfig,
    shutdown: Arc<AtomicBool>,
}
```

- 外层 RwLock：读多写少，读路径直接 get；写路径仅在创建新账号表时获取
- 内层 Mutex：账号粒度，compute 入口先 clone profile 出锁再算 hash（避免持锁 SHA-256）
- 后台 evict_expired 持读锁迭代 + 内层 Mutex 写删除，与请求路径竞争窗口可控

## 风险 / 权衡

| 项 | 权衡 |
|---|---|
| **指纹算法对工具调用 input 序列化稳定性** | 通过 BTreeMap 转换保证 key 顺序；tool_use input 含浮点数时 serde_json 输出可能差异（如 `1.0` vs `1`），spec 暂不覆盖此极端情形 |
| **累积 SHA-256 长会话开销** | 100 条 1KB 消息 ~ 100KB × O(N²)/2 ≈ 5MB 累积输入，SHA-256 单核 ~500MB/s，约 10ms；仅在 provider 返回后串行一次，不影响客户端 SSE 流时延；后续可引入 rolling hash 优化 |
| **TTL 后台任务测试干扰** | `FingerprintTracker::new_for_test()` 不启动后台任务；测试用同步 `evict_expired()` 入口 |
| **配置热更新** | 当前不支持 hot reload；新值需重启生效 |
| **`ephemeral1hRatio` 概率分配方差** | 改用确定性分配（决策 6），spec 场景"近似 30%"放宽到容差 ±5%（基于确定性公式） |
| **字符估算公式变动影响既有测试** | tasks B3 列出全部 4 个旧断言的新预期值 |
| **contextUsage 弃用导致 haiku 等模型口径变动** | 用户明确要求所有模型 1M；如出现错算（如真实 200K 模型上报 0.2 而非 1.0），通过 model_context_window_exceeded 触发条件保留 Kiro 100% 兜底判定 |
| **message_start 早期与 message_delta 终值不一致** | 早期 ephemeral_5m = creation, 1h = 0，下游若以 message_start 入账会和最终值偏差；建议下游以 message_delta 为准（Anthropic 协议本身设计如此） |
| **usage tracker 字段扩展** | `UsageRecord` 增加 2 个字段，serde default 兼容旧数据；Admin UI 不展示新字段（后续变更）|
