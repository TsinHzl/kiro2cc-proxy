# 任务清单：split-large-files

## 状态：IN_PROGRESS

## 任务
- [x] T1 拆分 `src/kiro/token_manager.rs`（子模块：单账号管理/刷新与 IDC/多账号状态条目/多账号选择与 sticky/统计与持久化/账号管理操作；tests 随归属移动）
- [x] T2 拆分 `src/anthropic/handlers.rs`（子模块：models 列表/错误与请求解析/SSE 公共小件/web_search bridge 流水线/消息构建与 thinking/主消息处理函数）
- [x] T3 拆分 `src/anthropic/stream.rs`（子模块：thinking 标签检测/SseStateManager 与 SseEvent/StreamContext/测试）
- [x] T4 拆分 `src/anthropic/converter/tests.rs`（该文件已是独立测试模块，按被测子模块拆为 `converter/tests/` 目录下多文件，无对外路径需保持）
- [x] T5 拆分 `src/openai/responses_response.rs`（非流式转换/流式转换器/测试）
- [ ] T6 拆分 `src/kiro/provider.rs`（请求构建与 header/重试与故障转移/错误分类与 body 改写/测试）
- [ ] T7 拆分 `src/kiro/model/credentials.rs`（凭据模型/配置反序列化）
- [ ] T8 拆分 `src/openai/responses_request.rs`（工具收集与转换/消息转换）
- [ ] T9 拆分 `src/openai/chat_response.rs`（非流式转换/流式转换器）
- [ ] T10 拆分 `src/model/usage.rs`（定价计算/用量记录与追踪/分页与日报查询）
- [ ] T11 拆分 `admin-ui/src/components/api-keys-panel.tsx`（子组件 CredentialMultiSelect 等提取到独立文件）
- [ ] T12 拆分 `admin-ui/src/components/dashboard.tsx`（常量/工具函数/子区块提取）
- [ ] T13 全量回归：`cargo fmt` + `cargo clippy` + `cargo test` + 前端双 build

## 验收标准
- [ ] `cargo test` 全部通过，测试总数与拆分前基线快照一致
- [ ] `cargo fmt --check` 与 `cargo clippy` clean
- [ ] `admin-ui`、`user-ui` build 通过（含项目内已有 tsc 检查）
- [ ] `git diff` 核验：无逻辑改动，仅代码搬移 + 编译必需的可见性/导入调整
