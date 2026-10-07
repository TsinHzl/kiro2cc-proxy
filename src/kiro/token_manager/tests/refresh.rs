// Copyright (c) 2026 Harllan He. Licensed under MIT.
// token_manager 测试（自 tests.rs 拆出，纯代码搬移）
#[cfg(test)]
pub(crate) mod tests {

    use super::super::super::refresh::{
        is_invalid_grant_response, sha256_hex, validate_refresh_token,
    };
    use super::super::super::types::MultiTokenManager;

    use crate::kiro::model::credentials::KiroCredentials;

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
    fn test_apply_update_fields_clears_subscription_for_identity_updates() {
        let updates = [
            crate::admin::types::UpdateCredentialRequest {
                refresh_token: Some("new-refresh-token".to_string()),
                ..Default::default()
            },
            crate::admin::types::UpdateCredentialRequest {
                auth_method: Some("idc".to_string()),
                ..Default::default()
            },
            crate::admin::types::UpdateCredentialRequest {
                client_id: Some("new-client-id".to_string()),
                ..Default::default()
            },
            crate::admin::types::UpdateCredentialRequest {
                client_secret: Some("new-client-secret".to_string()),
                ..Default::default()
            },
            crate::admin::types::UpdateCredentialRequest {
                profile_arn: Some("new-profile-arn".to_string()),
                ..Default::default()
            },
            crate::admin::types::UpdateCredentialRequest {
                provider: Some("Google".to_string()),
                ..Default::default()
            },
        ];

        for update in updates {
            let mut credentials = KiroCredentials {
                subscription_title: Some("KIRO PRO".to_string()),
                ..Default::default()
            };
            MultiTokenManager::apply_update_fields(&mut credentials, &update);
            assert_eq!(credentials.subscription_title, None);
        }
    }

    #[test]
    fn test_apply_update_fields_preserves_subscription_for_non_identity_updates() {
        let mut credentials = KiroCredentials {
            subscription_title: Some("KIRO PRO".to_string()),
            ..Default::default()
        };
        let update = crate::admin::types::UpdateCredentialRequest {
            nickname: Some("new-name".to_string()),
            thinking_adaptive: Some(true),
            ..Default::default()
        };

        MultiTokenManager::apply_update_fields(&mut credentials, &update);

        assert_eq!(credentials.subscription_title.as_deref(), Some("KIRO PRO"));
    }

    #[test]
    fn test_stale_subscription_title_cannot_overwrite_new_identity() {
        let queried_credentials = KiroCredentials {
            refresh_token: Some("old-refresh-token".to_string()),
            subscription_title: None,
            ..Default::default()
        };
        let manager = MultiTokenManager::new(
            crate::model::config::Config::default(),
            vec![queried_credentials.clone()],
            None,
            None,
            false,
        )
        .unwrap();
        manager.entries.lock()[0].credentials.refresh_token = Some("new-refresh-token".to_string());

        let error = manager
            .commit_subscription_title_if_identity_matches(
                1,
                &queried_credentials,
                Some("KIRO PRO"),
            )
            .unwrap_err();

        assert!(error.to_string().contains("身份已变更"));
        assert_eq!(
            manager.entries.lock()[0].credentials.subscription_title,
            None
        );
    }

    #[test]
    fn test_current_subscription_title_is_committed() {
        let initial_credentials = KiroCredentials {
            refresh_token: Some("current-refresh-token".to_string()),
            subscription_title: None,
            ..Default::default()
        };
        let manager = MultiTokenManager::new(
            crate::model::config::Config::default(),
            vec![initial_credentials],
            None,
            None,
            false,
        )
        .unwrap();
        let queried_credentials = manager.entries.lock()[0].credentials.clone();

        let changed = manager
            .commit_subscription_title_if_identity_matches(
                1,
                &queried_credentials,
                Some("KIRO PRO"),
            )
            .unwrap();

        assert!(changed);
        assert_eq!(
            manager.entries.lock()[0]
                .credentials
                .subscription_title
                .as_deref(),
            Some("KIRO PRO")
        );
    }

