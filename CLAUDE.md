# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

kiro2cc-proxy 是一个 Rust 代理服务，将 Anthropic Claude API 请求转换为 Kiro（AWS Q Developer）API 请求，使 Claude Code / Codex CLI / 任意 OpenAI SDK 客户端能够使用 Kiro 账号上的模型。技术栈：Rust 2024 edition + axum 0.8 + tokio + reqwest，前端（web-ui/admin-ui、web-ui/user-ui，React + Vite）构建产物经 rust-embed 嵌入二进制。

## 常用命令

```bash
# 完整构建（web-ui/admin-ui + web-ui/user-ui 前端 + cargo release），首次约 5~15 分钟
./build-mac.sh           # macOS
.\build-windows.ps1      # Windows

# 仅 Rust
cargo check            # 快速类型检查
cargo build            # dev；debug 模式 rust-embed 从磁盘读 dist
cargo build --release  # release 模式将 web-ui/admin-ui/dist、web-ui/user-ui/dist 编译期内嵌

# 测试（约 1170 个 #[test] / #[tokio::test]；模块内 #[cfg(test)] mod tests，大型模块用例放 tests/ 子目录）
cargo test                        # 全部
cargo test <name-substring>       # 单个/一组，如 cargo test additional_model_request_fields
cargo llvm-cov --lcov --output-path lcov.info   # CI 用的覆盖率跑法

# 代码检查（提交前必须 clean）
cargo fmt
cargo clippy

# 运行（本地 macOS）
./run-local-service-mac.sh   # 首次运行有配置向导，配置落在 app/config/
RUST_LOG=debug cargo run     # 调试日志直跑（默认读工作目录 config.json）
```

前端改动需先 `cd web-ui/admin-ui && pnpm install && pnpm build`（admin-ui 用 pnpm）或 `cd web-ui/user-ui && npm install && npm run build`，dist 产物不存在时 release 构建会失败。
CI（`.github/workflows/test.yaml`）在 `cargo llvm-cov` 前先构建两套前端。

## 架构

### 请求链路（Anthropic 协议层）

```
Client (Anthropic format)
  │
  ▼
src/anthropic/middleware.rs   ← API Key / 子 Key 认证（额度检查）、RPM 计数、用量追踪
  │
src/anthropic/handlers/      ← /v1/messages 直通流式（post_messages.rs）；/cc/v1/messages 额外带 300s 全局 deadline（post_messages_cc/）
  │
src/anthropic/converter/      ← Anthropic → Kiro 协议转换（model.rs 模型映射、fields.rs additionalModelRequestFields、
  │                              JSON Schema 规范化、session.rs 派生 agentContinuationId）
  │
src/kiro/provider/            ← HTTP 发送 + 多账号故障转移（每凭据最多 3 次重试，单请求总共 9 次，
  │                              全局并发 50 / 单账号并发 20，Semaphore）+ 统一上游超时 1000s（UPSTREAM_TIMEOUT_SECS）
  │
src/kiro/token_manager/      ← MultiTokenManager（types.rs）：OAuth token 刷新、priority（默认）/ balanced 轮询负载均衡
  │
  ▼  (AWS Event Stream 二进制帧协议)
src/kiro/parser/              ← 二进制帧解码（frame/decoder/header/crc，CRC32 ISO-HDLC 校验）
  │
src/anthropic/stream/         ← Kiro 事件 → Anthropic SSE 状态机（thinking 标签重建、usage 校准）
  ▼
Client (Anthropic SSE format)
```

### 关键设计点

