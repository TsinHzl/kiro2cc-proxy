# 任务清单：integrate-tiktoken-rs

## 状态：ARCHIVED

## 任务
- [x] T1：`Cargo.toml` 新增 `tiktoken-rs = "0.12.0"` 依赖
- [x] T2：`src/token.rs` 用 `OnceLock<CoreBPE>` 缓存 `cl100k_base` 编码器，替换 `count_tokens(text)` 核心实现
- [x] T3：验证 `count_all_tokens_local`、`count_prefix_tokens`、`estimate_output_tokens` 三个函数通过新 `count_tokens` 正确工作（无需改签名，逻辑路径不变）
- [x] T4：更新单元测试期望值为 tiktoken 真实输出，确保 `cargo test` 全绿

## 验收标准
- [ ] `cargo check` 无错误
- [ ] `cargo test` 全部通过（558 个测试 + 新期望值）
- [ ] 中文文本 `"你好世界"` 的 `count_tokens` 输出从 3 变为 5（cl100k_base 真实值）
- [ ] 英文 `"Hello world"` 输出从 3 变为 2（cl100k_base 真实值）