    #[test]
    fn test_unknown_subscription_clears_stale_pro_title() {
        let initial_credentials = KiroCredentials {
            refresh_token: Some("current-refresh-token".to_string()),
            subscription_title: Some("KIRO PRO".to_string()),
            ..Default::default()
        };
        let manager = MultiTokenManager::new(
            crate::model::config::Config::default(),
            vec![initial_credentials],
            None,
            None,
            false,
        )
        .unwrap();
        let queried_credentials = manager.entries.lock()[0].credentials.clone();

        let changed = manager
            .commit_subscription_title_if_identity_matches(1, &queried_credentials, None)
            .unwrap();

        assert!(changed);
        assert_eq!(
            manager.entries.lock()[0].credentials.subscription_title,
            None
        );
    }

    #[test]
    fn test_stale_token_refresh_cannot_overwrite_new_identity() {
        let initial_credentials = KiroCredentials {
            refresh_token: Some("old-refresh-token".to_string()),
            client_id: Some("old-client-id".to_string()),
            access_token: Some("old-access-token".to_string()),
            ..Default::default()
        };
        let manager = MultiTokenManager::new(
            crate::model::config::Config::default(),
            vec![initial_credentials],
            None,
            None,
            false,
        )
        .unwrap();
        let refreshing_credentials = manager.entries.lock()[0].credentials.clone();
        {
            let mut entries = manager.entries.lock();
            entries[0].credentials.refresh_token = Some("new-refresh-token".to_string());
            entries[0].credentials.client_id = Some("new-client-id".to_string());
            entries[0].credentials.access_token = Some("new-access-token".to_string());
        }
        let mut refreshed_credentials = refreshing_credentials.clone();
        refreshed_credentials.access_token = Some("stale-refreshed-access-token".to_string());
        refreshed_credentials.refresh_token = Some("stale-rotated-refresh-token".to_string());

        let error = manager
            .commit_refreshed_credentials_if_identity_matches(
                1,
                &refreshing_credentials,
                &refreshed_credentials,
            )
            .unwrap_err();

        assert!(error.to_string().contains("身份已变更"));
        let credentials = &manager.entries.lock()[0].credentials;
        assert_eq!(
            credentials.refresh_token.as_deref(),
            Some("new-refresh-token")
        );
        assert_eq!(credentials.client_id.as_deref(), Some("new-client-id"));
        assert_eq!(
            credentials.access_token.as_deref(),
            Some("new-access-token")
        );
    }