- **OpenAI 兼容层（`src/openai/`）是外挂适配器**：把 `/v1/chat/completions`、`/v1/responses` 的 OpenAI 请求翻译成 Anthropic Messages 请求后，复用 `anthropic::handlers::post_messages` 走完整下游链路，再把响应转回 OpenAI 格式。不改动 anthropic 模块任何实现（有意取舍，代价是 SSE 多一次解析/再编码）。
- **Kiro 上游 4 个端点对应 4 个独立限流桶**（`src/kiro/endpoint.rs`）：ide / runtime / codewhisperer / amazonq；桶级 429 状态按 `(credential_id, endpoint)` 二元组隔离在 `EndpointBucketRegistry`，单次 429 封禁 30s。
- **模型映射按关键词**（`converter/model.rs::map_model`）：请求模型名含 `sonnet`/`opus`/`haiku`/`gpt`/`fable` 等关键词即路由到对应 Kiro 模型，版本号（如 4.5、4.6、4.7、4.8、5、5.5）决定具体代际；未明确指定版本时 opus 兜底 4.6；未命中关键词的模型 ID 剥离 `-thinking` 后透传。
- **模型名含 `thinking` 时覆写 thinking 配置**（`handlers/helpers.rs::override_thinking_from_model_name`）：opus 4.6+ 覆写为 adaptive，其余为 enabled，budget 固定 20000。
- **additionalModelRequestFields 有"模型代际 × 字段"约束矩阵**：max_tokens 下限 1024 对全部 Claude 代际生效，上限按代际分 128K/64K 档；4.5 代际与 qwen3-coder-next / glm-5 / deepseek-3.2 / minimax-m2.5 / kimi-k3 整体跳过该字段（`converter/thinking.rs::additional_fields_skipped`）；GPT 系改发 `reasoning.effort`。详见 `converter/fields.rs::build_additional_model_request_fields` 注释与 `docs/源码全景解析.md` 难点 8。
- **Prompt cache 用量四层降级链**（`src/cache/mod.rs`）：metering 真值 → `token::count_prefix_tokens` 前缀估算 → fingerprint 前缀指纹追踪（账号级）→ 比例模拟兜底。
  开启 `promptCacheAccounting` 时，前缀估算被 `src/cache/breakpoint.rs` 的 cache_control 断点模拟取代（按 API Key + Kiro 模型 ID 隔离，直接解析原始 body，plan → 上游成功后 commit；本地模拟，不代表 Kiro 真实成本）。
- **Token 计数用 tiktoken cl100k_base BPE**（`src/token.rs`，全局单例）。`stream/calib.rs` 的 `CLIENT_TOKEN_DISPLAY_SCALE`（0.6657）是配合旧公式校准的显示缩放系数，控制客户端 auto-compact 触发时机——改动 token 口径时需评估是否需重新校准。
- **history[0] 冻结是 prompt cache 的命脉**：系统提示中的 `cch=` 计费哈希规范化（`normalize_billing_header`）、`agentContinuationId` 由 conversationId 派生、标题生成请求隔离等机制都为让 history[0] 跨请求逐字节稳定。改动 converter 中系统提示/历史消息构建时必须保持这些不变量，否则 Kiro 侧缓存全量失效。

### 配置与运行时

- `config.json` + `credentials.json`（默认工作目录，可 `--config`/`--credentials` 覆盖）；环境变量可覆盖配置（`Config::apply_env_overrides`，容器化部署用）。
- `admin_psw` 非空才启用 Admin API/UI（`/api/admin`、`/admin`）与 User API/UI（`/api/user`、`/user`）。
- 运行时凭据与状态文件在 `app/config/`，凭据及 `kiro_*.json` 等已 gitignore。

## 开发约定（来自 openspec/project.md）

- `cargo fmt` + `cargo clippy` clean 后才可提交
- **打 tag 必须同步版本号（强约束）**：创建 vX.Y.Z tag 前，必须先提交 `chore: 发布 vX.Y.Z`，同步更新 `Cargo.toml` 的 `version` 与 `Cargo.lock`（运行 `cargo check` 刷新），并同步更新 `src/admin/changelog_data.rs` 中的版本条目（否则 `is_latest` 高亮失效）。禁止在版本号未同步的情况下打 tag
- 不引入新外部 crate（能用已有依赖解决的不新增）
  - **例外**：`ip2region`（IP 归属地离线解析，`Cargo.toml` 已引入）
- 改动局限于最小必要范围，不做无关重构
- 所有公开行为变更需同步更新单元测试（`#[cfg(test)] mod tests`；大型模块用例放所属模块的 `tests/` 子目录，如 `src/anthropic/converter/tests/`）
- Commit 遵循 Conventional Commits（feat/fix/refactor/chore），提交信息用中文

## 深入文档

- `docs/源码阅读路线图.md` — 新手从零到精通的源码阅读路径（五阶段学习路线）
- `docs/源码全景解析.md` — 全链路深度解析 + 8 个难点攻坚记录（二进制帧协议、thinking 标签检测、prompt caching、tiktoken 等）
- `docs/代码速查表.md` — 功能 → 代码位置速查表
- `openspec/project.md` — 项目上下文与开发约定
- `.claude/skills/cache-credits-analyzer/` — 分析访问日志计算 prompt cache 节省 credits 的 skill
