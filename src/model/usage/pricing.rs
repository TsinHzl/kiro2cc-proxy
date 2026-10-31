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

/// 判断模型名是否属于 sonnet 5.5（与 `is_haiku_55` 同口径，兼容 `5-5` / `5.5`）。
fn is_sonnet_55(model_lower: &str) -> bool {
    model_lower.replace(['.', ' '], "-").contains("sonnet-5-5")
}

/// Haiku 5.5 长上下文阈值（tokens）。
///
/// 官方口径：单次请求 prompt 超过 100K 时，**整个请求**按 5 倍高档计费
/// （非超出部分累进）。低档 $0.10/$0.50，高档 $0.50/$2.50。
const HAIKU_55_LONG_CONTEXT_THRESHOLD: i32 = 100_000;

/// 触发 `creditsUsed` 缓存读取反推的最小输入 token 阈值。
///
/// 输入过短时，输入积分占总 `creditsUsed` 比重过小，输出分词微差除以较小的
/// `0.90 * r_miss` 会被放大，故低于 4096 tokens 直接保留静态前缀估算。
pub(crate) const MIN_INFER_INPUT_TOKENS: i32 = 4096;

/// Kiro 上游缓存读取相对未命中输入单价的节省比例（按 10% 计费，节省 90%）。
const CACHE_READ_SAVINGS_RATIO: f64 = 0.90;

/// Haiku 5.5 常态档（<= 100K）实测 credits/MTok 费率：(r_miss, r_out)。
const HAIKU_55_CREDIT_RATES_STANDARD: (f64, f64) = (1.6978, 19.1873);

/// Haiku 5.5 长上下文档（> 100K）实测 credits/MTok 费率：(r_miss, r_out)。
const HAIKU_55_CREDIT_RATES_LONG_CONTEXT: (f64, f64) = (6.7830, 93.9200);

/// Sonnet 5.5 实测 credits/MTok 费率：(r_miss, r_out)。
///
/// 按通用 sonnet 派生值（4.29 / 21.45）反推时，热请求的实际 credits ≈ 4.3/MTok
/// 恰好等于派生的 r_miss，于是 94% 的请求被判为「完全未命中」、缓存读取落库为 0。
/// 2026-10-31 单账号 457 个样本（55 个冷启动 + 402 个 4 分钟内的热续请求，
/// 输入 8K–168K、thinking adaptive）联合回归：未缓存 ≈ 10.8、已缓存 ≈ 4.9。
/// `r_out` 按可见输出 token 计（≈ 290），远高于定价派生值，因为上游不回传
/// Sonnet 5.5 的思考内容、但照常计费，隐藏的思考成本只能摊到可见输出上。
/// 未覆盖 >200K 输入，也未单独标定不开 thinking 的请求。
const SONNET_55_CREDIT_RATES: (f64, f64) = (10.8, 290.0);

/// Sonnet 5.5 缓存读取的节省比例：已缓存 ≈ 4.9 / 未缓存 ≈ 10.8，约省 55%（不是 90%）。
const SONNET_55_CACHE_READ_SAVINGS_RATIO: f64 = 0.55;

/// Opus 4.5+ 定价：$5 / $25。fable 仅在反推路径沿用此档。
const OPUS_PRICING: ModelPricing = ModelPricing {
    input_per_mtok: 5.0,
    output_per_mtok: 25.0,
};

/// 根据模型名获取定价
pub(crate) fn get_model_pricing(model: &str) -> ModelPricing {
    pricing_of_lower(&model.to_lowercase())
}

