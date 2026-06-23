// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Prompt Cache 模拟模块
//!
//! 实现比例模式的 prompt cache usage 模拟，使上报的 usage 字段
//! 包含 cache_creation_input_tokens 和 cache_read_input_tokens。

use serde::{Deserialize, Serialize};

/// 比例模式默认核心集中半径：峰值前后 5 个百分点。
#[allow(dead_code)]
pub const DEFAULT_CACHE_SIMULATION_RATIO_FOCUS_RADIUS: f64 = 0.05;

/// 比例模式默认核心集中概率：至少大部分请求落在核心区间内。
#[allow(dead_code)]
pub const DEFAULT_CACHE_SIMULATION_RATIO_FOCUS_PROBABILITY: f64 = 0.8;

/// 固定比例模式的随机比例配置。
///
/// 使用两层三角分布采样：大概率落在 `peak_ratio ± focus_radius`
/// 的核心区间内，小概率落在完整 `[min_ratio, max_ratio]` 内。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CacheSimulationRatioConfig {
    pub min_ratio: f64,
    pub max_ratio: f64,
    pub peak_ratio: f64,
    pub focus_radius: f64,
    pub focus_probability: f64,
}

impl CacheSimulationRatioConfig {
    pub fn fixed(ratio: f64) -> Self {
        let ratio = if ratio.is_finite() {
            ratio.clamp(0.0, 1.0)
        } else {
            0.0
        };

        Self {
            min_ratio: ratio,
            max_ratio: ratio,
            peak_ratio: ratio,
            focus_radius: 0.0,
            focus_probability: 1.0,
        }
    }

    #[allow(dead_code)]
    pub fn new(min_ratio: f64, max_ratio: f64, peak_ratio: f64) -> anyhow::Result<Self> {
        Self::with_focus(
            min_ratio,
            max_ratio,
            peak_ratio,
            DEFAULT_CACHE_SIMULATION_RATIO_FOCUS_RADIUS,
            DEFAULT_CACHE_SIMULATION_RATIO_FOCUS_PROBABILITY,
        )
    }

    #[allow(dead_code)]
    pub fn with_focus(
        min_ratio: f64,
        max_ratio: f64,
        peak_ratio: f64,
        focus_radius: f64,
        focus_probability: f64,
    ) -> anyhow::Result<Self> {
        let config = Self {
            min_ratio,
            max_ratio,
            peak_ratio,
            focus_radius,
            focus_probability,
        };
        config.validate()?;
        Ok(config)
    }

    #[allow(dead_code)]
    pub fn validate(self) -> anyhow::Result<()> {
        if !self.min_ratio.is_finite()
            || !self.max_ratio.is_finite()
            || !self.peak_ratio.is_finite()
            || !self.focus_radius.is_finite()
            || !self.focus_probability.is_finite()
        {
            anyhow::bail!("缓存模拟比例必须是有限数字");
        }

        for (name, ratio) in [
            ("minRatio", self.min_ratio),
            ("maxRatio", self.max_ratio),
            ("peakRatio", self.peak_ratio),
        ] {
            if ratio <= 0.0 || ratio > 1.0 {
                anyhow::bail!("{} 必须大于 0.0 且不超过 1.0，当前值: {}", name, ratio);
            }
        }

        if self.focus_radius <= 0.0 || self.focus_radius > 1.0 {
            anyhow::bail!(
                "focusRadius 必须大于 0.0 且不超过 1.0，当前值: {}",
                self.focus_radius
            );
        }

        if self.focus_probability <= 0.0 || self.focus_probability > 1.0 {
            anyhow::bail!(
                "focusProbability 必须大于 0.0 且不超过 1.0，当前值: {}",
                self.focus_probability
            );
        }

        if self.min_ratio > self.max_ratio {
            anyhow::bail!(
                "minRatio 不能大于 maxRatio，当前值: {} > {}",
                self.min_ratio,
                self.max_ratio
            );
        }

        if self.peak_ratio < self.min_ratio || self.peak_ratio > self.max_ratio {
            anyhow::bail!(
                "peakRatio 必须位于 minRatio 和 maxRatio 之间，当前值: {} 不在 {} ~ {} 内",
                self.peak_ratio,
                self.min_ratio,
                self.max_ratio
            );
        }

        Ok(())
    }

    pub fn is_fixed(self) -> bool {
        (self.min_ratio - self.max_ratio).abs() <= f64::EPSILON
    }

