// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 按账号实际可用模型路由（Issue #49）
#[cfg(test)]
mod tests {
    use super::super::super::types::{
        MODEL_SUPPORT_FAILURE_TTL, MODEL_SUPPORT_TTL, ModelSupport, MultiTokenManager,
    };
    use super::super::sticky::tests::make_valid_cred;
    use crate::kiro::model::available_models::{
        AvailableModelInfo, AvailableModelsResponse, TokenLimits,
    };
    use crate::model::config::Config;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    const MODEL: &str = "claude-sonnet-4.6";

    /// 两个账号：#1 优先级更高（priority 模式下默认被选中），#2 为备选
    fn manager_with_two() -> MultiTokenManager {
        let mut a = make_valid_cred("t1");
        a.priority = 0;
        let mut b = make_valid_cred("t2");
        b.priority = 1;
        MultiTokenManager::new(Config::default(), vec![a, b], None, None, false).unwrap()
    }

    fn set_models(m: &MultiTokenManager, id: u64, models: &[&str]) {
        set_support(m, id, Some(models), Instant::now());
    }

    fn set_support(m: &MultiTokenManager, id: u64, models: Option<&[&str]>, at: Instant) {
        let models: Option<HashSet<String>> =
            models.map(|ms| ms.iter().map(|s| s.to_ascii_lowercase()).collect());
        m.model_support.lock().insert(
            id,
            ModelSupport {
                models,
                checked_at: at,
            },
        );
    }

    #[tokio::test]
    async fn skips_account_known_not_to_support_model() {
        let m = manager_with_two();
        // 无探测结果：按优先级选 #1
        assert_eq!(m.acquire_context(Some(MODEL)).await.unwrap().id, 1);

        set_models(&m, 1, &["gpt-5.6-sol"]);
        set_models(&m, 2, &[MODEL]);
        assert_eq!(m.acquire_context(Some(MODEL)).await.unwrap().id, 2);
        // 其他模型不受影响
        assert_eq!(m.acquire_context(Some("gpt-5.6-sol")).await.unwrap().id, 1);
    }

    #[tokio::test]
    async fn model_match_is_case_insensitive() {
        let m = manager_with_two();
        set_models(&m, 1, &["Claude-Sonnet-4.6"]);
        set_models(&m, 2, &["gpt-5.6-sol"]);
        assert_eq!(m.acquire_context(Some(MODEL)).await.unwrap().id, 1);
    }

    #[tokio::test]
    async fn fails_open_when_every_account_is_known_unsupported() {
        // 上游列表可能不完整（例如新模型尚未出现）：全部不支持时保持原有行为
        let m = manager_with_two();
        set_models(&m, 1, &["gpt-5.6-sol"]);
        set_models(&m, 2, &["gpt-5.6-sol"]);
        assert_eq!(m.acquire_context(Some(MODEL)).await.unwrap().id, 1);
    }

    #[tokio::test]
    async fn unknown_status_is_not_excluded() {
        let m = manager_with_two();
        // #1 探测失败（None）、#2 明确支持：#1 仍按优先级参与调度
        set_support(&m, 1, None, Instant::now());
        set_models(&m, 2, &[MODEL]);
        assert_eq!(m.acquire_context(Some(MODEL)).await.unwrap().id, 1);
    }

    #[tokio::test]
    async fn expired_probe_result_is_ignored() {
        let m = manager_with_two();
        let stale = Instant::now()
            .checked_sub(MODEL_SUPPORT_TTL + Duration::from_secs(1))
            .expect("monotonic clock far enough from origin");
        set_support(&m, 1, Some(&["gpt-5.6-sol"]), stale);
        set_models(&m, 2, &[MODEL]);
        // #1 的结果已过期，视为未知，仍按优先级选 #1
        assert_eq!(m.acquire_context(Some(MODEL)).await.unwrap().id, 1);
    }

    #[tokio::test]
    async fn auto_and_missing_model_are_never_filtered() {
        let m = manager_with_two();
        set_models(&m, 1, &["gpt-5.6-sol"]);
        set_models(&m, 2, &["gpt-5.6-sol"]);
        assert_eq!(m.acquire_context(Some("auto")).await.unwrap().id, 1);
        assert_eq!(m.acquire_context(None).await.unwrap().id, 1);
        assert!(!m.is_known_unsupported(1, Some("auto")));
        assert!(!m.is_known_unsupported(1, None));
    }