    #[test]
    fn test_token_refresh_preserves_concurrent_non_identity_updates() {
        let initial_credentials = KiroCredentials {
            refresh_token: Some("current-refresh-token".to_string()),
            access_token: Some("old-access-token".to_string()),
            nickname: Some("old-name".to_string()),
            subscription_title: Some("KIRO PRO".to_string()),
            ..Default::default()
        };
        let manager = MultiTokenManager::new(
            crate::model::config::Config::default(),
            vec![initial_credentials],
            None,
            None,
            false,
        )
        .unwrap();
        let refreshing_credentials = manager.entries.lock()[0].credentials.clone();
        {
            let mut entries = manager.entries.lock();
            let credentials = &mut entries[0].credentials;
            credentials.nickname = Some("new-name".to_string());
            credentials.email = Some("fictional@example.invalid".to_string());
            credentials.proxy_url = Some("direct".to_string());
            credentials.priority = 7;
            credentials.thinking_adaptive = true;
            credentials.subscription_title = Some("KIRO FREE".to_string());
        }
        let mut refreshed_credentials = refreshing_credentials.clone();
        refreshed_credentials.access_token = Some("refreshed-access-token".to_string());
        refreshed_credentials.sso_access_token = Some("refreshed-sso-token".to_string());
        refreshed_credentials.refresh_token = Some("rotated-refresh-token".to_string());
        refreshed_credentials.profile_arn = Some("refreshed-profile-arn".to_string());
        refreshed_credentials.expires_at = Some("2099-01-01T00:00:00Z".to_string());

        let committed = manager
            .commit_refreshed_credentials_if_identity_matches(
                1,
                &refreshing_credentials,
                &refreshed_credentials,
            )
            .unwrap();

        assert_eq!(
            committed.access_token.as_deref(),
            Some("refreshed-access-token")
        );
        assert_eq!(
            committed.sso_access_token.as_deref(),
            Some("refreshed-sso-token")
        );
        assert_eq!(
            committed.refresh_token.as_deref(),
            Some("rotated-refresh-token")
        );
        assert_eq!(
            committed.profile_arn.as_deref(),
            Some("refreshed-profile-arn")
        );
        assert_eq!(
            committed.expires_at.as_deref(),
            Some("2099-01-01T00:00:00Z")
        );
        assert_eq!(committed.nickname.as_deref(), Some("new-name"));
        assert_eq!(
            committed.email.as_deref(),
            Some("fictional@example.invalid")
        );
        assert_eq!(committed.proxy_url.as_deref(), Some("direct"));
        assert_eq!(committed.priority, 7);
        assert!(committed.thinking_adaptive);
        assert_eq!(committed.subscription_title.as_deref(), Some("KIRO FREE"));
    }

    #[tokio::test]
    async fn test_identity_update_waits_for_refresh_identity_lock() {
        let manager = std::sync::Arc::new(
            MultiTokenManager::new(
                crate::model::config::Config::default(),
                vec![KiroCredentials {
                    refresh_token: Some("current-refresh-token".to_string()),
                    machine_id: Some("old-machine-id".to_string()),
                    ..Default::default()
                }],
                None,
                None,
                false,
            )
            .unwrap(),
        );
        let identity_guard = manager.credential_identity_lock.lock().await;
        let update_manager = std::sync::Arc::clone(&manager);
        let update_task = tokio::spawn(async move {
            update_manager
                .update_credential(
                    1,
                    crate::admin::types::UpdateCredentialRequest {
                        machine_id: Some("new-machine-id".to_string()),
                        ..Default::default()
                    },
                )
                .await
        });

        tokio::task::yield_now().await;
        assert!(!update_task.is_finished());

        drop(identity_guard);
        tokio::time::timeout(std::time::Duration::from_secs(1), update_task)
            .await
            .expect("身份锁释放后更新应及时完成")
            .expect("更新任务不应 panic")
            .expect("身份更新应成功");
        assert_eq!(
            manager.entries.lock()[0].credentials.machine_id.as_deref(),
            Some("new-machine-id")
        );
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

    #[test]
    fn test_apply_update_fields_provider() {
        let mut cred = KiroCredentials::default();

        // Some(非空) 写入登录来源
        let update = crate::admin::types::UpdateCredentialRequest {
            provider: Some("Google".to_string()),
            ..Default::default()
        };
        MultiTokenManager::apply_update_fields(&mut cred, &update);
        assert_eq!(cred.provider.as_deref(), Some("Google"));

        // 空字符串清除该字段
        let update = crate::admin::types::UpdateCredentialRequest {
            provider: Some(String::new()),
            ..Default::default()
        };
        MultiTokenManager::apply_update_fields(&mut cred, &update);
        assert_eq!(cred.provider, None);

        // None 表示不更新该字段
        cred.provider = Some("GitHub".to_string());
        let update = crate::admin::types::UpdateCredentialRequest {
            provider: None,
            ..Default::default()
        };
        MultiTokenManager::apply_update_fields(&mut cred, &update);
        assert_eq!(cred.provider.as_deref(), Some("GitHub"));
    }
}
