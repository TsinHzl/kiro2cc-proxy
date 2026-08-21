// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! OpenAI 客户端模型名 → Kiro 上游模型名映射
//!
//! Codex CLI 只会请求 OpenAI 自家的模型名（`gpt-5-codex` 等），而 Kiro 上游提供的是
//! `gpt-5.6-sol` / `gpt-5.6-terra` / `gpt-5.6-luna`（见 `crate::anthropic::handlers` 的
//! `build_model_list()`）。本模块负责把前者翻译成后者。
//!
//! 映射只作用于**请求方向**；响应体的 `model` 字段必须回写客户端请求的原始名，
//! 否则 Codex 会因请求/响应模型名不一致而告警。

/// Kiro 上游的默认 GPT 模型（未知 `gpt-*` 的兜底目标）
const DEFAULT_GPT_MODEL: &str = "gpt-5.6-terra";

/// Kiro 上游的高配 GPT 模型（codex-max 系映射目标）
const MAX_GPT_MODEL: &str = "gpt-5.6-luna";

/// Kiro 原生 GPT 模型的公共前缀
const KIRO_GPT_PREFIX: &str = "gpt-5.6-";

/// 把客户端请求的模型名映射为 Kiro 上游可识别的模型名。
///
/// 规则按优先级从上到下匹配（首个命中即返回）：
///
/// | 客户端模型名 | Kiro 模型名 |
/// |---|---|
/// | `gpt-5-codex`、`gpt-5.1-codex` | `gpt-5.6-terra` |
/// | `gpt-5.1-codex-max` | `gpt-5.6-luna` |
/// | `gpt-5.6-*`（`sol` / `terra` / `luna` 及上游未来新增型号） | 原样透传 |
/// | `claude-*` | 原样透传 |
/// | 其余 `gpt-*` | `gpt-5.6-terra`（兜底，记 WARN） |
/// | 其他（`deepseek-*`、`auto` 等） | 原样透传 |
///
/// `gpt-5.6-*` 用**前缀**而非枚举匹配，这样上游新增型号时无需改代码即自动透传，
/// 不会被兜底分支静默改写。
///
/// # 已知限制
///
/// `gpt-5.6-luna` 上游恒返回 `thinking=0`，故 `reasoning.effort` 对其无实际效果；
/// `gpt-5.6-*` 的真实上下文窗口尚未标定（`context_window_for_model` 对所有模型硬返回 1M）。
pub(crate) fn map_model(client_model: &str) -> &str {
    match client_model {
        "gpt-5-codex" | "gpt-5.1-codex" => DEFAULT_GPT_MODEL,
        "gpt-5.1-codex-max" => MAX_GPT_MODEL,
        // Kiro 原生 GPT 模型：原样透传
        m if m.starts_with(KIRO_GPT_PREFIX) => m,
        // Claude 系：原样透传（走既有 Anthropic 链路）
        m if m.starts_with("claude-") => m,
        // 未知 gpt 型号：兜底到默认 GPT 模型，留痕以便发现新型号
        m if m.starts_with("gpt-") => {
            tracing::warn!(
                requested_model = %m,
                fallback_model = %DEFAULT_GPT_MODEL,
                "未识别的 gpt 模型名，已兜底映射"
            );
            DEFAULT_GPT_MODEL
        }
        // 其余模型名（deepseek/minimax/glm/qwen/auto 等）原样透传给上游判断
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_models_map_to_terra() {
        assert_eq!(map_model("gpt-5-codex"), "gpt-5.6-terra");
        assert_eq!(map_model("gpt-5.1-codex"), "gpt-5.6-terra");
    }

    #[test]
    fn codex_max_maps_to_luna() {
        assert_eq!(map_model("gpt-5.1-codex-max"), "gpt-5.6-luna");
    }

    #[test]
    fn kiro_native_gpt_models_pass_through() {
        // 上游现有三款，全部必须原样透传（尤其 sol 不能被兜底改写）
        assert_eq!(map_model("gpt-5.6-sol"), "gpt-5.6-sol");
        assert_eq!(map_model("gpt-5.6-terra"), "gpt-5.6-terra");
        assert_eq!(map_model("gpt-5.6-luna"), "gpt-5.6-luna");
    }

    #[test]
    fn future_kiro_gpt_models_pass_through() {
        // 前缀规则保证上游新增型号自动透传，不落进兜底分支
        assert_eq!(map_model("gpt-5.6-nova"), "gpt-5.6-nova");
    }

    #[test]
    fn claude_models_pass_through() {
        assert_eq!(map_model("claude-opus-4-7"), "claude-opus-4-7");
        assert_eq!(map_model("claude-sonnet-4-5"), "claude-sonnet-4-5");
    }

    #[test]
    fn unknown_gpt_falls_back_to_terra() {
        assert_eq!(map_model("gpt-4o"), "gpt-5.6-terra");
        assert_eq!(map_model("gpt-3.5-turbo"), "gpt-5.6-terra");
    }

    #[test]
    fn non_gpt_non_claude_pass_through() {
        assert_eq!(map_model("deepseek-v3.2"), "deepseek-v3.2");
        assert_eq!(map_model("auto"), "auto");
        assert_eq!(map_model("minimax-m2.5"), "minimax-m2.5");
    }

    #[test]
    fn empty_string_passes_through() {
        // 空模型名不属于任何前缀分支，透传给下游做校验并返回其标准错误
        assert_eq!(map_model(""), "");
    }
}
