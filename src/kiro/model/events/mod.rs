// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 事件模型
//!
//! 定义 generateAssistantResponse 流式响应的事件类型

mod assistant;
mod base;
mod code_reference;
mod context_usage;

mod metering;
mod tool_use;

pub use assistant::AssistantResponseEvent;
pub use base::Event;
pub use code_reference::CodeReferenceEvent;
pub use context_usage::ContextUsageEvent;

pub use metering::MeteringEvent;
pub use tool_use::ToolUseEvent;
