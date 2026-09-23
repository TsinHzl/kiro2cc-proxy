#[cfg(test)]
mod tests {
    use super::super::TokenManager;
    use super::super::entry::{CredentialEntry, DisabledReason, HealthStatus};
    use super::super::refresh::RefreshTokenInvalidError;
    use super::super::refresh::{
        apply_idc_refresh_response, is_invalid_grant_response, select_usage_limits_token,
        sha256_hex, validate_refresh_token,
    };
    use super::super::types::{
        MAX_FAILURES_PER_CREDENTIAL, MultiTokenManager, QUOTA_EXHAUSTED_ALL_MARKER,
        STICKY_CACHE_TTL, STICKY_THROTTLE_EVICT_THRESHOLD,
    };
    use super::super::{is_token_expired, is_token_expiring_soon, refresh_token};
    use crate::kiro::model::credentials::{
        BUILDER_ID_PLACEHOLDER_PROFILE_ARN, KiroCredentials, SOCIAL_PROFILE_ARN,
    };
    use crate::kiro::model::token_refresh::IdcRefreshResponse;
    use crate::model::config::Config;
    use chrono::{DateTime, Duration, Utc};
    use std::time::Duration as StdDuration;

    #[test]
    fn test_token_manager_new() {
        let config = Config::default();
        let credentials = KiroCredentials::default();
        let tm = TokenManager::new(config, credentials, None);
        assert!(tm.credentials().access_token.is_none());
    }

    #[test]
    fn test_is_token_expired_with_expired_token() {
        let mut credentials = KiroCredentials::default();
        credentials.expires_at = Some("2020-01-01T00:00:00Z".to_string());
        assert!(is_token_expired(&credentials));
    }

    #[test]
    fn test_is_token_expired_with_valid_token() {
        let mut credentials = KiroCredentials::default();
        let future = Utc::now() + Duration::hours(1);
        credentials.expires_at = Some(future.to_rfc3339());
        assert!(!is_token_expired(&credentials));
    }

    #[test]
    fn test_is_token_expired_within_5_minutes() {
        let mut credentials = KiroCredentials::default();
        let expires = Utc::now() + Duration::minutes(3);
        credentials.expires_at = Some(expires.to_rfc3339());
        assert!(is_token_expired(&credentials));
    }

    #[test]
    fn test_is_token_expired_no_expires_at() {
        let credentials = KiroCredentials::default();
        assert!(is_token_expired(&credentials));
    }

    #[test]
    fn test_is_token_expiring_soon_within_10_minutes() {
        let mut credentials = KiroCredentials::default();
        let expires = Utc::now() + Duration::minutes(8);
        credentials.expires_at = Some(expires.to_rfc3339());
        assert!(is_token_expiring_soon(&credentials));
    }

    #[test]
    fn test_is_token_expiring_soon_beyond_10_minutes() {
        let mut credentials = KiroCredentials::default();
        let expires = Utc::now() + Duration::minutes(15);
        credentials.expires_at = Some(expires.to_rfc3339());
        assert!(!is_token_expiring_soon(&credentials));
    }

