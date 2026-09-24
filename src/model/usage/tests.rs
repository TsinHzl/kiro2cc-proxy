//! 用量追踪模块测试（原 usage.rs 内联测试搬移）

#[cfg(test)]
mod tests {

    use crate::model::usage::{UsageTracker, calculate_cost, get_k_ref};
    use std::collections::HashMap;

    fn temp_usage_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "kiro2cc_usage_test_{}_{}.json",
            name,
            uuid::Uuid::new_v4()
        ))
    }

    #[tokio::test]
    async fn test_get_total_credits_uses_real_credits_when_present() {
        let path = temp_usage_path("credits_present");
        let tracker = UsageTracker::load(&path).unwrap();
        // credits_used 显式提供时，应直接累加该值而非回退估算
        tracker.record(
            1,
            None,
            "claude-opus-4.6".to_string(),
            1000,
            100,
            None,
            Some(3.43),
            None,
            None,
            None,
        );
        tracker.record(
            1,
            None,
            "claude-opus-4.6".to_string(),
            1000,
            100,
            None,
            Some(1.0),
            None,
            None,
            None,
        );
        assert!((tracker.get_total_credits(1) - 4.43).abs() < 1e-9);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_get_total_credits_falls_back_to_estimated_cost_times_k_ref() {
        let path = temp_usage_path("credits_fallback");
        let tracker = UsageTracker::load(&path).unwrap();
        // 旧记录无 credits_used 时按 estimated_cost * k_ref 回退估算
        tracker.record(
            1,
            None,
            "claude-sonnet-4.5".to_string(),
            1_000_000,
            0,
            None,
            None,
            None,
            None,
            None,
        );
        let expected_cost = calculate_cost("claude-sonnet-4.5", 1_000_000, 0);
        let expected_credits = expected_cost * get_k_ref("claude-sonnet-4.5");
        assert!((tracker.get_total_credits(1) - expected_credits).abs() < 1e-9);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_get_total_credits_only_counts_matching_api_key() {
        let path = temp_usage_path("credits_scoped");
        let tracker = UsageTracker::load(&path).unwrap();
        tracker.record(
            1,
            None,
            "claude-opus-4.6".to_string(),
            100,
            10,
            None,
            Some(5.0),
            None,
            None,
            None,
        );
        tracker.record(
            2,
            None,
            "claude-opus-4.6".to_string(),
            100,
            10,
            None,
            Some(99.0),
            None,
            None,
            None,
        );
        assert!((tracker.get_total_credits(1) - 5.0).abs() < 1e-9);
        assert!((tracker.get_total_credits(2) - 99.0).abs() < 1e-9);
        assert_eq!(tracker.get_total_credits(3), 0.0);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_get_summary_total_credits_matches_get_total_credits() {
        let path = temp_usage_path("summary_credits");
        let tracker = UsageTracker::load(&path).unwrap();
        tracker.record(
            1,
            None,
            "claude-opus-4.8".to_string(),
            500,
            50,
            None,
            Some(2.5),
            None,
            None,
            None,
        );
        let summary = tracker.get_summary(1);
        assert!((summary.total_credits - tracker.get_total_credits(1)).abs() < 1e-9);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_record_effort_persisted_and_none_fallback() {
        let path = temp_usage_path("effort_field");
        let tracker = UsageTracker::load(&path).unwrap();
        tracker.record(
            1,
            None,
            "claude-opus-4.8".to_string(),
            100,
            10,
            None,
            None,
            None,
            None,
            Some("xhigh".to_string()),
        );
        tracker.record(
            1,
            None,
            "claude-sonnet-4.5".to_string(),
            100,
            10,
            None,
            None,
            None,
            None,
            None,
        );
        let page = tracker.get_records_paged(1, 1, 10, &HashMap::new());
        assert_eq!(page.records.len(), 2);
        // 有 effort 的记录映射到 UsageRecordItem
        assert!(
            page.records
                .iter()
                .any(|r| r.effort.as_deref() == Some("xhigh"))
        );
        // None 兜底：未传 output_config 的旧请求 effort 保持 None
        assert!(page.records.iter().any(|r| r.effort.is_none()));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_get_k_ref_opus_5() {
        assert_eq!(get_k_ref("claude-opus-5"), 2.36);
        assert_eq!(get_k_ref("claude-opus-5-thinking"), 2.36);
        assert_eq!(get_k_ref("Claude-Opus-5"), 2.36);

        // 回归：其他档位不变
        assert_eq!(get_k_ref("claude-opus-4-7"), 2.36);
        assert_eq!(get_k_ref("claude-opus-4-8"), 2.36);
        assert_eq!(get_k_ref("claude-opus-4-6"), 1.90);
        assert_eq!(get_k_ref("claude-opus-4-5"), 1.90);
        assert_eq!(get_k_ref("claude-sonnet-5"), 1.43);
        assert_eq!(get_k_ref("claude-sonnet-4.6"), 1.43);
        assert_eq!(get_k_ref("claude-haiku-4.5"), 1.43);
    }
}
