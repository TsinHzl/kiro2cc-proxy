//! credentials 测试（原内联测试整体迁移）
#![cfg(test)]

#[cfg(test)]
mod tests {
    use crate::http_client::ProxyConfig;
    use crate::kiro::model::credentials::{
        BUILDER_ID_PLACEHOLDER_PROFILE_ARN, CredentialsConfig, KiroCredentials, SOCIAL_PROFILE_ARN,
        canonicalize_auth_method_value, fallback_profile_arn_value,
    };
    use crate::model::config::Config;

    #[test]
    fn test_from_json() {
        let json = r#"{
            "accessToken": "test_token",
            "refreshToken": "test_refresh",
            "profileArn": "arn:aws:test",
            "expiresAt": "2024-01-01T00:00:00Z",
            "authMethod": "social"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.access_token, Some("test_token".to_string()));
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.profile_arn, Some("arn:aws:test".to_string()));
        assert_eq!(creds.expires_at, Some("2024-01-01T00:00:00Z".to_string()));
        assert_eq!(creds.auth_method, Some("social".to_string()));
    }

    #[test]
    fn test_from_json_with_unknown_keys() {
        let json = r#"{
            "accessToken": "test_token",
            "unknownField": "should be ignored"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.access_token, Some("test_token".to_string()));
    }

    #[test]
    fn test_fill_missing_profile_arn_social() {
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("social".to_string());
        assert!(cred.fill_missing_profile_arn());
        assert_eq!(cred.profile_arn, Some(SOCIAL_PROFILE_ARN.to_string()));
        // 已有 ARN 后再调用不覆盖
        assert!(!cred.fill_missing_profile_arn());
    }

    #[test]
    fn test_fill_missing_profile_arn_idc_placeholder() {
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("idc".to_string());
        assert!(cred.fill_missing_profile_arn());
        assert_eq!(
            cred.profile_arn,
            Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN.to_string())
        );
    }

    #[test]
    fn test_fill_missing_profile_arn_infers_idc_from_oidc_creds() {
        // auth_method 缺失但带 OIDC clientId/secret → 推断为 idc
        let mut cred = KiroCredentials::default();
        cred.client_id = Some("c".to_string());
        cred.client_secret = Some("s".to_string());
        assert!(cred.fill_missing_profile_arn());
        assert_eq!(
            cred.profile_arn,
            Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN.to_string())
        );
    }

    #[test]
    fn test_fill_missing_profile_arn_keeps_user_explicit_arn() {
        let mut cred = KiroCredentials::default();
        cred.profile_arn = Some("arn:aws:user-explicit".to_string());
        assert!(!cred.fill_missing_profile_arn());
        assert_eq!(cred.profile_arn, Some("arn:aws:user-explicit".to_string()));
    }

    #[test]
    fn test_fill_missing_profile_arn_treats_empty_string_as_missing() {
        // 空字符串 profile_arn（credentials.json 手工编辑产生的脏数据）
        // 视为缺失：补全覆盖，避免空值原样发往上游
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("idc".to_string());
        cred.profile_arn = Some(String::new());
        assert!(cred.fill_missing_profile_arn());
        assert_eq!(
            cred.profile_arn,
            Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN.to_string())
        );
    }

    #[test]
    fn test_fallback_profile_arn_value_empty_auth_method_falls_back_to_inference() {
        // 空字符串 auth_method 视为未声明：按 OIDC 凭据存在性推断（而非落入未知类型）
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("   ".to_string());
        cred.client_id = Some("c".to_string());
        cred.client_secret = Some("s".to_string());
        assert_eq!(
            fallback_profile_arn_value(&cred),
            Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN)
        );

        let mut cred = KiroCredentials::default();
        cred.auth_method = Some(String::new());
        assert_eq!(fallback_profile_arn_value(&cred), Some(SOCIAL_PROFILE_ARN));
    }

    #[test]
    fn test_fill_missing_profile_arn_skips_external_idp_and_unknown() {
        // 企业 IdC 与未知类型不补全：真实 ARN 因租户而异，
        // 保留 None 交由数据面 400 触发 ProfileArnMissing 禁用
        for method in ["external_idp", "enterprise", "unknown"] {
            let mut cred = KiroCredentials::default();
            cred.auth_method = Some(method.to_string());
            assert!(
                !cred.fill_missing_profile_arn(),
                "auth_method={method} 不应补全"
            );
            assert!(
                cred.profile_arn.is_none(),
                "auth_method={method} 应保持 None"
            );
        }
    }

    #[test]
    fn test_to_json() {
        let creds = KiroCredentials {
            id: None,
            access_token: Some("token".to_string()),
            sso_access_token: None,
            refresh_token: None,
            profile_arn: None,
            expires_at: None,
            auth_method: Some("social".to_string()),
            client_id: None,
            client_secret: None,
            provider: None,
            token_endpoint: None,
            scopes: None,
            priority: 0,
            region: None,
            auth_region: None,
            api_region: None,
            machine_id: None,
            email: None,
            nickname: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            endpoint: None,
            thinking_adaptive: false,
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("accessToken"));
        assert!(json.contains("authMethod"));
        assert!(!json.contains("refreshToken"));
        // priority 为 0 时不序列化
        assert!(!json.contains("priority"));
    }

    #[test]
    fn test_default_credentials_path() {
        assert_eq!(
            KiroCredentials::default_credentials_path(),
            "credentials.json"
        );
    }

    #[test]
    fn test_priority_default() {
        let json = r#"{"refreshToken": "test"}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.priority, 0);
    }

    #[test]
    fn test_priority_explicit() {
        let json = r#"{"refreshToken": "test", "priority": 5}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.priority, 5);
    }

    #[test]
    fn test_credentials_config_single() {
        let json = r#"{"refreshToken": "test", "expiresAt": "2025-12-31T00:00:00Z"}"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, CredentialsConfig::Single(_)));
        assert_eq!(config.len(), 1);
    }

    #[test]
    fn test_credentials_config_multiple() {
        let json = r#"[
            {"refreshToken": "test1", "priority": 1},
            {"refreshToken": "test2", "priority": 0}
        ]"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        assert!(matches!(config, CredentialsConfig::Multiple(_)));
        assert_eq!(config.len(), 2);
    }

    #[test]
    fn test_credentials_config_priority_sorting() {
        let json = r#"[
            {"refreshToken": "t1", "priority": 2},
            {"refreshToken": "t2", "priority": 0},
            {"refreshToken": "t3", "priority": 1}
        ]"#;
        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        let list = config.into_sorted_credentials();

        // 验证按优先级排序
        assert_eq!(list[0].refresh_token, Some("t2".to_string())); // priority 0
        assert_eq!(list[1].refresh_token, Some("t3".to_string())); // priority 1
        assert_eq!(list[2].refresh_token, Some("t1".to_string())); // priority 2
    }

    // ============ Region 字段测试 ============

    #[test]
    fn test_region_field_parsing() {
        // 测试解析包含 region 字段的 JSON
        let json = r#"{
            "refreshToken": "test_refresh",
            "region": "us-east-1"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.region, Some("us-east-1".to_string()));
    }

    #[test]
    fn test_region_field_missing_backward_compat() {
        // 测试向后兼容：不包含 region 字段的旧格式 JSON
        let json = r#"{
            "refreshToken": "test_refresh",
            "authMethod": "social"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.region, None);
    }

    #[test]
    fn test_region_field_serialization() {
        // 测试序列化时正确输出 region 字段
        let creds = KiroCredentials {
            id: None,
            access_token: None,
            sso_access_token: None,
            refresh_token: Some("test".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: None,
            client_id: None,
            client_secret: None,
            provider: None,
            token_endpoint: None,
            scopes: None,
            priority: 0,
            region: Some("eu-west-1".to_string()),
            auth_region: None,
            api_region: None,
            machine_id: None,
            email: None,
            nickname: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            endpoint: None,
            thinking_adaptive: false,
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("region"));
        assert!(json.contains("eu-west-1"));
    }

    #[test]
    fn test_region_field_none_not_serialized() {
        // 测试 region 为 None 时不序列化
        let creds = KiroCredentials {
            id: None,
            access_token: None,
            sso_access_token: None,
            refresh_token: Some("test".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: None,
            client_id: None,
            client_secret: None,
            provider: None,
            token_endpoint: None,
            scopes: None,
            priority: 0,
            region: None,
            auth_region: None,
            api_region: None,
            machine_id: None,
            email: None,
            nickname: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            endpoint: None,
            thinking_adaptive: false,
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("region"));
    }

    // ============ MachineId 字段测试 ============

    #[test]
    fn test_machine_id_field_parsing() {
        let machine_id = "a".repeat(64);
        let json = format!(
            r#"{{
                "refreshToken": "test_refresh",
                "machineId": "{machine_id}"
            }}"#
        );

        let creds = KiroCredentials::from_json(&json).unwrap();
        assert_eq!(creds.refresh_token, Some("test_refresh".to_string()));
        assert_eq!(creds.machine_id, Some(machine_id));
    }

    #[test]
    fn test_machine_id_field_serialization() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.machine_id = Some("b".repeat(64));

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("machineId"));
    }

    #[test]
    fn test_machine_id_field_none_not_serialized() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.machine_id = None;

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("machineId"));
    }

    #[test]
    fn test_multiple_credentials_with_different_regions() {
        // 测试多账号场景下不同账号使用各自的 region
        let json = r#"[
            {"refreshToken": "t1", "region": "us-east-1"},
            {"refreshToken": "t2", "region": "eu-west-1"},
            {"refreshToken": "t3"}
        ]"#;

        let config: CredentialsConfig = serde_json::from_str(json).unwrap();
        let list = config.into_sorted_credentials();

        assert_eq!(list[0].region, Some("us-east-1".to_string()));
        assert_eq!(list[1].region, Some("eu-west-1".to_string()));
        assert_eq!(list[2].region, None);
    }

    #[test]
    fn test_region_field_with_all_fields() {
        // 测试包含所有字段的完整 JSON
        let json = r#"{
            "id": 1,
            "accessToken": "access",
            "refreshToken": "refresh",
            "profileArn": "arn:aws:test",
            "expiresAt": "2025-12-31T00:00:00Z",
            "authMethod": "idc",
            "clientId": "client123",
            "clientSecret": "secret456",
            "priority": 5,
            "region": "ap-northeast-1"
        }"#;

        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.id, Some(1));
        assert_eq!(creds.access_token, Some("access".to_string()));
        assert_eq!(creds.refresh_token, Some("refresh".to_string()));
        assert_eq!(creds.profile_arn, Some("arn:aws:test".to_string()));
        assert_eq!(creds.expires_at, Some("2025-12-31T00:00:00Z".to_string()));
        assert_eq!(creds.auth_method, Some("idc".to_string()));
        assert_eq!(creds.client_id, Some("client123".to_string()));
        assert_eq!(creds.client_secret, Some("secret456".to_string()));
        assert_eq!(creds.priority, 5);
        assert_eq!(creds.region, Some("ap-northeast-1".to_string()));
    }

    #[test]
    fn test_region_roundtrip() {
        // 测试序列化和反序列化的往返一致性
        let original = KiroCredentials {
            id: Some(42),
            access_token: Some("token".to_string()),
            sso_access_token: None,
            refresh_token: Some("refresh".to_string()),
            profile_arn: None,
            expires_at: None,
            auth_method: Some("social".to_string()),
            client_id: None,
            client_secret: None,
            provider: None,
            token_endpoint: None,
            scopes: None,
            priority: 3,
            region: Some("us-west-2".to_string()),
            auth_region: None,
            api_region: None,
            machine_id: Some("c".repeat(64)),
            email: None,
            nickname: None,
            subscription_title: None,
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            disabled: false,
            endpoint: None,
            thinking_adaptive: false,
        };

        let json = original.to_pretty_json().unwrap();
        let parsed = KiroCredentials::from_json(&json).unwrap();

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.access_token, original.access_token);
        assert_eq!(parsed.refresh_token, original.refresh_token);
        assert_eq!(parsed.priority, original.priority);
        assert_eq!(parsed.region, original.region);
        assert_eq!(parsed.machine_id, original.machine_id);
    }

    // ============ auth_region / api_region 字段测试 ============

    #[test]
    fn test_auth_region_field_parsing() {
        let json = r#"{
            "refreshToken": "test_refresh",
            "authRegion": "eu-central-1"
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.auth_region, Some("eu-central-1".to_string()));
        assert_eq!(creds.api_region, None);
    }

    #[test]
    fn test_api_region_field_parsing() {
        let json = r#"{
            "refreshToken": "test_refresh",
            "apiRegion": "ap-southeast-1"
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.api_region, Some("ap-southeast-1".to_string()));
        assert_eq!(creds.auth_region, None);
    }

    #[test]
    fn test_auth_api_region_serialization() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.auth_region = Some("eu-west-1".to_string());
        creds.api_region = Some("us-west-2".to_string());

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("authRegion"));
        assert!(json.contains("eu-west-1"));
        assert!(json.contains("apiRegion"));
        assert!(json.contains("us-west-2"));
    }

    // ============ thinking_adaptive 字段测试 ============

    #[test]
    fn test_thinking_adaptive_default_false_for_legacy_json() {
        // 旧版本 credentials.json 不含 thinkingAdaptive 字段 → 反序列化为 false
        let json = r#"{"refreshToken": "test"}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert!(!creds.thinking_adaptive);
    }

    #[test]
    fn test_thinking_adaptive_explicit_true() {
        let json = r#"{"refreshToken": "test", "thinkingAdaptive": true}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert!(creds.thinking_adaptive);
    }

    #[test]
    fn test_thinking_adaptive_roundtrip() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.thinking_adaptive = true;

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("thinkingAdaptive"));
        let parsed = KiroCredentials::from_json(&json).unwrap();
        assert!(parsed.thinking_adaptive);
    }

    #[test]
    fn test_auth_api_region_none_not_serialized() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test".to_string());
        creds.auth_region = None;
        creds.api_region = None;

        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("authRegion"));
        assert!(!json.contains("apiRegion"));
    }

    #[test]
    fn test_auth_api_region_roundtrip() {
        let mut original = KiroCredentials::default();
        original.refresh_token = Some("refresh".to_string());
        original.region = Some("us-east-1".to_string());
        original.auth_region = Some("eu-west-1".to_string());
        original.api_region = Some("ap-northeast-1".to_string());

        let json = original.to_pretty_json().unwrap();
        let parsed = KiroCredentials::from_json(&json).unwrap();

        assert_eq!(parsed.region, original.region);
        assert_eq!(parsed.auth_region, original.auth_region);
        assert_eq!(parsed.api_region, original.api_region);
    }

    #[test]
    fn test_backward_compat_no_auth_api_region() {
        // 旧格式 JSON 不包含 authRegion/apiRegion，应正常解析
        let json = r#"{
            "refreshToken": "test_refresh",
            "region": "us-east-1"
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(creds.region, Some("us-east-1".to_string()));
        assert_eq!(creds.auth_region, None);
        assert_eq!(creds.api_region, None);
    }

    // ============ effective_auth_region / effective_api_region 优先级测试 ============

    #[test]
    fn test_effective_auth_region_credential_auth_region_highest() {
        // 账号.auth_region > 账号.region > config.auth_region > config.region
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.auth_region = Some("config-auth-region".to_string());

        let mut creds = KiroCredentials::default();
        creds.region = Some("cred-region".to_string());
        creds.auth_region = Some("cred-auth-region".to_string());

        assert_eq!(creds.effective_auth_region(&config), "cred-auth-region");
    }

    #[test]
    fn test_effective_auth_region_fallback_to_credential_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.auth_region = Some("config-auth-region".to_string());

        let mut creds = KiroCredentials::default();
        creds.region = Some("cred-region".to_string());
        // auth_region 未设置

        assert_eq!(creds.effective_auth_region(&config), "cred-region");
    }

    #[test]
    fn test_effective_auth_region_fallback_to_config_auth_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.auth_region = Some("config-auth-region".to_string());

        let creds = KiroCredentials::default();
        // auth_region 和 region 均未设置

        assert_eq!(creds.effective_auth_region(&config), "config-auth-region");
    }

    #[test]
    fn test_effective_auth_region_fallback_to_config_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        // config.auth_region 未设置

        let creds = KiroCredentials::default();

        assert_eq!(creds.effective_auth_region(&config), "config-region");
    }

    #[test]
    fn test_effective_api_region_credential_api_region_highest() {
        // 账号.api_region > config.api_region > config.region
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.api_region = Some("config-api-region".to_string());

        let mut creds = KiroCredentials::default();
        creds.api_region = Some("cred-api-region".to_string());

        assert_eq!(creds.effective_api_region(&config), "cred-api-region");
    }

    #[test]
    fn test_effective_api_region_fallback_to_config_api_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();
        config.api_region = Some("config-api-region".to_string());

        let creds = KiroCredentials::default();

        assert_eq!(creds.effective_api_region(&config), "config-api-region");
    }

    #[test]
    fn test_effective_api_region_fallback_to_config_region() {
        let mut config = Config::default();
        config.region = "config-region".to_string();

        let creds = KiroCredentials::default();

        assert_eq!(creds.effective_api_region(&config), "config-region");
    }

    #[test]
    fn test_effective_api_region_ignores_credential_region() {
        // 账号.region 不参与 api_region 的回退链
        let mut config = Config::default();
        config.region = "config-region".to_string();

        let mut creds = KiroCredentials::default();
        creds.region = Some("cred-region".to_string());

        assert_eq!(creds.effective_api_region(&config), "config-region");
    }

    #[test]
    fn test_auth_and_api_region_independent() {
        // auth_region 和 api_region 互不影响
        let mut config = Config::default();
        config.region = "default".to_string();

        let mut creds = KiroCredentials::default();
        creds.auth_region = Some("auth-only".to_string());
        creds.api_region = Some("api-only".to_string());

        assert_eq!(creds.effective_auth_region(&config), "auth-only");
        assert_eq!(creds.effective_api_region(&config), "api-only");
    }

    // ============ 账号级代理优先级测试 ============

    #[test]
    fn test_effective_proxy_credential_overrides_global() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("socks5://cred:1080".to_string());

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, Some(ProxyConfig::new("socks5://cred:1080")));
    }

    #[test]
    fn test_effective_proxy_credential_with_auth() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("http://proxy:3128".to_string());
        creds.proxy_username = Some("user".to_string());
        creds.proxy_password = Some("pass".to_string());

        let result = creds.effective_proxy(Some(&global));
        let expected = ProxyConfig::new("http://proxy:3128").with_auth("user", "pass");
        assert_eq!(result, Some(expected));
    }

    #[test]
    fn test_effective_proxy_direct_bypasses_global() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("direct".to_string());

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, None);
    }

    #[test]
    fn test_effective_proxy_direct_case_insensitive() {
        let global = ProxyConfig::new("http://global:8080");
        let mut creds = KiroCredentials::default();
        creds.proxy_url = Some("DIRECT".to_string());

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, None);
    }

    #[test]
    fn test_effective_proxy_fallback_to_global() {
        let global = ProxyConfig::new("http://global:8080");
        let creds = KiroCredentials::default();

        let result = creds.effective_proxy(Some(&global));
        assert_eq!(result, Some(ProxyConfig::new("http://global:8080")));
    }

    #[test]
    fn test_effective_proxy_none_when_no_proxy() {
        let creds = KiroCredentials::default();
        let result = creds.effective_proxy(None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_debug_redacts_sensitive_fields() {
        let mut creds = KiroCredentials::default();
        creds.access_token = Some("secret_access".to_string());
        creds.refresh_token = Some("secret_refresh".to_string());
        creds.client_secret = Some("secret_client".to_string());
        creds.proxy_password = Some("secret_proxy_pw".to_string());
        creds.email = Some("user@example.com".to_string());

        let debug_str = format!("{:?}", creds);
        assert!(!debug_str.contains("secret_access"));
        assert!(!debug_str.contains("secret_refresh"));
        assert!(!debug_str.contains("secret_client"));
        assert!(!debug_str.contains("secret_proxy_pw"));
        assert!(!debug_str.contains("user@example.com"));
        assert!(debug_str.contains("u***@example.com")); // 邮箱掩码，仅保留首字符与域名
        assert_eq!(debug_str.matches("[REDACTED]").count(), 4);
    }

    #[test]
    fn test_debug_none_sensitive_fields_shown_as_none() {
        let creds = KiroCredentials::default();
        let debug_str = format!("{:?}", creds);
        assert!(debug_str.contains("access_token: None"));
        assert!(debug_str.contains("refresh_token: None"));
        assert!(debug_str.contains("client_secret: None"));
        assert!(debug_str.contains("proxy_password: None"));
    }

    #[test]
    fn test_canonicalize_external_idp_variants() {
        let cases = [
            ("AzureAD", "external_idp"),
            ("azuread", "external_idp"),
            ("AZUREAD", "external_idp"),
            ("EntraID", "external_idp"),
            ("entraid", "external_idp"),
            ("external_idp", "external_idp"),
            ("social", "social"),
            ("idc", "idc"),
            ("IdC", "idc"),
            ("IDC", "idc"),
            ("builder-id", "idc"),
            ("iam", "idc"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                canonicalize_auth_method_value(input),
                expected,
                "canonicalize({input}) should be {expected}"
            );
        }
    }

    // -------- 多端点 LB: effective_endpoints --------

    use crate::kiro::endpoint::EndpointName;

    #[test]
    fn test_effective_endpoints_unset_returns_all_in_default_order() {
        let creds = KiroCredentials::default();
        let eps = creds.effective_endpoints("us-east-1");
        assert_eq!(eps.len(), 4);
        assert_eq!(eps[0].name, EndpointName::Ide);
        assert_eq!(eps[1].name, EndpointName::Runtime);
        assert_eq!(eps[2].name, EndpointName::Codewhisperer);
        assert_eq!(eps[3].name, EndpointName::Amazonq);
    }

    #[test]
    fn test_effective_endpoints_empty_vec_returns_all() {
        let creds = KiroCredentials {
            endpoint: Some(vec![]),
            ..Default::default()
        };
        let eps = creds.effective_endpoints("us-east-1");
        assert_eq!(eps.len(), 4);
        assert_eq!(eps[0].name, EndpointName::Ide);
    }

    #[test]
    fn test_effective_endpoints_single_preferred_prefixes_with_default_tail() {
        let creds = KiroCredentials {
            endpoint: Some(vec![EndpointName::Runtime]),
            ..Default::default()
        };
        let eps = creds.effective_endpoints("us-east-1");
        assert_eq!(
            eps.iter().map(|e| e.name).collect::<Vec<_>>(),
            vec![
                EndpointName::Runtime,
                EndpointName::Ide,
                EndpointName::Codewhisperer,
                EndpointName::Amazonq,
            ]
        );
    }

    #[test]
    fn test_effective_endpoints_multiple_preferred_dedups() {
        let creds = KiroCredentials {
            endpoint: Some(vec![EndpointName::Runtime, EndpointName::Codewhisperer]),
            ..Default::default()
        };
        let eps = creds.effective_endpoints("us-east-1");
        // 首选 [Runtime, Codewhisperer] + 剩余默认序去重 [Ide, Amazonq]
        assert_eq!(
            eps.iter().map(|e| e.name).collect::<Vec<_>>(),
            vec![
                EndpointName::Runtime,
                EndpointName::Codewhisperer,
                EndpointName::Ide,
                EndpointName::Amazonq,
            ]
        );
    }

    #[test]
    fn test_endpoint_field_serialization_roundtrip() {
        // 配置 endpoint 后，序列化 / 反序列化往返一致
        let json = r#"{
            "accessToken": "test",
            "endpoint": ["runtime", "codewhisperer"]
        }"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert_eq!(
            creds.endpoint,
            Some(vec![EndpointName::Runtime, EndpointName::Codewhisperer])
        );
        let serialized = creds.to_pretty_json().unwrap();
        assert!(serialized.contains("\"endpoint\""));
        assert!(serialized.contains("\"runtime\""));
        assert!(serialized.contains("\"codewhisperer\""));
    }

    #[test]
    fn test_endpoint_field_unset_not_serialized() {
        // 未配置时不写入 endpoint 字段（向后兼容）
        let creds = KiroCredentials {
            access_token: Some("token".to_string()),
            ..Default::default()
        };
        let json = creds.to_pretty_json().unwrap();
        assert!(!json.contains("endpoint"));
    }
}