fn pricing_of_lower(model_lower: &str) -> ModelPricing {
    if model_lower.contains("opus") {
        OPUS_PRICING
    } else if is_haiku_55(model_lower) {
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
/// 供 usage 报表 credits_saved（estimated_cost × k_ref - credits_used）与
/// creditsUsed 缓存读取反推（get_claude_credit_rates）共用。
/// 2026-06-30 重校：按 opus 版本分档，基于实测 d=0.50 缓存折扣反推。
/// 此处保留历史标定背景；这些值未按当前反推使用的 d=0.90 重新标定。
/// 2026-07-25 追加 opus-5 与 4.7/4.8 同档。
pub(crate) fn get_k_ref(model: &str) -> f64 {
    k_ref_of_lower(&model.to_lowercase())
}

fn k_ref_of_lower(m: &str) -> f64 {
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
/// 口径：`input_tokens` 为**含 cache 的总输入**（调用方传入 `final_input_tokens`，
/// cache_read / cache_creation 是其中的子集，`derive_report_cache_usage` 以
/// `input - read - creation` 得到未缓存部分），全部按输入全价计算。
/// 因此返回值是「假设完全没有缓存」的基线费用，`credits_saved =
/// estimated_cost × k_ref − credits_used` 依赖这一口径，**不要**在此扣减缓存部分。
/// Haiku 5.5 的 100K 阈值也据此按总输入判定，与官方「prompt 总长度」口径一致。
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

/// 判断模型是否属于 Claude 系列（排除 GPT / 第三方模型）。
fn is_claude_model(model_lower: &str) -> bool {
    model_lower.contains("claude")
        || model_lower.contains("sonnet")
        || model_lower.contains("opus")
        || model_lower.contains("haiku")
        || model_lower.contains("fable")
}

/// 获取 Claude 模型的 `(r_miss, r_out, read_savings)`：前两项单位 credits / MTok，
/// 第三项为缓存读取相对未命中单价的节省比例。
///
/// - `claude-haiku-5-5` 使用实测标定的 `<= 100K` / `> 100K` 双档参数；
/// - `claude-sonnet-5-5` 使用实测标定参数（见 `SONNET_55_CREDIT_RATES`）；
/// - 其余 Claude 模型沿用定价与历史 k_ref 派生，属于尚未实测标定的费率假设；
/// - 非 Claude 模型返回 `None`（跳过反推）。
fn get_claude_credit_rates(model: &str, input_tokens: i32) -> Option<(f64, f64, f64)> {
    let model_lower = model.to_lowercase();
    if !is_claude_model(&model_lower) {
        return None;
    }
    if is_haiku_55(&model_lower) {
        let (r_miss, r_out) = if input_tokens > HAIKU_55_LONG_CONTEXT_THRESHOLD {
            HAIKU_55_CREDIT_RATES_LONG_CONTEXT
        } else {
            HAIKU_55_CREDIT_RATES_STANDARD
        };
        Some((r_miss, r_out, CACHE_READ_SAVINGS_RATIO))
    } else if is_sonnet_55(&model_lower) {
        let (r_miss, r_out) = SONNET_55_CREDIT_RATES;
        Some((r_miss, r_out, SONNET_55_CACHE_READ_SAVINGS_RATIO))
    } else {
        // fable 仅反推路径按 opus 价，calculate_cost 口径保持不变
        let pricing = if model_lower.contains("fable") {
            OPUS_PRICING
        } else {
            pricing_of_lower(&model_lower)
        };
        let k_ref = k_ref_of_lower(&model_lower);
        Some((
            pricing.input_per_mtok * k_ref,
            pricing.output_per_mtok * k_ref,
            CACHE_READ_SAVINGS_RATIO,
        ))
    }
}

/// 由上游 `meteringEvent.usage`（`credits_used`）反推缓存读取 token 数。
///
/// 公式：`clamp(round((r_miss * I + r_out * O_billed - credits_used * 1e6) / (s * r_miss)), 0, I)`，
/// `s` 为缓存读取节省比例（默认 0.90，Sonnet 5.5 为实测 0.55）。
/// 仅对 Claude 系列模型且 `input_tokens >= 4096`、`credits_used > 0` 生效。
pub(crate) fn infer_cache_read_from_credits(
    model: &str,
    input_tokens: i32,
    billed_output_tokens: i32,
    credits_used: f64,
) -> Option<i32> {
    if input_tokens < MIN_INFER_INPUT_TOKENS || !credits_used.is_finite() || credits_used <= 0.0 {
        return None;
    }
    let (r_miss, r_out, read_savings) = get_claude_credit_rates(model, input_tokens)?;
    let input = input_tokens as f64;
    let output = billed_output_tokens.max(0) as f64;
    let expected_no_cache_micro = r_miss * input + r_out * output;
    let actual_micro = credits_used * 1_000_000.0;
    let saved_micro = expected_no_cache_micro - actual_micro;
    let raw_read = (saved_micro / (read_savings * r_miss)).round() as i64;
    Some(raw_read.clamp(0, input_tokens as i64) as i32)
}

/// 结合静态前缀上界与 `creditsUsed` 反推值校准最终 `cache_read_input_tokens`。
///
/// - `prefix_estimated_tokens <= 0`（首轮或 `history[0]` 重置）时锁定为 `0`；
/// - 满足反推条件时取 `min(prefix_estimated_tokens, inferred_read, final_input_tokens)`；
/// - 其余情况（`< 4096`、无 `credits_used`、非 Claude 模型）保留 `min(prefix_estimated_tokens, final_input_tokens)`。
pub(crate) fn refine_prefix_cache_read(
    model: &str,
    prefix_estimated_tokens: i32,
    final_input_tokens: i32,
    billed_output_tokens: i32,
    credits_used: Option<f64>,
) -> i32 {
    if prefix_estimated_tokens <= 0 || final_input_tokens <= 0 {
        return 0;
    }
    let capped_prefix = prefix_estimated_tokens.min(final_input_tokens);
    match credits_used.and_then(|cu| {
        infer_cache_read_from_credits(model, final_input_tokens, billed_output_tokens, cu)
    }) {
        Some(inferred) => capped_prefix.min(inferred),
        None => capped_prefix,
    }
}
