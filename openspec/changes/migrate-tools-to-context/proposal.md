# 变更提案：migrate-tools-to-context

## 背景

当前 `src/anthropic/converter.rs` 在 `convert_request()` 中以 hack 方式将工具完整定义注入 `history[2]` 作为 `<tools>{json}</tools>` 文本 user message + "OK" assistant，并在 `currentMessage.userInputMessageContext.tools` 仅放精简 name-only 骨架（converter.rs:618-672）。这一模式是早期猜测"Kiro 服务端只把 history 进入 prefix cache"的结果，没有实测依据。

最新抓包显示 Kiro 官方 CLI（Amazon Q for CLI）将完整 tools schema 直接放在 `currentMessage.userInputMessageContext.tools` 字段，history 中不出现任何 `<tools>` 文本块。这说明 Kiro 服务端对 `userInputMessageContext.tools` 字段是支持的、是首选的、且大概率参与 prefix cache 计算（否则 Kiro 自家产品就丧失缓存收益，与商业逻辑不符）。

继续保留 hack 的代价：
- 协议层污染：history 文本里出现非用户/非模型生成的伪 user message + 伪 "OK" assistant。
- 维护负担：~50 行 hack（618-638 + 658-672）以及围绕它的注释逻辑混淆。
- 阻碍后续优化：history[0..1] 才是真正的静态前缀缓存区，tools 占用 history[2..3] 增加 cache 失效面（不同 tools 集 → 不同哈希）。

## 目标范围

**在范围内：**
- 重写 `src/anthropic/converter.rs:618-672` 整段（不可拆分，删 `<tools>` 注入 + 删 slim tools 骨架 + 改 `[cache-check]` 日志为单一原子变更，否则中间状态不可编译）
- 把完整 tools 定义直接写入 `currentMessage.userInputMessageContext.tools`
- 保留 placeholder 工具补齐逻辑（converter.rs:604-616），其产物随完整 tools 列表一同进入 `userInputMessageContext.tools`
- 核对所有依赖 `history.len() >= 2` / `history[N]` 索引假设的代码点（含 PREV_H0 段 1376-1421）

**不在范围内：**
- 不改 placeholder 工具补齐的判定逻辑本身（lowercase 比对、name 收集）
- 不改 conversation_id / agent_continuation_id / PREV_H0 冻结逻辑（其只操作 history[0]，与 tools 位置无关）
- 不改 origin / agentTaskType / chat_trigger_type
- 不改 Anthropic→Kiro JSON Schema 规范化逻辑（`convert_tools` 内部）
- 不动 `--- USER MESSAGE BEGIN/END ---` 文本包装（属未来独立改动）
- 不改 Kiro response 侧任何代码（`src/kiro/parser/`、`src/anthropic/stream.rs`）
- 不加 feature flag（用户决策：直接替换）

## 技术方案

```
现状（converter.rs 关键段简化）:
  let mut tools = convert_tools(...)
  let mut history = build_history(...)
  // placeholder 补齐 → 仍写入 tools
  // 注入 history[2..3]: <tools>JSON</tools> + "OK"
  context.tools = slim_tools  // 仅 name + description 第一字符

目标:
  let mut tools = convert_tools(...)
  let mut history = build_history(...)
  // placeholder 补齐 → 仍写入 tools (不变)
  // 删除 history 注入段 (618-638)
  // 删除 slim_tools 构造 (661-671)
  context.tools = tools  // 完整 schema（move，不 clone）
```

关键细节：
1. `Tool` / `ToolSpecification` / `InputSchema` 类型已与 Kiro CLI 线型 JSON 完全一致（confirmed via `src/kiro/model/requests/tool.rs`），无需新增类型。
2. `UserInputMessageContext.tools: Vec<Tool>` 字段已存在（conversation.rs:153），无需新增字段。
3. `ToolSpecification::input_schema.json` 已是 `serde_json::Value`，可承载规范化后的完整 JSON Schema。
4. 经全仓搜索确认 `src/test.rs` 与其他模块**无**依赖 `<tools>` 文本注入或 `history[2..3]` 位置假设的代码（grep `<tools>` / `tools_history_idx` / `slim_tools` 仅命中 converter.rs:619-671 即将删除段）。

## 预期影响

**正向：**
- 协议向 Kiro 官方推荐形态对齐
- history hash 稳定性提升：history[0..1] 成为唯一静态前缀
- 减少 ~50 行 hack 代码

**风险：**
- 若 Kiro 服务端 prefix cache 不覆盖 `userInputMessageContext.tools`，可能导致 cache_read_input_tokens 显著下降（详见验收标准）
- metering_usage 上升超过 ±5% → 不可接受 → 回滚

