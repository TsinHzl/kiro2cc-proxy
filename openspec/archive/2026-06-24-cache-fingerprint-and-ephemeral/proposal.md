# 变更提案：cache-fingerprint-and-ephemeral

## 背景

当前 prompt cache token 上报与输入 token 估算存在多个精度短板：

1. **三层降级链最末层是随机模拟** — `PromptCacheUsage::from_ratio_config` 三角分布与请求历史无关，长会话与首次请求得到相似分布
2. **ephemeral 拆分缺失** — `cache_creation_input_tokens` 不区分 `ephemeral_5m` / `ephemeral_1h`，下游计费粒度受损
3. **字符估算二分类粗糙** — `count_tokens` 仅按"西文 / 非西文"分类，长 ASCII 文本偏高 ~30%，纯数字文本偏低 ~50%
4. **contextUsage 依赖 Kiro 上游值** — Kiro `contextUsageEvent.contextUsagePercentage` 取自上游账户级口径，对部分模型阈值与官方公开值不一致（如 haiku 在 200K 而非 1M），且事件时序晚于客户端早期上报需求

本变更引入账号级指纹追踪替换最末层模拟、补齐 ephemeral 拆分、精细化字符权重，并**用本地输入 token 估算 + 统一 1M 窗口反算 contextUsage**，彻底脱离 Kiro 上游值。

## 目标范围

**在范围内：**

### A. 配置项扩展（`src/model/config.rs`）

- `cacheSimulation.fingerprintEnabled: bool`（默认 `true`）
- `cacheSimulation.fingerprintTtl5m: u64`（默认 300 秒）
- `cacheSimulation.fingerprintTtl1h: u64`（默认 3600 秒）
- `cacheSimulation.ephemeral1hRatio: f64`（默认 0.0，0.0~1.0 范围）
- `cacheSimulation.fingerprintMaxBreakpointsPerAccount: usize`（默认 256）
- `apply_env_overrides` 支持嵌套字段：`CACHE_SIMULATION_FINGERPRINT_ENABLED` / `_TTL_5M` / `_TTL_1H` / `_EPHEMERAL_1H_RATIO` / `_FINGERPRINT_MAX_BREAKPOINTS`
- `config.example.json`（如存在）同步增加示例

### B. PromptCacheUsage 字段扩展（`src/cache.rs`）

- 新增 `cache_creation_5m_input_tokens: i32` 与 `cache_creation_1h_input_tokens: i32`
- 不变性：`cache_creation_5m + cache_creation_1h == cache_creation_input_tokens`
- `scale_to` 按比例同步缩放两个新字段并保持不变性
- `from_ratios / from_ratio_config / uncached` 输出时按 `ephemeral1hRatio` 切分

### C. 字符估算精细化（`src/token.rs`）

- 按四分类加权：ASCII 字母（A-Za-z）/ 4.5；数字（0-9）/ 2.0；其他 ASCII（符号、空白）/ 1.5；非 ASCII / 1.5
- 计算公式：`tokens = ceil(letters/4.5 + digits/2.0 + ascii_symbols/1.5 + non_ascii/1.5).max(1)`
- 现有 4 个测试断言必须更新到新公式预期值（详见 tasks.md B3）
- 模块文档注释同步更新

### D. 指纹追踪模块（新建 `src/cache/fingerprint.rs`）

- 模块组织：`src/cache.rs` → `src/cache/mod.rs` + `src/cache/simulation.rs`（原内容）+ `src/cache/fingerprint.rs`（新增）；公共导出 `crate::cache::PromptCacheUsage` 路径不变
- 数据结构：
  ```rust
  enum EphemeralTier { FiveM, OneH }
  struct Breakpoint { hash: [u8; 32], cumulative_tokens: i32, tier: EphemeralTier, last_hit_at: Instant }
  struct FingerprintTable { breakpoints: Vec<Breakpoint> }
  struct FingerprintTracker {
      tables: Arc<parking_lot::RwLock<HashMap<String, parking_lot::Mutex<FingerprintTable>>>>,
      config: CacheSimulationConfig,
      shutdown: Arc<AtomicBool>,
  }
  ```
- 算法：累积 SHA-256（保证前缀单调性）+ 顺序比对 + 0.85 × total_input 封顶
- canonicalize 规则：
  - `Value::String` → trim
  - `Value::Array` 元素按 `type` 分类处理：
    - `text` → 提取 `.text`
    - `tool_use` → `"tool_use:" + name + ":" + canonicalize(input)`（input 字段排序后 JSON 序列化）
    - `tool_result` → `"tool_result:" + tool_use_id + ":" + canonicalize(content)`
    - `image` / `document` → `"image:" + source.media_type + ":" + sha256_short(source.data)`
    - 其他 → `type` 字符串
- TTL：命中刷新；后台 `tokio::spawn` 任务每 30 秒调用 `evict_expired`；持 `shutdown: Arc<AtomicBool>` 优雅退出
- LRU：单账号断点 > `fingerprintMaxBreakpointsPerAccount` 时按 `last_hit_at` 升序淘汰

