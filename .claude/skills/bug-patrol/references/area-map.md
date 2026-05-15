# kiro.rs 功能区域地图

> 排查时参考此地图确定排查范围和检查点。
> 区域编号前缀：R = 高风险 / M = 中风险 / L = 低风险

---

## 高风险区域

### R1 — Token 管理与认证

#### R1.1 Token 刷新与管理

核心文件：
- `src/kiro/token_manager.rs`
- `src/token.rs`

检查点：
- OAuth token 刷新时序与并发安全
- Token 过期检测与自动续期
- 多 credential 场景下的 token 隔离
- 刷新失败时的重试与降级策略
- Token 缓存一致性

#### R1.2 认证与鉴权

核心文件：
- `src/common/auth.rs`

检查点：
- API Key 验证逻辑
- 认证中间件的请求拦截
- 常量时间比较（防时序攻击）

### R2 — API 代理与请求转换

#### R2.1 Anthropic API 兼容层

核心文件：
- `src/anthropic/` 目录下所有文件

检查点：
- 请求格式转换正确性（Anthropic → Kiro）
- 响应格式转换正确性（Kiro → Anthropic）
- SSE 流式响应的完整性与正确性
- 错误响应的格式兼容
- Content-Type 和 Header 处理

#### R2.2 Kiro Provider

核心文件：
- `src/kiro/provider.rs`
- `src/kiro/parser/` 目录
- `src/kiro/model/` 目录

检查点：
- 请求构建与发送
- 响应解析与流式处理
- 模型映射（Sonnet/Opus/Haiku）
- Tool use / Function calling 转换
- WebSearch 工具转换
- Extended thinking 支持

### R3 — Credential 管理与负载均衡

#### R3.1 多 Credential 调度

核心文件：
- `src/admin/service.rs`
- `src/admin/types.rs`

检查点：
- Credential 优先级/均衡调度算法
- 自动故障转移（failover）逻辑
- 重试策略（3 次/credential，9 次/request）
- Credential 状态管理（启用/禁用/错误计数）

---

## 中风险区域

### M1 — HTTP 客户端与网络

#### M1.1 HTTP 请求处理

核心文件：
- `src/http_client.rs`

检查点：
- 连接池管理
- 超时配置
- 代理支持（HTTP/SOCKS5）
- TLS 配置（rustls）
- 请求重试逻辑

### M2 — Admin API

#### M2.1 管理接口

核心文件：
- `src/admin/handlers.rs`
- `src/admin/router.rs`

检查点：
- CRUD 操作的数据一致性
- 并发修改的安全性
- 输入验证与错误处理
- 认证保护

### M3 — Admin UI（React 前端）

#### M3.1 Credential 管理界面

核心文件：
- `admin-ui/src/components/credential-card.tsx`
- `admin-ui/src/components/edit-credential-dialog.tsx`
- `admin-ui/src/api/credentials.ts`
- `admin-ui/src/hooks/use-credentials.ts`

检查点：
- API 调用与错误处理
- 状态同步（TanStack Query 缓存）
- 表单验证
- UI 状态一致性

---

## 低风险区域

### L1 — 配置与启动

#### L1.1 配置加载

核心文件：
- `src/model/` 目录
- `src/main.rs`
- `config.json`

检查点：
- 配置文件解析
- CLI 参数处理（Clap）
- 默认值与环境变量
- 配置验证

### L2 — Admin UI 静态资源

#### L2.1 静态文件服务

核心文件：
- `src/admin_ui/` 目录

检查点：
- rust-embed 静态文件嵌入
- 路由匹配与 fallback
- Content-Type 推断

### L3 — Docker 部署

#### L3.1 容器化

核心文件：
- `Dockerfile`
- `docker-compose.yml`

检查点：
- 多阶段构建正确性
- 环境变量传递
- 端口映射与网络配置
