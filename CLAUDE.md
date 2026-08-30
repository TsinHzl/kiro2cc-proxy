# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# 构建（admin-ui + user-ui 前端 + Rust 二进制）
./build-mac.sh            # macOS
.\build-windows.ps1       # Windows

# 仅编译 Rust
cargo build --release

# 本地运行（读取 app/config/config.json）
./run-local-service-mac.sh

# 直接运行（指定配置）
cargo run -- --config app/config/config.json

# 检查 / 测试
cargo check
cargo test
cargo test <test_name>       # 运行单个测试
RUST_LOG=debug cargo run     # 调试日志

# 格式化 + Lint
cargo fmt
cargo clippy
```

## Architecture

请求从 Anthropic API 格式入，经转换后发往 Kiro API，响应再转回 Anthropic SSE 格式输出。OpenAI 协议（`/v1/chat/completions`、`/v1/responses`）作为外挂适配器，复用同一下游链路。

```
Client (Anthropic / OpenAI format)
  │
  ▼
src/anthropic/middleware.rs   ← 认证（API Key / Bearer）、RPM 计数、用量追踪
  │
src/anthropic/handlers.rs     ← /v1/messages、/cc/v1/messages（带 300s deadline）路由入口
  │                            ← /v1/chat/completions、/v1/responses 也复用 post_messages 下游
  │
src/openai/                   ← OpenAI 协议外挂适配层（不改动 anthropic 核心）
  │   chat_request/response、responses_request/response、sse、model_map、handlers
  │
src/anthropic/converter.rs    ← Anthropic → Kiro 协议转换（工具 schema 规范化、消息结构重组）
  │
src/kiro/provider.rs          ← 多账号故障转移；MAX 3 retries/account，MAX 9 total
  │                            ← 全局并发上限 50，单账号并发上限 20（Semaphore）
  │
src/kiro/token_manager.rs     ← MultiTokenManager：OAuth token 刷新、账号优先级/负载均衡
  │                            ← load_balancing_mode: priority（默认）/ balanced（轮询）
  │
  ▼  (Kiro binary frame protocol)
src/kiro/parser/              ← 二进制帧解码（frame.rs + decoder.rs + header.rs + crc.rs CRC32C）
  │
src/anthropic/stream.rs       ← Kiro 事件 → Anthropic SSE 事件转换
  ▼
