---
name: cache-credits-analyzer
description: 分析 kiro2cc-proxy 访问日志，计算 Prompt Caching 节省的 credits。只要用户粘贴了含有"输入token 输出token 费用$ credits✓"格式的日志行，并询问节省了多少credits、缓存效率、cost分析等，立即使用此 skill。触发关键词：节省了多少credits、cache节省、分析日志、caching savings、credits分析、计算节省、这些数据节省了多少。
---

# Kiro Cache Credits 节省分析

## 日志格式说明

日志每行 8 列（制表符或多空格分隔）：

```
时间戳    IP    邮箱    模型    输入tokens    输出tokens    费用($)    credits✓
```

示例：
```
2026/05/25 16:19:38  127.0.0.1  user@example.com  claude-sonnet-4-6  11.3K  10  $0.0339  0.1382✓
```

- **输入tokens**：支持 `11.3K`、`920` 等格式（K = × 1000）
- **费用($)**：`estimated_cost`，按 Anthropic 全价计算（无缓存折扣）
- **credits✓**：`credits_used`，来自 Kiro meteringEvent 的真实 credits 消耗

## 分析步骤

### 1. 解析日志

从用户提供的日志文本中逐行提取：
- `email`
- `cost_usd`（去掉 `$`，转为 float）
- `credits_used`（去掉 `✓`，转为 float）

忽略无 `✓` 标记的行（缺少 meteringEvent 数据，不纳入分析）。

### 2. 确定基准 k（无缓存倍率）

计算每行的 `k = credits_used / cost_usd`。

**k 的物理含义**：
- k ≈ 7.06：该请求没有命中缓存（第一次请求或全新 context）
- k < 7.06：命中了 cache read（实际成本更低）
- k > 7.06：发生了 cache creation（写入缓存，1.25x 定价，比全价贵）

**基准 k 的取值**：优先从数据中提取 k 最接近 7.06 的几行验证；若数据中无明显无缓存行，使用固定值 **k_ref = 7.06**（来自项目历史实测）。

### 3. 计算节省

```
无缓存应消耗 credits（per row） = cost_usd × k_ref
实际消耗 credits               = credits_used
节省 credits（per row）        = 无缓存credits - 实际credits
```

- **正值**：缓存命中节省了 credits
- **负值**：cache creation 写入开销（属于"先花后省"）

### 4. 汇总输出

输出以下结构的分析结果：

```
## Prompt Caching Credits 节省分析

基准 k（无缓存）：7.06 credits/$

| 指标 | 数值 |
|------|------|
| 请求总数 | N 条 |
| 总 estimated_cost（Anthropic 全价） | $X.XXXX |
| 假设无缓存总 credits | X.XXXX |
| 实际消耗总 credits | X.XXXX |
| **净节省 credits** | **X.XXXX** |
| 节省比例 | XX.X% |
| 折算 API 成本节省 | ~$X.XXXX |

### Cache 构成
- 缓存命中节省（gross）：+X.XXXX credits
- Cache creation 额外开销：-X.XXXX credits（如有大输出行，通常为写缓存成本）

### 按用户分组
| 用户 | 实际 credits | 无缓存 credits | 节省 credits | 节省率 |
|------|-------------|---------------|-------------|--------|
| ... |

### 典型行分析
- 缓存最深（k最小）：... → k=X.XX，节省率XX%
- Cache creation行（k最大）：...  → k=X.XX（写缓存开销）
- 无缓存基准行（k≈7.06）：...
```

## 注意事项

- **k_ref = 7.06** 是从项目日志实测的无缓存基准，对应 Sonnet 模型。若日志包含 Opus/Haiku 模型，k_ref 可能不同，需从该模型的无缓存行重新推算。
- Cache creation 行（k > 7.06）表示该请求写入了 prompt cache，后续请求因此受益——它的"负节省"是整个对话缓存收益的前置成本。
- `estimated_cost` 是代理按 Anthropic 官方定价在本地估算的，不含缓存折扣；`credits_used` 是 Kiro 实际扣除的，反映了真实的缓存折扣。两者之差乘以 k_ref 即为节省量。
