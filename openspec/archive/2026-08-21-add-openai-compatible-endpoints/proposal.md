# 变更提案：add-openai-compatible-endpoints

## 背景

当前代理仅暴露 Anthropic 协议端点（`/v1/messages`、`/cc/v1/messages`）。两类客户端因此无法接入：

- **Codex CLI**（issue #24）：只会说 OpenAI 协议，`~/.codex/config.toml` 的 `wire_api` 取值为 `"responses"`（默认）或 `"chat"`
- **OpenAI SDK / newapi 等中转渠道**（issue #26）：只会说 `POST /v1/chat/completions`

而 Kiro 上游本身已提供原生 GPT 模型（`src/anthropic/handlers.rs:632-649`：`gpt-5.6-terra`、`gpt-5.6-luna`，`owned_by: "openai"`）。这意味着补上 OpenAI 协议入口后，Codex 能用 Kiro 额度真跑 GPT-5.6，而不是被迫跑 Claude 系模型。

目标：让 Codex CLI 只改一份 `~/.codex/config.toml` 就能把本代理当作 OpenAI 后端使用：

```toml
[model_providers.kiro]
name = "kiro2cc-proxy"
base_url = "http://127.0.0.1:5678/v1"
wire_api = "responses"        # 或 "chat"
env_key = "KIRO_API_KEY"
```

## 目标范围

**在范围内：**

- 新增模块 `src/openai/`，承载 OpenAI ↔ Anthropic 双向协议转换
- 新增端点 `POST /v1/chat/completions`（阶段一，Chat Completions 协议，流式 + 非流式）
- 新增端点 `POST /v1/responses`（阶段二，Responses 协议，流式 + 非流式）
- 模型名映射（见 spec，按优先级）：`gpt-5-codex` / `gpt-5.1-codex` → `gpt-5.6-terra`；`gpt-5.1-codex-max` → `gpt-5.6-luna`；`gpt-5.6-*` 前缀原样透传（覆盖上游现有的 `sol` / `terra` / `luna` 三款，`handlers.rs:623-649`）；`claude-*` 原样透传；其余 `gpt-*` 兜底 `gpt-5.6-terra` 并 `WARN`
- 工具调用双向闭环：`tools[].type=="function"` 与 Codex 的 `type=="custom"`（`apply_patch` / `shell`）；`function_call` / `custom_tool_call` ↔ Anthropic `tool_use`；`function_call_output` / `custom_tool_call_output` ↔ `tool_result`
- `reasoning.effort` → Anthropic `thinking` 映射；thinking 增量回写为 OpenAI 侧对应字段
- usage 上报（`prompt_tokens` / `completion_tokens` / `total_tokens`，含 `cached_tokens`）
- 单元测试覆盖：请求转换、响应/SSE 转换、模型映射、工具往返

**不在范围内：**

- **不修改任何现有端点的行为**。`/v1/messages`、`/cc/v1/messages`、`/v1/models`、`/v1/messages/count_tokens`、Admin / User API 的请求处理逻辑一行不改（唯一例外是在 `src/anthropic/router.rs` 的 `v1_routes` 上追加两条 `.route()`，属纯新增行）
- **不重构 `src/anthropic/handlers.rs` 的 `post_messages`**（不抽取内核、不改签名），故账号故障转移、RPM 计数、用量追踪、prompt cache 指纹、300s deadline 全部原状复用
- 不新增任何外部 crate 依赖
- 不实现 Responses API 的有状态语义（`previous_response_id` / `store=true`）
- 不实现 `reasoning.encrypted_content` 的加密回传
- 不实现 Embeddings / Images / Audio 等其他 OpenAI 端点
- 不接入 Codex 官方账号（本变更是「用 Kiro 额度跑 Codex 客户端」，不是「代理 OpenAI 账号」）
- 不修改前端（`admin-ui` / `user-ui`）
- 不修正已存在的遗留问题（客户端 token 缩放系数、`context_window_for_model` 硬返回 1M、`gpt-5.6-luna` 的 `k_ref` 未标定等），仅在受影响处以注释标注

## 技术方案

采用**外挂适配器**架构，新端点与现有端点完全并行，共享下游而不改下游：

```
Codex / OpenAI SDK
  │
  ├─ POST /v1/chat/completions ─┐
  └─ POST /v1/responses ────────┤  src/openai/handlers.rs
                                │  ① 请求侧：OpenAI JSON → Anthropic MessagesRequest JSON
                                ▼
              [直接调用 anthropic::handlers::post_messages(...)]   ← 现有代码，零改动
                                │
                                ▼  converter.rs → provider.rs（多账号故障转移）→ Kiro
                                │  ← parser/ → stream.rs（Anthropic SSE）
                                ▼
                                │  ② 响应侧：包裹返回的 Response body 流，
                                │     Anthropic SSE / JSON → OpenAI SSE / JSON
                                ▼
                        Codex / OpenAI SDK
```