    #[test]
    fn test_validate_refresh_token_missing() {
        let credentials = KiroCredentials::default();
        let result = validate_refresh_token(&credentials);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_refresh_token_valid() {
        let mut credentials = KiroCredentials::default();
        credentials.refresh_token = Some("a".repeat(150));
        let result = validate_refresh_token(&credentials);
        assert!(result.is_ok());
    }

    #[test]
    fn test_sha256_hex() {
        let result = sha256_hex("test");
        assert_eq!(
            result,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }

    #[test]
    fn test_is_invalid_grant_response_matches_400_with_expected_body() {
        let body =
            r#"{"error":"invalid_grant","error_description":"Invalid refresh token provided"}"#;
        assert!(is_invalid_grant_response(400, body));
    }

    #[test]
    fn test_is_invalid_grant_response_rejects_other_body() {
        let body = r#"{"error":"server_error","error_description":"something else"}"#;
        assert!(!is_invalid_grant_response(400, body));
    }

    #[test]
    fn test_is_invalid_grant_response_rejects_non_400_status() {
        let body =
            r#"{"error":"invalid_grant","error_description":"Invalid refresh token provided"}"#;
        assert!(!is_invalid_grant_response(401, body));
    }

    #[test]
    fn test_apply_update_fields_thinking_adaptive() {
        let mut cred = KiroCredentials::default();
        assert!(!cred.thinking_adaptive);

        // Some(v) 覆盖现有值
        let mut update = crate::admin::types::UpdateCredentialRequest {
            thinking_adaptive: Some(true),
            ..Default::default()
        };
        MultiTokenManager::apply_update_fields(&mut cred, &update);
        assert!(cred.thinking_adaptive);

        update.thinking_adaptive = Some(false);
        MultiTokenManager::apply_update_fields(&mut cred, &update);
        assert!(!cred.thinking_adaptive);

        // None 表示不更新该字段
        let update = crate::admin::types::UpdateCredentialRequest {
            thinking_adaptive: None,
            ..Default::default()
        };
        cred.thinking_adaptive = true;
        MultiTokenManager::apply_update_fields(&mut cred, &update);
        assert!(cred.thinking_adaptive);
    }

    #[tokio::test]
    async fn test_add_credential_reject_duplicate_refresh_token() {
        let config = Config::default();

        let mut existing = KiroCredentials::default();
        existing.refresh_token = Some("a".repeat(150));

        let manager = MultiTokenManager::new(config, vec![existing], None, None, false).unwrap();

        let mut duplicate = KiroCredentials::default();
        duplicate.refresh_token = Some("a".repeat(150));

        let result = manager.add_credential(duplicate).await;
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("账号已存在"));
    }

    /// 回归测试：账号删除后其 ID 不得被复用，否则新账号会"继承"已删除旧账号在
    /// usage/failure/throttle 日志中按 credential_id 存储的历史记录（表现为管理面板
    /// 中一个从未被调用过的新账号却显示历史请求日志）。
    ///
    /// 场景：账号 #1、#2 存在 -> 删除 #2 -> 通过 add_credential 新增一个账号，
    /// 新账号必须获得 #3，而不是复用刚被释放的 #2。
    #[tokio::test]
    async fn test_add_credential_never_reuses_deleted_id_within_same_process() {
        let body = r#"{"access_token":"new-access-token","expires_in":3600}"#;
        let endpoint = spawn_single_response_server(200, body).await;

        let mut cred1 = KiroCredentials::default();
        cred1.id = Some(1);
        cred1.refresh_token = Some("a".repeat(150));

        let mut cred2 = KiroCredentials::default();
        cred2.id = Some(2);
        cred2.refresh_token = Some("b".repeat(150));

        let config = Config::default();
        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 删除账号必须先禁用
        manager.set_disabled(2, true).unwrap();
        manager.delete_credential(2).unwrap();
        assert_eq!(manager.total_count(), 1);

        // 新增账号：走 external_idp 刷新路径，指向本地 mock server
        let new_cred = KiroCredentials {
            auth_method: Some("external_idp".to_string()),
            refresh_token: Some("c".repeat(150)),
            client_id: Some("client-id".to_string()),
            token_endpoint: Some(endpoint),
            ..Default::default()
        };

        let new_id = manager.add_credential(new_cred).await.unwrap();
        assert_eq!(
            new_id, 3,
            "已删除账号 #2 的 ID 不应被复用，新账号应分配 #3，实际: {}",
            new_id
        );
    }

    /// 回归测试：账号删除后重启进程（重新构造 MultiTokenManager），新增账号仍不得
    /// 复用已删除账号曾经使用过的 ID —— 覆盖"仅按当前 credentials 列表最大值 + 1"
    /// 分配 ID 在重启场景下的复用漏洞（持久化计数器文件是本次修复的核心）。
    #[tokio::test(flavor = "multi_thread")]
    async fn test_add_credential_never_reuses_deleted_id_after_restart() {
        let dir_guard = TempDirGuard::new(&format!("k2cc_id_reuse_restart_{}", std::process::id()));
        let cred_path = dir_guard.path().join("credentials.json");

        let mut cred1 = KiroCredentials::default();
        cred1.id = Some(1);
        cred1.refresh_token = Some("a".repeat(150));

        let mut cred2 = KiroCredentials::default();
        cred2.id = Some(2);
        cred2.refresh_token = Some("b".repeat(150));

        let config = Config::default();
        let manager = MultiTokenManager::new(
            config.clone(),
            vec![cred1, cred2],
            None,
            Some(cred_path.clone()),
            true,
        )
        .unwrap();

        // 删除账号 #2（必须先禁用），并持久化
        manager.set_disabled(2, true).unwrap();
        manager.delete_credential(2).unwrap();
        manager.persist_credentials().unwrap();
        assert_eq!(manager.total_count(), 1);

        // 模拟进程重启：从磁盘重新读取 credentials.json（此时只剩 #1），
        // 重新构造一个全新的 MultiTokenManager 实例
        let persisted: Vec<KiroCredentials> =
            serde_json::from_str(&std::fs::read_to_string(&cred_path).unwrap()).unwrap();
        assert_eq!(persisted.len(), 1, "磁盘上应只剩 1 个账号");

        let reloaded =
            MultiTokenManager::new(config, persisted, None, Some(cred_path.clone()), true).unwrap();
        assert_eq!(reloaded.total_count(), 1);

        // "重启"后新增账号：若仅按当前列表 max(1) + 1 = 2 分配，将复用已删除账号 #2 的 ID
        let body = r#"{"access_token":"new-access-token","expires_in":3600}"#;
        let endpoint = spawn_single_response_server(200, body).await;
        let new_cred = KiroCredentials {
            auth_method: Some("external_idp".to_string()),
            refresh_token: Some("d".repeat(150)),
            client_id: Some("client-id".to_string()),
            token_endpoint: Some(endpoint),
            ..Default::default()
        };

        let new_id = reloaded.add_credential(new_cred).await.unwrap();
        assert_eq!(
            new_id, 3,
            "重启后新增账号仍不应复用已删除账号 #2 的 ID，实际: {}",
            new_id
        );
    }

    /// 回归测试：启动加载时为无 profileArn 的 social/idc 存量账号自动补全 fallback ARN
    /// 并写回配置文件（覆盖旧版本添加的账号，如 BuilderId 注册但 authMethod 标 idc 的形态）。
    #[test]
    fn test_new_fills_missing_profile_arn_and_persists() {
        let dir_guard = TempDirGuard::new(&format!("k2cc_fill_arn_load_{}", std::process::id()));
        let cred_path = dir_guard.path().join("credentials.json");

        let mut cred = KiroCredentials::default();
        cred.id = Some(14);
        cred.auth_method = Some("idc".to_string());
        cred.client_id = Some("client-id".to_string());
        cred.client_secret = Some("client-secret".to_string());
        cred.refresh_token = Some("r".repeat(150));

        let config = Config::default();
        let manager =
            MultiTokenManager::new(config, vec![cred], None, Some(cred_path.clone()), true)
                .unwrap();

        // 内存中已补全
        let ids = manager.credential_ids();
        assert_eq!(ids, vec![14]);
        // 触发写回：磁盘上的 credentials.json 应包含补全后的占位符 ARN
        let persisted: Vec<KiroCredentials> =
            serde_json::from_str(&std::fs::read_to_string(&cred_path).unwrap()).unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(
            persisted[0].profile_arn.as_deref(),
            Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN),
            "存量账号的 profileArn 应已补全并持久化"
        );
    }

    /// 回归测试：add_credential 添加链路会调用 fill_missing_profile_arn，
    /// 对 external_idp（不补全类型）保守跳过。
    ///
    /// 注：idc/social 形态的刷新分别硬编码请求真实 AWS OIDC / Kiro OAuth 端点
    /// （无 tokenEndpoint 注入点，external_idp 的 tokenEndpoint 仅 external_idp
    /// 刷新路径消费），单测无法 mock，故正向补全覆盖落在：
    /// - credentials.rs 纯函数单测（fill_missing_profile_arn 各分支）
    /// - MultiTokenManager::new 加载路径持久化测试（test_new_fills_missing_profile_arn_and_persists）
    /// 此处用可 mock 的 external_idp 形态验证添加链路走通且对不补全类型保守跳过。
    #[tokio::test]
    async fn test_add_credential_skips_profile_arn_fill_for_external_idp() {
        let body = r#"{"access_token":"new-access-token","expires_in":3600}"#;
        let endpoint = spawn_single_response_server(200, body).await;

        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("external_idp".to_string());
        cred.refresh_token = Some("c".repeat(150));
        cred.client_id = Some("client-id".to_string());
        cred.token_endpoint = Some(endpoint);

        let config = Config::default();
        let manager = MultiTokenManager::new(config, vec![], None, None, false).unwrap();
        let new_id = manager.add_credential(cred).await.unwrap();

        let entries = manager.entries.lock();
        let added = entries.iter().find(|e| e.id == new_id).unwrap();
        assert!(
            added.credentials.profile_arn.is_none(),
            "external_idp 不应补全 ARN（真实 ARN 因租户而异）"
        );
    }

