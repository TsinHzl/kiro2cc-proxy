# 项目上下文

## 技术栈
- 语言：Rust 2024 edition（v2.2.29）
- HTTP 框架：axum 0.8 + tokio 1.x（full features）
- HTTP 客户端：reqwest 0.12（stream、json、socks、rustls-tls）
- 序列化：serde / serde_json 1.0
- 同步：parking_lot 0.12（Mutex/RwLock）、tokio::sync::Semaphore
- 安全：sha2 0.10、subtle 2.6（常量时间比较）、uuid 1.10
- 静态资源：rust-embed 8（前端资源内嵌二进制）
- 构建：Cargo，release profile 启用 lto=thin + strip

## 架构概览

```
Client (Anthropic format)
  │
  ▼
src/anthropic/middleware.rs   ← 认证（API Key / Bearer）、RPM 计数、用量追踪
  │
src/anthropic/handlers.rs     ← /v1/messages 路由入口；/cc/v1/messages 为缓冲模式
  │
src/anthropic/converter.rs    ← Anthropic → Kiro 协议转换（工具 schema 规范化）
  │
src/kiro/provider.rs          ← 多账号故障转移；MAX 3 retries/account，MAX 9 total
  │
src/kiro/token_manager.rs     ← MultiTokenManager：OAuth token 刷新、负载均衡
  │
  ▼  (Kiro binary frame protocol)
src/kiro/parser/              ← 二进制帧解码（frame.rs + decoder.rs + crc.rs）
  │
src/anthropic/stream.rs       ← Kiro 事件 → Anthropic SSE 转换（状态机）
  ▼
Client (Anthropic SSE format)
```

## 目录结构

```
src/
├── anthropic/          # Anthropic 协议层
│   ├── converter.rs    # Anthropic→Kiro 格式转换（含 JSON Schema 规范化）
│   ├── handlers.rs     # 请求入口（/v1 直通 + /cc/v1 缓冲）
│   ├── stream.rs       # SSE 流式状态机
│   ├── middleware.rs   # 认证、RPM 计数
│   ├── types.rs        # Anthropic API 数据类型
│   └── websearch.rs    # Web 搜索相关
├── kiro/               # Kiro API 客户端
│   ├── provider.rs     # HTTP 发送 + 重试 + 故障转移（并发上限 50）
│   ├── token_manager.rs# 多账号 token 池（priority/balanced 负载均衡）
│   ├── model/          # 请求/响应/事件数据类型
│   └── parser/         # 私有二进制帧协议解码（CRC32 校验）
├── model/              # 通用数据模型
│   ├── config.rs       # 全局配置（apply_env_overrides）
│   ├── rpm.rs          # RPM 追踪
│   ├── throttle_log.rs # 限流日志存储
│   └── usage.rs        # 用量统计
├── admin/              # Admin REST API
├── admin_ui/           # rust-embed 嵌入 Admin 前端路由
├── user/               # 用户端 API
├── user_ui/            # rust-embed 嵌入 User 前端路由
├── common/auth.rs      # 公共认证工具
├── cache.rs            # Prompt cache 用量追踪
├── http_client.rs      # reqwest Client 构建器（代理支持）
└── main.rs             # 启动入口

admin-ui/               # Admin 前端源码（Vue/TS，构建后嵌入二进制）
user-ui/                # User 前端源码（构建后嵌入二进制）
app/config/             # 运行时配置（gitignore）
openspec/               # OpenSpec 规范驱动开发目录
docs/                   # 文档（含 code-index.md 功能速查表）
```

## 开发约定
- 代码风格：`cargo fmt` + `cargo clippy` clean 后才可提交
- 不引入新外部 crate（能用已有依赖解决的不新增）
- 改动局限于最小必要范围，不做无关重构
- 所有公开行为变更需同步更新单元测试（src/test.rs）
- Commit：Conventional Commits（feat/fix/refactor/chore）

## 关键文件
| 文件 | 职责 |
|------|------|
| `src/kiro/parser/` | Kiro 私有二进制帧协议解码（带 CRC32 校验） |
| `src/anthropic/converter.rs` | 协议转换 + JSON Schema 规范化 |
| `src/anthropic/stream.rs` | Kiro events → Anthropic SSE 状态机 |
| `src/kiro/token_manager.rs` | 多账号 token 池 + 负载均衡 |
| `src/model/config.rs` | 全局配置结构 + env overrides |
| `src/cache.rs` | Prompt cache 计费追踪 |
| `docs/code-index.md` | 功能 → 代码位置速查表 |
