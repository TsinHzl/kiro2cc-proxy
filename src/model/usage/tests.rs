//! 用量追踪模块测试（原 usage.rs 内联测试搬移）

#[cfg(test)]
mod tests {

    use crate::model::usage::{UsageTracker, calculate_cost, get_k_ref};
    use std::collections::HashMap;

    /// 每个测试使用独立目录隔离明细文件与累计基数文件（api_key_lifetime.json
    /// 固定落在明细同目录，仅隔离文件名会让所有测试共享/互踩同一累计文件）
    fn temp_usage_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kiro2cc_usage_test_{}_{}",
            name,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("api_key_usage.json")
    }

    /// 清理测试独立目录（明细与累计基数同目录）
    fn cleanup_usage_path(path: &std::path::Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
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
        cleanup_usage_path(&path);
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
        cleanup_usage_path(&path);
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
        cleanup_usage_path(&path);
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
        cleanup_usage_path(&path);
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
        cleanup_usage_path(&path);
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

    #[test]
    fn test_get_k_ref_haiku_5_5() {
        // haiku 5.5 沿用 haiku 默认档（暂无 credits 实测系数）
        assert_eq!(get_k_ref("claude-haiku-5.5"), 1.43);
        assert_eq!(get_k_ref("claude-haiku-5-5"), 1.43);
        assert_eq!(get_k_ref("claude-haiku-5-5-thinking"), 1.43);
    }

    #[test]
    fn test_pricing_haiku_5_5() {
        // 低档（≤100K）：1M output 单独不触发跳档（阈值只判 input）
        assert!((calculate_cost("claude-haiku-5.5", 0, 1_000_000) - 0.50).abs() < 1e-9);

        // 回归：haiku 4.5 定价不变（1 + 5 = 6.0）
        assert!((calculate_cost("claude-haiku-4.5", 1_000_000, 1_000_000) - 6.0).abs() < 1e-9);

        // 回归：连字符别名与映射层口径一致（曾按 4.5 定价高估 10 倍）
        // 取 50K 输入以留在低档，避免与跳档测试混淆
        assert!((calculate_cost("claude-haiku-5-5", 50_000, 0) - 0.005).abs() < 1e-9);
    }

    #[test]
    fn test_pricing_haiku_5_5_long_context_tier() {
        // 整档跳价：超 100K 时**整个请求**按高档，非超出部分累进
        // 200_000 input + 1_000 output = 0.5*0.2 + 2.5*0.001 = 0.1025
        assert!(
            (calculate_cost("claude-haiku-5.5", 200_000, 1_000) - 0.1025).abs() < 1e-9,
            "超阈值应整档跳价至 $0.50/$2.50"
        );

        // 边界：100_000 走低档，100_001 跳高档
        assert!(
            (calculate_cost("claude-haiku-5.5", 100_000, 0) - 0.01).abs() < 1e-9,
            "恰好 100K 仍属低档"
        );
        assert!(
            (calculate_cost("claude-haiku-5.5", 100_001, 0) - 0.0500005).abs() < 1e-9,
            "超过 100K 即整档跳价"
        );

        // 别名口径一致
        assert!(
            (calculate_cost("Claude Haiku 5.5", 200_000, 0) - 0.10).abs() < 1e-9,
            "空格别名同样跳档"
        );
        // 点号变体（回归：谓词改写曾丢失点号写法，会回退 4.5 定价高估 10 倍）
        assert!(
            (calculate_cost("claude-haiku.5.5", 200_000, 0) - 0.10).abs() < 1e-9,
            "点号别名同样跳档"
        );

        // 回归：haiku 4.5 无分档，1M 输入仍按低档单档计价
        assert!((calculate_cost("claude-haiku-4.5", 1_000_000, 0) - 1.0).abs() < 1e-9);
    }

    /// 回归：明细超过 MAX_RECORDS_PER_KEY 被裁剪后，total_requests 仍持续增长
    /// （历史缺陷：直接数现存记录条数导致请求数封顶 10,000）
    #[tokio::test]
    async fn test_total_requests_not_capped_by_record_pruning() {
        let path = temp_usage_path("pruning_not_capped");
        let tracker = UsageTracker::load(&path).unwrap();
        // 记录上限 + 5 条：应裁掉最老的 5 条并累计进基数
        let total = crate::model::usage::MAX_RECORDS_PER_KEY_FOR_TEST + 5;
        for _ in 0..total {
            tracker.record(
                9,
                None,
                "claude-opus-4.6".to_string(),
                10,
                10,
                None,
                None,
                None,
                None,
                None,
            );
        }
        let summary = tracker.get_summary(9);
        assert_eq!(summary.total_requests, total as u64);
        // 现存明细恰好为上限条数
        cleanup_usage_path(&path);
    }

    /// 回归：reset 后请求数从 0 重新计数（基数同步清零）
    #[tokio::test]
    async fn test_reset_clears_lifetime_base() {
        let path = temp_usage_path("reset_clears_base");
        let tracker = UsageTracker::load(&path).unwrap();
        for _ in 0..(crate::model::usage::MAX_RECORDS_PER_KEY_FOR_TEST + 3) {
            tracker.record(
                8,
                None,
                "claude-opus-4.6".to_string(),
                10,
                10,
                None,
                None,
                None,
                None,
                None,
            );
        }
        assert!(tracker.get_summary(8).total_requests > 0);
        tracker.reset(8).unwrap();
        assert_eq!(tracker.get_summary(8).total_requests, 0);
        cleanup_usage_path(&path);
    }

    /// 回归：生命周期基数持久化后重启不丢（裁剪掉的部分仍计入请求数）
    #[tokio::test]
    async fn test_lifetime_base_persisted_across_reload() {
        let path = temp_usage_path("lifetime_persisted");
        {
            let tracker = UsageTracker::load(&path).unwrap();
            for _ in 0..(crate::model::usage::MAX_RECORDS_PER_KEY_FOR_TEST + 7) {
                tracker.record(
                    7,
                    None,
                    "claude-opus-4.6".to_string(),
                    10,
                    10,
                    None,
                    None,
                    None,
                    None,
                    None,
                );
            }
            // 等待后台任务把脏数据落盘（周期 5s 太长，直接触发 shutdown 刷盘：
            // drop tracker 即关闭通道，graceful shutdown 分支会执行落盘）
        }
        // 轮询等待刷盘完成。不能仅等基数文件存在：后台先处理 interval tick 时
        // （局部 dirty 尚为 false）可能只写基数、不写明细，此时明细文件可能
        // 还未就绪；改为等待明细文件落盘且记录数达到上限，配合基数文件存在的
        // 前置条件，两者都就绪才认为 shutdown 刷盘完成
        let lifetime_path = path.parent().unwrap().join("api_key_lifetime.json");
        let mut persisted = false;
        for _ in 0..100 {
            let records_ready = std::fs::read_to_string(&path)
                .map(|c| {
                    serde_json::from_str::<Vec<crate::model::usage::UsageRecord>>(&c)
                        .map(|r| r.len() == crate::model::usage::MAX_RECORDS_PER_KEY_FOR_TEST)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if records_ready && lifetime_path.exists() {
                persisted = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(
            persisted,
            "lifetime base file should be written on shutdown"
        );
        let tracker2 = UsageTracker::load(&path).unwrap();
        // 精确断言：明细（上限条）+ 基数（裁剪掉的 7 条）完整持久化，
        // 基数文件缺失或仅靠现存明细求和都会在此失败
        assert_eq!(
            tracker2.get_summary(7).total_requests,
            (crate::model::usage::MAX_RECORDS_PER_KEY_FOR_TEST + 7) as u64
        );
        cleanup_usage_path(&path);
    }
}
