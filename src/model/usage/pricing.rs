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
/// 仅服务美元定价路径；credits 反推（`get_claude_credit_rates`）已直接按 `map_model`
/// 映射结果查表，不经过本函数。
fn is_haiku_55(model_lower: &str) -> bool {
    model_lower.replace(['.', ' '], "-").contains("haiku-5-5")
}

/// Haiku 5.5 长上下文阈值（tokens）。
///
/// 官方口径：单次请求 prompt 超过 100K 时，**整个请求**按 5 倍高档计费
/// （非超出部分累进）。低档 $0.10/$0.50，高档 $0.50/$2.50。
const HAIKU_55_LONG_CONTEXT_THRESHOLD: i32 = 100_000;

/// 触发 `creditsUsed` 缓存读取反推的最小输入 token 阈值。
///
/// 输入过短时，输入积分占总 `creditsUsed` 比重过小，输出分词微差除以较小的
/// `r_miss - r_cached` 会被放大，故低于 4096 tokens 直接保留静态前缀估算。
pub(crate) const MIN_INFER_INPUT_TOKENS: i32 = 4096;

/// Opus 4.5+ 定价：$5 / $25。
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
/// 仅供 usage 报表 credits_saved（estimated_cost × k_ref - credits_used）使用；
/// 缓存读取反推已改用 `get_claude_credit_rates` 的实测费率，不再依赖本值。
/// 2026-06-30 重校：按 opus 版本分档，基于实测 d=0.50 缓存折扣反推。
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

/// 单个模型的 Kiro credits 实测费率（credits / MTok，token 按代理的 cl100k 估算口径）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CreditRates {
    /// 未命中缓存的输入单价
    pub r_miss: f64,
    /// 命中缓存的输入单价
    pub r_cached: f64,
    /// 输出单价（含客户端可见的 thinking）
    pub r_out: f64,
}

const fn rates(r_miss: f64, r_cached: f64, r_out: f64) -> CreditRates {
    CreditRates {
        r_miss,
        r_cached,
        r_out,
    }
}

// 2026-11-01 受控实验实测（单账号、关闭 thinking、每模型独立会话）：
// 输入为仓库 Rust 源码 + 中英文文档（贴近 Claude Code 实际内容），约 18K–250K token。
// - r_miss：同会话首个请求（冷启动）的 credits / 输入，2–5 个样本取均值；
// - r_cached：同一请求原样重发（全量命中）的 credits / 输入；
// - r_out：前缀已缓存时让模型复述约 1000 token 代码 / 中文、写 600 词短文或数数，
//   扣除缓存部分后除以输出 token。中英文、代码输出单价一致。
// 同一模型不同内容样本的单价波动约 ±3–6%（Kiro 分词与 cl100k 的比例随内容变化），
// 折算到反推出的命中量约 ±15%（5.5 之前的模型缓存只省一半，误差放大更多）。
// 规律：5.5 之前的模型缓存价恒为未命中价的 52.8%（只省约 47%），Sonnet / Opus 5.5 约 27%，
// Haiku 5.5 约 10%；输出价约为未命中输入价的 29–33 倍（Sonnet / Opus 5.5 约 15 倍，
// Haiku 5.5 约 5 倍）。早前按「美元定价 × k_ref」派生并统一假设缓存省 90%，与实测相差
// 1.1–2.9 倍（输入）和 7–9 倍（输出），导致反推的缓存读取大面积为 0。
// 实测 Sonnet 5.5 在 250K 输入时仍为同一档；其余模型未测 >200K。
const HAIKU_45: CreditRates = rates(1.735, 0.916, 55.6);
/// Haiku 5.5 ≤ 100K：缓存价为未命中的 10.4%（与 07be80b4 标定的 1/10 一致）；
/// 输出 9.0 低于 07be80b4 由真实流量回归得到的 19.19，后者混入了部分未命中的成本
const HAIKU_55: CreditRates = rates(1.78, 0.186, 9.0);
/// Haiku 5.5 > 100K：整档约 5 倍（实测 124K 冷启动 8.757、重发 0.885；输出按 5 倍推算，未单独测）
const HAIKU_55_LONG_CONTEXT: CreditRates = rates(8.757, 0.885, 45.0);
const SONNET_45: CreditRates = rates(5.684, 3.002, 165.8);
const SONNET_46: CreditRates = rates(5.613, 2.963, 164.7);
const SONNET_5: CreditRates = rates(6.295, 3.322, 186.1);
/// Sonnet 5.5：实测 250K 输入仍为同一档（无长上下文加价）
const SONNET_55: CreditRates = rates(12.626, 3.370, 192.4);
const OPUS_45: CreditRates = rates(9.344, 4.932, 278.7);
const OPUS_46: CreditRates = rates(9.558, 5.105, 279.3);
const OPUS_47: CreditRates = rates(11.409, 6.028, 329.1);
const OPUS_48: CreditRates = rates(10.729, 5.661, 326.6);
const OPUS_5: CreditRates = rates(10.706, 5.649, 315.2);
const OPUS_55: CreditRates = rates(19.286, 5.235, 296.7);

