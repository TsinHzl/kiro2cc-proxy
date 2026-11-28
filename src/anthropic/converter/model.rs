// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic 模型名 → Kiro 模型 ID 映射

/// 判断模型名是否属于 haiku 5.5（5.5 代际）。
///
/// 单一真源：`map_model`、`model_max_output_tokens`、`get_model_pricing`
/// 三处共用（三者入参均为客户端原始模型名），避免判定口径漂移导致
/// 「映射到 5.5 但按 4.5 定价」的错配。
///
/// 先把 `.` / 空格归一为 `-` 再匹配版本号，覆盖全部分隔符变体
/// （与 opus 分支的 `opus-5` / `opus.5` / `opus 5` 惯例一致）：
/// `haiku-5.5` / `haiku-5-5` / `haiku.5.5` / `haiku.5-5` / `haiku 5.5` 等；
/// 同时不误命中 `claude-haiku-5.0` / `5.6` / `50`（原先只判主版本前缀会命中）。
pub(crate) fn is_haiku_55(model_lower: &str) -> bool {
    model_lower.replace(['.', ' '], "-").contains("haiku-5-5")
}

/// 判断模型名是否带有指定的版本号（`key` 为归一化后的形式，如 `fable-5-1`、`gpt-6-2`）。
///
/// 把 `.` / 空格 / `_` 归一为 `-` 后匹配；`key` 之后紧跟数字（如 `fable-5-10`）不算，
/// 避免把其他版本号误判。
fn has_model_version(model_lower: &str, key: &str) -> bool {
    let normalized = model_lower.replace(['.', ' ', '_'], "-");
    normalized
        .find(key)
        .is_some_and(|i| !normalized[i + key.len()..].starts_with(|c: char| c.is_ascii_digit()))
}

/// 判断模型名是否属于 fable 5.1，覆盖 `fable-5.1` / `fable-5-1` / `fable 5.1` 等写法。
pub(crate) fn is_fable_51(model_lower: &str) -> bool {
    has_model_version(model_lower, "fable-5-1")
}

/// 判断模型名是否属于 fable 5.2，覆盖 `fable-5.2` / `fable-5-2` / `fable 5.2` 等写法。
pub(crate) fn is_fable_52(model_lower: &str) -> bool {
    has_model_version(model_lower, "fable-5-2")
}

/// 判断模型名是否属于 GPT 6.2 Astra，覆盖 `gpt-6.2-astra` / `gpt-6-2` / `GPT 6.2 Astra` 等写法。
///
/// 含 `astra` 即命中；否则按版本号匹配 `gpt-6-2`（`gpt-6.20` 之类不会误判为 6.2）。
pub(crate) fn is_gpt_62_astra(model_lower: &str) -> bool {
    model_lower.contains("astra") || has_model_version(model_lower, "gpt-6-2")
}

/// 判断模型名是否属于 GPT 6 Luna，覆盖 `gpt-6-luna` / `GPT 6 Luna` 等写法。
///
/// 需同时含 `luna` 与 `gpt-6` 版本号；`gpt-5.6-luna` 归一后为 `gpt-5-6-luna`，不含 `gpt-6`，不会误判。
pub(crate) fn is_gpt_6_luna(model_lower: &str) -> bool {
    model_lower.contains("luna") && has_model_version(model_lower, "gpt-6")
}

/// 模型映射：将 Anthropic 模型名映射到 Kiro 模型 ID
///
/// 按照用户要求：
/// - sonnet 5.5/5-5 → claude-sonnet-5.5
/// - sonnet 4.6/4-6 → claude-sonnet-4.6
/// - 其他 sonnet → claude-sonnet-4.5
/// - fable 5.2/5-2 → claude-fable-5.2
/// - fable 5.1/5-1 → claude-fable-5.1
/// - 其他 fable → claude-fable-5
/// - opus 5.5/5-5 → claude-opus-5.5
/// - opus 5/5 → claude-opus-5
/// - opus 4.5/4-5 → claude-opus-4.5
/// - 其他 opus → claude-opus-4.6
/// - haiku 5.5/5-5 → claude-haiku-5.5
/// - 其他 haiku → claude-haiku-4.5
/// - gpt 6.2 / astra → gpt-6.2-astra
/// - gpt 6 luna → gpt-6-luna
pub fn map_model(model: &str) -> Option<String> {
    let model_lower = model.to_lowercase();

    if model_lower.contains("sonnet") {
        if model_lower.contains("5.5") || model_lower.contains("5-5") {
            // claude-sonnet-5.5：5.5 代际，Max Output 64K（档位沿用 5 代）
            Some("claude-sonnet-5.5".to_string())
        } else if model_lower.contains("4-6") || model_lower.contains("4.6") {
            Some("claude-sonnet-4.6".to_string())
        } else if model_lower.contains("sonnet-5") || model_lower.contains("sonnet.5") {
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
        if is_fable_52(&model_lower) {
            // claude-fable-5.2：Kiro 目前仅以 Preview 形式对 Enterprise 组织开放（仅 US 区域），
            // 且需管理员在 approved list 中启用；是否可用由上游账号权限决定
            Some("claude-fable-5.2".to_string())
        } else if is_fable_51(&model_lower) {
            // claude-fable-5.1：Kiro 仅对特定账号 / 区域（Enterprise，us-east-1）开放，
            // 是否可用由上游账号权限决定；代理只负责把名字映射到正确的上游 ID
            Some("claude-fable-5.1".to_string())
        } else {
            Some("claude-fable-5".to_string())
        }
    } else if model_lower.contains("opus") {
        if model_lower.contains("5.5") || model_lower.contains("5-5") {
            // claude-opus-5.5：5.5 代际，Max Input 1M / Max Output 128K（档位沿用 5 代）
            Some("claude-opus-5.5".to_string())
        } else if model_lower.contains("opus-5")
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
        if is_haiku_55(&model_lower) {
            // claude-haiku-5.5：5.5 代际，Max Input 1M / Max Output 128K
            Some("claude-haiku-5.5".to_string())
        } else {
            Some("claude-haiku-4.5".to_string())
        }
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
    } else if model_lower.contains("kimi") {
        // kimi-k3（上游 2026-10-16 新增）；-thinking 后缀与别名同样归一到该 ID
        Some("kimi-k3".to_string())
    } else if model_lower.contains("grok") {
        // grok-4.7（上游 2026-11-11 新增，xAI Grok 4.7 实验预览）；
        // -thinking 后缀与空格别名同样归一到该 ID
        Some("grok-4.7".to_string())
    } else if model_lower.contains("gpt") {
        if is_gpt_62_astra(&model_lower) {
            // 可用性由上游账号权限决定，本层只做名称归一
            Some("gpt-6.2-astra".to_string())
        } else if is_gpt_6_luna(&model_lower) {
            // 须在 5.6 Luna 分支之前判定，否则会被 contains("luna") 吞并
            Some("gpt-6-luna".to_string())
        } else if model_lower.contains("terra") {
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
