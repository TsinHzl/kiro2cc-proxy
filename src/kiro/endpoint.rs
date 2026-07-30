// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Kiro 上游端点枚举与桶级 429 状态注册表。
//!
//! 4 个端点对应上游 4 个独立限流桶：
//! - `ide` → `q.{region}.amazonaws.com`，默认 CodeWhisperer 路由
//! - `runtime` → `runtime.{region}.kiro.dev`，与 `q.*` 域名独立
//! - `codewhisperer` → `codewhisperer.{region}.amazonaws.com`（us-east-1 独占主机）/ `q.{region}.amazonaws.com`（其它区域）
//! - `amazonq` → `q.{region}.amazonaws.com`，携带 `x-amz-target: AmazonQDeveloperStreamingService.SendMessage`
//!
//! 桶级 429 状态由 `EndpointBucketRegistry` 维护，按 `(credential_id, EndpointName)` 二元组隔离。

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 单次 429 后桶封禁时长（与 `provider::throttle_delay` 最大档对齐）
pub const BUCKET_THROTTLE_DURATION: Duration = Duration::from_secs(30);

/// Kiro 上游端点名称
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EndpointName {
    /// 默认 CodeWhisperer 路由（q.{region}.amazonaws.com，无 x-amz-target）
    Ide,
    /// Kiro 专用域（runtime.{region}.kiro.dev，与 q.* 独立）
    Runtime,
    /// 显式 CodeWhisperer Streaming（us-east-1 独占主机，其它区域走 q.* + x-amz-target）
    Codewhisperer,
    /// Amazon Q Developer SendMessage（q.{region}.amazonaws.com + x-amz-target）
    Amazonq,
}

impl EndpointName {
    /// 所有端点（固定顺序，与 `default_order` 一致）
    pub const ALL: [EndpointName; 4] = [
        EndpointName::Ide,
        EndpointName::Runtime,
        EndpointName::Codewhisperer,
        EndpointName::Amazonq,
    ];

    /// 序列化 / 选择时使用的 kebab-case 字符串
    #[allow(dead_code)] // 后续 task 4 (tracing/log) 使用
    pub fn as_str(&self) -> &'static str {
        match self {
            EndpointName::Ide => "ide",
            EndpointName::Runtime => "runtime",
            EndpointName::Codewhisperer => "codewhisperer",
            EndpointName::Amazonq => "amazonq",
        }
    }
}

/// 上游端点描述（host 在构造时按 region 一次性物化）
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub name: EndpointName,
    pub host: String,
    pub amz_target: Option<&'static str>,
}

impl Endpoint {
    /// 默认端点顺序（轮询偏移基准）
    pub fn default_order() -> &'static [EndpointName] {
        &EndpointName::ALL
    }

    /// 按名称 + region 构造单个端点
    ///
    /// `region` 自动归一化为小写（AWS 规范），调用方可传入任意大小写组合
    pub fn by_name(name: EndpointName, region: &str) -> Self {
        let region = region.to_lowercase();
        match name {
            EndpointName::Ide => Self {
                name,
                host: format!("q.{region}.amazonaws.com"),
                amz_target: None,
            },
            EndpointName::Runtime => Self {
                name,
                host: format!("runtime.{region}.kiro.dev"),
                amz_target: None,
            },
            EndpointName::Codewhisperer => Self {
                name,
                host: codewhisperer_host_lowered(&region),
                amz_target: Some("AmazonCodeWhispererStreamingService.GenerateAssistantResponse"),
            },
            EndpointName::Amazonq => Self {
                name,
                host: format!("q.{region}.amazonaws.com"),
                amz_target: Some("AmazonQDeveloperStreamingService.SendMessage"),
            },
        }
    }

    /// 4 个端点全集（顺序 = `default_order`）
    pub fn all(region: &str) -> [Endpoint; 4] {
        [
            Self::by_name(EndpointName::Ide, region),
            Self::by_name(EndpointName::Runtime, region),
            Self::by_name(EndpointName::Codewhisperer, region),
            Self::by_name(EndpointName::Amazonq, region),
        ]
    }
}

/// `codewhisperer` 端点在 us-east-1 走独占主机，其它区域回退到 `q.*`
///
/// **前置条件**：`region` 必须已是小写（由 `by_name` 入口归一化保证）
fn codewhisperer_host_lowered(region: &str) -> String {
    if region == "us-east-1" {
        format!("codewhisperer.{region}.amazonaws.com")
    } else {
        format!("q.{region}.amazonaws.com")
    }
}

/// 桶级 429 状态注册表
///
/// key = `(credential_id, EndpointName)`；value = 封禁到期时刻
/// 到期即视为可用，无需主动清理 Map 条目
#[derive(Default)]
pub struct EndpointBucketRegistry {
    inner: Mutex<HashMap<(u64, EndpointName), Instant>>,
}

