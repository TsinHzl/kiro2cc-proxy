// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic API 兼容服务模块
//!
//! 提供与 Anthropic Claude API 兼容的 HTTP 服务端点。
//!
//! # 支持的端点
//!
//! ## 标准端点 (/v1)
//! - `GET /v1/models` - 获取可用模型列表
//! - `POST /v1/messages` - 创建消息（对话）
//! - `POST /v1/messages/count_tokens` - 计算 token 数量
//!
//! ## Claude Code 兼容端点 (/cc/v1)
//! - `POST /cc/v1/messages` - 创建消息（流式响应实时转发，额外带 300s 上游挂起保护）
//! - `POST /cc/v1/messages/count_tokens` - 计算 token 数量（与 /v1 相同）
//!
//! # 使用示例
//! ```rust,ignore
//! use kiro_rs::anthropic;
//!
//! let app = anthropic::create_router("your-api-key");
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
//! axum::serve(listener, app).await?;
//! ```

pub(crate) mod converter;
pub(crate) mod handlers;
pub mod middleware;
mod router;
mod stream;
pub mod types;
mod websearch;

pub use converter::{
    DEFAULT_WRITE_CHUNKING_MAX_LINES, set_write_chunking_max_lines, write_chunking_max_lines,
};
pub(crate) use handlers::kiro_body_dump_enabled;
pub(crate) use handlers::set_kiro_body_dump_enabled;
pub use handlers::set_models_official_aliases;
pub use router::create_router_with_provider_and_state;
pub use stream::{
    client_token_passthrough_enabled, set_client_token_passthrough,
    set_output_tokens_include_thinking, set_show_session_credits, set_thinking_as_text,
    set_thinking_collapse, show_session_credits_enabled, thinking_as_text_enabled,
    thinking_collapse_enabled,
};

#[cfg(test)]
pub(crate) use converter::convert_request;
