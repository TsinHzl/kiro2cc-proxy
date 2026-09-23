// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Kiro API Provider
//!
//! 核心组件，负责与 Kiro API 通信
//! 支持流式和非流式请求
//! 支持多账号故障转移和重试
//!
//! 按功能拆分：core（结构与基础方法）、headers（请求头构建）、api（公开 API 入口与
//! MCP 调用）、retry（重试与故障转移）、errors（错误分类与请求体改写）、tests（测试）。

mod api;
mod core;
mod errors;
mod headers;
mod retry;

#[cfg(test)]
mod tests;

pub use core::KiroProvider;