### E. 降级链接入（`src/anthropic/handlers.rs`、`src/anthropic/stream.rs`）

- `Arc<AppState>` 注入 `Arc<FingerprintTracker>`
- **接入时序关键约定**：fingerprint compute 与 update 必须延后到 `provider.call_api_stream` 返回 `credential_id` 之后；message_start 阶段 fingerprint 暂不参与，由 `from_ratio_config` 给早期值
- 终值（message_delta / 缓冲端点 message_start）使用以下优先级：

  ```
  Layer 1: meteringEvent 双字段齐全 → 直接采用真值
  Layer 2: infer_cache_read_tokens 反推成功 → cache_read=真值，cache_creation=0
  Layer 3: FingerprintTracker.compute(credential_id, ...) 返回 Some → 完整双字段
  Layer 4: PromptCacheUsage::from_ratio_config → 兜底
  ```

- 写入指纹表：仅在 **provider 返回成功且 credential_id 确定后** 调用 `tracker.update(credential_id, profile, total_input)`
- 截断不变性：所有 layer 输出后强制 `cache_read + cache_creation ≤ total_input`，超出则按 `cache_read` 优先保留

### F. ephemeral 字段输出（`src/anthropic/stream.rs`）

- 输出 SSE usage 时构造 `cache_creation: { ephemeral_5m_input_tokens, ephemeral_1h_input_tokens }` 嵌套对象
- message_start 阶段早期上报：`ephemeral_5m_input_tokens = cache_creation_input_tokens, ephemeral_1h_input_tokens = 0`（标记为粗估，与最终值不一致由 message_delta 修正）
- message_delta 终值：按真实 tier 拆分输出
- 即使两值为 0 也保留 `cache_creation` 嵌套对象，保证下游解析稳定

### G. contextUsage 本地化（核心改动 — `src/anthropic/stream.rs`、`src/anthropic/handlers.rs`）

- `context_window_for_model` 统一返回 `1_000_000`（所有模型，含 haiku）
- **弃用** Kiro `Event::ContextUsage` 的 `actual_input_tokens` 反算路径：
  - `stream.rs:772-790` 与 `handlers.rs:940-948` 中 `context_input_tokens = Some(percentage × window / 100)` 删除
  - 保留事件接收以维持 `model_context_window_exceeded` stop_reason 判定，但不再用于 input_tokens 反算
- `final_input_tokens` 来源变为：
  ```
  metering 真值优先（如 inputTokens 字段存在）
   → 本地 count_all_tokens 估算
  ```
- contextUsage 百分比由本地估算反算输出：`contextUsage% = local_estimate / 1_000_000 × 100`（如下游需要可选输出）
- `model_context_window_exceeded` 触发条件改为：**本地估算 ≥ 1_000_000** 或 Kiro `ContextUsage` 上报 100%（保留兜底）
- `empty_response_oversized_threshold` 阈值更新：所有模型按 `1_000_000 × 0.45 = 450_000`

### H. 用量持久化（`src/model/usage.rs`）

- `UsageRecord` 增加 `cache_creation_5m_input_tokens` 与 `cache_creation_1h_input_tokens` 字段（默认 0，向后兼容）
- `tracker.record` 调用点（`handlers.rs:1040` 等）传入拆分值
- Admin UI 后续如需展示，另起变更

### I. 测试（各模块 `#[cfg(test)] mod tests`）

- `src/cache/fingerprint.rs` 内联测试：相同前缀 / 不同前缀 / 部分前缀 / TTL 过期 / 命中刷新 / LRU 淘汰 / tool_use 命中差异 / image hash 差异 / 完全相等命中
- `src/token.rs` 内联测试：四类字符独立 + 混合 + 极短输入（旧断言数值同步更新）
- `src/cache/mod.rs` 内联测试：PromptCacheUsage::scale_to 保持 5m+1h 不变性
- `src/anthropic/stream.rs` 内联测试：`context_window_for_model` 所有已知模型返回 1M；contextUsage 事件不再写入 `context_input_tokens`
- `src/anthropic/handlers.rs` 集成测试：降级链优先级（metering > credits > fingerprint > ratio）— 用 mock provider

### J. 文档与速查表

- `docs/代码速查表.md`：新增"prompt cache 指纹追踪"小节
- `CLAUDE.md` 关键模块表：增加 `src/cache/fingerprint.rs` 一行
- `src/cache/fingerprint.rs` 模块级 doc 注释：说明算法、不变性、配置项

**不在范围内：**

- 指纹持久化（重启即丢失，与 Anthropic 5min TTL 对齐，无业务损失）
- 跨账号指纹共享（账号隔离原则）
- 修改 `infer_cache_read_tokens` 现有公式（保留作为 Layer 2）
- 修改 `count_all_tokens` 远程 API 路径（仅改本地公式）
- Admin UI 暴露指纹表查看接口
- 完全删除 `Event::ContextUsage` 事件类型（保留接收以兜底 stop_reason 判定）
- Layer 2 与 Layer 3 互补（仅串行降级，不组合）

