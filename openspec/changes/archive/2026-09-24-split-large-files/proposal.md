# 变更提案：split-large-files

## 背景
仓库中存在 10 个超过 1000 行的 Rust 源文件与 2 个超过 1000 行的前端组件文件，单文件过大导致可读性与维护性差。需要按功能模块将代码块移动到子文件中。

## 硬性约束（用户要求）
**纯代码搬移，不改动任何业务逻辑与功能逻辑。** 不重命名、不修改函数体、不调整可见性语义（除 Rust 模块拆分技术必需的 `pub(...)` 可见性放宽——这是编译必需，不是逻辑变更）。

## 目标范围
**在范围内：**
- `src/kiro/token_manager.rs` (5247 行)
- `src/anthropic/handlers.rs` (4284 行)
- `src/anthropic/stream.rs` (2855 行)
- `src/anthropic/converter/tests.rs` (2613 行)
- `src/openai/responses_response.rs` (2034 行)
- `src/kiro/provider.rs` (1857 行)
- `src/kiro/model/credentials.rs` (1347 行)
- `src/openai/responses_request.rs` (1092 行)
- `src/openai/chat_response.rs` (1061 行)
- `src/model/usage.rs` (1061 行)
- `admin-ui/src/components/api-keys-panel.tsx` (1400 行)
- `admin-ui/src/components/dashboard.tsx` (1386 行)

**不在范围内：**
- 任何逻辑重构、重命名、优化
- 未超过 1000 行的文件
- `src/kiro/token_manager.rs` 中单账号 `TokenManager`（原样保留在主文件）
- 文档同步（`docs/代码速查表.md`、`docs/源码全景解析.md` 中的代码位置引用将失效，另行变更更新）

## 技术方案
Rust 文件改为「主文件 + 同目录子模块」结构：原文件保留为模块入口（或新增 mod.rs），代码块按功能段落移动到子文件，主文件 `mod` 声明 + `pub(...) use` 再导出，保证所有外部路径 `crate::xxx::原符号` 不变。`#[cfg(test)] mod tests` 跟随其测试目标的归属文件移动或拆分到对应子文件的 tests 模块，测试内容零修改。可见性放宽规则：优先 `pub(super)`，仅跨目录访问时才用 `pub(crate)`。前端组件按内部子组件拆分为同目录文件，import 路径调整属技术必需。

验证方法（Rust 为单 crate 编译单元，无法按文件独立 check）：
- 拆分前先记录测试基线：`cargo test 2>&1 | tail -3` 快照（测试总数）
- 每拆完一个文件：`cargo check` + `cargo clippy` + 相关 `cargo test <模块名>`
- 全部完成后：`cargo fmt` + `cargo clippy` + `cargo test`（对比基线数量一致）+ 前端 `pnpm build` / `npm run build`（含项目内已有的 tsc 检查脚本，如无则 build 兜底）
- 最终以 `git diff` 逐文件核验仅呈现代码搬移

## 预期影响
零行为变更；模块可见性可能出现编译器强制的技术性 `pub(crate)`/`pub(super)` 放宽。外部调用路径全部通过再导出保持不变。

## 风险
- 搬移时误删/误改代码 → 以 `cargo test`（600 个测试）+ clippy 兜底
- 依赖注入顺序（Rust 中 items 顺序无关，风险低）
- 前端闭包/作用域搬移断裂 → build 兜底