    #[tokio::test]
    async fn respects_allowed_ids_whitelist() {
        let m = manager_with_two();
        set_models(&m, 1, &["gpt-5.6-sol"]);
        set_models(&m, 2, &[MODEL]);
        // 白名单只含 #1 且其明确不支持：候选全部不支持，保持原行为（不回退到白名单之外）
        let ctx = m
            .acquire_context_sticky(Some(MODEL), &[1], None, &[])
            .await
            .unwrap();
        assert_eq!(ctx.id, 1);
        let ctx = m
            .acquire_context_sticky(Some(MODEL), &[1, 2], None, &[])
            .await
            .unwrap();
        assert_eq!(ctx.id, 2);
    }

    #[tokio::test]
    async fn sticky_binding_to_unsupported_account_is_replaced() {
        let m = manager_with_two();
        // 先让会话绑定到 #1
        let first = m
            .acquire_context_sticky(Some(MODEL), &[], Some("session-x"), &[])
            .await
            .unwrap();
        assert_eq!(first.id, 1);

        // 之后探测到 #1 不支持该模型：会话应改绑到 #2
        set_models(&m, 1, &["gpt-5.6-sol"]);
        set_models(&m, 2, &[MODEL]);
        let second = m
            .acquire_context_sticky(Some(MODEL), &[], Some("session-x"), &[])
            .await
            .unwrap();
        assert_eq!(second.id, 2);
    }

    #[tokio::test]
    async fn balanced_mode_only_rotates_among_supporters() {
        let mut cfg = Config::default();
        cfg.load_balancing_mode = "balanced".to_string();
        let creds = vec![
            make_valid_cred("t1"),
            make_valid_cred("t2"),
            make_valid_cred("t3"),
        ];
        let m = MultiTokenManager::new(cfg, creds, None, None, false).unwrap();
        set_models(&m, 1, &["gpt-5.6-sol"]);
        set_models(&m, 2, &[MODEL]);
        set_models(&m, 3, &[MODEL]);
        for _ in 0..12 {
            let id = m.acquire_context(Some(MODEL)).await.unwrap().id;
            assert_ne!(id, 1, "不支持该模型的账号不应被轮询到");
        }
    }

    #[test]
    fn probe_targets_skip_single_account_pool_and_fresh_results() {
        // 单账号池：没有路由可言，不探测
        let single = MultiTokenManager::new(
            Config::default(),
            vec![make_valid_cred("t1")],
            None,
            None,
            false,
        )
        .unwrap();
        assert!(
            single
                .model_support_probe_targets(&[], Some(MODEL))
                .is_empty()
        );

        let m = manager_with_two();
        assert_eq!(m.model_support_probe_targets(&[], Some(MODEL)), vec![1, 2]);

        // 有效的探测结果不重复探测
        set_models(&m, 1, &[MODEL]);
        assert_eq!(m.model_support_probe_targets(&[], Some(MODEL)), vec![2]);

        // 失败结果在重试间隔后才重新探测
        set_support(&m, 2, None, Instant::now());
        assert!(m.model_support_probe_targets(&[], Some(MODEL)).is_empty());
        set_support(
            &m,
            2,
            None,
            Instant::now()
                .checked_sub(MODEL_SUPPORT_FAILURE_TTL + Duration::from_secs(1))
                .unwrap(),
        );
        assert_eq!(m.model_support_probe_targets(&[], Some(MODEL)), vec![2]);
    }

    #[test]
    fn probe_targets_exclude_disabled_accounts_and_honor_whitelist() {
        let m = manager_with_two();
        // 白名单只有 1 个账号：同样没有路由可言
        assert!(m.model_support_probe_targets(&[1], Some(MODEL)).is_empty());
        // 禁用其中一个后只剩 1 个可调度账号
        m.entries.lock()[1].disabled = true;
        assert!(m.model_support_probe_targets(&[], Some(MODEL)).is_empty());
    }

    fn resp(ids: &[&str]) -> AvailableModelsResponse {
        AvailableModelsResponse {
            models: ids
                .iter()
                .map(|id| AvailableModelInfo {
                    model_id: id.to_string(),
                    model_name: String::new(),
                    description: None,
                    rate_multiplier: None,
                    supported_input_types: Vec::new(),
                    token_limits: TokenLimits::default(),
                    additional_model_request_fields_schema: None,
                })
                .collect(),
        }
    }

