# 变更提案：fix-retry-permit-release-and-token-dedup

## 背景

用户反馈 issue #32「opus5 变慢」已被仓库所有者以「上游网络/限流」理由关闭。经代码复查，发现两个真实存在、可能放大该现象的结构性问题：

1. `src/kiro/provider.rs` 中 `call_api_with_retry` / `call_mcp_with_retry` 在收到 429 / 5xx / 发送失败等错误后进入指数退避 `sleep()` 期间，仍持有该账号的每账号并发 semaphore permit（`_cred_permit`）。该变量声明在 `for` 循环体作用域内，直到本次循环迭代结束（包括 `continue` 之后）才会被 drop，意味着退避等待期间该账号的并发槽位被无谓占用。在已被限流的账号上，代理自身的重试逻辑反而加剧了同账号内的排队争用——对 opus5（`rateMultiplier` 2.2，更易触发 429）尤为明显。此外，两个函数在获取 `_cred_permit` 之后才调用 `wait_for_rpm_gate`（内部最长 5s 的 `sleep`），RPM 限流等待期间同样无谓占用该账号的并发槽位，且这条路径与"账号被限流导致变慢"直接相关。
2. `src/anthropic/handlers.rs` 的 `post_messages` 与 `post_messages_cc` 两个处理函数中，对同一请求分别调用 `token::count_prefix_tokens`（前缀 tokens，用于 cache_read 估算）与 `token::count_all_tokens`（全量 tokens），两者对 system + tools + 除最后一条消息外的历史消息做了完全重复的 BPE 编码计算，是结构性冗余的双重编码。

## 目标范围

**在范围内：**
- 修复 1a：`provider.rs` 中所有退避 `sleep()` 调用点（`call_api_with_retry` 5 处 + `call_mcp_with_retry` 4 处），在进入 sleep 前显式释放 `_cred_permit`
- 修复 1a（补充）：`call_api_with_retry` / `call_mcp_with_retry` 两处，将 `wait_for_rpm_gate` 调用调整到 `_cred_permit` 获取之前，避免 RPM 等待期间占用并发槛位
- 修复 2：新增 `token::count_all_tokens_with_prefix`，复用已计算的前缀 token 数，避免与 `count_all_tokens` 重复编码；改造 `post_messages`、`post_messages_cc` 两处调用点

**不在范围内：**
- 修复 1b（全局 semaphore acquire 超时保护）——用户未选择，本次不实施
- 可选修复 3（针对高 rateMultiplier 模型单独调整 throttle_delay）——用户未选择，本次不实施
- `src/cache/fingerprint.rs` 中用于 cache 命中模拟的独立 BPE 编码路径——语义不同（规范化字符串 vs 原始内容），合并会改变缓存模拟数值，不安全，明确排除

## 技术方案

- **修复 1a**：在每个 sleep 调用前插入 `drop(_cred_permit);`。对于先判断是否满足重试条件、再决定是否 sleep 的分支，drop 放在判断之后、sleep 之前——即使最终不 sleep，提前释放也无副作用（本次循环迭代随后即 `continue`，不再使用该变量）。不改变全局 `_permit`（并发上限 50）的持有时长，仅缩短单账号并发槽位（上限 20）被占用的时间窗口。已确认全部 9 处调用点均满足「drop 后本次循环迭代不再使用该变量」的条件，编译期可验证安全（若有遗漏，use-after-move 会直接报错，无静默风险）。
- **修复 1a（补充）**：`call_api_with_retry`（728-732 行）与 `call_mcp_with_retry`（467-471 行）中，`_cred_permit` 获取与 `wait_for_rpm_gate` 调用之间没有任何相互依赖，直接调换两者顺序（先 RPM 门检查，检查通过/需切账号的分支照常 `continue`；仅当确认继续用当前账号发请求时才获取 `_cred_permit`），无需 drop/重新获取，是更简洁的结构性修复。
- **修复 2**：新增 `count_message_tokens(msg: &Message) -> u64` 辅助函数，提取 `count_all_tokens_local` 与 `count_prefix_tokens` 中共享的单条消息计数逻辑（两处原有内联逻辑也同步改为调用该辅助函数，消除内部重复）；新增 `count_all_tokens_with_prefix(model, system, messages, tools, prefix_tokens: u64) -> u64`，远程 API 路径保持不变，本地回退路径改为 `(prefix_tokens + messages.last().map(count_message_tokens).unwrap_or(0)).max(1)`，与原 `count_all_tokens_local` 在数学上等价（已逐行验证两函数除遍历范围外逻辑完全一致：均对 system 逐条计数、对 messages 逐条计数、对 tools 逐条计数，唯一差异是 `count_prefix_tokens` 只遍历 `messages[..n-1]`）。`messages` 为空时 `.last()` 返回 `None`，按 0 处理，此时 `prefix_tokens` 本身已等于 system+tools 全部 token（`prior_messages` 传入 `&[]`），与原函数在空消息场景下的结果一致。调用方复用已算好的 `prefix_estimated_tokens`，不再重复计算前缀部分。
- **不创建 design.md**：两处修复均只有单一确定方案，无需要记录的架构权衡或待决问题，方案已在本节完整说明。

## 预期影响

- 修复 1a：仅影响退避等待期间的并发槽位可用性，不改变重试次数、退避时长计算、错误处理分支或最终返回结果；对下游账号选择逻辑（`token_manager`）无影响。
- 修复 2：`input_tokens` 计算结果与改动前逐位一致（数学等价），仅减少一次 BPE 编码遍历；不改变任何 API 响应字段。

## 风险

- 修复 1a：若某分支的 drop 时机判断有误（例如提前 drop 导致后续代码路径仍需使用该变量），编译器会直接报错（moved value），风险在 `cargo check` 阶段即可发现，无运行时静默风险。「修复 1a（补充）」的顺序调换属于纯逻辑重排，无此类编译期兜底，需靠 code review + 单测覆盖 RPM-gate 分支来验证行为不变。
- 修复 2：若新函数与原函数行为不完全等价，会导致 `input_tokens` 计费/展示偏差；通过新增单元测试对比新旧函数在多组输入（含 tool_use/tool_result 混合内容）下的结果一致性来兜底。
