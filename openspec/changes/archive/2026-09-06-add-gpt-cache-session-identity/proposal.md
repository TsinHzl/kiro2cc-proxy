# 变更提案：add-gpt-cache-session-identity

## 背景

实测证实（2026-09-05 对照实验，gpt-5.6-sol 4 轮相同前缀，本地代理）：

| 场景 | R1 | R2 | R3 | R4 | R2-R4 均值 |
|---|---|---|---|---|---|
| 裸请求（无 metadata/system/tools） | 0.1106 | 0.1023 | 0.0985 | 0.0892 | 0.0967 |
| 带 metadata.user_id 稳定会话 | 0.1055 | 0.0646 | 0.0657 | 0.0501 | 0.0601 |

带稳定会话身份后 R2-R4 credits 下降约 37.9%（样本 n=3 轮，存在混杂变量：
稳定会话同时带来 sticky 路由同账号连续请求，账号级缓存与上游会话缓存
两效应无法在此实验中完全剥离；实际降幅待实施后验证），证实上游 GPT 缓存
折扣依赖 conversationId/agentContinuationId 的跨轮稳定。

三个断裂点导致经代理的请求无法命中：

1. **converter.rs 裸请求退化**：`derive_fallback_conversation_id`
   （converter.rs:706）在 system 与 tools 皆空时返回 None → 每轮随机 UUID
   → 上游视为全新会话。Claude Code 等带 system/tools 的客户端不受影响，
   但裸 API 调用（脚本、简单 SDK 用法）全部漏掉。
   该短路是 issue #27（会话折叠致 sticky 钉死单账号）时代的防御性设计，
   当时 fallback seed 尚未包含首条消息；现在 seed 已含首条消息，同 seed
   折叠已大幅收窄（需同时满足相同 system+相同 tools+相同首条消息），
   且实测收益（credits 降约 37.9%）远大于收窄后的折叠风险，故移除该短路。
2. **OpenAI 层会话身份丢失**：`/v1/chat/completions` 与 `/v1/responses`
   的转换器（chat_request.rs:85-112、responses_request.rs:217-246）从不
   构造 `metadata` 字段 → 即使客户端有会话概念（user 字段）也无法传递。
3. **metadata 路径解析太窄**：`convert_request`（converter.rs:802-806）对
   metadata.user_id 必须经 `extract_session_id` 解析成功才走 metadata 路径，
   该函数仅识别 `user_xxx_account__session_<UUID>` 与 JSON
   `{"session_id":...}` 两种格式。OpenAI 客户端传入的 `user`（如
   `user-123`、邮箱、纯 UUID）均解析失败 → 即使任务 2 透传成功也退回
   fallback，透传形同虚设。必须同步扩展解析链路。

## 目标范围

**在范围内：**
- converter.rs：裸请求（system+tools 皆空）时改用「首条消息内容」派生稳定
  conversationId，替代随机 UUID
- converter.rs：`extract_session_id` 新增「纯 UUID 直通」格式（OpenAI `user`
  为纯 UUID 时直接采用为 session UUID）
- openai/chat_request.rs：透传 OpenAI 请求的 `user` 字段 → metadata.user_id
- openai/responses_request.rs：同上
- 既有回归测试 `test_derive_fallback_none_without_system_and_tools`
  （converter.rs:2831）按新行为改写 + 补充新行为单测

**不在范围内：**
- metering/cache_read 上报逻辑（本地估算层不动）
- Kiro CLI 直连场景（用户已在用，不受本变更影响）
- additionalModelRequestFields 对 4.5 代际模型的 400 回归
  （commit 3996ba8「refresh compact history for GPT requests」将
  `build_additional_model_request_fields` 的跳过条件从
  `ends_with("4.5") || starts_with("gpt-")` 改为仅 `is_gpt_model`，删除了
  df7f471 加入的 4.5 代际跳过，已用 git show 复核；另行修复）

> **主动超出（经用户确认保留，2026-09-06）：** 原始需求仅提 GPT，但
> ① 裸请求 fallback 派生是模型无关的，将同时惠及 Claude 裸请求；
> ② extract_session_id 纯 UUID 直通对全部 Anthropic 原生请求生效。
> 两者均已在核验中披露，用户确认保留（converter 层保持模型无关，
> 不按模型条件化）。

## 技术方案

1. converter.rs `derive_fallback_conversation_id`：移除 system+tools 皆空的
   None 短路；空时退化为仅用首条消息派生（seed 仅含 `|first=...` 段）。
   折叠风险缓解：派生值同时影响 sticky 路由与上游缓存，
   为把「会话折叠钉死账号」的影响限制在缓存收益层面、不恶化负载均衡，
   sticky 侧维持现状（TTL 60min + 不健康驱逐已兜底）；文档注释同步改写。
2. converter.rs `extract_session_id`：在 JSON 解析前新增「字符串本身是合法
   UUID」直通分支（`is_valid_uuid(user_id)` → Some(user_id)）。
   注意该函数被全部 Anthropic 原生请求共享：第三方客户端传纯 UUID
   `metadata.user_id` 时行为从「fallback 派生」变为「直通采用」，方向合理
   （显式声明身份应优先），属全局变化而非 OpenAI 层局部改动。
   语义注意：OpenAI `user` 官方语义是「终端用户标识」，
   同一 user 的多个并行对话会折叠进同一 session UUID——这是有意取舍：
   对缓存而言同 user 前缀重合度高，折叠反而是收益；正确性不受影响。
3. OpenAI 层（chat_request.rs / responses_request.rs）：
   `body.user` 为非空字符串时写入 `anthropic["metadata"]["user_id"]`。
   converter 侧解析链不变：`user_xxx_account__session_<UUID>` 与 JSON 格式
   照旧走 metadata 路径；纯 UUID 新增直通；不可解析值（如 user-123、邮箱）
   落到 fallback（现在含裸请求首条消息派生），不会因透传失败而退化为
   随机 UUID。
4. 不做 metadata 伪造：仅透传客户端已声明身份或派生，不引入新配置项。

## 预期影响

- 裸请求 GPT/Claude 场景：R2 起 credits 下降（**方向**有实测支撑，**幅度**
  因 sticky 同账号混杂变量与 n=3 小样本无法在实施前精确承诺，以实施后
  同脚本复测为准）
- OpenAI 客户端（Codex CLI 等）：获得与 Kiro CLI 同等的会话缓存能力
- Claude Code 等已有 metadata 的客户端：无行为变化（metadata 路径优先级
  与解析格式完全不变）
- sticky 路由：裸请求会话从「每轮随机账号」变为「按派生 ID 稳定账号」；
  可通过既有 sticky_hits/sticky_misses 指标（token_manager.rs:1079）观察
  折叠程度，同 seed 折叠由 sticky TTL 60min + 不健康账号驱逐兜底

## 风险

- 不同会话首条消息恰好相同（探活脚本、"hi"、模板化首问等低熵场景）→
  折叠为同一 conversationId：影响是共享 sticky 路由（钉同一账号，多账号
  局部退化为单账号、可能加剧 429）+ 上游缓存共享（不影响正确性）；
  缓解依赖 sticky 缓存 TTL 与健康度驱逐，实施后通过 sticky_hits/misses
  指标观察，若折叠显著再评估首条消息熵/长度下限
- OpenAI 客户端 user 字段填 PII（邮箱等）→ 原样透传给上游，与 OpenAI
  官方语义一致，可接受
- 同一 OpenAI user 的并行多对话折叠为同一 session → 上游缓存前缀混合，
  命中率可能下降（缓存抖动），但不劣于现状（现状每轮随机 UUID）
