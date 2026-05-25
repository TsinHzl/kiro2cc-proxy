# 项目上下文

## 技术栈
- 语言：Rust（2024 edition）
- 框架：axum（HTTP）、tokio（async runtime）、reqwest（HTTP 客户端）
- 序列化：serde_json
- 加密：sha2、uuid

## 架构约定
- `src/anthropic/`：Anthropic 协议处理层（converter、handlers、stream、types）
- `src/kiro/`：Kiro API 客户端层（provider、token_manager、model/）
- `src/model/`：通用数据模型（config、usage 等）
- `converter.rs`：Anthropic → Kiro 协议转换，生成 conversationId + agentContinuationId
- `token_manager.rs`：多凭据管理，支持 priority / balanced 两种负载均衡模式
- `provider.rs`：HTTP 请求发送，支持多凭据故障转移与重试

## 目录结构
```
src/
├── anthropic/          # Anthropic 协议层
│   ├── converter.rs    # 协议转换（含 agentContinuationId 派生）
│   ├── handlers.rs     # 请求入口
│   └── stream.rs       # 流式响应处理
├── kiro/               # Kiro API 客户端
│   ├── provider.rs     # HTTP 发送 + 重试 + 故障转移
│   ├── token_manager.rs# 多凭据管理 + 负载均衡
│   └── model/          # 请求/响应数据类型
└── model/              # 通用模型
```

## 开发约定
- 代码风格：Rust 官方 fmt，clippy clean
- 不引入新的外部 crate（能用标准库或已有依赖解决的不新增）
- 改动局限于最小必要范围，不做无关重构
- 所有公开 API 变更需同步更新单元测试
