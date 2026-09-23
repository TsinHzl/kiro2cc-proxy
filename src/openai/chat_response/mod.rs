// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic Messages 响应 → OpenAI Chat Completions 响应
//!
//! 非流式部分把 `handle_non_stream_request` 产出的 JSON（`src/anthropic/handlers.rs`
//! 末尾的 `response_body`）重新组装为 `chat.completion` 对象。
//!
//! 响应的 `model` 字段一律回写**客户端请求的原始模型名**，而非映射后的 Kiro 模型名 ——
//! Codex 会校验请求与响应的模型名一致性。
mod nonstream;
mod stream;

#[cfg(test)]
mod tests;

pub(crate) use nonstream::convert_non_stream;
#[allow(unused_imports)] // 测试引用
pub(crate) use nonstream::map_finish_reason;
#[allow(unused_imports)] // 跨模块测试（responses_response）引用
pub(super) use nonstream::unix_now;
pub(crate) use stream::ChatStreamConverter;
pub(super) use stream::{INPUT_JSON_DELTA, SIGNATURE_DELTA, TEXT_DELTA, THINKING_DELTA};
