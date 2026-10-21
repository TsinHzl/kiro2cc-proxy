// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! API Key（`ksk_...`）凭据专项测试
//!
//! 覆盖 kiro-go 语义：静态 Bearer、不刷新、额度查询带 TokenType、启动缺 key 自动禁用、
//! 管理端按 kiroApiKey 去重、禁用状态持久化。
#[cfg(test)]
pub(crate) mod tests {

    use super::super::super::refresh::{
        refresh_token, usage_limits_token_type_header, validate_refresh_token,
    };
    use super::super::super::types::MultiTokenManager;
    use crate::kiro::model::credentials::KiroCredentials;
    use crate::kiro::token_manager::DisabledReason;
    use crate::kiro::token_manager::tests::ext_idp::tests::TempDirGuard;
    use crate::model::config::Config;

    fn api_key_cred(key: &str) -> KiroCredentials {
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("api_key".to_string());
        cred.kiro_api_key = Some(key.to_string());
        cred
    }

    #[test]
    fn test_validate_api_key_credential_skips_refresh_token_checks() {
        // API Key 凭据无 refreshToken 也应通过校验（短 key 也不触发 100 字符下限）
        let cred = api_key_cred("ksk_short");
        assert!(
            validate_refresh_token(&cred).is_ok(),
            "API Key 凭据应跳过 refreshToken 校验"
        );

        // 声明了 api_key 但缺 key → 明确报错
        let mut missing = KiroCredentials::default();
        missing.auth_method = Some("api_key".to_string());
        let err = validate_refresh_token(&missing).unwrap_err().to_string();
        assert!(err.contains("kiroApiKey"), "错误信息应指明 kiroApiKey: {}", err);

        // key 为空白串同样报错
        let mut blank = api_key_cred("   ");
        blank.kiro_api_key = Some("   ".to_string());
        assert!(validate_refresh_token(&blank).is_err());
    }