/// 按上游实际使用的 Kiro 模型（`map_model` 映射结果）取实测费率。
///
/// 未实测的模型（`claude-sonnet-4`、`claude-fable-5`）与非 Claude 模型返回 `None`，
/// 跳过反推、保留静态前缀估算。新模型实测后在此追加分支。
pub(crate) fn get_claude_credit_rates(model: &str, input_tokens: i32) -> Option<CreditRates> {
    let kiro_model = crate::anthropic::converter::map_model(model)?;
    Some(match kiro_model.as_str() {
        "claude-haiku-4.5" => HAIKU_45,
        "claude-haiku-5.5" if input_tokens > HAIKU_55_LONG_CONTEXT_THRESHOLD => {
            HAIKU_55_LONG_CONTEXT
        }
        "claude-haiku-5.5" => HAIKU_55,
        "claude-sonnet-4.5" => SONNET_45,
        "claude-sonnet-4.6" => SONNET_46,
        "claude-sonnet-5" => SONNET_5,
        "claude-sonnet-5.5" => SONNET_55,
        "claude-opus-4.5" => OPUS_45,
        "claude-opus-4.6" => OPUS_46,
        "claude-opus-4.7" => OPUS_47,
        "claude-opus-4.8" => OPUS_48,
        "claude-opus-5" => OPUS_5,
        "claude-opus-5.5" => OPUS_55,
        _ => return None,
    })
}

/// 由上游 `meteringEvent.usage`（`credits_used`）反推缓存读取 token 数。
///
/// 设命中 `R` 个 token：`credits = r_miss·(I−R) + r_cached·R + r_out·O`，解得
/// `R = clamp(round((r_miss·I + r_out·O − credits·1e6) / (r_miss − r_cached)), 0, I)`。
/// 仅对 `get_claude_credit_rates` 已实测的模型且 `input_tokens >= 4096`、`credits_used > 0` 生效。
pub(crate) fn infer_cache_read_from_credits(
    model: &str,
    input_tokens: i32,
    billed_output_tokens: i32,
    credits_used: f64,
) -> Option<i32> {
    if input_tokens < MIN_INFER_INPUT_TOKENS || !credits_used.is_finite() || credits_used <= 0.0 {
        return None;
    }
    let CreditRates {
        r_miss,
        r_cached,
        r_out,
    } = get_claude_credit_rates(model, input_tokens)?;
    let input = input_tokens as f64;
    let output = billed_output_tokens.max(0) as f64;
    let expected_no_cache_micro = r_miss * input + r_out * output;
    let actual_micro = credits_used * 1_000_000.0;
    let saved_micro = expected_no_cache_micro - actual_micro;
    let raw_read = (saved_micro / (r_miss - r_cached)).round() as i64;
    Some(raw_read.clamp(0, input_tokens as i64) as i32)
}

/// 测试辅助：按实测费率构造「命中 `cache_read` 个 token」时上游应扣的 credits。
#[cfg(test)]
pub(crate) fn credits_for_cache_read(
    model: &str,
    input_tokens: i32,
    billed_output_tokens: i32,
    cache_read: i32,
) -> f64 {
    let r = get_claude_credit_rates(model, input_tokens).expect("测试只用于已实测模型");
    (r.r_miss * f64::from(input_tokens - cache_read)
        + r.r_cached * f64::from(cache_read)
        + r.r_out * f64::from(billed_output_tokens))
        / 1_000_000.0
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
