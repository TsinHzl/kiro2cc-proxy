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

// fingerprint 模块内容由 C2-C8 填充