    /// 回归测试：并发调用 `allocate_new_id` 时分配的 ID 必须两两不同。
    ///
    /// `allocate_new_id` 依赖 `AtomicU64::fetch_add` 保证内存中的分配互斥唯一，但落盘
    /// 由 `save_id_counter_at_least` 负责——本测试只验证内存分配层的并发唯一性（磁盘落盘
    /// 的单调性由 `save_id_counter_at_least` 内部锁内重新读取磁盘取 max 后写入来保证，
    /// 已在实现中处理，不依赖此测试）。
    #[tokio::test(flavor = "multi_thread")]
    async fn test_allocate_new_id_concurrent_calls_never_collide() {
        let config = Config::default();
        let manager =
            std::sync::Arc::new(MultiTokenManager::new(config, vec![], None, None, false).unwrap());

        let mut handles = Vec::new();
        for _ in 0..20 {
            let m = manager.clone();
            handles.push(tokio::spawn(async move { m.allocate_new_id() }));
        }
        let mut ids: Vec<u64> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        ids.sort_unstable();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(
            unique.len(),
            ids.len(),
            "并发分配的 ID 不应出现重复，实际: {:?}",
            ids
        );
    }

    // MultiTokenManager 测试

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

    #[test]
    fn test_credential_region_priority_uses_credential_auth_region() {
        // 账号配置了 auth_region 时，应使用账号的 auth_region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.auth_region = Some("eu-west-1".to_string());

        let region = credentials.effective_auth_region(&config);
        assert_eq!(region, "eu-west-1");
    }

    #[test]
    fn test_credential_region_priority_fallback_to_credential_region() {
        // 账号未配置 auth_region 但配置了 region 时，应回退到账号.region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.region = Some("eu-central-1".to_string());

        let region = credentials.effective_auth_region(&config);
        assert_eq!(region, "eu-central-1");
    }

    #[test]
    fn test_credential_region_priority_fallback_to_config() {
        // 账号未配置 auth_region 和 region 时，应回退到 config
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let credentials = KiroCredentials::default();
        assert!(credentials.auth_region.is_none());
        assert!(credentials.region.is_none());

        let region = credentials.effective_auth_region(&config);
        assert_eq!(region, "us-west-2");
    }

    #[test]
    fn test_multiple_credentials_use_respective_regions() {
        // 多账号场景下，不同账号使用各自的 auth_region
        let mut config = Config::default();
        config.region = "ap-northeast-1".to_string();

        let mut cred1 = KiroCredentials::default();
        cred1.auth_region = Some("us-east-1".to_string());

        let mut cred2 = KiroCredentials::default();
        cred2.region = Some("eu-west-1".to_string());

        let cred3 = KiroCredentials::default(); // 无 region，使用 config

        assert_eq!(cred1.effective_auth_region(&config), "us-east-1");
        assert_eq!(cred2.effective_auth_region(&config), "eu-west-1");
        assert_eq!(cred3.effective_auth_region(&config), "ap-northeast-1");
    }

    #[test]
    fn test_idc_oidc_endpoint_uses_credential_auth_region() {
        // 验证 IdC OIDC endpoint URL 使用账号 auth_region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.auth_region = Some("eu-central-1".to_string());

        let region = credentials.effective_auth_region(&config);
        let refresh_url = format!("https://oidc.{}.amazonaws.com/token", region);

        assert_eq!(refresh_url, "https://oidc.eu-central-1.amazonaws.com/token");
    }

    #[test]
    fn test_apply_idc_refresh_response_saves_both_id_and_access_token() {
        // issue #31：IdC 刷新后需同时保存 idToken（数据面用，写入 access_token 字段）
        // 和 accessToken（控制面 getUsageLimits 用，写入新增的 sso_access_token 字段），
        // 二者不能相互覆盖。
        let mut credentials = KiroCredentials {
            auth_method: Some("idc".to_string()),
            ..Default::default()
        };
        let data = IdcRefreshResponse {
            access_token: "sso-portal-access-token".to_string(),
            id_token: Some("jwt-id-token".to_string()),
            refresh_token: Some("new-refresh-token".to_string()),
            expires_in: Some(3600),
        };

        apply_idc_refresh_response(&mut credentials, data);

        assert_eq!(
            credentials.access_token.as_deref(),
            Some("jwt-id-token"),
            "access_token 字段应保存 idToken（供数据面接口使用）"
        );
        assert_eq!(
            credentials.sso_access_token.as_deref(),
            Some("sso-portal-access-token"),
            "sso_access_token 字段应保存原始 accessToken（供 getUsageLimits 使用）"
        );
        assert_eq!(
            credentials.refresh_token.as_deref(),
            Some("new-refresh-token")
        );
        assert!(credentials.expires_at.is_some());
    }

    #[test]
    fn test_apply_idc_refresh_response_falls_back_when_no_id_token() {
        // 若上游响应未返回 id_token（历史/异常场景），access_token 字段回退用
        // accessToken 顶替，保持与 commit 4f39dd7 之前一致的 fallback 行为；
        // sso_access_token 仍然独立保存该值。
        let mut credentials = KiroCredentials {
            auth_method: Some("idc".to_string()),
            ..Default::default()
        };
        let data = IdcRefreshResponse {
            access_token: "only-access-token".to_string(),
            id_token: None,
            refresh_token: None,
            expires_in: None,
        };

        apply_idc_refresh_response(&mut credentials, data);

        assert_eq!(
            credentials.access_token.as_deref(),
            Some("only-access-token")
        );
        assert_eq!(
            credentials.sso_access_token.as_deref(),
            Some("only-access-token")
        );
    }

    #[test]
    fn test_select_usage_limits_token_idc_prefers_sso_access_token() {
        // issue #31 核心修复：IdC 账号查询 getUsageLimits 时，即便传入的 token
        // 参数是 idToken（credentials.access_token），也应优先使用
        // sso_access_token（原始 accessToken），避免 403 Invalid token。
        let credentials = KiroCredentials {
            auth_method: Some("idc".to_string()),
            access_token: Some("id-token-for-data-plane".to_string()),
            sso_access_token: Some("sso-access-token-for-control-plane".to_string()),
            ..Default::default()
        };

        let selected = select_usage_limits_token(&credentials, "id-token-for-data-plane");

        assert_eq!(selected, "sso-access-token-for-control-plane");
    }

    #[test]
    fn test_select_usage_limits_token_idc_falls_back_without_sso_token() {
        // 旧版 credentials.json（尚无 sso_access_token 字段）或尚未刷新过的 IdC
        // 账号，应回退使用传入的 token 参数，保持向后兼容，不 panic、不报错。
        let credentials = KiroCredentials {
            auth_method: Some("idc".to_string()),
            access_token: Some("legacy-id-token".to_string()),
            sso_access_token: None,
            ..Default::default()
        };

        let selected = select_usage_limits_token(&credentials, "legacy-id-token");

        assert_eq!(selected, "legacy-id-token");
    }