    pub fn sample_ratio(self) -> f64 {
        if self.is_fixed() {
            return self.peak_ratio;
        }

        let use_focus_band = self.focus_radius > 0.0 && fastrand::f64() < self.focus_probability;
        if use_focus_band {
            let min = self.min_ratio.max(self.peak_ratio - self.focus_radius);
            let max = self.max_ratio.min(self.peak_ratio + self.focus_radius);
            return sample_triangular_ratio(min, max, self.peak_ratio);
        }

        sample_triangular_ratio(self.min_ratio, self.max_ratio, self.peak_ratio)
    }
}

fn sample_triangular_ratio(min: f64, max: f64, peak: f64) -> f64 {
    if (min - max).abs() <= f64::EPSILON {
        return peak.clamp(min, max);
    }

    let peak = peak.clamp(min, max);
    let span = max - min;
    let split = (peak - min) / span;
    let u = fastrand::f64();

    let sampled = if u < split {
        min + (u * span * (peak - min)).sqrt()
    } else {
        max - ((1.0 - u) * span * (max - peak)).sqrt()
    };

    sampled.clamp(min, max)
}

/// 模拟出的 Anthropic prompt cache usage 字段。
///
/// 不变性：`cache_creation_5m_input_tokens + cache_creation_1h_input_tokens == cache_creation_input_tokens`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PromptCacheUsage {
    pub input_tokens: i32,
    pub cache_creation_input_tokens: i32,
    pub cache_read_input_tokens: i32,
    pub cache_creation_5m_input_tokens: i32,
    pub cache_creation_1h_input_tokens: i32,
}

/// 按 ephemeral1hRatio 拆分 cache_creation 到 5m / 1h tier（确定性分配）。
pub fn split_creation_by_ephemeral_ratio(creation: i32, ratio_1h: f64) -> (i32, i32) {
    let ratio = ratio_1h.clamp(0.0, 1.0);
    let one_h = ((creation as f64 * ratio) + 0.5).floor() as i32;
    let one_h = one_h.clamp(0, creation.max(0));
    let five_m = creation.saturating_sub(one_h);
    (five_m, one_h)
}

impl PromptCacheUsage {
    pub fn uncached(input_tokens: i32) -> Self {
        Self {
            input_tokens: input_tokens.max(0),
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            cache_creation_5m_input_tokens: 0,
            cache_creation_1h_input_tokens: 0,
        }
    }

    pub fn from_ratios(
        input_tokens: i32,
        cache_simulation_ratio: f64,
        cache_creation_ratio: f64,
    ) -> Self {
        Self::from_ratios_with_ephemeral(
            input_tokens,
            cache_simulation_ratio,
            cache_creation_ratio,
            0.0,
        )
    }

    pub fn from_ratios_with_ephemeral(
        input_tokens: i32,
        cache_simulation_ratio: f64,
        cache_creation_ratio: f64,
        ephemeral_1h_ratio: f64,
    ) -> Self {
        let cached_total = ((input_tokens as f64) * cache_simulation_ratio.clamp(0.0, 1.0)) as i32;
        let cache_creation = ((cached_total as f64) * cache_creation_ratio.clamp(0.0, 1.0)) as i32;
        let cache_read = cached_total.saturating_sub(cache_creation);
        let (creation_5m, creation_1h) =
            split_creation_by_ephemeral_ratio(cache_creation, ephemeral_1h_ratio);
        Self {
            input_tokens: input_tokens.saturating_sub(cached_total),
            cache_creation_input_tokens: cache_creation,
            cache_read_input_tokens: cache_read,
            cache_creation_5m_input_tokens: creation_5m,
            cache_creation_1h_input_tokens: creation_1h,
        }
    }

    pub fn from_ratio_config(
        input_tokens: i32,
        cache_simulation_ratio: CacheSimulationRatioConfig,
        cache_creation_ratio: f64,
    ) -> Self {
        Self::from_ratios(
            input_tokens,
            cache_simulation_ratio.sample_ratio(),
            cache_creation_ratio,
        )
    }

    #[allow(dead_code)]
    pub fn from_ratio_config_with_ephemeral(
        input_tokens: i32,
        cache_simulation_ratio: CacheSimulationRatioConfig,
        cache_creation_ratio: f64,
        ephemeral_1h_ratio: f64,
    ) -> Self {
        Self::from_ratios_with_ephemeral(
            input_tokens,
            cache_simulation_ratio.sample_ratio(),
            cache_creation_ratio,
            ephemeral_1h_ratio,
        )
    }