impl EndpointBucketRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 标记 `(credential_id, name)` 桶封禁至 `now + duration`
    pub fn throttle(&self, credential_id: u64, name: EndpointName, duration: Duration) {
        let mut guard = self.inner.lock();
        guard.insert((credential_id, name), Instant::now() + duration);
    }

    /// 查询桶是否仍被封禁（到期返回 false）
    pub fn is_throttled(&self, credential_id: u64, name: EndpointName) -> bool {
        let mut guard = self.inner.lock();
        let Some(until) = guard.get(&(credential_id, name)).copied() else {
            return false;
        };
        if Instant::now() >= until {
            // 过期则惰性清理
            guard.remove(&(credential_id, name));
            return false;
        }
        true
    }

    /// 测试辅助：当前所有被封禁桶的数量（不计过期）
    #[cfg(test)]
    pub fn len_for_test(&self) -> usize {
        let guard = self.inner.lock();
        guard.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_all_returns_four_with_correct_hosts_and_amz_targets() {
        let all = Endpoint::all("us-east-1");
        assert_eq!(all.len(), 4);

        let by_name = |n| all.iter().find(|e| e.name == n).unwrap();

        assert_eq!(by_name(EndpointName::Ide).host, "q.us-east-1.amazonaws.com");
        assert_eq!(by_name(EndpointName::Ide).amz_target, None);

        assert_eq!(
            by_name(EndpointName::Runtime).host,
            "runtime.us-east-1.kiro.dev"
        );
        assert_eq!(by_name(EndpointName::Runtime).amz_target, None);

        assert_eq!(
            by_name(EndpointName::Codewhisperer).host,
            "codewhisperer.us-east-1.amazonaws.com"
        );
        assert_eq!(
            by_name(EndpointName::Codewhisperer).amz_target,
            Some("AmazonCodeWhispererStreamingService.GenerateAssistantResponse")
        );

        assert_eq!(
            by_name(EndpointName::Amazonq).host,
            "q.us-east-1.amazonaws.com"
        );
        assert_eq!(
            by_name(EndpointName::Amazonq).amz_target,
            Some("AmazonQDeveloperStreamingService.SendMessage")
        );
    }

    #[test]
    fn codewhisperer_uses_dedicated_host_in_us_east_1_only() {
        // 小写规范 host
        assert_eq!(
            Endpoint::by_name(EndpointName::Codewhisperer, "us-east-1").host,
            "codewhisperer.us-east-1.amazonaws.com"
        );
        assert_eq!(
            Endpoint::by_name(EndpointName::Codewhisperer, "eu-central-1").host,
            "q.eu-central-1.amazonaws.com"
        );
        // 大小写输入归一化为小写 host（AWS region 规范）
        assert_eq!(
            Endpoint::by_name(EndpointName::Codewhisperer, "US-EAST-1").host,
            "codewhisperer.us-east-1.amazonaws.com"
        );
        // 混合大小写同样归一化
        assert_eq!(
            Endpoint::by_name(EndpointName::Codewhisperer, "Us-East-1").host,
            "codewhisperer.us-east-1.amazonaws.com"
        );
    }

    #[test]
    fn all_four_endpoints_normalize_region_to_lowercase() {
        // 入口归一化保证 4 端点对大小写输入行为一致（CR Round 2 F1/F2 防护）
        for name in EndpointName::ALL {
            let host = Endpoint::by_name(name, "US-EAST-1").host;
            assert!(
                !host.contains("US-EAST-1"),
                "endpoint {:?} host {:?} 应已归一化为小写 region",
                name,
                host
            );
            assert!(
                host.contains("us-east-1"),
                "endpoint {:?} host {:?} 应包含小写 us-east-1",
                name,
                host
            );
        }
        // 混合大小写同样归一化（F3 防护）
        for name in EndpointName::ALL {
            let host = Endpoint::by_name(name, "Us-East-1").host;
            assert!(host.contains("us-east-1"), "{:?} host {:?}", name, host);
        }
    }

    #[test]
    fn default_order_matches_all() {
        let order = Endpoint::default_order();
        assert_eq!(order, &EndpointName::ALL);
    }

    #[test]
    fn endpoint_name_serializes_as_kebab_case() {
        assert_eq!(
            serde_json::to_string(&EndpointName::Codewhisperer).unwrap(),
            "\"codewhisperer\""
        );
        assert_eq!(
            serde_json::from_str::<EndpointName>("\"amazonq\"").unwrap(),
            EndpointName::Amazonq
        );
        // 非法值反序列化失败（由调用方上层处理 → fallback to ALL）
        assert!(serde_json::from_str::<EndpointName>("\"invalid\"").is_err());
    }

    #[test]
    fn bucket_state_is_isolated_by_credential_and_endpoint() {
        let reg = EndpointBucketRegistry::new();
        reg.throttle(1, EndpointName::Ide, BUCKET_THROTTLE_DURATION);

        // 同账号其它端点仍可用
        assert!(reg.is_throttled(1, EndpointName::Ide));
        assert!(!reg.is_throttled(1, EndpointName::Runtime));
        assert!(!reg.is_throttled(1, EndpointName::Codewhisperer));
        assert!(!reg.is_throttled(1, EndpointName::Amazonq));

        // 其它账号的 4 桶不受影响
        assert!(!reg.is_throttled(2, EndpointName::Ide));
        assert!(!reg.is_throttled(2, EndpointName::Runtime));
        assert!(!reg.is_throttled(2, EndpointName::Codewhisperer));
        assert!(!reg.is_throttled(2, EndpointName::Amazonq));
    }

    #[test]
    fn bucket_state_expires_automatically() {
        let reg = EndpointBucketRegistry::new();
        // 用极短时长模拟过期
        reg.throttle(1, EndpointName::Ide, Duration::from_millis(10));
        assert!(reg.is_throttled(1, EndpointName::Ide));
        std::thread::sleep(Duration::from_millis(20));
        assert!(!reg.is_throttled(1, EndpointName::Ide));
        // 惰性清理：过期后再查 len 不会留下残留条目
        assert_eq!(reg.len_for_test(), 0);
    }

    #[test]
    fn bucket_state_throttle_overwrites_previous_window() {
        let reg = EndpointBucketRegistry::new();
        reg.throttle(1, EndpointName::Ide, Duration::from_millis(100));
        reg.throttle(1, EndpointName::Ide, BUCKET_THROTTLE_DURATION);
        assert!(reg.is_throttled(1, EndpointName::Ide));
        // 100ms 后第一次封禁到期，但第二次写入的 30s 仍在生效
        std::thread::sleep(Duration::from_millis(150));
        assert!(reg.is_throttled(1, EndpointName::Ide));
    }
}
