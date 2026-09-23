// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic Messages 响应 → OpenAI Responses 响应（模块目录）
//!
//! 与 Chat Completions 的关键差异：产物是一棵 `response` 对象树（`output[]` 里每个
//! item 有独立 id 与 status），而不是 `choices[]`；截断不体现在 `finish_reason`，
//! 而是 `status: "incomplete"` + `incomplete_details.reason`。
//!
//! 按功能拆分：nonstream（非流式转换）、stream（流式转换器）、tests（测试）。

mod nonstream;
mod stream;

#[cfg(test)]
mod tests;

pub(crate) use nonstream::{convert_non_stream, convert_non_stream_compaction};
pub(crate) use stream::ResponsesStreamConverter;
