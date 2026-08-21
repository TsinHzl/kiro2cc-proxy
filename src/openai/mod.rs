// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! OpenAI 协议兼容层
//!
//! 把 OpenAI 协议的请求转换为 Anthropic Messages 请求，复用现有
//! [`crate::anthropic::handlers::post_messages`] 走完整下游链路（多账号故障转移、
//! RPM 计数、用量追踪、prompt cache），再把返回的 Anthropic 响应转回 OpenAI 格式。
//!
//! # 支持的端点
//!
//! - `POST /v1/chat/completions` — OpenAI Chat Completions 协议（流式 + 非流式）
//! - `POST /v1/responses` — OpenAI Responses 协议（Codex CLI 默认 `wire_api`）
//!
//! # 设计要点
//!
//! 本模块是**外挂适配器**：不改动 `anthropic` 模块的任何实现，只在其外层做协议翻译。
//! 代价是响应侧要把 Anthropic SSE 解析一遍再重新编码为 OpenAI SSE（多一次序列化往返），
//! 换来的是对下游 900 行核心逻辑的零侵入。这是有意的取舍，不是疏漏。
//!
//! 详见 `openspec/changes/add-openai-compatible-endpoints/design.md`。

mod chat_request;
mod chat_response;
mod error;
mod handlers;
mod model_map;
mod responses_request;
mod responses_response;
mod sse;

pub(crate) use handlers::{post_chat_completions, post_responses};
