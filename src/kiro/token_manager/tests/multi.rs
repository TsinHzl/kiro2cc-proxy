// Copyright (c) 2026 Harllan He. Licensed under MIT.
// token_manager 测试（自 tests.rs 拆出，纯代码搬移）
#[cfg(test)]
pub(crate) mod tests {

    use super::super::super::entry::DisabledReason;

    use super::super::super::types::{
        MAX_FAILURES_PER_CREDENTIAL, MultiTokenManager, QUOTA_EXHAUSTED_ALL_MARKER,
    };

    use crate::kiro::model::credentials::KiroCredentials;

    use crate::kiro::token_manager::tests::ext_idp::tests::spawn_single_response_server;
    use crate::kiro::token_manager::tests::sticky::tests::make_valid_cred;
    use crate::model::config::Config;
    use chrono::{Duration, Utc};

    #[test]
    fn test_multi_token_manager_new() {
        let config = Config::default();
        let mut cred1 = KiroCredentials::default();
        cred1.priority = 0;
        let mut cred2 = KiroCredentials::default();
        cred2.priority = 1;

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();
        assert_eq!(manager.total_count(), 2);
        assert_eq!(manager.available_count(), 2);
    }

    #[test]
    fn test_multi_token_manager_empty_credentials() {
        let config = Config::default();
        let result = MultiTokenManager::new(config, vec![], None, None, false);
        // 支持 0 个账号启动（可通过管理面板添加）
        assert!(result.is_ok());
        let manager = result.unwrap();
        assert_eq!(manager.total_count(), 0);
        assert_eq!(manager.available_count(), 0);
    }