    const WAIT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn refresh_is_noop_for_auto_and_missing_model() {
        // 不会发起任何探测：auto / 无模型直接返回，缓存保持为空
        let m = Arc::new(manager_with_two());
        let calls = Arc::new(AtomicUsize::new(0));
        for model in [Some("auto"), None] {
            let c = calls.clone();
            m.refresh_model_support_with(model, &[], WAIT, move |_, _| {
                c.fetch_add(1, Ordering::SeqCst);
                async { Ok(resp(&[])) }
            })
            .await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(m.model_support.lock().is_empty());
    }

    #[tokio::test]
    async fn refresh_records_probe_results() {
        let m = Arc::new(manager_with_two());
        m.refresh_model_support_with(Some(MODEL), &[], WAIT, |_, id| async move {
            if id == 1 {
                Ok(resp(&["GPT-5.6-Sol"]))
            } else {
                Ok(resp(&[MODEL]))
            }
        })
        .await;

        assert!(m.is_known_unsupported(1, Some(MODEL)));
        assert!(!m.is_known_unsupported(2, Some(MODEL)));
        // 探测结果生效后，调度直接排除 #1
        assert_eq!(m.acquire_context(Some(MODEL)).await.unwrap().id, 2);
        // 已新鲜，不再有探测目标
        assert!(m.model_support_probe_targets(&[], Some(MODEL)).is_empty());
    }

    #[tokio::test]
    async fn empty_list_and_probe_error_are_unknown_with_short_ttl() {
        let m = Arc::new(manager_with_two());
        m.refresh_model_support_with(Some(MODEL), &[], WAIT, |_, id| async move {
            if id == 1 {
                Ok(resp(&[]))
            } else {
                Err(anyhow::anyhow!("boom"))
            }
        })
        .await;

        // 两者都视为未知：不排除
        assert!(!m.is_known_unsupported(1, Some(MODEL)));
        assert!(!m.is_known_unsupported(2, Some(MODEL)));
        let cache = m.model_support.lock();
        assert!(cache.get(&1).unwrap().models.is_none());
        assert!(cache.get(&2).unwrap().models.is_none());
    }

    #[tokio::test]
    async fn concurrent_refreshes_probe_each_account_once() {
        let m = Arc::new(manager_with_two());
        let calls = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let m = m.clone();
            let calls = calls.clone();
            handles.push(tokio::spawn(async move {
                m.refresh_model_support_with(Some(MODEL), &[], WAIT, move |_, _| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async {
                        tokio::time::sleep(Duration::from_millis(30)).await;
                        Ok(resp(&[MODEL]))
                    }
                })
                .await;
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        // 2 个账号各探测一次，其余并发请求复用结果
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn slow_probe_does_not_block_request_beyond_max_wait() {
        let m = Arc::new(manager_with_two());
        let started = Instant::now();
        m.refresh_model_support_with(Some(MODEL), &[], Duration::from_millis(30), |_, _| async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            Ok(resp(&["gpt-5.6-sol"]))
        })
        .await;
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "不应等满探测时长"
        );
        // 此刻结果尚未写入：本次调度按未知处理
        assert!(!m.is_known_unsupported(1, Some(MODEL)));

        // 探测没有被取消，在后台完成后写入缓存
        let deadline = Instant::now() + Duration::from_secs(3);
        while !m.is_known_unsupported(1, Some(MODEL)) {
            assert!(Instant::now() < deadline, "后台探测应当完成");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[test]
    fn probe_targets_skip_unhealthy_accounts() {
        let creds = vec![
            make_valid_cred("t1"),
            make_valid_cred("t2"),
            make_valid_cred("t3"),
        ];
        let m = MultiTokenManager::new(Config::default(), creds, None, None, false).unwrap();
        m.entries.lock()[2].failure_count = 3; // Unhealthy
        assert_eq!(m.model_support_probe_targets(&[], Some(MODEL)), vec![1, 2]);
    }

    // ===== Free 账号的 Opus 放行：以上游 ListAvailableModels 为准（Issue #71） =====

    const OPUS: &str = "claude-opus-4.6";

    fn free_cred(token: &str) -> crate::kiro::model::credentials::KiroCredentials {
        let mut c = make_valid_cred(token);
        c.subscription_title = Some("KIRO FREE".to_string());
        c
    }

    fn single_free_manager() -> MultiTokenManager {
        MultiTokenManager::new(
            Config::default(),
            vec![free_cred("free")],
            None,
            None,
            false,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn free_account_without_probe_result_is_still_excluded_for_opus() {
        let m = single_free_manager();
        assert!(m.acquire_context(Some(OPUS)).await.is_err());
        // 非 Opus 不受影响
        assert_eq!(m.acquire_context(Some(MODEL)).await.unwrap().id, 1);
    }

    #[tokio::test]
    async fn free_account_is_allowed_for_opus_when_probe_lists_it() {
        let m = single_free_manager();
        set_models(&m, 1, &[OPUS, MODEL]);
        assert_eq!(m.acquire_context(Some(OPUS)).await.unwrap().id, 1);
    }

    #[tokio::test]
    async fn free_account_stays_excluded_when_probe_lacks_opus() {
        let m = single_free_manager();
        set_models(&m, 1, &[MODEL]);
        assert!(m.acquire_context(Some(OPUS)).await.is_err());
    }

    #[tokio::test]
    async fn free_account_stays_excluded_when_probe_failed_or_expired() {
        let m = single_free_manager();
        set_support(&m, 1, None, Instant::now());
        assert!(m.acquire_context(Some(OPUS)).await.is_err());

        let stale = Instant::now()
            .checked_sub(MODEL_SUPPORT_TTL + Duration::from_secs(1))
            .expect("monotonic clock far enough from origin");
        set_support(&m, 1, Some(&[OPUS]), stale);
        assert!(m.acquire_context(Some(OPUS)).await.is_err());
    }

    #[tokio::test]
    async fn free_account_with_opus_is_selected_in_mixed_pool() {
        // #1 Free（高优先级），#2 Pro
        fn pool() -> MultiTokenManager {
            let mut free = free_cred("free");
            free.priority = 0;
            let mut pro = make_valid_cred("pro");
            pro.priority = 1;
            pro.subscription_title = Some("KIRO PRO".to_string());
            MultiTokenManager::new(Config::default(), vec![free, pro], None, None, false).unwrap()
        }

        // 探测不含 Opus：落到 Pro
        let m = pool();
        set_models(&m, 1, &[MODEL]);
        assert_eq!(m.acquire_context(Some(OPUS)).await.unwrap().id, 2);

        // 探测含 Opus：高优先级的 Free 账号可用
        let m = pool();
        set_models(&m, 1, &[OPUS]);
        assert_eq!(m.acquire_context(Some(OPUS)).await.unwrap().id, 1);
    }

    #[tokio::test]
    async fn sticky_binding_to_free_account_with_opus_is_kept() {
        let m = single_free_manager();
        set_models(&m, 1, &[OPUS]);
        for _ in 0..2 {
            let ctx = m
                .acquire_context_sticky(Some(OPUS), &[], Some("session-free"), &[])
                .await
                .unwrap();
            assert_eq!(ctx.id, 1);
        }
    }

    #[test]
    fn probe_targets_include_single_tier_gated_account_for_opus_only() {
        // 单个 Free 账号请求 Opus：探测结果是它能否承接的唯一依据，必须探测
        let free = single_free_manager();
        assert_eq!(free.model_support_probe_targets(&[], Some(OPUS)), vec![1]);
        // 非 Opus 请求：单账号池仍无路由可言
        assert!(
            free.model_support_probe_targets(&[], Some(MODEL))
                .is_empty()
        );

        // 单个付费账号请求 Opus：订阅已允许，不需要探测
        let mut pro = make_valid_cred("pro");
        pro.subscription_title = Some("KIRO PRO".to_string());
        let pro = MultiTokenManager::new(Config::default(), vec![pro], None, None, false).unwrap();
        assert!(pro.model_support_probe_targets(&[], Some(OPUS)).is_empty());

        // 已有新鲜结果时不重复探测
        set_models(&free, 1, &[OPUS]);
        assert!(free.model_support_probe_targets(&[], Some(OPUS)).is_empty());
    }

    #[tokio::test]
    async fn refresh_probes_single_free_account_and_unlocks_opus() {
        let m = Arc::new(single_free_manager());
        assert!(m.acquire_context(Some(OPUS)).await.is_err());

        m.refresh_model_support_with(Some(OPUS), &[], WAIT, |_, _| async { Ok(resp(&[OPUS])) })
            .await;

        assert_eq!(m.acquire_context(Some(OPUS)).await.unwrap().id, 1);
    }

    #[tokio::test]
    async fn refresh_keeps_single_free_account_excluded_when_probe_lacks_opus() {
        let m = Arc::new(single_free_manager());
        m.refresh_model_support_with(Some(OPUS), &[], WAIT, |_, _| async { Ok(resp(&[MODEL])) })
            .await;
        assert!(m.acquire_context(Some(OPUS)).await.is_err());
    }

    #[test]
    fn describe_unavailable_counts_free_account_with_opus_in_scope() {
        let m = single_free_manager();
        assert!(
            m.describe_unavailable(Some(OPUS), &[])
                .contains("没有支持该模型的账号")
        );
        set_models(&m, 1, &[OPUS]);
        assert!(
            !m.describe_unavailable(Some(OPUS), &[])
                .contains("没有支持该模型的账号")
        );
    }

    #[tokio::test]
    async fn deleting_account_drops_its_probe_result() {
        let m = manager_with_two();
        set_models(&m, 1, &[MODEL]);
        set_models(&m, 2, &[MODEL]);
        m.delete_credential(1).await.unwrap();
        let cache = m.model_support.lock();
        assert!(!cache.contains_key(&1));
        assert!(cache.contains_key(&2));
    }
}