Client (Anthropic SSE / OpenAI SSE format)
```

### 关键模块

| 路径 | 职责 |
|------|------|
| `src/anthropic/converter.rs` | Anthropic→Kiro 格式转换，含 JSON Schema 规范化（Kiro 对 `null` 字段/复杂 schema 拒绝） |
| `src/anthropic/stream.rs` | 流式状态机：Kiro events → Anthropic SSE，处理 thinking 标签、tool_use 块拼装 |
| `src/anthropic/websearch.rs` | web_search 工具的 MCP 协议封装（条件：tools 仅含单个 web_search） |
| `src/openai/` | OpenAI 协议外挂适配层：chat/completions + responses 双协议，复用 anthropic 下游链路 |
| `src/kiro/parser/` | Kiro 私有二进制帧协议解码（frame + decoder + header + crc CRC32C） |
| `src/kiro/provider.rs` | 多账号故障转移；全局并发 50 / 单账号并发 20 Semaphore 限流 |
| `src/kiro/token_manager.rs` | 多账号 token 池，social/IDC 双认证，priority/balanced 负载均衡，刷新回写 credentials.json |
| `src/kiro/model/` | Kiro 数据模型：requests/（conversation、kiro、tool）、events/（assistant、tool_use、metering…）、credentials、usage_limits、token_refresh |
| `src/model/config.rs` | 全局配置结构，`apply_env_overrides()` 支持容器环境变量覆盖 |
| `src/model/api_key.rs` | API Key 管理（含无限额度选项） |
| `src/cache/` | Prompt cache 模块：`simulation.rs` 比例模拟、`fingerprint.rs` 账号级指纹追踪、`mod.rs` 统一导出 |
| `src/http_client.rs` | reqwest Client 构建器，支持账号级独立代理配置（ProxyConfig + 鉴权） |
| `src/token.rs` | tiktoken-rs BPE token 计数（cl100k_base），用于 count_tokens 估算 |
| `src/admin/` | Admin REST API（凭据/API Key/用量/限流日志/配置/Geo/日志流），挂载于 `/api/admin` |
| `src/admin_ui/` + `src/user_ui/` | rust-embed 嵌入前端静态资源路由（`/admin`、`/user`） |
| `src/user/` | User REST API（用户登录/用量查询），挂载于 `/api/user` |
| `src/common/auth.rs` | 公共认证工具：extract_api_key（x-api-key / Bearer）、constant_time_eq 常量时间比较 |

### 运行时配置

- `app/config/config.json` — 主配置（host/port/adminPsw/proxyUrl/loadBalancingMode/cacheSimulation 等），已在 `.gitignore` 中
- `app/config/credentials.json` — Kiro 账号 token，支持单对象或数组格式
- Docker 部署时以上两文件在 `data/` 目录下

### 环境变量覆盖（`apply_env_overrides()`）

容器/Docker 部署时，以下环境变量覆盖 `config.json` 同名字段（优先级最高）：

| 环境变量 | 覆盖字段 | 说明 |
|---|---|---|
| `HOST` / `PORT` | host / port | 监听地址 |
| `REGION` / `AUTH_REGION` / `API_REGION` | region / auth_region / api_region | Kiro 区域 |
| `ADMIN_PSW` 或 `ADMIN_API_KEY` | admin_psw | Admin 鉴权密钥（二者等价） |
| `PROXY_URL` / `PROXY_USERNAME` / `PROXY_PASSWORD` | proxy_url 等 | 出站代理 |
| `LOAD_BALANCING_MODE` | load_balancing_mode | `priority`（默认）/ `balanced` |
| `MODEL_CACHE_TTL_SECS` | model_cache_ttl_secs | 模型列表缓存 TTL |
| `CACHE_SIMULATION_*` | cache_simulation 嵌套字段 | 指纹/比例模拟开关与 TTL |

### 端点路由总览

| 路径 | 方法 | 说明 |
|---|---|---|
| `/v1/models`、`/v1/models/{id}` | GET | 模型列表/详情 |
| `/v1/messages` | POST | Anthropic Messages（实时流式） |
| `/v1/messages/count_tokens` | POST | token 计数估算 |
| `/v1/chat/completions` | POST | OpenAI Chat Completions 兼容 |
| `/v1/responses` | POST | OpenAI Responses 兼容（Codex CLI） |
| `/cc/v1/messages` | POST | Claude Code 专用，带 300s 全局 deadline |
| `/api/admin/*` | 多 | Admin REST API（需 admin_psw） |
| `/admin` | GET | Admin UI 静态资源 |
| `/api/user/*` | 多 | User REST API（API Key 登录） |
| `/user` | GET | User UI 静态资源 |

Admin/User API 仅在 `admin_psw` 配置非空时挂载。

### 负载均衡与故障转移

- `priority` 模式（默认）：选优先级最高（`priority` 值最小）的可用账号，同优先级内 round-robin
- `balanced` 模式：在所有可用账号间轮询
- 故障转移：单账号最多重试 3 次（`MAX_RETRIES_PER_CREDENTIAL`），全局最多 9 次（`MAX_TOTAL_RETRIES`，受可用账号数约束）
- 并发限流：全局 50（`MAX_CONCURRENT_REQUESTS`）、单账号 20（`MAX_CONCURRENT_PER_CREDENTIAL`）

### /cc/v1 vs /v1

`/cc/v1/messages` 是 Claude Code 专用端点，与 `/v1/messages` 同为实时流式转发，唯一差异是带 300s 全局 deadline（上游挂起保护），超时发 `overloaded_error` 后终止。两者均每 25s 发送 SSE ping 保活，`input_tokens` 均为 `message_start` 给估算值、末尾 `message_delta` 校准为终值。

### OpenAI 兼容层设计取舍

`src/openai/` 是**外挂适配器**，不改动 `anthropic` 核心模块：把 OpenAI 请求转成 Anthropic Messages，复用 `post_messages` 下游链路（多账号故障转移、RPM、用量、prompt cache），再把 Anthropic SSE 重新编码为 OpenAI SSE。代价是多一次序列化往返，换得对 900 行核心逻辑的零侵入——这是有意取舍，详见 `openspec/changes/add-openai-compatible-endpoints/design.md`。

## 代码索引

详细的功能 → 代码位置速查表：`docs/代码速查表.md`

在需要定位特定功能（如账号选择、格式转换、认证、限流等）时，**优先读取此文件**再作答。