    #[test]
    fn test_multi_token_manager_duplicate_ids() {
        let config = Config::default();
        let mut cred1 = KiroCredentials::default();
        cred1.id = Some(1);
        let mut cred2 = KiroCredentials::default();
        cred2.id = Some(1); // 重复 ID

        let result = MultiTokenManager::new(config, vec![cred1, cred2], None, None, false);
        assert!(result.is_err());
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("重复的账号 ID"),
            "错误消息应包含 '重复的账号 ID'，实际: {}",
            err_msg
        );
    }

    #[test]
    fn test_multi_token_manager_report_failure() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();
        let cred2 = KiroCredentials::default();

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 账号会自动分配 ID（从 1 开始）
        // 前两次失败不会禁用（使用 ID 1）
        assert!(manager.report_failure(1));
        assert!(manager.report_failure(1));
        assert_eq!(manager.available_count(), 2);

        // 第三次失败会禁用第一个账号
        assert!(manager.report_failure(1));
        assert_eq!(manager.available_count(), 1);

        // 继续失败第二个账号（使用 ID 2）
        assert!(manager.report_failure(2));
        assert!(manager.report_failure(2));
        assert!(!manager.report_failure(2)); // 所有账号都禁用了
        assert_eq!(manager.available_count(), 0);
    }

    #[test]
    fn test_multi_token_manager_report_success() {
        let config = Config::default();
        let cred = KiroCredentials::default();

        let manager = MultiTokenManager::new(config, vec![cred], None, None, false).unwrap();

        // 失败两次（使用 ID 1）
        manager.report_failure(1);
        manager.report_failure(1);

        // 成功后重置计数（使用 ID 1）
        manager.report_success(1);

        // 再失败两次不会禁用
        manager.report_failure(1);
        manager.report_failure(1);
        assert_eq!(manager.available_count(), 1);
    }

    #[test]
    fn test_multi_token_manager_switch_to_next() {
        let config = Config::default();
        let mut cred1 = KiroCredentials::default();
        cred1.refresh_token = Some("token1".to_string());
        let mut cred2 = KiroCredentials::default();
        cred2.refresh_token = Some("token2".to_string());

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 初始是第一个账号
        assert_eq!(
            manager.credentials().refresh_token,
            Some("token1".to_string())
        );

        // 切换到下一个
        assert!(manager.switch_to_next());
        assert_eq!(
            manager.credentials().refresh_token,
            Some("token2".to_string())
        );
    }

    #[test]
    fn test_set_load_balancing_mode_persists_to_config_file() {
        let config_path =
            std::env::temp_dir().join(format!("kiro-load-balancing-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&config_path, r#"{"loadBalancingMode":"priority"}"#).unwrap();

        let config = Config::load(&config_path).unwrap();
        let manager =
            MultiTokenManager::new(config, vec![KiroCredentials::default()], None, None, false)
                .unwrap();

        manager
            .set_load_balancing_mode("balanced".to_string())
            .unwrap();

        let persisted = Config::load(&config_path).unwrap();
        assert_eq!(persisted.load_balancing_mode, "balanced");
        assert_eq!(manager.get_load_balancing_mode(), "balanced");

        std::fs::remove_file(&config_path).unwrap();
    }

    #[tokio::test]
    async fn test_multi_token_manager_acquire_context_auto_recovers_all_disabled() {
        let config = Config::default();
        let mut cred1 = KiroCredentials::default();
        cred1.access_token = Some("t1".to_string());
        cred1.expires_at = Some((Utc::now() + Duration::hours(1)).to_rfc3339());
        let mut cred2 = KiroCredentials::default();
        cred2.access_token = Some("t2".to_string());
        cred2.expires_at = Some((Utc::now() + Duration::hours(1)).to_rfc3339());

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 账号会自动分配 ID（从 1 开始）
        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            manager.report_failure(1);
        }
        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            manager.report_failure(2);
        }

        assert_eq!(manager.available_count(), 0);

        // 应触发自愈：重置失败计数并重新启用，避免必须重启进程
        let ctx = manager.acquire_context(None).await.unwrap();
        assert!(ctx.token == "t1" || ctx.token == "t2");
        assert_eq!(manager.available_count(), 2);
    }

    #[tokio::test]
    async fn test_acquire_context_self_heal_excludes_invalid_refresh_token() {
        // TooManyRefreshFailures 属于瞬态故障，应参与全灭自愈；InvalidRefreshToken 是
        // 服务端确认的永久性失效，重置重试只会立即再次失败，因此不参与自愈（覆盖 T12）
        let config = Config::default();
        let cred1 = make_valid_cred("t1");
        let cred2 = make_valid_cred("t2");
        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            manager.report_refresh_failure(1);
        }
        manager.report_refresh_token_invalid(2);
        assert_eq!(manager.available_count(), 0);

        // 触发自愈：#1（TooManyRefreshFailures）应被重置，#2（InvalidRefreshToken）应保持禁用
        let ctx = manager.acquire_context(None).await.unwrap();
        assert_eq!(ctx.id, 1);
        assert_eq!(manager.available_count(), 1);

        let entries = manager.entries.lock();
        let entry1 = entries.iter().find(|e| e.id == 1).unwrap();
        assert!(!entry1.disabled);
        assert_eq!(entry1.disabled_reason, None);
        assert_eq!(entry1.refresh_failure_count, 0);

        let entry2 = entries.iter().find(|e| e.id == 2).unwrap();
        assert!(
            entry2.disabled,
            "InvalidRefreshToken 不应参与全灭自愈，需人工更换凭证"
        );
        assert_eq!(
            entry2.disabled_reason,
            Some(DisabledReason::InvalidRefreshToken)
        );
    }

    #[tokio::test]
    async fn test_refresh_success_clears_refresh_failure_count() {
        // 对称于 report_success 清零 failure_count：孤立的偶发刷新失败不应无限累积
        let body = r#"{"access_token":"new-access-token","expires_in":3600}"#;
        let endpoint = spawn_single_response_server(200, body).await;

        let cred1 = KiroCredentials {
            auth_method: Some("external_idp".to_string()),
            refresh_token: Some("short-refresh-token".to_string()),
            client_id: Some("client-id".to_string()),
            token_endpoint: Some(endpoint),
            expires_at: Some((Utc::now() - Duration::hours(1)).to_rfc3339()),
            ..Default::default()
        };

        let config = Config::default();
        let manager = MultiTokenManager::new(config, vec![cred1], None, None, false).unwrap();

        manager.report_refresh_failure(1);
        manager.report_refresh_failure(1);
        {
            let entries = manager.entries.lock();
            assert_eq!(entries[0].refresh_failure_count, 2);
        }

        let ctx = manager.acquire_context_filtered(None, &[1]).await.unwrap();
        assert_eq!(ctx.id, 1);

        let entries = manager.entries.lock();
        assert_eq!(
            entries[0].refresh_failure_count, 0,
            "刷新成功后应清零 refresh_failure_count"
        );
    }

    #[test]
    fn test_set_disabled_enable_clears_refresh_failure_count() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();

        let manager = MultiTokenManager::new(config, vec![cred1], None, None, false).unwrap();

        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            manager.report_refresh_failure(1);
        }
        assert_eq!(manager.available_count(), 0);

        manager.set_disabled(1, false).unwrap();

        let entries = manager.entries.lock();
        assert_eq!(entries[0].refresh_failure_count, 0);
        assert!(!entries[0].disabled);
        assert_eq!(entries[0].disabled_reason, None);
    }

    #[test]
    fn test_reset_and_enable_clears_refresh_failure_count() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();

        let manager = MultiTokenManager::new(config, vec![cred1], None, None, false).unwrap();

        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            manager.report_refresh_failure(1);
        }
        assert_eq!(manager.available_count(), 0);

        manager.reset_and_enable(1).unwrap();

        let entries = manager.entries.lock();
        assert_eq!(entries[0].refresh_failure_count, 0);
        assert!(!entries[0].disabled);
        assert_eq!(entries[0].disabled_reason, None);
    }

    #[test]
    fn test_multi_token_manager_report_quota_exhausted() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();
        let cred2 = KiroCredentials::default();

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 账号会自动分配 ID（从 1 开始）
        assert_eq!(manager.available_count(), 2);
        assert!(manager.report_quota_exhausted(1));
        assert_eq!(manager.available_count(), 1);

        // 再禁用第二个后，无可用账号
        assert!(!manager.report_quota_exhausted(2));
        assert_eq!(manager.available_count(), 0);
    }

    #[test]
    fn test_report_refresh_token_invalid_disables_immediately_without_counting() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();
        let cred2 = KiroCredentials::default();

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        assert_eq!(manager.available_count(), 2);
        // 单次调用即立即禁用，不像 report_refresh_failure 需要累计到阈值
        assert!(manager.report_refresh_token_invalid(1));
        assert_eq!(manager.available_count(), 1);

        let entries = manager.entries.lock();
        let entry = entries.iter().find(|e| e.id == 1).unwrap();
        assert!(entry.disabled);
        assert_eq!(
            entry.disabled_reason,
            Some(DisabledReason::InvalidRefreshToken)
        );
        assert_eq!(
            entry.refresh_failure_count, 0,
            "invalid_grant 是永久性失效，不应计入 refresh_failure_count"
        );
    }

    #[test]
    fn test_report_refresh_failure_counts_to_threshold_then_disables() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();
        let cred2 = KiroCredentials::default();

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 未达阈值前保持可用
        assert!(manager.report_refresh_failure(1));
        assert!(manager.report_refresh_failure(1));
        assert_eq!(manager.available_count(), 2);

        // 第 3 次达到 MAX_FAILURES_PER_CREDENTIAL 阈值，禁用并设置正确的 disabled_reason
        assert!(manager.report_refresh_failure(1));
        assert_eq!(manager.available_count(), 1);

        let entries = manager.entries.lock();
        let entry = entries.iter().find(|e| e.id == 1).unwrap();
        assert!(entry.disabled);
        assert_eq!(
            entry.disabled_reason,
            Some(DisabledReason::TooManyRefreshFailures)
        );
        assert_eq!(entry.refresh_failure_count, MAX_FAILURES_PER_CREDENTIAL);
    }

    #[tokio::test]
    async fn test_multi_token_manager_quota_disabled_is_not_auto_recovered() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();
        let cred2 = KiroCredentials::default();

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        manager.report_quota_exhausted(1);
        manager.report_quota_exhausted(2);
        assert_eq!(manager.available_count(), 0);

        let err = manager
            .acquire_context(None)
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(
            err.contains("本月请求额度已用尽") && err.contains(QUOTA_EXHAUSTED_ALL_MARKER),
            "错误应明确指出额度用尽并带机器可识别标记，实际: {}",
            err
        );
        assert_eq!(manager.available_count(), 0);
    }

    #[tokio::test]
    async fn test_report_failure_preserves_quota_disabled_reason() {
        // 回归：并发下账号已被 report_quota_exhausted 禁用后，
        // 再来一个普通失败（report_failure）不得覆盖 disabled_reason，
        // 否则会被自愈逻辑（只重置 TooManyFailures）错误重新启用。
        let config = Config::default();
        let mut cred1 = KiroCredentials::default();
        cred1.access_token = Some("t1".to_string());
        cred1.expires_at = Some((Utc::now() + Duration::hours(1)).to_rfc3339());
        let mut cred2 = KiroCredentials::default();
        cred2.access_token = Some("t2".to_string());
        cred2.expires_at = Some((Utc::now() + Duration::hours(1)).to_rfc3339());

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 账号 #1 额度用尽被禁用
        manager.report_quota_exhausted(1);
        // 模拟在途的另一请求随后对同一账号报告普通失败
        manager.report_failure(1);

        // disabled_reason 必须仍是 QuotaExceeded，不能被改写为 TooManyFailures
        {
            let entries = manager.entries.lock();
            let entry = entries.iter().find(|e| e.id == 1).unwrap();
            assert!(entry.disabled);
            assert_eq!(
                entry.disabled_reason,
                Some(DisabledReason::QuotaExceeded),
                "QuotaExceeded 禁用原因被 report_failure 覆盖"
            );
        }

        // 仅 #2 可用，acquire 不应自愈被额度禁用的 #1
        let ctx = manager.acquire_context(None).await.unwrap();
        assert_eq!(ctx.token, "t2", "额度耗尽账号 #1 不应被重新启用");
        assert_eq!(manager.available_count(), 1);
    }

    // ============ 账号级 Region 优先级测试 ============
}
