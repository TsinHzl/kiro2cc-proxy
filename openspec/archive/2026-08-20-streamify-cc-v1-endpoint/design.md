# 设计：streamify-cc-v1-endpoint

## 上下文

`/cc/v1/messages` 的缓冲模式是为「等 `contextUsageEvent` 反算精确 `input_tokens`」
而生。该前提在「contextUsage 本地化」改动后消失：

- `StreamContext::context_input_tokens` 已弃用、恒为 `None`（stream.rs:1428）
- `BufferedStreamContext::finish_and_get_all_events` 中
  `context_input_tokens.unwrap_or(estimated_input_tokens)` 恒取后者（stream.rs:1645-1648）
- `metering_cache_read_tokens` / `metering_cache_creation_tokens` 挂在
  `StreamContext` 上，流式路径的 `generate_final_events`（stream.rs:1454-1458）
  同样读取；且上游实测不透传（stream.rs:1450 注释）
- 文档所称的 `infer_cache_read_tokens()`（docs/源码全景解析.md:1085）在当前代码
  已不存在（grep 零命中）

因此缓冲的唯一残余作用是「让 `message_start` 里的 usage 直接等于终值」，
代价是整个生成期间零进展、turn 内工具无法提前执行、中断即全废。

两条流式函数的调用签名已完全一致（`handle_stream_request` @handlers.rs:936 与
`handle_stream_request_buffered` @handlers.rs:1758，均为 11 个参数、顺序与类型相同，
仅第 4 参数命名不同：`input_tokens` vs `estimated_input_tokens`，同为 `i32`），
说明这两条路径此前已被有意保持可互换。

## 目标 / 非目标

**目标**

- `/cc/v1` 流式请求实时转发，保留原有的 300s 上游挂起保护
- `/v1` 的可观测行为零变化
- 删除失效的缓冲实现，不留双路径

**非目标**

- 不引入 config 开关或运行时切换
- 不改 `message_start` 的 usage 口径
- 不改非流式路径、不改 usage 入库、不改账号故障转移
- 不新增外部 crate

## 决策

### D1：直接替换分派，不加 config 开关

`handlers.rs:1723` 的 `handle_stream_request_buffered(...)` 改为
`handle_stream_request(...)`，参数原样传递。

**理由**：用户明确选择不加开关。双路径会让缓冲代码继续存活，也让「哪条路径生效」
成为线上排障时的额外变量。

**保留意见（记录在案）**：`/cc/v1` 是 README 引导所有 Claude Code 用户配置的默认
端点。不加开关意味着上线后若该端点出问题，只能整体回退版本，无法运行时降级。
此项已在 proposal 风险表中列为高等级风险。

### D2：deadline 以 `Option<Instant>` 参数注入，用 `std::future::pending()` 占位

`tokio::select!` **支持** `if <precondition>` 形式的条件分支（见 tokio 宏文档），
但那条路子在本场景不划算：分支的 future 表达式必须无论 precondition 真假都能构造，
而 `sleep_until` 需要一个已解包的 `Instant`，因此得写成
`_ = sleep_until(deadline.unwrap_or(<哨兵值>)), if deadline.is_some()` —— 需要凭空
造一个「很远的未来」哨兵值，且该值参与 `Instant` 加法有溢出顾虑。

改为把等待动作抽成一个辅助 async 函数，语义直接、无哨兵值：

```rust
/// deadline 为 None 时永不就绪，使该 select! 分支等价于不存在
async fn wait_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(d) => tokio::time::sleep_until(d).await,
        None => std::future::pending::<()>().await,
    }
}
```

`std::future::pending` 属标准库，不新增依赖。`deadline` 随 `unfold` state 传递
（state 从 5 元组扩为 6 元组，与 `create_buffered_sse_stream` 现有 6 元组同构）。

**被否方案**：`select!` 的 `if` precondition —— 见上，需要哨兵值。

**被否方案**：写两份 `create_sse_stream`（有 / 无 deadline）—— 重复 120 行核心
转发逻辑，正是本次要消除的问题。

**被否方案**：把 deadline 做成必填、`/v1` 传一个极大值 —— 语义上是「有超时但很久」，
与「无超时」不同；且极大值会让 `Instant` 加法有溢出顾虑。

### D3：`/v1` 传 `None`，不统一启用 deadline

**理由**：`/v1` 当前无 deadline，给它加是独立的行为变更，应单独评估（例如
长思考任务是否会被 300s 误杀）。本次变更的目标是「`/cc/v1` 不退化」，不是
「给 `/v1` 加保护」。

**代价**：两个端点在超时语义上不一致。可接受 —— 它们在文档中本就是两个用途不同
的端点，且差异从 `create_sse_stream` 的调用点一眼可见。

