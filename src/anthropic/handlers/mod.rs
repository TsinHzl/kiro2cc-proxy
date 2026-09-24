// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic API Handler 函数（模块目录）
//!
//! 按功能拆分：models（模型列表）、error（错误与请求解析）、bridge（web_search 桥接）、
//! stream（SSE 流式）、nonstream（非流式）、post_messages / post_messages_cc（端点主函数）、
//! helpers（杂项工具）、ping（诊断端点）。

mod bridge;
mod error;
mod helpers;
mod models;
mod nonstream;
mod ping;
mod post_messages;
mod post_messages_cc;
mod stream;

pub use helpers::count_tokens;
pub(crate) use models::available_model_to_model;
pub(crate) use models::build_model_list;
pub use models::get_model;
pub use models::get_models;
pub use ping::ping;
pub use post_messages::post_messages;
pub use post_messages_cc::post_messages_cc;

#[cfg(test)]
mod tests;
