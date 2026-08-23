# 变更提案：integrate-tiktoken-rs

## 背景

`src/token.rs` 中的本地 token 估算使用自研四分类加权公式（英文字母 /4.5、数字 /2.0、ASCII 符号 /1.5、非 ASCII /1.5）。该公式对中文、代码混合内容的误差可达 10–30%，无法可靠支撑 auto-compact 阈值判断、cache 前缀模拟和 message_start 上报。

## 目标范围

**在范围内：**
- 在 `Cargo.toml` 中引入 `tiktoken-rs = "0.12.0"` 依赖
- 用 `cl100k_base` BPE 编码器替换 `count_tokens(text)` 的核心实现
- 同步更新 `count_all_tokens_local`、`count_prefix_tokens`、`estimate_output_tokens` 三个函数，使其底层调用新的 tiktoken 实现
- 用 `OnceLock<CoreBPE>` 全局缓存编码器实例，避免每次调用重新构建词表
- 更新受影响的单元测试（期望值改为 tiktoken 真实输出）
- 远程 API 分支（`call_remote_count_tokens`）不改动

**不在范围内：**
- 替换远程 count_tokens API 调用路径
- 修改调用方（`handlers.rs`、`cache/mod.rs` 等）
- 引入 tiktoken 以外的其他 tokenizer 库
- 修改 `estimate_output_tokens` 的签名或调用方

## 技术方案

- `tiktoken-rs` 已验证可在当前 Rust 2024 edition 下正常编译，无冲突依赖
- `cl100k_base()` 构建耗时约 50ms（一次性），用 `OnceLock` 在首次调用时初始化
- `encode_with_special_tokens(text).len()` 直接返回 token 数，替换原有加权公式
- 空字符串返回 0，调用方已有 `.max(1)` 兜底，无需额外处理

## 预期影响

- token 估算精度显著提升，尤其中文（原误差 20-30% → 接近真实值）
- 首次调用 `count_tokens` 有约 50ms 一次性初始化开销（进程生命周期内只发生一次）
- 二进制体积增大约 2-3MB（tiktoken-rs 词表数据打包进二进制）
- 测试期望值变化：单元测试数值需按 tiktoken 真实输出更新

## 风险

- `tiktoken-rs` 使用 `cl100k_base`（GPT-4 词表），与 Claude 实际词表仍有差异，但误差从 10-30% 降至 2-15%
- 词表数据打包进二进制，不支持运行时切换编码器（可接受，当前也不支持）
