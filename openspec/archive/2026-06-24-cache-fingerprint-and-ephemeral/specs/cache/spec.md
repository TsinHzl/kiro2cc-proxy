# 规范增量：cache 模块 + token 估算 + contextUsage 本地化

## 新增需求

### 需求：指纹追踪命中计算

#### 场景：相同前缀二次请求命中
- **WHEN** 同一 `credential_id` 在 `fingerprintTtl5m` 秒内发起两次请求，且第二次请求的 system + messages 是第一次的完整前缀
- **THEN** 第二次请求 message_delta 终值 usage 满足 `cache_read_input_tokens > 0` 且 `cache_read_input_tokens ≤ floor(0.85 × total_input_tokens)`
- **AND** `input_tokens + cache_read_input_tokens + cache_creation_input_tokens == total_input_tokens`

#### 场景：完全不同请求首次进入
- **WHEN** 同一 `credential_id` 发起一个前缀与现有断点表零重叠的新请求
- **THEN** usage 满足 `cache_read_input_tokens == 0`，且 `cache_creation_input_tokens > 0`，且 `input_tokens == total_input_tokens - cache_creation_input_tokens`

#### 场景：完全相等的二次请求
- **WHEN** 同一 `credential_id` 连续发起两次完全相同的请求（system + messages 字字相同）
- **THEN** 第二次请求 `cache_read_input_tokens` 接近 total_input 且 `≤ 0.85 × total_input`
- **AND** `cache_creation_input_tokens == 0`
- **AND** `input_tokens == total_input - cache_read_input_tokens`

#### 场景：部分前缀命中
- **WHEN** 第二次请求与第一次请求的 system 一致，但 messages 从第二条起内容不同
- **THEN** `cache_read_input_tokens` 约等于 system 段的 cumulative_tokens（误差 ≤ 1 个 token，源于估算单字符权重精度）
- **AND** `cache_creation_input_tokens` 等于第二次请求 messages 部分的 token 数

#### 场景：TTL 过期淘汰
- **WHEN** 一个断点的 `last_hit_at` 距今超过 `fingerprintTtl5m` 秒，调用 `evict_expired()` 后
- **THEN** 该断点不再参与后续命中比对
- **AND** 下一次相同前缀的请求被视为首次请求（cache_read = 0）

#### 场景：命中刷新 TTL
- **WHEN** 一个断点在 `last_hit_at + fingerprintTtl5m` 之前被命中
- **THEN** 该断点的 `last_hit_at` 更新为当前时刻，过期时刻顺延

#### 场景：LRU 淘汰
- **WHEN** 单账号断点数量达到 `fingerprintMaxBreakpointsPerAccount` 上限，又有新的不同前缀请求进入
- **THEN** 最早 `last_hit_at` 的断点被移除
- **AND** 新断点被加入表中

#### 场景：tool_use input 改变不命中
- **WHEN** 两次请求 system + messages 文本相同，但其中一个 message 含 `tool_use` 块且 `input` 字段值不同
- **THEN** 第二次请求在该 tool_use 所在 message 处中断匹配
- **AND** `cache_read_input_tokens` 仅反映前 N-1 个消息的累积值

#### 场景：image source.data 改变不命中
- **WHEN** 两次请求文本相同，但其中一个 message 含 `image` 块且 `source.data` 不同
- **THEN** 第二次请求在该 image 所在 message 处中断匹配

#### 场景：账号隔离
- **WHEN** 账号 A 发起请求建立断点；账号 B 发起完全相同的请求
- **THEN** 账号 B 的 `cache_read_input_tokens == 0`（视为首次）

#### 场景：截断不变性
- **WHEN** 任意一层降级产生 `PromptCacheUsage` 后
- **THEN** `cache_read + cache_creation ≤ total_input`
- **AND** `input_tokens >= 0`
- **AND** 当 `cache_read + cache_creation > total_input` 时优先保留 `cache_read`，截断 `cache_creation`

### 需求：ephemeral TTL 拆分输出

#### 场景：默认配置仅 5m
- **WHEN** `ephemeral1hRatio == 0.0` 且产生 `cache_creation_input_tokens > 0` 的请求
- **THEN** usage 输出的 `cache_creation.ephemeral_5m_input_tokens == cache_creation_input_tokens`
- **AND** `cache_creation.ephemeral_1h_input_tokens == 0`