## 技术方案

### 接入时序（关键）

```
1. handler 接收请求 → 本地估算 input_tokens（用 token.rs 新公式）
2. 调用 provider.call_api_stream → 流式接收
3. message_start 早期上报：cache_creation = from_ratio_config（不查 fingerprint）
4. 流式接收 metering / contextUsage 事件（仅 metering 用于 final_input_tokens）
5. 流结束，provider 返回 (response, credential_id)
6. fingerprint_tracker.compute(credential_id, profile, total_input) → Option<PromptCacheUsage>
7. 按降级链选择最终 usage
8. fingerprint_tracker.update(credential_id, profile, total_input) → 写入新断点
9. message_delta 输出最终 usage（含 ephemeral 拆分）
10. tracker.record(... 拆分字段 ...) → 持久化
```

### 字符估算公式

```rust
fn count_tokens(text: &str) -> u64 {
    let mut letters = 0usize;
    let mut digits = 0usize;
    let mut ascii_symbols = 0usize;
    let mut non_ascii = 0usize;
    for c in text.chars() {
        match c {
            'A'..='Z' | 'a'..='z'                 => letters += 1,
            '0'..='9'                              => digits += 1,
            c if (c as u32) < 0x80                 => ascii_symbols += 1,
            _                                      => non_ascii += 1,
        }
    }
    let units = letters as f64 / 4.5
              + digits as f64 / 2.0
              + ascii_symbols as f64 / 1.5
              + non_ascii as f64 / 1.5;
    (units.ceil() as u64).max(1)
}
```

### 降级链截断不变性

任意 layer 产出 `PromptCacheUsage` 后，调用 `clamp_to_total(total_input)` 强制：
- `cache_read = min(cache_read, total_input)`
- `cache_creation = min(cache_creation, total_input - cache_read)`
- `input_tokens = total_input - cache_read - cache_creation`（保证非负）

## 预期影响

| 维度 | 影响 |
|------|------|
| 准确度 | 末层从随机模拟 → 指纹追踪；contextUsage 弃用 Kiro 不一致口径 |
| 字段完整 | 补全 `ephemeral_5m_input_tokens` / `ephemeral_1h_input_tokens` |
| 内存占用 | 单账号 256 断点 × ~96B ≈ 24KB；10 账号 ≈ 240KB |
| 性能 | 命中比对 O(N≤256) 顺序扫描 < 10μs；累积 SHA-256 O(总字节数) 在 1M context 约 5-10ms（仅在 provider 返回后串行一次） |
| 兼容性 | `fingerprintEnabled: false` 完全回退；usage 输出对未升级的下游解析器无影响（新增字段嵌套不破坏旧 schema） |
| 依赖 | 不新增 crate（sha2 / parking_lot / tokio 均已存在） |
| Kiro 上游依赖 | contextUsageEvent 仅保留兜底，input_tokens 完全本地化 |

## 风险

| 风险 | 应对 |
|------|------|
| 指纹算法对 tool_use input 序列化不稳定 | input 字段按 key 字典序序列化（serde_json 默认 + BTreeMap 转换）；spec 增加场景验证 |
| credential_id 在 provider 失败时不可用 | provider 返回 `(_, None)` 时跳过 fingerprint update，使用 ratio 兜底 |
| 累积 SHA-256 计算长会话开销 | 在 provider 返回后串行计算（已脱离客户端关键路径）；可在后续变更引入增量哈希 |
| `count_tokens` 公式变更导致 token.rs 既有测试断言失败 | tasks.md B3 显式列出新预期值，作为单独子任务 |
| haiku 等模型窗口从 200K 改为 1M 导致 `empty_response_oversized_threshold` 阈值放宽 | 该阈值用于检测"上下文过大导致空响应"，1M 阈值仍保留功能（极端长上下文）；如出现误判，可后续单独调阈值 |
| 多账号故障转移导致写入归属错位 | 写入仅在 provider 返回的 credential_id 上发生；A 失败回退到 B，写入到 B 的表 |
| `Event::ContextUsage` 弃用导致 `model_context_window_exceeded` 漏判 | 触发条件改为"本地估算 ≥ 1M OR Kiro 事件 100%"双重保险 |
| 配置回滚 | `fingerprintEnabled: false` + 保留旧 from_ratio_config 路径 |
| 后台 evict 任务在测试中干扰 | tracker 暴露同步 `evict_expired()` 入口；测试构造时不启动后台任务 |
| 并发写竞争 | 外层 `parking_lot::RwLock` 仅锁哈希表入口；内层 `parking_lot::Mutex<FingerprintTable>` 账号粒度，compute 先 clone profile 出锁再计算 hash |
