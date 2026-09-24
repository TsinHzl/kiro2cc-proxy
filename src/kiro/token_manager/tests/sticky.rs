// Copyright (c) 2026 Harllan He. Licensed under MIT.
// token_manager 测试（自 tests.rs 拆出，纯代码搬移）
#[cfg(test)]
pub(crate) mod tests {

    use super::super::super::types::{
        MultiTokenManager, STICKY_CACHE_TTL, STICKY_THROTTLE_EVICT_THRESHOLD,
    };

    use crate::kiro::model::credentials::KiroCredentials;

    use crate::model::config::Config;
    use chrono::{Duration, Utc};

    pub(crate) fn make_valid_cred(token: &str) -> KiroCredentials {
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
}