关键点：

- **认证、限流、用量统计零成本继承** —— 两条新 route 挂进现有 `v1_routes`（`src/anthropic/router.rs:58-66`），自动套用 `auth_middleware`（已同时支持 `x-api-key` 与 `Authorization: Bearer`，即 Codex 的 `env_key` 形态）
- **`GET /v1/models` 无需改动** —— 现有响应已是 `{object:"list", data:[{id, object, created, owned_by, ...}]}`（`handlers.rs:233`），OpenAI 客户端会忽略多出的 `display_name` / `model_type` / `max_tokens` 字段
- **响应侧走二次转换而非内核抽取** —— 代价是多一层 SSE 文本解析（每 chunk 微秒级，相对上游网络 IO 可忽略），换来对 900 行核心逻辑的零侵入
- **模型名双向不对称** —— 请求侧映射为 Kiro 模型名，响应侧回写客户端原始请求的 model 名（Codex 会校验一致性）
- 事件序列参考 CLIProxyAPI 的 `internal/translator/claude/openai/responses/`（MIT 许可）作为**行为规范**，用 Rust 重写，不复制代码；在源文件头部标注参考来源

## 预期影响

- **对现有功能**：无行为变化。回归验收以「现有 420 个测试全绿」+「`/v1/messages` 与 `/cc/v1/messages` 手工冒烟」为门槛
- **性能**：新端点比原生 Anthropic 端点多一次 JSON/SSE 解析与重编码；现有端点无额外开销（新增 route 只影响路由匹配表）
- **二进制体积**：新增约 6 个源文件，预计 +2000 行，release 体积增幅可忽略
- **兼容性**：纯新增端点，无破坏性变更；旧客户端行为不变
- 解决 issue #24（Codex CLI）与 #26（OpenAI SDK 渠道）

## 风险

| 风险 | 应对 |
|---|---|
| Codex 的 `custom` 工具（`apply_patch` / `shell` 为 freeform 文本入参）与 Kiro 严格的 JSON Schema 规范化冲突（Kiro 拒绝 `null` 字段与复杂 schema） | 将 `custom` 工具降级为固定 schema 的 function tool（`{type:"object",properties:{input:{type:"string"}},required:["input"]}`），确保通过现有 `converter.rs` 的规范化 |
| Responses API 的有状态语义（`previous_response_id`）未实现，Codex 某些配置下会启用 | 显式返回 400 + 明确错误文案，不静默降级；文档注明须 `store=false`（Codex CLI 默认） |
| Responses SSE 事件序列繁多（40+ 种），漏发某个事件会让 Codex 挂起 | 以 CLIProxyAPI 的事件序列为规范逐一对齐；阶段二完成后用真实 Codex CLI 端到端验证 |
| 响应侧包裹 body 流若吞掉上游错误，会退化为静默失败 | 转换层遇到无法识别的上游事件时透传为 OpenAI `error` 事件并终止流，禁止静默丢弃 |
| 新增 route 若误挂在 `v1_routes` 之外，会绕过认证形成未授权入口 | 任务级验收：新端点在无 API Key 时必须返回 401 |
| `gpt-5.6-luna` 的真实上下文窗口与 `k_ref` 未标定，且上游恒 `thinking=0` | 本次不修正，在模型映射处以注释标注为已知限制 |
| 二次转换可能改变 usage 数值语义 | 单元测试断言 `prompt_tokens` / `completion_tokens` 与上游 Anthropic `usage` 逐字段一致 |
| 现有 `stream.rs` 会在 thinking block 上注入伪造的 `signature_delta`（≥100 字符无语义串），朴素实现会把它当推理文本下发给客户端 | 增量转换按 `delta.type` 白名单过滤，只认 `text_delta` / `thinking_delta` / `input_json_delta`；单元测试断言签名串不出现在输出中 |
| 现有实现每 25s 注入 `event: ping` 保活帧，若归为"未识别事件"会产生周期性 WARN 噪声 | `ping` 列入静默白名单，丢弃且不记录日志 |
| 上游 `stop_reason` 除 `end_turn` / `tool_use` 外还有 `max_tokens` 与 `model_context_window_exceeded`，漏映射会让 Codex 无法判断截断 | spec 明确 4 种取值到 `finish_reason` / Responses `status` 的映射，未枚举值回退并 WARN |
