//! 模型定价与费用估算：每百万 tokens 美元定价、credits/USD 换算率、单次请求费用。
/// 模型定价（每百万 tokens，美元）
/// 使用 200K context 标准定价
pub(crate) struct ModelPricing {
    input_per_mtok: f64,
    output_per_mtok: f64,
}

/// 判断模型名是否属于 haiku 5.5（5.5 代际）。
///
/// 与 `crate::anthropic::converter::model::is_haiku_55` 保持同一判定口径
/// （同一模型族在映射层与定价层必须一致，否则会出现「映射到 5.5 但按 4.5 计价」）。
/// 不跨层引用是为避免 `model` 层反向依赖 `anthropic` 层。
fn is_haiku_55(model_lower: &str) -> bool {
    model_lower.replace(['.', ' '], "-").contains("haiku-5-5")
}

/// Haiku 5.5 长上下文阈值（tokens）。
///
/// 官方口径：单次请求 prompt 超过 100K 时，**整个请求**按 5 倍高档计费
/// （非超出部分累进）。低档 $0.10/$0.50，高档 $0.50/$2.50。
const HAIKU_55_LONG_CONTEXT_THRESHOLD: i32 = 100_000;

/// 根据模型名获取定价
pub(crate) fn get_model_pricing(model: &str) -> ModelPricing {
    let model_lower = model.to_lowercase();

    if model_lower.contains("opus") {
        // Opus 4.5+: $5 / $25
        ModelPricing {
            input_per_mtok: 5.0,
            output_per_mtok: 25.0,
        }
    } else if is_haiku_55(&model_lower) {
        // Haiku 5.5: $0.10 / $0.50（≤100K 低档，官方称覆盖约 90% 请求量）。
        // >100K 的高档由 calculate_cost 按整档跳价处理（本函数不持有 token 数）。
        ModelPricing {
            input_per_mtok: 0.10,
            output_per_mtok: 0.50,
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
        // sonnet 系列 / haiku（含 haiku 5.5）默认
        1.43
    }
}

/// 计算单次请求的估算费用
///
/// Haiku 5.5 按官方「整档跳价」口径：输入超 100K 时整个请求走高档
/// （见 HAIKU_55_LONG_CONTEXT_THRESHOLD）。
///
/// 已知偏差：`input_tokens` 为**不含 cache token** 的净输入
/// （cache_read / cache_creation 单独字段存储，未参与定价计算），
/// 故阈值按净输入判定，与官方「prompt 总长度」口径存在系统性低估，
/// 带长缓存前缀的会话可能被判入低档。
pub(crate) fn calculate_cost(model: &str, input_tokens: i32, output_tokens: i32) -> f64 {
    let model_lower = model.to_lowercase();
    let pricing = if is_haiku_55(&model_lower) && input_tokens > HAIKU_55_LONG_CONTEXT_THRESHOLD {
        ModelPricing {
            input_per_mtok: 0.50,
            output_per_mtok: 2.50,
        }
    } else {
        get_model_pricing(model)
    };
    let input_cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * pricing.output_per_mtok;
    input_cost + output_cost
}
