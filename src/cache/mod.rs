// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Prompt Cache 模块
//!
//! - `simulation` - 三角分布与比例模式模拟（旧 cache.rs 内容）
//! - `fingerprint` - 账号级前缀指纹追踪（替代末层兜底）
//!
//! 公共 API 保持 `crate::cache::PromptCacheUsage` 路径不变。

pub mod fingerprint;
pub mod simulation;

pub use simulation::{
    CacheSimulationRatioConfig, PromptCacheUsage, split_creation_by_ephemeral_ratio,
};
#[allow(unused_imports)]
pub use simulation::{
    DEFAULT_CACHE_SIMULATION_RATIO_FOCUS_PROBABILITY, DEFAULT_CACHE_SIMULATION_RATIO_FOCUS_RADIUS,
};

/// 四层降级链选择终值 usage：
///
/// 优先级（高→低）：
/// 1. **metering 真值**：上游 Kiro 返回的 cache_read / cache_creation 原始值
/// 2. **credits 反推结果**：调用方提前用 `infer_cache_read_tokens` 反推后的 cache_read
/// 3. **fingerprint 命中**：账号级前缀指纹追踪输出
/// 4. **ratio 兜底**：比例模拟（`from_ratio_config`）的产出
///
/// 所有分支输出均经 `clamp_to_total(final_input_tokens)` 截断，保证 5m/1h 不变性。
pub fn select_final_usage(
    final_input_tokens: i32,
    metering: Option<(i32, i32)>,
    credits_inferred_read: Option<i32>,
    fingerprint_usage: Option<PromptCacheUsage>,
    ratio_fallback: PromptCacheUsage,
) -> PromptCacheUsage {
    if let Some((read, creation)) = metering {
        // Kiro metering 不返回 5m/1h 拆分，默认全部归为 5m
        return PromptCacheUsage {
            input_tokens: final_input_tokens
                .saturating_sub(read)
                .saturating_sub(creation),
            cache_creation_input_tokens: creation,
            cache_read_input_tokens: read,
            cache_creation_5m_input_tokens: creation,
            cache_creation_1h_input_tokens: 0,
        }
        .clamp_to_total(final_input_tokens);
    }
    if let Some(inferred) = credits_inferred_read {
        return PromptCacheUsage {
            input_tokens: final_input_tokens.saturating_sub(inferred),
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: inferred,
            cache_creation_5m_input_tokens: 0,
            cache_creation_1h_input_tokens: 0,
        }
        .clamp_to_total(final_input_tokens);
    }
    if let Some(fp) = fingerprint_usage {
        return fp.clamp_to_total(final_input_tokens);
    }
    ratio_fallback.clamp_to_total(final_input_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ratio_fallback(total: i32) -> PromptCacheUsage {
        // 模拟 from_ratios 的产出：50% 缓存，其中 30% creation
        let cached = ((total as f64) * 0.5) as i32;
        let creation = ((cached as f64) * 0.3) as i32;
        let read = cached - creation;
        PromptCacheUsage {
            input_tokens: total - cached,
            cache_creation_input_tokens: creation,
            cache_read_input_tokens: read,
            cache_creation_5m_input_tokens: creation,
            cache_creation_1h_input_tokens: 0,
        }
    }

    fn invariant_holds(u: PromptCacheUsage, total: i32) -> bool {
        u.input_tokens >= 0
            && u.cache_creation_5m_input_tokens + u.cache_creation_1h_input_tokens
                == u.cache_creation_input_tokens
            && u.cache_read_input_tokens + u.cache_creation_input_tokens <= total
            && u.total_input_tokens() == total
    }

    #[test]
    fn layer1_metering_wins_over_all() {
        let total = 1000;
        let metering = Some((600, 200));
        let credits = Some(500); // 应被忽略
        let fp = Some(PromptCacheUsage {
            input_tokens: 0,
            cache_creation_input_tokens: 100,
            cache_read_input_tokens: 900,
            cache_creation_5m_input_tokens: 100,
            cache_creation_1h_input_tokens: 0,
        });
        let final_u = select_final_usage(total, metering, credits, fp, ratio_fallback(total));
        assert_eq!(final_u.cache_read_input_tokens, 600);
        assert_eq!(final_u.cache_creation_input_tokens, 200);
        assert_eq!(final_u.input_tokens, 200);
        assert!(invariant_holds(final_u, total));
    }

    #[test]
    fn layer2_credits_wins_when_metering_absent() {
        let total = 1000;
        let credits = Some(400);
        let fp = Some(PromptCacheUsage {
            input_tokens: 0,
            cache_creation_input_tokens: 100,
            cache_read_input_tokens: 900,
            cache_creation_5m_input_tokens: 100,
            cache_creation_1h_input_tokens: 0,
        });
        let final_u = select_final_usage(total, None, credits, fp, ratio_fallback(total));
        assert_eq!(final_u.cache_read_input_tokens, 400);
        assert_eq!(final_u.cache_creation_input_tokens, 0);
        assert_eq!(final_u.input_tokens, 600);
        assert!(invariant_holds(final_u, total));
    }

    #[test]
    fn layer3_fingerprint_wins_when_metering_and_credits_absent() {
        let total = 1000;
        let fp = Some(PromptCacheUsage {
            input_tokens: 200,
            cache_creation_input_tokens: 100,
            cache_read_input_tokens: 700,
            cache_creation_5m_input_tokens: 70,
            cache_creation_1h_input_tokens: 30,
        });
        let final_u = select_final_usage(total, None, None, fp, ratio_fallback(total));
        assert_eq!(final_u.cache_read_input_tokens, 700);
        assert_eq!(final_u.cache_creation_input_tokens, 100);
        assert_eq!(final_u.cache_creation_1h_input_tokens, 30);
        assert!(invariant_holds(final_u, total));
    }

    #[test]
    fn layer4_ratio_fallback_when_all_higher_absent() {
        let total = 1000;
        let fallback = ratio_fallback(total);
        let final_u = select_final_usage(total, None, None, None, fallback);
        assert_eq!(
            final_u.cache_read_input_tokens,
            fallback.cache_read_input_tokens
        );
        assert_eq!(
            final_u.cache_creation_input_tokens,
            fallback.cache_creation_input_tokens
        );
        assert!(invariant_holds(final_u, total));
    }

    #[test]
    fn metering_over_total_is_clamped() {
        // metering 数值大于 total，需被截断
        let total = 100;
        let metering = Some((80, 50)); // 80+50 = 130 > 100
        let final_u = select_final_usage(total, metering, None, None, ratio_fallback(total));
        // cache_read 优先保留：80，剩余 20 全给 creation
        assert_eq!(final_u.cache_read_input_tokens, 80);
        assert_eq!(final_u.cache_creation_input_tokens, 20);
        assert_eq!(final_u.input_tokens, 0);
        assert!(invariant_holds(final_u, total));
    }

    #[test]
    fn credits_inferred_zero_is_valid_layer2() {
        // credits 反推为 0（baseline ≤ credits） — 仍走 Layer 2，cache_read = 0
        let total = 1000;
        let final_u = select_final_usage(total, None, Some(0), None, ratio_fallback(total));
        assert_eq!(final_u.cache_read_input_tokens, 0);
        assert_eq!(final_u.cache_creation_input_tokens, 0);
        assert_eq!(final_u.input_tokens, 1000);
        assert!(invariant_holds(final_u, total));
    }

    #[test]
    fn fingerprint_with_5m_1h_ratio_preserved() {
        // 指纹层输出含 5m/1h 拆分，clamp 后比例需保持
        let total = 1000;
        let fp = Some(PromptCacheUsage {
            input_tokens: 0,
            cache_creation_input_tokens: 200,
            cache_read_input_tokens: 800,
            cache_creation_5m_input_tokens: 140,
            cache_creation_1h_input_tokens: 60,
        });
        let final_u = select_final_usage(total, None, None, fp, ratio_fallback(total));
        // 1h 比例 60/200 = 0.3 保持
        assert_eq!(final_u.cache_creation_1h_input_tokens, 60);
        assert_eq!(final_u.cache_creation_5m_input_tokens, 140);
        assert!(invariant_holds(final_u, total));
    }
}
