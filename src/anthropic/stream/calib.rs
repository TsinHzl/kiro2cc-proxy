//! 流式响应处理模块
//!
//! 实现 Kiro → Anthropic 流式响应转换和 SSE 状态管理
/// 所有模型统一按 100 万 token 上下文窗口计算。
///
/// 历史上按模型分支返回 200K/1M；本变更改为统一 1M，与"contextUsage 本地化"决策一致：
/// final_input_tokens 不再依赖 Kiro `contextUsageEvent` 反算；如需差异化窗口
/// 可恢复 match 分支。
pub(crate) fn context_window_for_model(_model: &str) -> i32 {
    1_000_000
}

/// 空响应判定为「上下文过大」的输入 token 阈值（取窗口的 28%）。
///
/// 实测 input≈297K 时上游已频繁返回空/极短响应（4~13 tokens 无工具调用），
/// 取窗口的 28%（≈280K for 1M 窗口）作为判定阈值。
pub(crate) fn empty_response_oversized_threshold(model: &str) -> i32 {
    (context_window_for_model(model) as f64 * 0.28) as i32
}

/// "近似空响应"的 output token 阈值。
///
/// 当上下文压力大时，模型可能返回极短的无意义文本（如 4~13 tokens）而非工具调用，
/// 导致客户端 agentic 循环卡住。output < 此阈值且无工具调用时，视为近似空响应。
pub(crate) const NEAR_EMPTY_OUTPUT_THRESHOLD: i32 = 30;

/// 返回给客户端的 token 类字段缩放系数。
///
/// 仅影响给客户端（如 Claude Code）看到的 usage.input_tokens / cache_* 字段。
/// 内部计费与 usage_tracker 入库仍写入真实值，admin/user UI 显示不受影响。
///
/// Claude Code 对所有模型（含 opus 系列）均按 200K 窗口计算 Ctx%，
/// 实测 82% 触发 auto-compact，即显示值 164,000。
/// 当前系数下的真实触发点：164,000 / 0.6657 ≈ 246,357。
///
/// 实测依据：
/// - Sonnet 5：Ctx 79% + "3% until auto-compact" → 触发线 82%
/// - Opus 5：显示 113.4K / Ctx 56%（113,400 / 200,000 = 56.7%，floor 56%）→ 窗口确认 200K
/// - Opus 4.7：显示 111.7K / Ctx 55%（111,700 / 200,000 = 55.85%，floor 55%）→ 窗口确认 200K
/// - Opus 4.8：显示 80.0K / Ctx 39%（舍入前略低于 80,000，/200,000 → floor 39%）→ 窗口确认 200K
/// - Opus 4.6：显示 71.2K / Ctx 35%（71,200 / 200,000 = 35.6%，floor 35%）→ 窗口确认 200K
///
/// 统一应用于所有模型，不再按窗口代际区分（原 opus-4.7/4.8/5 大窗口分支已合并）。
/// 注意：曾按 opus 官方 1M 窗口给其单独放大系数 3.3285，导致新窗口首次会话即
/// Ctx 100%，已回滚 —— Claude Code 对 opus 的 Ctx% 分母同为 200K，不是 1M。
const CLIENT_TOKEN_DISPLAY_SCALE: f64 = 0.6657;

/// Claude Code 计算 Ctx% 时假设的上下文窗口（分母）。
///
/// 与 `CLIENT_TOKEN_DISPLAY_SCALE` 同属「客户端展示口径」——客户端对所有模型
/// （含官方 1M 窗口的 opus 系列）均按 200K 算 Ctx%，实测依据见上方常量注释。
/// 超窗错误文案里的 maximum 必须取这个值，而不是 `context_window_for_model`：
/// 后者是上游真实窗口（1M），用它会让「N tokens > M maximum」里 N < M 自相矛盾。
pub(crate) const CLIENT_ASSUMED_CONTEXT_WINDOW: i32 = 200_000;

/// 对客户端展示用的 token 值缩放（向上取整保证非零）。
pub(crate) fn scale_for_client(n: i32, _model: &str) -> i32 {
    if n <= 0 {
        return n.max(0);
    }
    ((n as f64) * CLIENT_TOKEN_DISPLAY_SCALE).ceil() as i32
}

pub(crate) fn cap_input_tokens(
    context_input_tokens: i32,
    _local_estimate: i32,
    model: &str,
) -> i32 {
    let cap = context_window_for_model(model);
    context_input_tokens.clamp(1, cap)
}

pub fn cap_input_tokens_pub(context_input_tokens: i32, local_estimate: i32, model: &str) -> i32 {
    cap_input_tokens(context_input_tokens, local_estimate, model)
}
