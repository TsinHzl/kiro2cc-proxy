//! 流式响应处理模块
//!
//! 实现 Kiro → Anthropic 流式响应转换和 SSE 状态管理

mod calib;
mod context;
mod credits_footer;
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
pub use calib::{
    client_token_passthrough_enabled, output_tokens_include_thinking, set_client_token_passthrough,
    set_output_tokens_include_thinking,
};
pub use context::StreamContext;
pub(crate) use credits_footer::strip_credits_footer;
pub use credits_footer::{
    CreditsFooterRequest, begin_request, credits_footer_request, finish_turn, record_credits,
    render_footer, set_show_session_credits, show_session_credits_enabled,
};
pub(crate) use echo_guard::ResponseEchoGuard;
pub(crate) use helpers::generate_fake_signature;
pub use state::SseEvent;
pub(crate) use thinking::split_thinking_and_visible;
pub use thinking_text::{
    ThinkingTextRewriter, ThinkingTextStyle, set_thinking_as_text, set_thinking_collapse,
    thinking_as_text_enabled, thinking_collapse_enabled, thinking_text_style,
};
pub(crate) use thinking_text::{strip_dim_markers, strip_rendered_thinking};

#[cfg(test)]
pub(crate) use calib::context_window_for_model;
#[cfg(test)]
pub(crate) use state::SseStateManager;
#[cfg(test)]
pub(crate) use thinking::{find_real_thinking_end_tag, find_real_thinking_start_tag};