#### 场景：1h 比例非零（确定性分配）
- **WHEN** `ephemeral1hRatio == 0.3` 且产生 `cache_creation_input_tokens == 100` 的请求
- **THEN** `ephemeral_1h_input_tokens == floor(100 × 0.3 + 0.5) == 30`
- **AND** `ephemeral_5m_input_tokens == 70`
- **AND** `ephemeral_5m + ephemeral_1h == 100`

#### 场景：1h 比例 100%
- **WHEN** `ephemeral1hRatio == 1.0` 且产生 `cache_creation_input_tokens == 100` 的请求
- **THEN** `ephemeral_1h_input_tokens == 100`
- **AND** `ephemeral_5m_input_tokens == 0`

#### 场景：缺失 cache_creation 时字段仍存在
- **WHEN** 一次请求的 `cache_creation_input_tokens == 0`（完全 cache_read 命中或 uncached）
- **THEN** usage 输出仍包含 `cache_creation` 嵌套对象，`ephemeral_5m_input_tokens == 0` 且 `ephemeral_1h_input_tokens == 0`

#### 场景：message_start 早期上报
- **WHEN** 流式响应处于 message_start 阶段（credential_id 尚未确定）
- **THEN** usage 中 `cache_creation_input_tokens` 为 `from_ratio_config` 早期粗估值
- **AND** `ephemeral_5m_input_tokens == cache_creation_input_tokens`
- **AND** `ephemeral_1h_input_tokens == 0`
- **AND** message_delta 阶段最终值可能与 message_start 不同（下游应以 message_delta 为准）

### 需求：字符估算精细化

#### 场景：纯 ASCII 字母文本
- **WHEN** 输入 1000 个连续 ASCII 字母字符
- **THEN** `count_tokens` 返回值落在 `[200, 240]`（`ceil(1000/4.5) = 223`）

#### 场景：纯数字文本
- **WHEN** 输入 1000 个连续数字字符（'0'-'9'）
- **THEN** `count_tokens` 返回值落在 `[480, 520]`（`ceil(1000/2.0) = 500`）

#### 场景：纯 ASCII 符号
- **WHEN** 输入 100 个连续符号字符（如 `'!'`）
- **THEN** `count_tokens` 返回值落在 `[60, 80]`（`ceil(100/1.5) = 67`）

#### 场景：纯 CJK 文本
- **WHEN** 输入 1000 个连续中文字符
- **THEN** `count_tokens` 返回值落在 `[660, 700]`（`ceil(1000/1.5) = 667`）

#### 场景：空字符串与极短输入
- **WHEN** 输入 `""` 或单个字符
- **THEN** `count_tokens` 至少返回 `1`

#### 场景：混合字符
- **WHEN** 输入包含 10 字母 + 5 数字 + 3 符号 + 2 CJK 字符
- **THEN** `count_tokens` 返回值等于 `ceil(10/4.5 + 5/2.0 + 3/1.5 + 2/1.5) = ceil(2.22 + 2.5 + 2.0 + 1.33) = ceil(8.05) = 9`

### 需求：contextUsage 本地化

#### 场景：所有模型窗口统一为 1M
- **WHEN** 调用 `context_window_for_model(model)` 传入任意模型名（含 `claude-haiku-4-5`、`claude-sonnet-4-5`、`claude-opus-4-8`、`unknown-model` 等）
- **THEN** 返回值始终为 `1_000_000`

#### 场景：弃用 Kiro contextUsage 反算
- **WHEN** Kiro `Event::ContextUsage` 上报 `contextUsagePercentage == 50.0`
- **THEN** 代理不再设置 `context_input_tokens = Some(500_000)`
- **AND** `final_input_tokens` 仍由 `metering.inputTokens` 真值（如有）或本地 `count_all_tokens` 估算给出

#### 场景：contextUsage 触发 stop_reason 兜底保留
- **WHEN** Kiro `Event::ContextUsage` 上报 `contextUsagePercentage == 100.0`
- **THEN** `stop_reason` 设置为 `model_context_window_exceeded`（与原行为一致）

#### 场景：本地估算触发 stop_reason
- **WHEN** 本地 `count_all_tokens` 估算结果 ≥ `1_000_000`
- **THEN** `stop_reason` 设置为 `model_context_window_exceeded`