**性能：**
- 单次请求 payload 大小变化：原 history `<tools>JSON</tools>` block ≈ context.tools 完整 schema，差异 < 1%（仅多出 `<tools></tools>` 12 字节包装与 history 双向消息封装开销）
- 序列化耗时：无显著变化

**兼容性：**
- 客户端无感知（请求/响应仍走 Anthropic 协议）
- 上游 Kiro API 已支持目标字段（Kiro CLI 抓包验证）

## 风险

| 风险 | 等级 | 应对 |
|---|---|---|
| Kiro 服务端不缓存 userInputMessageContext.tools，cache 命中率塌方 | 高 | 验收硬指标：`cache_read_input_tokens` 均值下降幅度 > 10% 即回滚 |
| history.len()/history[N] 索引在他处仍有依赖（如 1376-1421 PREV_H0 段） | 中 | 任务 T4 显式核对所有索引点；PREV_H0 实际只读 history[0]，但需核实无遗漏 |
| 完整 tools schema 体积大放 currentMessage 触发请求大小限制 | 低 | 实测：当前 history `<tools>` 块即等量 → 风险已被现状证伪 |
| placeholder 工具被 Kiro 误判为真实工具 | 低 | 保留现有 placeholder 逻辑（统一 default schema）；本变更不引入新混淆来源 |
| metering 抖动导致 ±5% 难判定 | 中 | 测量方法见验收标准 |

## 验收标准

### 功能正确性

1. `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test` 全部通过
2. 本地端到端测试：启动 `./run-local-service-mac.sh`，发起含 ≥3 个工具（read / grep / shell）的请求并完成至少一轮工具调用 + 工具结果回传，请求不返回 4xx/5xx，无解析错误日志

### Cache / 计费回归（A/B 验收）

测量字段（**字段名以代码为准，文档不另起别名**）：
- `cache_read_input_tokens`：直接读 `MeteringEvent.cache_read_input_tokens`，若为 null 则取 `src/anthropic/stream.rs::infer_cache_read_tokens` 反推值
- `metering_usage`（即 `MeteringEvent.usage: f64`）：在 stream.rs:1410 日志中以 `metering_credits=...` 为 key 输出（仅日志标签命名，源字段是 `usage`）。数据采集脚本可 grep `metering_credits=` 提取该 f64 值

测量方法：
- 测试 prompt：固定一个含 ≥3 工具定义、≥2 轮历史、输入 token ≥ 10k（避免小 prompt 噪声放大）的请求
- 数据采集：实验组 5 次 + 对照组 5 次，**取去极值后均值**（剔除最大与最小值，剩 3 次取算术平均）
- 部署方式：预编译两个独立 binary（`kiro2cc-proxy.old` 与 `kiro2cc-proxy.new`），交替启动跑测试，避免 stash 切换 + 重新构建带来的窗口拉长
- 同一账号 + 同一 conversation_id（手动指定）确保 sticky 路由与 prefix cache 条件一致

PASS 阈值：
- `metering_usage` 均值差 ≤ ±5%
- `cache_read_input_tokens` 均值下降幅度 ≤ 10%

### 不变性

3. `[cache-check]` 日志中 history[0] 与 history[1] 哈希在多次请求间保持一致（PREV_H0 冻结仍生效）
4. git diff 仅触及 `src/anthropic/converter.rs`，无意外的相邻代码格式化

## 回滚条件与决策树

| 实测情况 | 决策 |
|---|---|
| `cache_read_input_tokens` 下降幅度 > 10% | 回滚 |
| `metering_usage` 上升 > 5% | 回滚 |
| 端到端工具调用返回 4xx/5xx | 立即回滚（止血优先于数据采集） |
| `cache_read_input_tokens` 持平或上升、`metering_usage` 持平或下降 | 通过 |
| `cache_read_input_tokens` 下降幅度 ≤ 10% 但 `metering_usage` 下降 ≥ 5% | 通过（净收益为正） |
| `cache_read_input_tokens` 下降幅度 ≤ 10% 但 `metering_usage` 上升 ≤ 5% | 通过（变更在阈值内，留作下版本继续观察） |
| 其他混合情况 | 阶段 4 用户验收时由用户拍板 |

回滚操作：`git revert <commit-hash>`（单 commit 完成本变更，便于一键回滚）；不要恢复成 hack 状态——若回滚则维持 master 分支主线无 hack 形态，问题在新分支调查。

## OPEN QUESTION（不阻塞阶段 1 通过）

无。所有关键决策已闭环。