    pub fn total_input_tokens(self) -> i32 {
        self.input_tokens
            .saturating_add(self.cache_creation_input_tokens)
            .saturating_add(self.cache_read_input_tokens)
    }

    pub fn scale_to(self, total_input_tokens: i32) -> Self {
        let old_total = self.total_input_tokens();
        if old_total <= 0 {
            return Self::uncached(total_input_tokens);
        }
        if old_total == total_input_tokens {
            return self;
        }

        let scale = total_input_tokens as f64 / old_total as f64;
        let mut cache_read = ((self.cache_read_input_tokens as f64) * scale).round() as i32;
        let mut cache_creation = ((self.cache_creation_input_tokens as f64) * scale).round() as i32;

        cache_read = cache_read.clamp(0, total_input_tokens.max(0));
        cache_creation = cache_creation.clamp(0, total_input_tokens.saturating_sub(cache_read));

        // 按原 5m/1h 比例同步缩放，保持 5m + 1h == creation 不变性
        let (creation_5m, creation_1h) = if self.cache_creation_input_tokens > 0 {
            let ratio_1h = self.cache_creation_1h_input_tokens as f64
                / self.cache_creation_input_tokens as f64;
            split_creation_by_ephemeral_ratio(cache_creation, ratio_1h)
        } else {
            (0, 0)
        };

        Self {
            input_tokens: total_input_tokens
                .saturating_sub(cache_read)
                .saturating_sub(cache_creation),
            cache_creation_input_tokens: cache_creation,
            cache_read_input_tokens: cache_read,
            cache_creation_5m_input_tokens: creation_5m,
            cache_creation_1h_input_tokens: creation_1h,
        }
    }