#### 场景：final_input_tokens 来源优先级
- **WHEN** metering 提供 `inputTokens` 真值
- **THEN** `final_input_tokens == metering.inputTokens`

- **WHEN** metering 缺失 `inputTokens`
- **THEN** `final_input_tokens == count_all_tokens(system, messages, tools)`
- **AND** 不再依赖 `Event::ContextUsage` 反算

#### 场景：final_input_tokens 不依赖 credential_id
- **WHEN** 多账号故障转移场景下 provider 在 credential A 上失败、credential B 上成功
- **THEN** `final_input_tokens` 来源（metering / 本地估算）不受 credential 切换影响
- **AND** 仅 fingerprint 表写入归属到实际成功的 credential B

### 需求：降级链优先级

#### 场景：metering 真值齐全
- **WHEN** Kiro `meteringEvent` 同时含 `cache_read_input_tokens` 与 `cache_creation_input_tokens`
- **THEN** 最终 usage 直接采用 metering 真值，不调用其他三层

#### 场景：metering 缺失但 credits 反推成功
- **WHEN** metering 字段缺失，且 `infer_cache_read_tokens` 返回 `Some(read)`
- **THEN** usage 中 `cache_read_input_tokens == read`，`cache_creation_input_tokens == 0`
- **AND** 不再调用 `FingerprintTracker`

#### 场景：credits 反推失败但指纹追踪可用
- **WHEN** metering 缺失，`infer_cache_read_tokens` 返回 `None`，且 `tracker.compute(credential_id, ...)` 返回 `Some`
- **THEN** usage 采用指纹追踪结果，`cache_creation_input_tokens` 可能 > 0

#### 场景：全部失败兜底
- **WHEN** metering 缺失，credits 反推失败，且 `fingerprintEnabled == false` 或 tracker 表为空
- **THEN** usage 采用 `PromptCacheUsage::from_ratio_config` 三角分布模拟（保持现有行为）

#### 场景：fingerprint 写入仅在 credential_id 已确定后
- **WHEN** provider 返回 `(_, Some(credential_id))`
- **THEN** 调用 `tracker.update(credential_id, profile, total_input)` 写入新断点

- **WHEN** provider 返回 `(_, None)`（所有重试失败）
- **THEN** 不调用 `tracker.update`

### 需求：配置项兼容性

#### 场景：禁用指纹回退到现行行为
- **WHEN** 配置 `cacheSimulation.fingerprintEnabled == false`
- **THEN** 系统行为与变更前一致（除 contextUsage 本地化与字符估算公式变更外）：metering 缺失 + credits 反推失败时直接走 `from_ratio_config`
- **AND** 后台 evict 任务不执行实际清理操作

#### 场景：环境变量覆盖
- **WHEN** 启动时设置 `CACHE_SIMULATION_FINGERPRINT_ENABLED=false`
- **THEN** 运行时配置 `fingerprintEnabled == false`，行为如上一场景

#### 场景：嵌套环境变量覆盖
- **WHEN** 启动时设置 `CACHE_SIMULATION_FINGERPRINT_TTL_5M=600`
- **THEN** 运行时 `fingerprintTtl5m == 600`

## 修改需求

### 需求：PromptCacheUsage 结构扩展
- 新增字段 `cache_creation_5m_input_tokens: i32` 与 `cache_creation_1h_input_tokens: i32`
- 不变性：`cache_creation_5m + cache_creation_1h == cache_creation_input_tokens`
- 现有方法 `total_input_tokens / scale_to / uncached / from_ratios / from_ratio_config` 行为保持兼容（新字段默认 0 或按 `ephemeral1hRatio` 拆分填充）
- 新增 `clamp_to_total(total_input: i32) -> Self` 强制截断方法（保证降级链不变性）

### 需求：UsageRecord 结构扩展（向后兼容）
- 新增 `cache_creation_5m_input_tokens: i32` 与 `cache_creation_1h_input_tokens: i32`（serde default = 0）
- 旧数据反序列化不破坏
- Admin UI 当前不展示新字段（后续变更）

### 需求：context_window_for_model 简化
- 函数始终返回 `1_000_000`
- 删除 200K 兜底分支
- 模块文档注释更新："所有模型按 Anthropic 公开 1M 窗口；如需差异化窗口可恢复 match 分支"
