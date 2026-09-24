// Copyright (c) 2026 Harllan He. Licensed under MIT.
// token_manager 测试（自 tests.rs 拆出，纯代码搬移）
#[cfg(test)]
pub(crate) mod tests {

    use super::super::super::entry::DisabledReason;

    use super::super::super::refresh::validate_refresh_token;
    use super::super::super::types::{
        MAX_FAILURES_PER_CREDENTIAL, MultiTokenManager, QUOTA_EXHAUSTED_ALL_MARKER,
    };

    use crate::kiro::model::credentials::KiroCredentials;

    use crate::kiro::token_manager::tests::ext_idp::tests::{
        TempDirGuard, spawn_single_response_server,
    };
    use crate::kiro::token_manager::tests::sticky::tests::make_valid_cred;
    use crate::model::config::Config;
    use chrono::{Duration, Utc};

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