    /// 强制截断保证 `cache_read + cache_creation <= total_input`、`input_tokens >= 0`。
    /// 截断时优先保留 cache_read。
    pub fn clamp_to_total(self, total_input: i32) -> Self {
        let total = total_input.max(0);
        let cache_read = self.cache_read_input_tokens.clamp(0, total);
        let remaining = total.saturating_sub(cache_read);
        let cache_creation = self.cache_creation_input_tokens.clamp(0, remaining);
        let input_tokens = remaining.saturating_sub(cache_creation);

        // 按原比例同步缩放 5m/1h
        let (creation_5m, creation_1h) = if self.cache_creation_input_tokens > 0 {
            let ratio_1h = self.cache_creation_1h_input_tokens as f64
                / self.cache_creation_input_tokens as f64;
            split_creation_by_ephemeral_ratio(cache_creation, ratio_1h)
        } else {
            (0, 0)
        };

        Self {
            input_tokens,
            cache_creation_input_tokens: cache_creation,
            cache_read_input_tokens: cache_read,
            cache_creation_5m_input_tokens: creation_5m,
            cache_creation_1h_input_tokens: creation_1h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: i32, creation: i32, read: i32, c_5m: i32, c_1h: i32) -> PromptCacheUsage {
        PromptCacheUsage {
            input_tokens: input,
            cache_creation_input_tokens: creation,
            cache_read_input_tokens: read,
            cache_creation_5m_input_tokens: c_5m,
            cache_creation_1h_input_tokens: c_1h,
        }
    }

    fn assert_invariant(u: PromptCacheUsage) {
        assert!(u.input_tokens >= 0, "input_tokens 不可为负: {:?}", u);
        assert!(
            u.cache_creation_input_tokens
                == u.cache_creation_5m_input_tokens + u.cache_creation_1h_input_tokens,
            "5m + 1h 不等于 creation: {:?}",
            u
        );
        assert!(
            u.cache_read_input_tokens + u.cache_creation_input_tokens <= u.total_input_tokens(),
            "read+creation 不应超 total: {:?}",
            u
        );
    }

    #[test]
    fn scale_to_keeps_5m_1h_ratio_when_scaling_up() {
        // 原始：creation=100 (5m=70, 1h=30)，read=50，input=50，total=200
        let u = usage(50, 100, 50, 70, 30);
        // 放大到 total=400 → scale=2.0：creation≈200 (5m≈140, 1h≈60)，read≈100
        let scaled = u.scale_to(400);
        assert_eq!(scaled.total_input_tokens(), 400);
        assert_eq!(scaled.cache_read_input_tokens, 100);
        assert_eq!(scaled.cache_creation_input_tokens, 200);
        // 1h 比例为 30/100 = 0.3，缩放后 200 * 0.3 = 60
        assert_eq!(scaled.cache_creation_1h_input_tokens, 60);
        assert_eq!(scaled.cache_creation_5m_input_tokens, 140);
        assert_invariant(scaled);
    }

    #[test]
    fn scale_to_keeps_5m_1h_ratio_when_scaling_down() {
        // 原始：creation=200 (5m=140, 1h=60)，read=100，input=100，total=400
        let u = usage(100, 200, 100, 140, 60);
        let scaled = u.scale_to(200);
        assert_eq!(scaled.total_input_tokens(), 200);
        assert_eq!(scaled.cache_creation_input_tokens, 100);
        // 1h 比例 60/200 = 0.3 → 100*0.3 = 30
        assert_eq!(scaled.cache_creation_1h_input_tokens, 30);
        assert_eq!(scaled.cache_creation_5m_input_tokens, 70);
        assert_invariant(scaled);
    }

    #[test]
    fn scale_to_pure_5m_stays_pure_5m() {
        let u = usage(50, 100, 50, 100, 0);
        let scaled = u.scale_to(400);
        assert_eq!(scaled.cache_creation_1h_input_tokens, 0);
        assert_eq!(
            scaled.cache_creation_5m_input_tokens,
            scaled.cache_creation_input_tokens
        );
        assert_invariant(scaled);
    }

    #[test]
    fn scale_to_pure_1h_stays_pure_1h() {
        let u = usage(50, 100, 50, 0, 100);
        let scaled = u.scale_to(400);
        assert_eq!(scaled.cache_creation_5m_input_tokens, 0);
        assert_eq!(
            scaled.cache_creation_1h_input_tokens,
            scaled.cache_creation_input_tokens
        );
        assert_invariant(scaled);
    }

    #[test]
    fn scale_to_zero_creation_keeps_zero() {
        let u = usage(100, 0, 100, 0, 0);
        let scaled = u.scale_to(50);
        assert_eq!(scaled.cache_creation_input_tokens, 0);
        assert_eq!(scaled.cache_creation_5m_input_tokens, 0);
        assert_eq!(scaled.cache_creation_1h_input_tokens, 0);
        assert_invariant(scaled);
    }

    #[test]
    fn scale_to_zero_total_returns_uncached() {
        let u = usage(0, 0, 0, 0, 0);
        let scaled = u.scale_to(123);
        assert_eq!(scaled.total_input_tokens(), 123);
        assert_eq!(scaled.input_tokens, 123);
        assert_eq!(scaled.cache_creation_input_tokens, 0);
        assert_eq!(scaled.cache_read_input_tokens, 0);
        assert_invariant(scaled);
    }

    #[test]
    fn scale_to_same_total_is_identity() {
        let u = usage(50, 100, 50, 70, 30);
        let scaled = u.scale_to(200);
        assert_eq!(scaled, u);
    }

    #[test]
    fn clamp_to_total_keeps_5m_1h_ratio() {
        // 给定 read=100, creation=100 (5m=80, 1h=20)，total_input=150 → creation 截断到 50
        let u = usage(0, 100, 100, 80, 20);
        let clamped = u.clamp_to_total(150);
        assert_eq!(clamped.cache_read_input_tokens, 100);
        assert_eq!(clamped.cache_creation_input_tokens, 50);
        // 1h 比例 20/100 = 0.2 → 50*0.2 = 10
        assert_eq!(clamped.cache_creation_1h_input_tokens, 10);
        assert_eq!(clamped.cache_creation_5m_input_tokens, 40);
        assert_invariant(clamped);
    }

    #[test]
    fn clamp_to_total_zero_creation_keeps_zero_split() {
        let u = usage(0, 0, 80, 0, 0);
        let clamped = u.clamp_to_total(100);
        assert_eq!(clamped.cache_creation_5m_input_tokens, 0);
        assert_eq!(clamped.cache_creation_1h_input_tokens, 0);
        assert_invariant(clamped);
    }

    #[test]
    fn split_creation_by_ephemeral_ratio_boundaries() {
        assert_eq!(split_creation_by_ephemeral_ratio(100, 0.0), (100, 0));
        assert_eq!(split_creation_by_ephemeral_ratio(100, 1.0), (0, 100));
        assert_eq!(split_creation_by_ephemeral_ratio(100, 0.3), (70, 30));
        assert_eq!(split_creation_by_ephemeral_ratio(0, 0.5), (0, 0));
        // clamp 负数 ratio
        assert_eq!(split_creation_by_ephemeral_ratio(100, -0.5), (100, 0));
        // clamp > 1 ratio
        assert_eq!(split_creation_by_ephemeral_ratio(100, 1.5), (0, 100));
    }
}
