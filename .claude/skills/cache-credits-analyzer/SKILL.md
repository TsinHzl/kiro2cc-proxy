---
name: cache-credits-analyzer
description: 分析 kiro2cc-proxy 访问日志，计算 Prompt Caching 节省的 credits。只要用户粘贴了含有"输入token 输出token 费用$ credits✓"格式的日志行，并询问节省了多少credits、缓存效率、cost分析等，立即使用此 skill。触发关键词：节省了多少credits、cache节省、分析日志、caching savings、credits分析、计算节省、这些数据节省了多少。
---

# Kiro Cache Credits 节省分析

## 日志格式说明

支持两种日志来源：

### 格式 A：访问日志（每行 8 列，制表符/多空格分隔）

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

### 格式 B：runtime-log（`[usage] 入库` 行）

```
... [usage] 入库: model=<m> input=<n> output=<n> metering_credits=Some(<f>) credits_per_ktok=Some(<f>) effective_rate=Some(<f>) cache_read=<None|Some(n)> cache_creation=<None|Some(n)> ...
```

解析时：
- `model` → 模型名
- `input` / `output` → token 数（已是整数，无需 K 换算）
- `metering_credits` → `credits_used`（Some 内取值）
- `cost_usd` 需用 Anthropic 官方定价本地折算：
  ```
  cost_usd = (input * input_price + output * output_price) / 1e6
  ```
  各模型定价（$/1M tokens）：
  | 模型 | input | output |
  |------|------:|------:|
  | claude-opus-4-6 / 4-7 | 15.0 | 75.0 |
  | claude-sonnet-4-6 / 4-7 | 3.0 | 15.0 |
  | claude-haiku-* | 0.80 | 4.0 |

## 分析步骤

### 1. 解析日志

从用户提供的日志文本中逐行提取：
- `model`
- `cost_usd`（格式 A 直接取；格式 B 由 input/output 折算）
- `credits_used`

格式 A 忽略无 `✓` 标记的行；格式 B 忽略无 `metering_credits=Some(...)` 的行。

### 2. 确定基准 k_ref（按模型分别推算）

**核心原则：每个模型独立推 k_ref，绝不全局共用一个值。**

不同模型的 credit/$ 倍率显著不同（Sonnet ≈ 7.06，Opus ≈ 2.5）。
混用单一 k_ref 会让 Opus 的"基准"虚抬 ~2.7 倍，节省被严重高估。

#### 推算流程（每个模型独立执行）

1. 计算该模型每行的 `k = credits_used / cost_usd`
2. 取该模型样本中 **k 的最大值** 作为 k_ref（命中缓存只会让 k 变小；
   最大 k 最接近"无缓存"基准）
3. 若 k_max > 经验值 × 1.30，判为 cache-creation 行污染（写缓存定价上限 1.25×，留 5% 容差），剔除后取次大
4. 该模型样本数 < 3 条 → 回退到下表经验值，并在输出中显式 ⚠️ 告警

#### 各模型经验值 k_ref（fallback 用）

| 模型 | k_ref (credits/$) | 来源 |
|------|------------------:|------|
| claude-sonnet-4-6 | 7.06 | 项目历史实测 |
| claude-sonnet-4-7 | 7.06 | 同上（同价位）|
| claude-opus-4-6   | 2.40 | 实测 |
| claude-opus-4-7   | 2.60 | 实测 |
| claude-haiku-*    | 未知 | 首次出现时实测，回退前先告警 |

### 3. 计算节省（按模型分别折算）

对每行：
```
该模型的 k_ref = 上一步推算结果
无缓存应消耗 credits = cost_usd × k_ref(model)
实际消耗 credits     = credits_used
节省 credits         = 无缓存credits − 实际credits
```

汇总时分别累加各模型的 baseline 与 actual，最后再求总和。
**禁止用单一全局 k_ref 折算所有模型。**

- **正值**：缓存命中节省了 credits
- **负值**：cache creation 写入开销（属于"先花后省"）

### 4. 汇总输出

输出以下结构的分析结果：

```
## Prompt Caching Credits 节省分析

### k_ref 推算结果（按模型）

| 模型 | 样本数 | 实测 k_max | 采用 k_ref | 来源 |
|------|------:|----------:|----------:|------|
| claude-sonnet-4-6 | 90 | 7.18 | 7.18 | 实测 |
| claude-opus-4-7   |  7 | 2.60 | 2.60 | 实测 |
| claude-opus-4-6   |  7 | 2.38 | 2.38 | 实测 |

> 样本 < 3 条的模型必须标 ⚠️ "回退经验值，结果不可靠"

### 总览

| 指标 | 数值 |
|------|------|
| 请求总数 | N 条 |
| 总 estimated_cost（Anthropic 全价） | $X.XXXX |
| 假设无缓存总 credits（按模型 k_ref 折算后求和） | X.XXXX |
| 实际消耗总 credits | X.XXXX |
| **净节省 credits** | **X.XXXX** |
| 节省比例 | XX.X% |
| 折算 API 成本节省 | ~$X.XXXX |

### 按模型分组

| 模型 | 请求数 | k_ref | baseline | actual | 节省 | 节省率 |
|------|------:|-----:|--------:|------:|----:|------:|
| ...  |       |      |         |       |     |       |

### Cache 构成
- 缓存命中节省（gross）：+X.XXXX credits
- Cache creation 额外开销：-X.XXXX credits（如有大输出行，通常为写缓存成本）

### 按用户分组（仅格式 A 有 email 字段时）
| 用户 | 实际 credits | 无缓存 credits | 节省 credits | 节省率 |
|------|-------------|---------------|-------------|--------|
| ... |

### 典型行分析（按模型分别列出）
- 该模型缓存最深（k最小）：... → k=X.XX，节省率XX%
- 该模型 cache creation 行（k > k_ref）：...
- 该模型无缓存基准行（k≈k_ref）：...
```

## 注意事项

- **绝不全局共用 k_ref**：每个模型独立推算。混用单一 k_ref 会让 Opus 节省
  被高估约 2.7 倍。
- **k_max 异常告警**：若某模型 k_max 超过经验值 × 1.30（与推算流程阈值一致），
  怀疑有 cache-creation 行污染样本，剔除后重算。
- **样本不足告警**：模型样本数 < 3 → 输出标 ⚠️，提醒该模型节省量是估算。
- **runtime-log 缺 cache 字段**：当 `cache_read=None cache_creation=None` 时
  （上游代理未透传），节省量是 baseline − actual 反推，无法用真实 cache token
  交叉验证；报告里必须注明"间接估计"。
- **Cache creation 行**（k > 该模型 k_ref）表示该请求写入了 prompt cache，
  后续请求因此受益——它的"负节省"是整个对话缓存收益的前置成本。
- `estimated_cost` 是按 Anthropic 全价的本地估算（不含缓存折扣）；
  `credits_used` 是 Kiro 实际扣除值（含缓存折扣）；两者之差乘以**该模型的**
  k_ref 即为该模型节省量。
