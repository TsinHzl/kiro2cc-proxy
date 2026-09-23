//! 模型定价与费用估算：每百万 tokens 美元定价、credits/USD 换算率、单次请求费用

/// 模型定价（每百万 tokens，美元）
/// 使用 200K context 标准定价
pub(crate) struct ModelPricing {
    input_per_mtok: f64,
    output_per_mtok: f64,
}

/// 根据模型名获取定价
pub(crate) fn get_model_pricing(model: &str) -> ModelPricing {
    let model_lower = model.to_lowercase();

    if model_lower.contains("opus") {
        // Opus 4.5+: $5 / $25
        ModelPricing {
            input_per_mtok: 5.0,
            output_per_mtok: 25.0,
        }
    } else if model_lower.contains("haiku") {
        // Haiku 4.5: $1 / $5
        ModelPricing {
            input_per_mtok: 1.0,
            output_per_mtok: 5.0,
        }
    } else {
        // Sonnet 4 / sonnet-5 / haiku: $3 / $15
        // claude-sonnet-5 Rate = 1.3 Credit，与 sonnet-4.x 同档，定价一致
        ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        }
    }
}

/// 平台级 credits/USD 换算率，按模型档位差异化（代理实测 2026-06-25）。
/// 仅 usage 报表 credits_saved 字段使用（estimated_cost × k_ref - credits_used）。
/// cache_read 派生已切换为前缀估算路径，不再依赖此值。
/// 2026-06-30 重校：按 opus 版本分档，基于实测 d=0.50 缓存折扣反推。
/// 2026-07-25 追加 opus-5 与 4.7/4.8 同档。
pub(crate) fn get_k_ref(model: &str) -> f64 {
    let m = model.to_lowercase();
    if m.contains("opus-4-7")
        || m.contains("opus-4.7")
        || m.contains("opus-4-8")
        || m.contains("opus-4.8")
        || m.contains("opus-5")
        || m.contains("opus.5")
    {
        // opus 4.7/4.8/5 共用同档（实测 4.8 ≈ 2.36，5 沿用 4.8 档位）
        2.36
    } else if m.contains("opus-4-5")
        || m.contains("opus-4.5")
        || m.contains("opus-4-6")
        || m.contains("opus-4.6")
    {
        // 旧 opus 4.5/4.6（实测 4.6 ≈ 1.90）
        1.90
    } else if m.contains("opus") || m.contains("fable") {
        // 未知 opus / fable 兜底沿用最新档
        2.36
    } else if m.contains("sonnet-5") || m.contains("sonnet.5") {
        // claude-sonnet-5: Rate = 1.3 Credit，与 sonnet-4.5/4.6 同档（实测确认）
        1.43
    } else {
        // sonnet 系列 / haiku 默认
        1.43
    }
}

/// 计算单次请求的估算费用
pub(crate) fn calculate_cost(model: &str, input_tokens: i32, output_tokens: i32) -> f64 {
    let pricing = get_model_pricing(model);
    let input_cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * pricing.output_per_mtok;
    input_cost + output_cost
}
