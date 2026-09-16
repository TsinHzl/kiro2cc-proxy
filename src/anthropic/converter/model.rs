// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic 模型名 → Kiro 模型 ID 映射

/// 模型映射：将 Anthropic 模型名映射到 Kiro 模型 ID
///
/// 按照用户要求：
/// - sonnet 4.6/4-6 → claude-sonnet-4.6
/// - 其他 sonnet → claude-sonnet-4.5
/// - opus 5/5 → claude-opus-5
/// - opus 4.5/4-5 → claude-opus-4.5
/// - 其他 opus → claude-opus-4.6
/// - 所有 haiku → claude-haiku-4.5
pub fn map_model(model: &str) -> Option<String> {
    let model_lower = model.to_lowercase();

    if model_lower.contains("sonnet") {
        if model_lower.contains("4-6") || model_lower.contains("4.6") {
            Some("claude-sonnet-4.6".to_string())
        } else if model_lower.contains("sonnet-5") || model_lower.contains("sonnet.5") {
            // claude-sonnet-5: Max Input 1M, Max Output 64K, Rate 1.3 Credit（与 sonnet-4.x 同档）
            Some("claude-sonnet-5".to_string())
        } else if model_lower.contains("4-5") || model_lower.contains("4.5") {
            Some("claude-sonnet-4.5".to_string())
        } else if model_lower.contains("sonnet-4") || model_lower.contains("sonnet.4") {
            // claude-sonnet-4 / claude-sonnet-4-20250514 等，精确匹配 sonnet-4 前缀
            Some("claude-sonnet-4".to_string())
        } else {
            Some("claude-sonnet-4.5".to_string())
        }
    } else if model_lower.contains("fable") {
        Some("claude-fable-5".to_string())
    } else if model_lower.contains("opus") {
        if model_lower.contains("opus-5")
            || model_lower.contains("opus.5")
            || model_lower.contains("opus 5")
        {
            // claude-opus-5: Max Input 1M, Max Output 128K, Rate 2.2 Credit（与 4.7/4.8 同档）
            Some("claude-opus-5".to_string())
        } else if model_lower.contains("4-5") || model_lower.contains("4.5") {
            Some("claude-opus-4.5".to_string())
        } else if model_lower.contains("4-8") || model_lower.contains("4.8") {
            Some("claude-opus-4.8".to_string())
        } else if model_lower.contains("4-7") || model_lower.contains("4.7") {
            Some("claude-opus-4.7".to_string())
        } else {
            Some("claude-opus-4.6".to_string())
        }
    } else if model_lower.contains("haiku") {
        Some("claude-haiku-4.5".to_string())
    } else if model_lower == "auto" {
        Some("auto".to_string())
    } else if model_lower.contains("deepseek") {
        Some("deepseek-3.2".to_string())
    } else if model_lower.contains("glm") {
        Some("glm-5".to_string())
    } else if model_lower.contains("minimax") {
        if model_lower.contains("2.5") || model_lower.contains("2-5") {
            Some("minimax-m2.5".to_string())
        } else {
            Some("minimax-m2.1".to_string())
        }
    } else if model_lower.contains("qwen") {
        Some("qwen3-coder-next".to_string())
    } else if model_lower.contains("gpt") {
        if model_lower.contains("terra") {
            Some("gpt-5.6-terra".to_string())
        } else if model_lower.contains("luna") {
            Some("gpt-5.6-luna".to_string())
        } else if model_lower.contains("sol")
            || model_lower.contains("5.6")
            || model_lower.contains("5-6")
        {
            // 未指定具体变体（sol/terra/luna）时默认落到旗舰档 sol
            Some("gpt-5.6-sol".to_string())
        } else {
            // gpt-* 未命中已知变体：走开放透传兜底
            passthrough_model(model)
        }
    } else {
        // 未命中任何内置关键词规则：开放透传，可用性交由上游判断
        passthrough_model(model)
    }
}

/// 开放透传兜底：对未命中内置规则的模型 ID 剥离 `-thinking` 后原样透传；
/// 空字符串仍返回 `None`（上层转为 `UnsupportedModel`）。
///
/// thinking 由 `req.thinking` / additionalModelRequestFields 单独控制，
/// 透传给上游的后端模型 ID 不应携带 `-thinking` 标记。
fn passthrough_model(model: &str) -> Option<String> {
    let stripped = model.replace("-thinking", "");
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        None
    } else {
        tracing::info!(target: "model_passthrough", original = %model, passthrough = %trimmed, "未知模型开放透传");
        Some(trimmed.to_string())
    }
}
