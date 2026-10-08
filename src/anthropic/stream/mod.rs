//! 流式响应处理模块
//!
//! 实现 Kiro → Anthropic 流式响应转换和 SSE 状态管理

mod calib;
mod context;
mod echo_guard;
mod helpers;
mod state;
mod tests;
mod thinking;
mod thinking_text;

pub use calib::cap_input_tokens_pub;
#[cfg(test)]
pub(crate) use calib::scale_for_client_with;
pub(crate) use calib::{CLIENT_ASSUMED_CONTEXT_WINDOW, scale_for_client};
pub use calib::{client_token_passthrough_enabled, set_client_token_passthrough};
pub use context::StreamContext;
pub(crate) use echo_guard::ResponseEchoGuard;
pub(crate) use helpers::generate_fake_signature;
pub use state::SseEvent;
pub(crate) use thinking::split_thinking_and_visible;
pub use thinking_text::{
    ThinkingTextRewriter, is_claude_code_client, set_thinking_as_text, thinking_as_text_enabled,
};
pub(crate) use thinking_text::{strip_dim_markers, strip_rendered_thinking};

#[cfg(test)]
pub(crate) use calib::context_window_for_model;
#[cfg(test)]
pub(crate) use helpers::{count_token_chars, tokens_from_chars};
#[cfg(test)]
pub(crate) use state::SseStateManager;
#[cfg(test)]
pub(crate) use thinking::{find_real_thinking_end_tag, find_real_thinking_start_tag};