    #[test]
    fn test_select_usage_limits_token_non_idc_ignores_sso_access_token() {
        // 非 IdC 账号（social/external_idp）逻辑不受影响：即便 sso_access_token
        // 意外被设置，也应始终使用传入的 token 参数。
        let credentials = KiroCredentials {
            auth_method: Some("social".to_string()),
            access_token: Some("social-access-token".to_string()),
            sso_access_token: Some("should-be-ignored".to_string()),
            ..Default::default()
        };

        let selected = select_usage_limits_token(&credentials, "social-access-token");

        assert_eq!(selected, "social-access-token");
    }

    #[test]
    fn test_sso_access_token_not_serialized_when_none() {
        // 向后兼容：旧版 credentials.json 没有 sso_access_token 字段，
        // 序列化时该字段为 None 也不应输出到 JSON，避免污染旧格式文件。
        let credentials = KiroCredentials {
            auth_method: Some("idc".to_string()),
            ..Default::default()
        };

        let json = serde_json::to_string(&credentials).unwrap();
        assert!(!json.contains("ssoAccessToken"));
    }

    #[test]
    fn test_sso_access_token_roundtrip_serialization() {
        // camelCase 序列化/反序列化正确性验证
        let credentials = KiroCredentials {
            auth_method: Some("idc".to_string()),
            sso_access_token: Some("abc123".to_string()),
            ..Default::default()
        };

        let json = serde_json::to_string(&credentials).unwrap();
        assert!(json.contains("\"ssoAccessToken\":\"abc123\""));

        let parsed: KiroCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.sso_access_token.as_deref(), Some("abc123"));
    }

    #[test]
    fn test_legacy_credentials_json_without_sso_access_token_deserializes() {
        // 向后兼容：不含 ssoAccessToken 字段的旧版 credentials.json 应能正常解析，
        // 且新字段默认为 None。
        let legacy_json = r#"{
            "accessToken": "old-token",
            "refreshToken": "refresh-abc",
            "authMethod": "idc",
            "clientId": "client-1",
            "clientSecret": "secret-1"
        }"#;

        let parsed: KiroCredentials = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(parsed.access_token.as_deref(), Some("old-token"));
        assert_eq!(parsed.sso_access_token, None);
    }

    #[test]
    fn test_social_refresh_endpoint_uses_credential_auth_region() {
        // 验证 Social refresh endpoint URL 使用账号 auth_region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.auth_region = Some("ap-southeast-1".to_string());

        let region = credentials.effective_auth_region(&config);
        let refresh_url = format!("https://prod.{}.auth.desktop.kiro.dev/refreshToken", region);

        assert_eq!(
            refresh_url,
            "https://prod.ap-southeast-1.auth.desktop.kiro.dev/refreshToken"
        );
    }

    /// 启动一个仅响应一次请求的本地 TCP 服务，返回固定的 HTTP 状态码 + body。
    ///
    /// 不引入 mock server crate（design.md 决策 6）：`external_idp` 的
    /// `token_endpoint` 是账号级可配置 URL，可以直接指向本机地址，用已有的
    /// tokio `net`（`full` feature 已启用）搭建裸响应即可，无需新依赖。
    async fn spawn_single_response_server(status: u16, body: &'static str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status,
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        format!("http://{}/token", addr)
    }

    #[tokio::test]
    async fn test_refresh_external_idp_invalid_grant_returns_typed_error() {
        let body =
            r#"{"error":"invalid_grant","error_description":"Invalid refresh token provided"}"#;
        let endpoint = spawn_single_response_server(400, body).await;

        let credentials = KiroCredentials {
            auth_method: Some("external_idp".to_string()),
            refresh_token: Some("short-refresh-token".to_string()),
            client_id: Some("client-id".to_string()),
            token_endpoint: Some(endpoint),
            ..Default::default()
        };

        let config = Config::default();
        let err = refresh_token(&credentials, &config, None)
            .await
            .unwrap_err();

        assert!(
            err.downcast_ref::<RefreshTokenInvalidError>().is_some(),
            "external_idp 400+invalid_grant 应返回 RefreshTokenInvalidError，实际: {}",
            err
        );
    }

    #[test]
    fn test_api_call_uses_effective_api_region() {
        // 验证 API 调用使用 effective_api_region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.region = Some("eu-west-1".to_string());

        // 账号.region 不参与 api_region 回退链
        let api_region = credentials.effective_api_region(&config);
        let api_host = format!("q.{}.amazonaws.com", api_region);

        assert_eq!(api_host, "q.us-west-2.amazonaws.com");
    }

    #[test]
    fn test_api_call_uses_credential_api_region() {
        // 账号配置了 api_region 时，API 调用应使用账号的 api_region
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.api_region = Some("eu-central-1".to_string());

        let api_region = credentials.effective_api_region(&config);
        let api_host = format!("q.{}.amazonaws.com", api_region);

        assert_eq!(api_host, "q.eu-central-1.amazonaws.com");
    }

    #[test]
    fn test_credential_region_empty_string_treated_as_set() {
        // 空字符串 auth_region 被视为已设置（虽然不推荐，但行为应一致）
        let mut config = Config::default();
        config.region = "us-west-2".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.auth_region = Some("".to_string());

        let region = credentials.effective_auth_region(&config);
        // 空字符串被视为已设置，不会回退到 config
        assert_eq!(region, "");
    }

    #[test]
    fn test_auth_and_api_region_independent() {
        // auth_region 和 api_region 互不影响
        let mut config = Config::default();
        config.region = "default".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.auth_region = Some("auth-only".to_string());
        credentials.api_region = Some("api-only".to_string());

        assert_eq!(credentials.effective_auth_region(&config), "auth-only");
        assert_eq!(credentials.effective_api_region(&config), "api-only");
    }

    // ============ sticky cache 测试 ============

    /// 测试专用临时目录：Drop 时清理，即使中途 assert! panic 也不残留（标准
    /// 库 unwind 会执行 Drop），避免 CI 机器上堆积垂悬的重启测试临时目录。
    struct TempDirGuard(std::path::PathBuf);

    impl TempDirGuard {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn make_valid_cred(token: &str) -> KiroCredentials {
        let mut c = KiroCredentials::default();
        c.access_token = Some(token.to_string());
        c.expires_at = Some((Utc::now() + Duration::hours(1)).to_rfc3339());
        c
    }

    #[tokio::test]
    async fn test_sticky_cache_no_continuation_id_falls_back() {
        let config = Config::default();
        let manager =
            MultiTokenManager::new(config, vec![make_valid_cred("t1")], None, None, false).unwrap();

        // continuation_id = None 时正常返回账号
        let ctx = manager
            .acquire_context_sticky(None, &[], None, &[])
            .await
            .unwrap();
        assert_eq!(ctx.token, "t1");
    }

    #[tokio::test]
    async fn test_sticky_cache_same_id_returns_same_credential() {
        let config = Config::default();
        let cred1 = make_valid_cred("t1");
        let cred2 = make_valid_cred("t2");
        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 首次调用选定某账号
        let ctx1 = manager
            .acquire_context_sticky(None, &[], Some("session-abc"), &[])
            .await
            .unwrap();
        // 再次调用同一 continuation_id，应返回同一账号
        let ctx2 = manager
            .acquire_context_sticky(None, &[], Some("session-abc"), &[])
            .await
            .unwrap();
        assert_eq!(ctx1.id, ctx2.id);
    }

    #[tokio::test]
    async fn test_sticky_throttle_below_threshold_keeps_binding() {
        let config = Config::default();
        let manager = MultiTokenManager::new(
            config,
            vec![make_valid_cred("t1"), make_valid_cred("t2")],
            None,
            None,
            false,
        )
        .unwrap();

        let ctx = manager
            .acquire_context_sticky(None, &[], Some("session-throttle"), &[])
            .await
            .unwrap();

        // 阈值以下的连续限流不应解绑，保住已建立的 prompt cache
        for _ in 0..(STICKY_THROTTLE_EVICT_THRESHOLD - 1) {
            assert!(!manager.report_sticky_throttled("session-throttle", ctx.id));
        }

        let again = manager
            .acquire_context_sticky(None, &[], Some("session-throttle"), &[])
            .await
            .unwrap();
        assert_eq!(ctx.id, again.id);
    }

    #[tokio::test]
    async fn test_sticky_throttle_reaching_threshold_evicts() {
        let config = Config::default();
        let manager = MultiTokenManager::new(
            config,
            vec![make_valid_cred("t1"), make_valid_cred("t2")],
            None,
            None,
            false,
        )
        .unwrap();

        let ctx = manager
            .acquire_context_sticky(None, &[], Some("session-evict"), &[])
            .await
            .unwrap();

        let mut evicted = false;
        for _ in 0..STICKY_THROTTLE_EVICT_THRESHOLD {
            evicted = manager.report_sticky_throttled("session-evict", ctx.id);
        }
        assert!(evicted);
        assert!(!manager.sticky_cache.lock().contains_key("session-evict"));
    }

    #[tokio::test]
    async fn test_sticky_avoid_switches_credential_but_keeps_binding() {
        // 请求内重试：绑定账号刚被限流时应换账号完成本次调用，
        // 但绑定关系必须保留，下次请求仍回到原账号命中 prompt cache。
        let config = Config::default();
        let manager = MultiTokenManager::new(
            config,
            vec![make_valid_cred("t1"), make_valid_cred("t2")],
            None,
            None,
            false,
        )
        .unwrap();

        let bound = manager
            .acquire_context_sticky(None, &[], Some("session-avoid"), &[])
            .await
            .unwrap();

        // 避让绑定账号：应拿到另一个账号
        let retry = manager
            .acquire_context_sticky(None, &[], Some("session-avoid"), &[bound.id])
            .await
            .unwrap();
        assert_ne!(retry.id, bound.id);

        // 绑定未被改写，也未被删除
        assert_eq!(
            manager
                .sticky_cache
                .lock()
                .get("session-avoid")
                .map(|e| e.credential_id),
            Some(bound.id)
        );

        // 下一次正常请求回到原账号
        let back = manager
            .acquire_context_sticky(None, &[], Some("session-avoid"), &[])
            .await
            .unwrap();
        assert_eq!(back.id, bound.id);
    }

    #[tokio::test]
    async fn test_sticky_avoid_all_falls_back_instead_of_failing() {
        // 所有候选账号都已在本次请求内限流时，不能因避让而彻底失败，
        // 应回退到原选择逻辑，由上层重试与退避处理。
        let config = Config::default();
        let manager =
            MultiTokenManager::new(config, vec![make_valid_cred("t1")], None, None, false).unwrap();

        let bound = manager
            .acquire_context_sticky(None, &[], Some("session-avoid-all"), &[])
            .await
            .unwrap();

        let ctx = manager
            .acquire_context_sticky(None, &[], Some("session-avoid-all"), &[bound.id])
            .await
            .unwrap();
        assert_eq!(ctx.id, bound.id);
    }

    #[tokio::test]
    async fn test_sticky_hit_resets_throttle_counter() {
        let config = Config::default();
        let manager = MultiTokenManager::new(
            config,
            vec![make_valid_cred("t1"), make_valid_cred("t2")],
            None,
            None,
            false,
        )
        .unwrap();

        let ctx = manager
            .acquire_context_sticky(None, &[], Some("session-reset"), &[])
            .await
            .unwrap();

        assert!(!manager.report_sticky_throttled("session-reset", ctx.id));
        // 一次成功命中即清零，避免跨越较长时间的零散限流累积成解绑
        manager
            .acquire_context_sticky(None, &[], Some("session-reset"), &[])
            .await
            .unwrap();
        assert_eq!(
            manager
                .sticky_cache
                .lock()
                .get("session-reset")
                .map(|e| e.consecutive_throttles),
            Some(0)
        );
    }

    #[tokio::test]
    async fn test_sticky_cache_ttl_expired_reselects() {
        let config = Config::default();
        let cred1 = make_valid_cred("t1");
        let cred2 = make_valid_cred("t2");
        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 手动写入一条已过期的条目，指向账号 #1
        manager.insert_expired_sticky_entry("session-xyz", 1);

        // 过期后应重新选择（不一定是 #1）
        let ctx = manager
            .acquire_context_sticky(None, &[], Some("session-xyz"), &[])
            .await
            .unwrap();
        // 只要能正常返回账号即可；过期条目已被替换
        assert!(ctx.token == "t1" || ctx.token == "t2");

        // 新写入的条目应未过期
        let cache = manager.sticky_cache.lock();
        let entry = cache.get("session-xyz").unwrap();
        assert!(entry.inserted_at.elapsed() < STICKY_CACHE_TTL);
    }

    #[tokio::test]
    async fn test_sticky_cache_balanced_mode_bypasses_round_robin() {
        let mut config = Config::default();
        config.load_balancing_mode = "balanced".to_string();
        let cred1 = make_valid_cred("t1");
        let cred2 = make_valid_cred("t2");
        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // balanced 模式下，无 sticky cache 时 round-robin 会轮转 t1→t2→t1→t2
        // 有 sticky cache 时，同一 continuation_id 应始终返回同一账号
        let ctx1 = manager
            .acquire_context_sticky(None, &[], Some("session-balanced"), &[])
            .await
            .unwrap();
        let expected_id = ctx1.id;

        for _ in 0..5 {
            let ctx = manager
                .acquire_context_sticky(None, &[], Some("session-balanced"), &[])
                .await
                .unwrap();
            assert_eq!(
                ctx.id, expected_id,
                "balanced 模式下 sticky cache 应固定路由到同一账号"
            );
        }
    }

    #[tokio::test]
    async fn test_sticky_cache_disabled_credential_evicted() {
        let config = Config::default();
        let cred1 = make_valid_cred("t1");
        let cred2 = make_valid_cred("t2");
        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 首次调用建立绑定
        let ctx1 = manager
            .acquire_context_sticky(None, &[], Some("session-dis"), &[])
            .await
            .unwrap();
        let bound_id = ctx1.id;

        // 禁用已绑定的账号
        manager.report_quota_exhausted(bound_id);

        // 再次调用同一 continuation_id：缓存命中但账号已禁用，应驱逐并重选
        let ctx2 = manager
            .acquire_context_sticky(None, &[], Some("session-dis"), &[])
            .await
            .unwrap();
        // 返回另一个账号
        assert_ne!(ctx2.id, bound_id);
    }

    #[tokio::test]
    async fn test_acquire_context_filtered_refresh_failure_counts_and_disables() {
        // 缺少 refreshToken 会在 validate_refresh_token 阶段失败（无需真实网络请求），
        // 命中 acquire_context_filtered 的 Err(e) 分支，验证其也接入了与 acquire_context
        // 相同的分类/计数/禁用逻辑（覆盖 T10）
        let config = Config::default();
        // 无 access_token/refresh_token，强制走刷新且必然失败
        let cred1 = KiroCredentials {
            expires_at: Some((Utc::now() - Duration::hours(1)).to_rfc3339()),
            ..Default::default()
        };
        let mut cred2 = make_valid_cred("t2");
        // 更低优先级数值 = 更高优先级，固定 #1 为首选，避免同优先级 round-robin 导致选择不确定
        cred2.priority = 1;
        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            let ctx = manager
                .acquire_context_filtered(None, &[1, 2])
                .await
                .unwrap();
            // 白名单内 #1 必然失败，最终应回退到 #2
            assert_eq!(ctx.id, 2);
        }

        let entries = manager.entries.lock();
        let entry = entries.iter().find(|e| e.id == 1).unwrap();
        assert!(entry.disabled);
        assert_eq!(
            entry.disabled_reason,
            Some(DisabledReason::TooManyRefreshFailures)
        );
    }

    #[tokio::test]
    async fn test_acquire_context_sticky_production_path_classifies_invalid_grant() {
        // 验证生产路径 acquire_context_sticky 命中缓存后的 Err(e) 分支确实接入了
        // RefreshTokenInvalidError 分类逻辑（覆盖 T11 / design.md 决策 10 的核心风险点：
        // provider.rs 实际调用的是 acquire_context_sticky，遗漏该路径等于本次改动未生效）
        let config = Config::default();
        let cred1 = make_valid_cred("t1");
        let cred2 = make_valid_cred("t2");
        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 首次调用建立 sticky 绑定
        let ctx1 = manager
            .acquire_context_sticky(None, &[], Some("session-invalid-grant"), &[])
            .await
            .unwrap();
        let bound_id = ctx1.id;

        // 让已绑定账号的下一次刷新命中 invalid_grant
        let body =
            r#"{"error":"invalid_grant","error_description":"Invalid refresh token provided"}"#;
        let endpoint = spawn_single_response_server(400, body).await;
        {
            let mut entries = manager.entries.lock();
            let entry = entries.iter_mut().find(|e| e.id == bound_id).unwrap();
            entry.credentials.expires_at = Some((Utc::now() - Duration::hours(1)).to_rfc3339());
            entry.credentials.auth_method = Some("external_idp".to_string());
            entry.credentials.client_id = Some("client-id".to_string());
            entry.credentials.refresh_token = Some("short-refresh-token".to_string());
            entry.credentials.token_endpoint = Some(endpoint);
        }

        // 命中同一 continuation_id：走缓存命中分支，刷新失败应驱逐并重选到另一账号
        let ctx2 = manager
            .acquire_context_sticky(None, &[], Some("session-invalid-grant"), &[])
            .await
            .unwrap();
        assert_ne!(ctx2.id, bound_id);

        let entries = manager.entries.lock();
        let entry = entries.iter().find(|e| e.id == bound_id).unwrap();
        assert!(entry.disabled);
        assert_eq!(
            entry.disabled_reason,
            Some(DisabledReason::InvalidRefreshToken)
        );
        assert_eq!(
            entry.refresh_failure_count, 0,
            "invalid_grant 不应计入 refresh_failure_count"
        );
    }

    #[tokio::test]
    async fn test_quota_disabled_recovers_after_month_rollover() {
        // 回归：月度额度按自然月重置，跨月后账号必须自动回到可用池，
        // 而不是停留在 disabled 直到人工去 admin 面板启用。
        let config = Config::default();
        let manager = MultiTokenManager::new(
            config,
            vec![make_valid_cred("t1"), make_valid_cred("t2")],
            None,
            None,
            false,
        )
        .unwrap();

        manager.report_quota_exhausted(1);
        manager.report_quota_exhausted(2);
        assert_eq!(manager.available_count(), 0);

        // 同月内不得恢复
        assert_eq!(manager.recover_expired_quota_disables(), 0);
        assert_eq!(manager.available_count(), 0);

        // 把耗尽时间回拨到上个月，模拟跨月
        {
            let mut entries = manager.entries.lock();
            for e in entries.iter_mut() {
                e.quota_exhausted_at = Some(Utc::now() - Duration::days(45));
            }
        }

        let ctx = manager.acquire_context(None).await.unwrap();
        assert!(ctx.token == "t1" || ctx.token == "t2");
        assert_eq!(manager.available_count(), 2);

        let entries = manager.entries.lock();
        for e in entries.iter() {
            assert_eq!(e.disabled_reason, None, "恢复后禁用原因应清空");
            assert_eq!(e.quota_exhausted_at, None, "恢复后耗尽时间戳应清空");
            assert_eq!(e.failure_count, 0, "恢复后失败计数应清零");
        }
    }

    #[test]
    fn test_quota_disabled_missing_timestamp_is_recoverable() {
        // 旧版本持久化数据没有 quota_exhausted_at，不得因此被永久钉死
        let config = Config::default();
        let manager =
            MultiTokenManager::new(config, vec![make_valid_cred("t1")], None, None, false).unwrap();

        manager.report_quota_exhausted(1);
        {
            let mut entries = manager.entries.lock();
            entries[0].quota_exhausted_at = None;
        }

        assert_eq!(manager.recover_expired_quota_disables(), 1);
        assert_eq!(manager.available_count(), 1);
    }

    #[test]
    fn test_describe_unavailable_distinguishes_reasons() {
        // 核心诊断能力：三种禁用原因不得塌缩成同一句"均已禁用"
        let config = Config::default();
        let manager = MultiTokenManager::new(
            config,
            vec![
                make_valid_cred("t1"),
                make_valid_cred("t2"),
                make_valid_cred("t3"),
            ],
            None,
            None,
            false,
        )
        .unwrap();

        manager.report_quota_exhausted(1);
        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            manager.report_failure(2);
        }
        manager.set_disabled(3, true).unwrap();

        let msg = manager.describe_unavailable(None, &[]);
        assert!(msg.contains("1 个额度用尽"), "实际: {}", msg);
        assert!(msg.contains("1 个连续认证失败"), "实际: {}", msg);
        assert!(msg.contains("1 个手动禁用"), "实际: {}", msg);
        // 混合原因时不应带 402 标记（只有全部因额度耗尽才可判定不可重试）
        assert!(
            !msg.contains(QUOTA_EXHAUSTED_ALL_MARKER),
            "混合原因不应标记为额度耗尽，实际: {}",
            msg
        );
    }

    #[test]
    fn test_report_profile_arn_missing_disables_immediately() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();
        let cred2 = KiroCredentials::default();

        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        assert_eq!(manager.available_count(), 2);
        assert!(manager.report_profile_arn_missing(1));
        assert_eq!(manager.available_count(), 1);

        {
            let entries = manager.entries.lock();
            assert!(entries[0].disabled);
            assert_eq!(
                entries[0].disabled_reason,
                Some(DisabledReason::ProfileArnMissing)
            );
            assert_eq!(entries[0].failure_count, MAX_FAILURES_PER_CREDENTIAL);
        }

        // 再禁用第二个后，无可用账号
        assert!(!manager.report_profile_arn_missing(2));
        assert_eq!(manager.available_count(), 0);
    }

    #[test]
    fn test_describe_unavailable_profile_arn_missing_has_dedicated_label() {
        let config = Config::default();
        let cred1 = KiroCredentials::default();

        let manager = MultiTokenManager::new(config, vec![cred1], None, None, false).unwrap();

        manager.report_profile_arn_missing(1);

        let msg = manager.describe_unavailable(None, &[]);
        assert!(msg.contains("1 个缺少 profileArn"), "实际: {}", msg);
        // 不应误报为连续认证失败（issue 场景的核心误导点）
        assert!(!msg.contains("连续认证失败"), "实际: {}", msg);
    }

    #[test]
    fn test_describe_unavailable_respects_bound_scope() {
        // 绑定账号白名单场景：只统计白名单内的账号
        let config = Config::default();
        let manager = MultiTokenManager::new(
            config,
            vec![make_valid_cred("t1"), make_valid_cred("t2")],
            None,
            None,
            false,
        )
        .unwrap();

        manager.report_quota_exhausted(1);

        // 白名单只含额度耗尽的 #1 → 应判定为全部额度耗尽
        let msg = manager.describe_unavailable(None, &[1]);
        assert!(msg.contains("绑定的账号"), "实际: {}", msg);
        assert!(msg.contains(QUOTA_EXHAUSTED_ALL_MARKER), "实际: {}", msg);
        assert!(msg.contains("共 1 个"), "实际: {}", msg);
    }

    #[test]
    fn test_describe_unavailable_model_scoped_excludes_non_opus_accounts() {
        // C1 回归：opus 专属账号全部额度耗尽，但池中还有一个不支持 opus 的
        // 健康账号时，不传 model 会被健康账号稀释掉 quota 计数，永远触发不了
        // 402 标记；传入 model 后必须正确排除不相关账号，判定为全部耗尽。
        let config = Config::default();
        let mut free_cred = make_valid_cred("free1");
        free_cred.subscription_title = Some("FREE".to_string());
        let manager = MultiTokenManager::new(
            config,
            vec![make_valid_cred("opus1"), free_cred],
            None,
            None,
            false,
        )
        .unwrap();

        manager.report_quota_exhausted(1);

        let msg_no_model = manager.describe_unavailable(None, &[]);
        assert!(
            !msg_no_model.contains(QUOTA_EXHAUSTED_ALL_MARKER),
            "不传 model 时健康的 FREE 账号会稀释 quota 计数，实际: {}",
            msg_no_model
        );

        let msg_opus = manager.describe_unavailable(Some("claude-opus-4-7"), &[]);
        assert!(
            msg_opus.contains(QUOTA_EXHAUSTED_ALL_MARKER),
            "opus model 过滤后应排除不支持 opus 的账号，判定为全部耗尽，实际: {}",
            msg_opus
        );
    }

    #[test]
    fn test_describe_unavailable_no_matching_model_returns_zero_total() {
        // total==0 分支：scope 内所有账号（而非部分）都不支持该模型，
        // 必须走"没有支持该模型的账号"分支，不能误报额度耗尽标记。
        let config = Config::default();
        let mut free_cred = make_valid_cred("free1");
        free_cred.subscription_title = Some("FREE".to_string());
        let manager = MultiTokenManager::new(config, vec![free_cred], None, None, false).unwrap();

        let msg = manager.describe_unavailable(Some("claude-opus-4-7"), &[]);
        assert!(
            !msg.contains(QUOTA_EXHAUSTED_ALL_MARKER),
            "全部账号均不支持该模型时不应误报额度耗尽标记，实际: {}",
            msg
        );
        assert!(
            msg.contains("没有支持该模型的账号"),
            "应走 total==0 分支提示无匹配模型账号，实际: {}",
            msg
        );
    }

    #[test]
    fn test_quota_disabled_reason_survives_restart() {
        // 回归：persist_credentials 会把 disabled=true 写回 credentials.json，
        // 重启后若一律推断为 Manual，额度耗尽的账号将永不自动恢复。
        let dir_guard = TempDirGuard::new(&format!("k2cc_quota_restart_{}", std::process::id()));
        let cred_path = dir_guard.path().join("credentials.json");

        let mut seed = make_valid_cred("t1");
        seed.id = Some(1);

        let config = Config::default();
        let manager = MultiTokenManager::new(
            config.clone(),
            vec![seed],
            None,
            Some(cred_path.clone()),
            true,
        )
        .unwrap();
        // 不手动调用 save_stats：report_quota_exhausted 内部必须自行立即落盘，
        // 否则进程崩溃/被杀会丢失关键禁用状态——手动补调用会掩盖这一验证目标
        manager.report_quota_exhausted(1);
        manager.persist_credentials().unwrap();

        // 从磁盘读回账号，模拟进程重启。
        // 额度耗尽不得写入 credentials.json 的 disabled —— 该字段只承载手动禁用意图
        let persisted: Vec<KiroCredentials> =
            serde_json::from_str(&std::fs::read_to_string(&cred_path).unwrap()).unwrap();
        assert!(
            !persisted[0].disabled,
            "额度耗尽不应写入 credentials.json，否则重启后退化为手动禁用"
        );

        let reloaded =
            MultiTokenManager::new(config, persisted, None, Some(cred_path), true).unwrap();

        {
            let entries = reloaded.entries.lock();
            assert_eq!(
                entries[0].disabled_reason,
                Some(DisabledReason::QuotaExceeded),
                "重启后禁用原因退化为 Manual，账号将被永久钉死"
            );
        }
    }

    #[test]
    fn test_too_many_failures_disabled_reason_survives_restart() {
        // 与 test_quota_disabled_reason_survives_restart 对称：TooManyFailures
        // 也必须只落盘到 kiro_stats.json，重启后不退化为 Manual、不被永久钉死。
        let dir_guard = TempDirGuard::new(&format!("k2cc_failures_restart_{}", std::process::id()));
        let cred_path = dir_guard.path().join("credentials.json");

        let mut seed = make_valid_cred("t1");
        seed.id = Some(1);

        let config = Config::default();
        let manager = MultiTokenManager::new(
            config.clone(),
            vec![seed],
            None,
            Some(cred_path.clone()),
            true,
        )
        .unwrap();
        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            manager.report_failure(1);
        }
        manager.persist_credentials().unwrap();

        let persisted: Vec<KiroCredentials> =
            serde_json::from_str(&std::fs::read_to_string(&cred_path).unwrap()).unwrap();
        assert!(
            !persisted[0].disabled,
            "连续失败禁用不应写入 credentials.json，否则重启后退化为手动禁用"
        );

        let reloaded =
            MultiTokenManager::new(config, persisted, None, Some(cred_path), true).unwrap();

        {
            let entries = reloaded.entries.lock();
            assert_eq!(
                entries[0].disabled_reason,
                Some(DisabledReason::TooManyFailures),
                "重启后禁用原因退化为 Manual，账号将被永久钉死"
            );
        }
    }

    #[test]
    fn test_invalid_refresh_token_disabled_reason_survives_restart() {
        // 对称于 test_quota_disabled_reason_survives_restart：InvalidRefreshToken
        // 必须只落盘到 kiro_stats.json，重启后不退化为 Manual，否则违反"需人工介入才能
        // 恢复"的设计目标（覆盖 T13 白名单扩展）
        let dir_guard = TempDirGuard::new(&format!(
            "k2cc_invalid_grant_restart_{}",
            std::process::id()
        ));
        let cred_path = dir_guard.path().join("credentials.json");

        let mut seed = make_valid_cred("t1");
        seed.id = Some(1);

        let config = Config::default();
        let manager = MultiTokenManager::new(
            config.clone(),
            vec![seed],
            None,
            Some(cred_path.clone()),
            true,
        )
        .unwrap();
        manager.report_refresh_token_invalid(1);
        manager.persist_credentials().unwrap();

        let persisted: Vec<KiroCredentials> =
            serde_json::from_str(&std::fs::read_to_string(&cred_path).unwrap()).unwrap();
        assert!(
            !persisted[0].disabled,
            "invalid_grant 禁用不应写入 credentials.json，否则重启后退化为手动禁用"
        );

        let reloaded =
            MultiTokenManager::new(config, persisted, None, Some(cred_path), true).unwrap();

        {
            let entries = reloaded.entries.lock();
            assert_eq!(
                entries[0].disabled_reason,
                Some(DisabledReason::InvalidRefreshToken),
                "重启后禁用原因退化为 Manual，账号将被永久钉死或错误自愈"
            );
        }
    }

    #[test]
    fn test_too_many_refresh_failures_disabled_reason_survives_restart() {
        // 对称于 test_too_many_failures_disabled_reason_survives_restart：
        // TooManyRefreshFailures 必须只落盘到 kiro_stats.json（覆盖 T13 白名单扩展）
        let dir_guard = TempDirGuard::new(&format!(
            "k2cc_refresh_failures_restart_{}",
            std::process::id()
        ));
        let cred_path = dir_guard.path().join("credentials.json");

        let mut seed = make_valid_cred("t1");
        seed.id = Some(1);

        let config = Config::default();
        let manager = MultiTokenManager::new(
            config.clone(),
            vec![seed],
            None,
            Some(cred_path.clone()),
            true,
        )
        .unwrap();
        for _ in 0..MAX_FAILURES_PER_CREDENTIAL {
            manager.report_refresh_failure(1);
        }
        manager.persist_credentials().unwrap();

        let persisted: Vec<KiroCredentials> =
            serde_json::from_str(&std::fs::read_to_string(&cred_path).unwrap()).unwrap();
        assert!(
            !persisted[0].disabled,
            "连续刷新失败禁用不应写入 credentials.json，否则重启后退化为手动禁用"
        );

        let reloaded =
            MultiTokenManager::new(config, persisted, None, Some(cred_path), true).unwrap();

        {
            let entries = reloaded.entries.lock();
            assert_eq!(
                entries[0].disabled_reason,
                Some(DisabledReason::TooManyRefreshFailures),
                "重启后禁用原因退化为 Manual，账号将被永久钉死"
            );
        }
    }

    #[test]
    fn test_validate_refresh_token_external_idp_skips_length_check() {
        // Azure AD refresh_token 可能短于 100 字符，external_idp 账号应跳过长度限制
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("external_idp".to_string());
        cred.refresh_token = Some("short_token_42".to_string()); // 远小于 100 字符
        assert!(
            validate_refresh_token(&cred).is_ok(),
            "external_idp 账号应跳过 100 字符下限"
        );
    }

    #[test]
    fn test_validate_refresh_token_social_still_enforces_length() {
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("social".to_string());
        cred.refresh_token = Some("short".to_string());
        assert!(
            validate_refresh_token(&cred).is_err(),
            "social 账号仍然应该强制 100 字符下限"
        );
    }

    #[test]
    fn test_validate_refresh_token_no_auth_method_enforces_length() {
        let mut cred = KiroCredentials::default();
        cred.auth_method = None;
        cred.refresh_token = Some("short".to_string());
        assert!(
            validate_refresh_token(&cred).is_err(),
            "未指定 auth_method 应使用默认长度限制"
        );
    }
}
