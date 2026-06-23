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

请求从 Anthropic API 格式入，经转换后发往 Kiro API，响应再转回 Anthropic SSE 格式输出。

```
Client (Anthropic format)
  │
  ▼
src/anthropic/middleware.rs   ← 认证（API Key / Bearer）、RPM 计数、用量追踪
  │
src/anthropic/handlers.rs     ← /v1/messages 路由入口；/cc/v1/messages 为缓冲模式
  │
src/anthropic/converter.rs    ← Anthropic → Kiro 协议转换（工具 schema 规范化、消息结构重组）
  │
src/kiro/provider.rs          ← 多账号故障转移；MAX 3 retries/account，MAX 9 total
  │
src/kiro/token_manager.rs     ← MultiTokenManager：OAuth token 刷新、账号优先级/负载均衡
  │
  ▼  (Kiro binary frame protocol)
src/kiro/parser/              ← 二进制帧解码（frame.rs + decoder.rs + crc.rs）
  │
src/anthropic/stream.rs       ← Kiro 事件 → Anthropic SSE 事件转换
  ▼
Client (Anthropic SSE format)
```

### 关键模块

| 路径 | 职责 |
|------|------|
| `src/kiro/parser/` | Kiro 私有二进制帧协议解码（带 CRC32 校验） |
| `src/anthropic/converter.rs` | Anthropic→Kiro 格式转换，含 JSON Schema 规范化（Kiro 对 `null` 字段/复杂 schema 拒绝） |
| `src/anthropic/stream.rs` | 流式状态机：Kiro events → Anthropic SSE，处理 thinking 标签、tool_use 块拼装 |
| `src/kiro/token_manager.rs` | 多账号 token 池，支持 social/IDC 双认证方式，刷新后回写 credentials.json |
| `src/model/config.rs` | 全局配置结构，`apply_env_overrides()` 支持容器环境变量覆盖 |
| `src/cache/` | Prompt cache 模块：`simulation.rs` 比例模拟、`fingerprint.rs` 账号级指纹追踪、`mod.rs` 统一导出 |
| `src/http_client.rs` | reqwest Client 构建器，支持账号级独立代理配置 |
| `src/admin/` + `src/admin_ui/` | Admin REST API + rust-embed 嵌入前端静态资源 |

### 运行时配置

- `app/config/config.json` — 主配置（host/port/apiKey/proxyUrl 等），已在 `.gitignore` 中
- `app/config/credentials.json` — Kiro 账号 token，支持单对象或数组格式
- Docker 部署时以上两文件在 `data/` 目录下

### /cc/v1 vs /v1

`/cc/v1/messages` 是 Claude Code 专用端点：等待上游流完成后再返回（缓冲模式），`input_tokens` 使用实际值而非估算；期间每 25s 发送 SSE ping 保活。`/v1/messages` 为直通流式转发。

## 代码索引

详细的功能 → 代码位置速查表：`docs/代码速查表.md`

在需要定位特定功能（如账号选择、格式转换、认证、限流等）时，**优先读取此文件**再作答。