### D4：不给 `create_sse_stream` 的 `select!` 加 `biased`

关键在于 **`select!` 的分支选择是否具有确定性优先级**，而非「外层有没有 `loop`」：

- `tokio::select!` 不加 `biased` 时对同时就绪的分支做**随机**选择，任一分支被连续
  跳过的概率随迭代次数指数衰减 —— 不存在确定性饿死。
- `create_buffered_sse_stream` 需要 `biased` 的真正原因是：它在**单次 `Future::poll`
  内部**用显式 `loop` 反复 select，chunk 分支处理完不返回而是继续下一轮内部循环。
  这是与执行器调度无关的、确定性的「总是优先处理 chunk 从而饿死 ping」风险源，
  必须靠 `biased` 显式排序。
- `create_sse_stream` 的 `select!` 是 `unfold` 闭包的尾表达式，无这种内部循环；
  默认的随机分支选择已足够保证 ping 与 deadline 分支不被饿死。

且它当前就没有 `biased`。加上会改变 `/v1` 现有的分支优先级 —— 违反
「`/v1` 行为零变化」目标。

> 注意：不要把结论简化为「只要没有显式外层 `loop` 就天然无饿死风险」。`unfold`
> 生成的 `Stream` 被 hyper 的响应体写出循环反复 `poll_next`，若消费方在同一 poll
> 周期内连续拉取，效果上仍等价于一个隐式驱动循环 —— 真正的保障来自随机分支选择。

deadline 分支不加 `biased` 也是正确的：撞 deadline 的场景本就是「上游长时间无数据」，
此时数据分支 pending，deadline 分支是唯一就绪者，无需优先级保证。最坏情况是
在 chunk 与 deadline 同时就绪时多转发一个 chunk 再终止 —— 无害。

### D5：删除缓冲实现而非标注 `#[allow(dead_code)]`

`BufferedStreamContext`（stream.rs:1533-1695）是对 `StreamContext` 的独立包装
（`inner: StreamContext`），全部方法为委托或缓冲专用；grep 确认无其他模块引用、
无测试引用。连同 `handle_stream_request_buffered`、`create_buffered_sse_stream`
共约 340 行整体删除。

保留死代码会让后续读者误以为存在两种可选模式，也会让 `StreamContext` 的
`with_*` 构造链背负仅缓冲路径使用的方法。

### D6：不对齐 `message_start` 与 `message_delta` 的 usage 口径

`create_message_start_event`（stream.rs:747-767）用 `self.prompt_cache_usage` 直接
取值；`generate_final_events`（stream.rs:1449-1473）走「metering → 前缀估算 →
`scale_to` 模拟」三级优先链的拆分口径。两者口径不同。

**决定不动**，理由三条：

1. 该差异在 `/v1` 上长期存在，无相关问题报告
2. 末尾 `message_delta` 会校准终值，已确证客户端两条 usage 合并路径都吃它
3. 改它会连带变更 `/v1` 的 `message_start` 输出 —— 与「`/v1` 零变化」目标冲突

**遗留后果**：`/cc/v1` 客户端在 turn 进行中看到的 `input_tokens` 是估算口径，
turn 结束时跳变为终值。原缓冲模式因为「算完才发」不存在这个跳变。
列为观察项，若真机验证发现 Claude Code 的 `/context` 显示异常，单独立项处理。

## 风险 / 权衡

| 项 | 权衡 |
|---|---|
| all-or-nothing 契约丢失 | 换取中断时已生成内容不作废。`/v1` 一直如此，且 `stream_interrupted_error_event()` 保证不把中断伪装成正常完成 |
| turn 中 usage 跳变（D6） | 换取 `/v1` 零行为变化与最小改动面。turn 结束即校准 |
| 两端点超时语义不一致（D3） | 换取不给 `/v1` 引入未评估的 300s 上限 |
| 无开关可降级（D1） | 用户明确决定；换取无双路径维护成本 |
| auto-compact 触发时机 | 已确证客户端合并末尾 `message_delta.usage.input_tokens`，但未静态追到 auto-compact 判定点引用累积结果的那行 → 列为真机验证项，非代码层可消除的风险 |

## 验证策略

- `cargo fmt` + `cargo clippy` clean；`cargo test` 全绿（当前 412 passed）
- 新增单元测试覆盖 `wait_deadline` 的两个分支语义（`None` 永不就绪、
  `Some(past)` 立即就绪）
- 真机：Claude Code 指向 `/cc/v1`，观察流式增量输出与 turn 内工具提前执行
- 真机：长上下文会话比对 `/context` 显示百分比与后端
  `[input_tokens] 本地化: estimated=N final=M` 日志
