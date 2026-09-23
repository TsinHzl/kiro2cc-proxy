// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic API Handler 函数

use crate::anthropic::converter::is_luna_model;
use crate::anthropic::types::{
    CountTokensRequest, CountTokensResponse, MessagesRequest, OutputConfig, Thinking,
};

use crate::token;
use axum::{
    Json as JsonExtractor,
    response::{IntoResponse, Json},
};

/// 去除 JSON 响应中模型可能添加的 Markdown 代码围栏
///
/// 当请求 JSON schema 结构化输出时，部分模型仍会将结果包裹在 ```json...``` 中。
/// 此函数识别并剥离这些围栏，返回纯 JSON 文本。
pub(crate) fn strip_json_fences(text: String) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return text;
    }
    let after_fence = if let Some(rest) = trimmed.strip_prefix("```json\n") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("```json\r\n") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("```\n") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("```\r\n") {
        rest
    } else {
        return text;
    };
    let result = after_fence
        .strip_suffix("\n```")
        .or_else(|| after_fence.strip_suffix("\r\n```"))
        .or_else(|| after_fence.strip_suffix("```"))
        .unwrap_or(after_fence);
    result.to_string()
}

/// 从请求头或连接信息提取客户端真实 IP
pub(crate) fn extract_client_ip(
    headers: &axum::http::HeaderMap,
    connect_info: Option<&std::net::SocketAddr>,
) -> Option<String> {
    if let Some(val) = headers.get("x-forwarded-for")
        && let Ok(s) = val.to_str()
    {
        let ip = s.split(',').next().unwrap_or("").trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }
    if let Some(val) = headers.get("x-real-ip")
        && let Ok(s) = val.to_str()
    {
        let ip = s.trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }
    connect_info.map(|addr| addr.ip().to_string())
}

/// 检测模型名是否包含 "thinking" 后缀，若包含则覆写 thinking 配置
///
/// - Opus 4.6/4.7/4.8/5：覆写为 adaptive 类型
/// - 其他模型：覆写为 enabled 类型
/// - budget_tokens 固定为 20000
pub(crate) fn override_thinking_from_model_name(payload: &mut MessagesRequest) {
    let model_lower = payload.model.to_lowercase();
    if !model_lower.contains("thinking") {
        return;
    }

    let is_opus_adaptive = model_lower.contains("opus")
        && (model_lower.contains("4-6")
            || model_lower.contains("4.6")
            || model_lower.contains("4-7")
            || model_lower.contains("4.7")
            || model_lower.contains("4-8")
            || model_lower.contains("4.8")
            || model_lower.contains("opus-5")
            || model_lower.contains("opus.5")
            || model_lower.contains("opus 5"));

    let thinking_type = if is_opus_adaptive {
        "adaptive"
    } else {
        "enabled"
    };

    tracing::info!(
        model = %payload.model,
        thinking_type = thinking_type,
        "模型名包含 thinking 后缀，覆写 thinking 配置"
    );

    payload.thinking = Some(Thinking {
        thinking_type: thinking_type.to_string(),
        budget_tokens: 20000,
    });

    if is_opus_adaptive {
        payload.output_config = Some(OutputConfig {
            effort: "high".to_string(),
            format: None,
        });
    }
}

/// 判断本次请求是否应启用流式 thinking 处理路径（`<thinking>` 标签的提取与转换）。
///
/// **范围仅限 `gpt-5.6-luna`**，不包括 terra/sol。原因：
/// - `converter::generate_thinking_prefix` / `build_additional_model_request_fields`
///   对全部 GPT 系模型跳过 thinking 注入（有 400 REQUEST_BODY_INVALID 实测依据，
///   范围覆盖整个 gpt-5.6 系列），这一点本函数无需重复处理。
/// - 但"上游是否会真的输出 `<thinking>...</thinking>` 标签、是否具备可用的推理
///   能力"是另一件事，代码库里只有 luna 的实测记录（"上游恒返回 `thinking=0`"，
///   见 README/`openai::model_map` 已知限制）。terra/sol 没有类似证据，不应假定
///   它们也不产出 thinking——若在此处也对它们强制关闭，会误伤其本该具备的、可能
///   仍受客户端 thinking 请求影响的推理能力。
///
/// 因此仅对 luna 强制关闭 `thinking_enabled`：若仍按客户端请求启用，流式状态机会
/// 持续寻找永不出现的 `<thinking>` 标签，而 luna 在缺乏协议约束时有概率自行选择用
/// `<analysis>`/`<summary>` 等自造伪标签组织输出，被当作普通可见文本原样转发给
/// 客户端（配合 `converter::gpt_anti_pseudo_tag_hint` 的提示词引导兜底）。
/// terra/sol 恢复原有行为：`thinking_enabled` 仅取决于客户端是否请求。
pub(crate) fn resolve_thinking_enabled(model: &str, thinking: &Option<Thinking>) -> bool {
    if is_luna_model(model) {
        return false;
    }
    thinking.as_ref().map(|t| t.is_enabled()).unwrap_or(false)
}

/// POST /v1/messages/count_tokens
///
/// 计算消息的 token 数量
pub async fn count_tokens(
    JsonExtractor(payload): JsonExtractor<CountTokensRequest>,
) -> impl IntoResponse {
    tracing::info!(
        model = %payload.model,
        message_count = %payload.messages.len(),
        "Received POST /v1/messages/count_tokens request"
    );

    let total_tokens = token::count_all_tokens(
        payload.model,
        payload.system,
        payload.messages,
        payload.tools,
    ) as i32;

    Json(CountTokensResponse {
        input_tokens: total_tokens.max(1),
    })
}