    #[tokio::test]
    async fn test_refresh_token_refuses_api_key_credential() {
        // API Key 凭据没有刷新语义。若某路径误把它当 OAuth 凭据处理，
        // 必须明确报错，而不是拿 kiroApiKey 当 refreshToken 发给上游。
        let cred = api_key_cred("ksk_abc");
        let result = refresh_token(&cred, &Config::default(), None).await;
        let err = result.err().expect("应拒绝刷新 API Key 凭据");
        assert!(
            err.to_string().contains("不支持 token 刷新"),
            "应明确拒绝而非退化为 social 刷新，实际: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_try_ensure_token_short_circuits_api_key_credential() {
        // API Key 凭据应直接把 kiroApiKey 作为 token 返回，不走刷新
        let mut cred = api_key_cred("ksk_bearer_value");
        cred.id = Some(1);

        let manager =
            MultiTokenManager::new(Config::default(), vec![cred.clone()], None, None, false)
                .unwrap();

        let ctx = manager.try_ensure_token(1, &cred).await.unwrap();
        assert_eq!(ctx.id, 1);
        assert_eq!(
            ctx.token, "ksk_bearer_value",
            "token 应直接取 kiroApiKey，无需刷新"
        );
    }

    #[tokio::test]
    async fn test_try_ensure_token_rejects_api_key_credential_without_key() {
        let mut cred = api_key_cred("");
        cred.id = Some(1);
        cred.kiro_api_key = None;

        let manager = MultiTokenManager::new(Config::default(), vec![], None, None, false).unwrap();
        let msg = match manager.try_ensure_token(1, &cred).await {
            Ok(_) => panic!("缺少 kiroApiKey 时不应返回 token"),
            Err(e) => e.to_string(),
        };
        assert!(msg.contains("kiroApiKey"), "错误应指明缺少 kiroApiKey: {}", msg);
    }

    #[test]
    fn test_api_key_credential_missing_key_auto_disabled_at_startup() {
        // 声明 authMethod=api_key 但缺 kiroApiKey → 启动时自动禁用，标 InvalidConfig
        let mut bad = KiroCredentials::default();
        bad.auth_method = Some("api_key".to_string());
        bad.id = Some(1);

        let manager =
            MultiTokenManager::new(Config::default(), vec![bad], None, None, false).unwrap();
        let snapshot = manager.snapshot();
        assert!(snapshot.entries[0].disabled, "缺 key 的账号应被自动禁用");
        assert_eq!(
            snapshot.entries[0].disabled_reason,
            Some(DisabledReason::InvalidConfig),
            "前端需要靠这个字段区分「配置写错」和「手动禁用」"
        );
        assert_eq!(snapshot.available, 0);

        // 配置完整的 API Key 账号不应被禁用
        let mut good = api_key_cred("ksk_ok");
        good.id = Some(1);
        let manager2 =
            MultiTokenManager::new(Config::default(), vec![good], None, None, false).unwrap();
        assert!(
            !manager2.snapshot().entries[0].disabled,
            "配置完整的 API Key 账号不应被禁用"
        );
    }

    #[test]
    fn test_invalid_config_disabled_reason_describes_root_cause() {
        // 面板/日志需要能区分"配置错误"与"手动禁用"、"连续失败"
        assert!(
            DisabledReason::InvalidConfig.describe().contains("kiroApiKey"),
            "禁用原因应指明缺少 kiroApiKey"
        );
        assert_ne!(
            DisabledReason::InvalidConfig.describe(),
            DisabledReason::Manual.describe()
        );
        // 序列化为 snake_case，供前端 i18n key 映射
        assert_eq!(
            serde_json::to_string(&DisabledReason::InvalidConfig).unwrap(),
            "\"invalid_config\""
        );
    }

    #[tokio::test]
    async fn test_add_credential_api_key_dedups_by_api_key() {
        let mut existing = api_key_cred("ksk_dup");
        existing.id = Some(1);
        let manager =
            MultiTokenManager::new(Config::default(), vec![existing], None, None, false).unwrap();

        // 同一 kiroApiKey 应被判重（而非拿 refreshToken 去比对）
        let duplicate = api_key_cred("ksk_dup");
        let result = manager.add_credential(duplicate).await;
        assert!(result.is_err());
        let msg = result.err().unwrap().to_string();
        assert!(
            msg.contains("kiroApiKey"),
            "去重错误信息应指明 kiroApiKey，实际: {}",
            msg
        );
    }

    #[tokio::test]
    async fn test_add_credential_api_key_succeeds_without_upstream_refresh() {
        // API Key 凭据没有刷新流程可用于验证，add 应直接接受而不发网络请求
        let manager = MultiTokenManager::new(Config::default(), vec![], None, None, false).unwrap();

        let new_id = manager
            .add_credential(api_key_cred("ksk_abcdefghijklmnop1234"))
            .await
            .unwrap();
        let snapshot = manager.snapshot();
        let added = snapshot.entries.iter().find(|e| e.id == new_id).unwrap();
        assert!(!added.disabled);
        assert_eq!(added.masked_api_key.as_deref(), Some("ksk_...1234"));
        assert_eq!(added.auth_method.as_deref(), Some("api_key"));
        // API Key 凭据不应被补全 profileArn
        assert!(
            !added.has_profile_arn,
            "API Key 凭据不得携带 profileArn"
        );
    }

    #[test]
    fn test_usage_limits_request_carries_token_type_for_api_key() {
        // 控制面 /getUsageLimits 缺少 TokenType 时会按 social 凭据校验 Bearer Token
        // 并返回 403 Invalid token（实测），所以 API Key 凭据的额度查询必须带上
        // TokenType: API_KEY —— 与数据面、ListAvailableModels 的行为保持一致。
        let api_key = api_key_cred("ksk_usage_probe");
        assert_eq!(
            usage_limits_token_type_header(&api_key),
            Some(("TokenType", "API_KEY")),
            "API Key 凭据的额度查询必须带 TokenType: API_KEY"
        );

        // 普通 OAuth/social 凭据不带该头，鉴权方式保持不变。
        let social = KiroCredentials::default();
        assert_eq!(usage_limits_token_type_header(&social), None);

        // 企业 SSO 凭据与数据面一致，使用 EXTERNAL_IDP。
        let mut idp = KiroCredentials::default();
        idp.auth_method = Some("external_idp".to_string());
        assert_eq!(
            usage_limits_token_type_header(&idp),
            Some(("TokenType", "EXTERNAL_IDP"))
        );
    }

    #[tokio::test]
    async fn test_enable_api_key_account_skips_subscription_lookup() {
        // 启用 API Key 账号不得因"订阅等级为空"被拒绝，也不能发起额度查询
        let mut cred = api_key_cred("ksk_enable");
        cred.id = Some(1);
        cred.disabled = true;

        let manager =
            MultiTokenManager::new(Config::default(), vec![cred], None, None, false).unwrap();
        assert!(manager.snapshot().entries[0].disabled);

        manager.set_disabled(1, false).await.unwrap();
        let entry = &manager.snapshot().entries[0];
        assert!(!entry.disabled, "API Key 账号应可直接启用");
        assert_eq!(entry.disabled_reason, None);
    }

    #[test]
    fn test_persist_credentials_keeps_disabled_and_api_key() {
        // 禁用状态与 kiroApiKey 必须原样写回 credentials.json，
        // 供下一次启动继续识别（禁用账号不被丢弃）
        let dir = TempDirGuard::new("kiro2cc-test-api-key-persist");
        let path = dir.path().join("credentials.json");

        let mut disabled = api_key_cred("ksk_disabled_persist");
        disabled.id = Some(1);
        disabled.disabled = true;
        let mut enabled = api_key_cred("ksk_enabled_persist");
        enabled.id = Some(2);

        let _manager = MultiTokenManager::new(
            Config::default(),
            vec![disabled, enabled],
            None,
            Some(path.clone()),
            true,
        )
        .unwrap();

        let json = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let arr = parsed.as_array().expect("应为多账号数组格式");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["disabled"], true, "禁用状态必须保留: {}", json);
        assert_eq!(arr[0]["kiroApiKey"], "ksk_disabled_persist");
        assert_eq!(arr[1]["disabled"], false);
        assert!(
            arr.iter().all(|c| c.get("profileArn").is_none()),
            "API Key 凭据不得被补全 profileArn: {}",
            json
        );
    }
}
