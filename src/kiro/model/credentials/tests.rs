//! credentials 测试（原内联测试整体迁移）
#![cfg(test)]

#[cfg(test)]
mod tests {
    use crate::http_client::ProxyConfig;
    use crate::kiro::model::credentials::{
        BUILDER_ID_PLACEHOLDER_PROFILE_ARN, CredentialsConfig, ENTERPRISE_FALLBACK_PROFILE_ARN_EU,
        ENTERPRISE_FALLBACK_PROFILE_ARN_US, KiroCredentials, SOCIAL_PROFILE_ARN,
        canonicalize_auth_method_value, fallback_profile_arn_value,
    };
    use crate::model::config::Config;

    const TENANT_ARN: &str = "arn:aws:codewhisperer:us-east-1:123456789012:profile/TENANTPROFILE";

    #[test]
    fn test_supports_opus_non_idc_requires_known_non_free_subscription() {
        // 无租户 IdC 身份的账号：等级未知（None / 空 / 空白）一律不支持，只有明确的非 Free 才支持
        let cases = [
            (None, false),
            (Some(""), false),
            (Some("   "), false),
            (Some("KIRO FREE"), false),
            (Some("  Kiro Free  "), false),
            (Some("KIRO PRO"), true),
            (Some("Kiro Pro+"), true),
            (Some("ENTERPRISE"), true),
        ];

        for (title, expected) in cases {
            let credentials = KiroCredentials {
                subscription_title: title.map(str::to_string),
                ..Default::default()
            };
            assert_eq!(credentials.supports_opus(), expected, "title={title:?}");
        }
    }

    #[test]
    fn test_supports_opus_tenant_idc_without_title_is_allowed() {
        // 企业版 IdC（真实租户 profileArn）上游可能不返回标题：无标题 / 空白标题视为支持
        for title in [None, Some(""), Some("   ")] {
            let credentials = KiroCredentials {
                auth_method: Some("idc".to_string()),
                profile_arn: Some(TENANT_ARN.to_string()),
                subscription_title: title.map(str::to_string),
                ..Default::default()
            };
            assert!(credentials.is_tenant_idc());
            assert!(credentials.supports_opus(), "title={title:?}");
        }
    }

    #[test]
    fn test_supports_opus_tenant_idc_with_explicit_free_title_is_rejected() {
        // 明确的 Free 标题优先于租户身份
        let credentials = KiroCredentials {
            auth_method: Some("idc".to_string()),
            profile_arn: Some(TENANT_ARN.to_string()),
            subscription_title: Some("KIRO FREE".to_string()),
            ..Default::default()
        };
        assert!(!credentials.supports_opus());
    }

    #[test]
    fn test_supports_opus_idc_without_real_profile_arn_stays_rejected() {
        // 个人 BuilderId（占位 ARN）、企业备用 ARN、无 ARN 都不是租户身份，等级未知时不支持
        for arn in [
            None,
            Some(""),
            Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN),
            Some(ENTERPRISE_FALLBACK_PROFILE_ARN_US),
            Some(ENTERPRISE_FALLBACK_PROFILE_ARN_EU),
        ] {
            let credentials = KiroCredentials {
                auth_method: Some("idc".to_string()),
                profile_arn: arn.map(str::to_string),
                subscription_title: None,
                ..Default::default()
            };
            assert!(!credentials.is_tenant_idc(), "arn={arn:?}");
            assert!(!credentials.supports_opus(), "arn={arn:?}");
        }
    }

    #[test]
    fn test_supports_opus_social_with_profile_arn_and_no_title_stays_rejected() {
        // social 账号即便带 profileArn 也不是 IdC 租户
        let credentials = KiroCredentials {
            auth_method: Some("social".to_string()),
            profile_arn: Some(TENANT_ARN.to_string()),
            subscription_title: None,
            ..Default::default()
        };
        assert!(!credentials.is_tenant_idc());
        assert!(!credentials.supports_opus());
    }

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
    fn test_enterprise_provider_never_gets_builder_id_placeholder() {
        // issue #43：provider=Enterprise 的账号不得被补全 BuilderId 占位符
        // （占位符属于另一个 AWS 账号，企业账号用它会被上游拒绝），改用区域化企业备用 ARN
        for provider in ["Enterprise", "enterprise", " ENTERPRISE "] {
            let mut cred = KiroCredentials::default();
            cred.auth_method = Some("idc".to_string());
            cred.client_id = Some("c".to_string());
            cred.client_secret = Some("s".to_string());
            cred.provider = Some(provider.to_string());
            assert_eq!(
                fallback_profile_arn_value(&cred),
                Some(ENTERPRISE_FALLBACK_PROFILE_ARN_US),
                "provider={provider}"
            );
            assert!(cred.fill_missing_profile_arn(), "provider={provider}");
            assert_eq!(
                cred.profile_arn.as_deref(),
                Some(ENTERPRISE_FALLBACK_PROFILE_ARN_US)
            );
        }

        // eu-* 区域用欧洲备用 ARN（api_region 优先于 region）
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("idc".to_string());
        cred.provider = Some("Enterprise".to_string());
        cred.region = Some("us-east-1".to_string());
        cred.api_region = Some("eu-west-1".to_string());
        assert_eq!(
            fallback_profile_arn_value(&cred),
            Some(ENTERPRISE_FALLBACK_PROFILE_ARN_EU)
        );

        // BuilderId / 未声明 provider 的 idc 账号行为不变
        for provider in [None, Some("BuilderId")] {
            let mut cred = KiroCredentials::default();
            cred.auth_method = Some("idc".to_string());
            cred.provider = provider.map(str::to_string);
            assert_eq!(
                fallback_profile_arn_value(&cred),
                Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN)
            );
        }
    }

    #[test]
    fn test_is_idc_infers_from_oidc_fields_only_when_auth_method_missing() {
        // 与 refresh_token 一致：缺失 authMethod 且 clientId/clientSecret 齐全 → idc
        let mut cred = KiroCredentials {
            client_id: Some("cid".to_string()),
            client_secret: Some("secret".to_string()),
            ..Default::default()
        };
        assert!(cred.is_idc());

        // 字段不完整 → 不是 idc
        cred.client_secret = None;
        assert!(!cred.is_idc());
        cred.client_secret = Some("secret".to_string());
        cred.client_id = None;
        assert!(!cred.is_idc());

        // 显式声明优先：external_idp / social 即使带完整 OIDC 字段也不是 idc
        cred.client_id = Some("cid".to_string());
        for m in ["external_idp", "social"] {
            cred.auth_method = Some(m.to_string());
            assert!(!cred.is_idc(), "{m}");
        }
    }

    #[test]
    fn test_is_idc_and_has_no_real_profile_arn() {
        let mut cred = KiroCredentials::default();
        assert!(!cred.is_idc());
        for m in ["idc", "IdC", "builder-id", "IAM"] {
            cred.auth_method = Some(m.to_string());
            assert!(cred.is_idc(), "{m}");
        }
        for m in ["social", "external_idp"] {
            cred.auth_method = Some(m.to_string());
            assert!(!cred.is_idc(), "{m}");
        }

        // 缺失 / 空 / 仍是 BuilderId 占位符都视为“没有真实 ARN”
        for arn in [
            None,
            Some(""),
            Some("  "),
            Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN),
        ] {
            cred.profile_arn = arn.map(str::to_string);
            assert!(cred.has_no_real_profile_arn(), "{arn:?}");
        }
        cred.profile_arn =
            Some("arn:aws:codewhisperer:us-east-1:123456789012:profile/REALTENANT".to_string());
        assert!(!cred.has_no_real_profile_arn());
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
            kiro_api_key: None,
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
            created_at: None,
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
            kiro_api_key: None,
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
            created_at: None,
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
            kiro_api_key: None,
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
            created_at: None,
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
            kiro_api_key: None,
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
            created_at: None,
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
    fn test_thinking_adaptive_default_true_for_legacy_json() {
        // 旧版本 credentials.json 不含 thinkingAdaptive 字段 → 反序列化为 true（默认开启）
        let json = r#"{"refreshToken": "test"}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert!(creds.thinking_adaptive);
    }

    #[test]
    fn test_thinking_adaptive_explicit_true() {
        let json = r#"{"refreshToken": "test", "thinkingAdaptive": true}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert!(creds.thinking_adaptive);
    }

    #[test]
    fn test_thinking_adaptive_explicit_false_is_respected() {
        // 显式落盘 false 的账号不得被 default_true 覆盖（升级后仍保持关闭）
        let json = r#"{"refreshToken": "test", "thinkingAdaptive": false}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert!(!creds.thinking_adaptive);
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

    // ============ created_at 字段测试 ============

    #[test]
    fn test_created_at_missing_for_legacy_json() {
        // 旧版 credentials.json 不含 createdAt 字段 → 反序列化为 None（由加载流程补写）
        let json = r#"{"refreshToken": "test"}"#;
        let creds = KiroCredentials::from_json(json).unwrap();
        assert!(creds.created_at.is_none());
    }

    #[test]
    fn test_created_at_roundtrip() {
        let creds = KiroCredentials {
            refresh_token: Some("test".to_string()),
            created_at: Some("2026-11-01T13:00:00+00:00".to_string()),
            ..Default::default()
        };

        let json = creds.to_pretty_json().unwrap();
        assert!(json.contains("createdAt"));
        let parsed = KiroCredentials::from_json(&json).unwrap();
        assert_eq!(
            parsed.created_at.as_deref(),
            Some("2026-11-01T13:00:00+00:00")
        );
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

    // -------- Kiro API Key 凭据（ksk_...） --------

    #[test]
    fn test_api_key_credential_detected_by_field_or_auth_method() {
        // 只填 kiroApiKey（未声明 authMethod）也算 API Key 凭据
        let mut only_field = KiroCredentials::default();
        only_field.kiro_api_key = Some("ksk_abc".to_string());
        assert!(only_field.is_api_key_credential());

        // 只声明 authMethod（缺 key）同样命中，便于启动校验报错
        let mut only_method = KiroCredentials::default();
        only_method.auth_method = Some("api_key".to_string());
        assert!(only_method.is_api_key_credential());

        // apikey 别名
        let mut alias = KiroCredentials::default();
        alias.auth_method = Some("apikey".to_string());
        assert!(alias.is_api_key_credential());

        // social 凭据不应命中
        let mut social = KiroCredentials::default();
        social.auth_method = Some("social".to_string());
        social.refresh_token = Some("rt".to_string());
        assert!(!social.is_api_key_credential());
    }

    #[test]
    fn test_canonicalize_api_key_aliases() {
        assert_eq!(canonicalize_auth_method_value("apikey"), "api_key");
        assert_eq!(canonicalize_auth_method_value("API_KEY"), "api_key");
        assert_eq!(canonicalize_auth_method_value("api_key"), "api_key");

        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("apikey".to_string());
        cred.canonicalize_auth_method();
        assert_eq!(cred.auth_method.as_deref(), Some("api_key"));
    }

    #[test]
    fn test_masked_api_key_shows_head_and_tail() {
        let mut c = KiroCredentials::default();
        c.kiro_api_key = Some("ksk_abcdefghijklmnop1234".to_string());
        assert_eq!(c.masked_api_key().as_deref(), Some("ksk_...1234"));

        // 非 API Key 凭据无值
        let plain = KiroCredentials::default();
        assert_eq!(plain.masked_api_key(), None);
    }

    #[test]
    fn test_masked_api_key_falls_back_for_short_and_non_ascii() {
        // 短 key 保留首尾会暴露过高比例，回退 ***
        let mut short = KiroCredentials::default();
        short.kiro_api_key = Some("ksk_short".to_string());
        assert_eq!(short.masked_api_key().as_deref(), Some("***"));

        // 恰好 16 字节（边界）仍回退
        let mut boundary = KiroCredentials::default();
        boundary.kiro_api_key = Some("0123456789abcdef".to_string());
        assert_eq!(boundary.masked_api_key().as_deref(), Some("***"));

        // 非 ASCII：按字节切片可能落在 UTF-8 边界内导致 panic，必须回退
        let mut cjk = KiroCredentials::default();
        cjk.kiro_api_key = Some("密钥密钥密钥密钥密钥".to_string());
        assert_eq!(
            cjk.masked_api_key().as_deref(),
            Some("***"),
            "非 ASCII key 必须回退，否则字节切片会 panic"
        );
    }

    #[test]
    fn test_token_type_header_api_key_takes_precedence() {
        // API Key → API_KEY
        let mut api = KiroCredentials::default();
        api.kiro_api_key = Some("ksk_abc".to_string());
        assert_eq!(api.token_type_header(), Some("API_KEY"));

        // external_idp → EXTERNAL_IDP（含别名）
        let mut idp = KiroCredentials::default();
        idp.auth_method = Some("AzureAD".to_string());
        assert_eq!(idp.token_type_header(), Some("EXTERNAL_IDP"));
        assert!(idp.is_external_idp_credential());

        // social / idc 不带该头
        let mut social = KiroCredentials::default();
        social.auth_method = Some("social".to_string());
        assert_eq!(social.token_type_header(), None);
        let mut idc = KiroCredentials::default();
        idc.auth_method = Some("idc".to_string());
        assert_eq!(idc.token_type_header(), None);

        // 两者同时命中时按 API Key 处理：实际发出的 Bearer 来自 kiroApiKey
        let mut both = KiroCredentials::default();
        both.auth_method = Some("external_idp".to_string());
        both.kiro_api_key = Some("ksk_abc".to_string());
        assert_eq!(both.token_type_header(), Some("API_KEY"));
    }

    #[test]
    fn test_api_key_bearer_token_uses_verbatim_key() {
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("api_key".to_string());
        cred.kiro_api_key = Some("ksk_verbatim".to_string());
        assert_eq!(cred.api_key_bearer_token(), Some("ksk_verbatim"));

        // 空白 key 视为无效
        let mut blank = KiroCredentials::default();
        blank.kiro_api_key = Some("   ".to_string());
        assert_eq!(blank.api_key_bearer_token(), None);

        // 非 API Key 凭据不提供该 token（由 OAuth 刷新流程负责）
        let mut social = KiroCredentials::default();
        social.auth_method = Some("social".to_string());
        social.refresh_token = Some("rt".to_string());
        assert_eq!(social.api_key_bearer_token(), None);
    }

    #[test]
    fn test_kiro_api_key_serialization_roundtrip() {
        let json = r#"{"kiroApiKey":"ksk_abc123","authMethod":"api_key"}"#;
        let cred = KiroCredentials::from_json(json).unwrap();
        assert_eq!(cred.kiro_api_key.as_deref(), Some("ksk_abc123"));
        assert!(cred.is_api_key_credential());

        // 往返一致
        let out = cred.to_pretty_json().unwrap();
        assert!(out.contains("kiroApiKey"));
        let back = KiroCredentials::from_json(&out).unwrap();
        assert_eq!(back.kiro_api_key.as_deref(), Some("ksk_abc123"));

        // 未设置时不序列化该字段（旧配置文件不会凭空多出字段）
        let plain = KiroCredentials {
            refresh_token: Some("rt".to_string()),
            ..Default::default()
        };
        assert!(!plain.to_pretty_json().unwrap().contains("kiroApiKey"));
    }

    #[test]
    fn test_debug_redacts_kiro_api_key() {
        let mut cred = KiroCredentials::default();
        cred.kiro_api_key = Some("ksk_super_secret".to_string());
        let dbg = format!("{:?}", cred);
        assert!(!dbg.contains("ksk_super_secret"), "API Key 不得出现在日志里");
        assert!(dbg.contains("kiro_api_key"));
    }

    #[test]
    fn test_api_key_credential_never_gets_profile_arn() {
        // fallback_profile_arn_value 对 API Key 凭据恒为 None
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("api_key".to_string());
        cred.kiro_api_key = Some("ksk_abc".to_string());
        assert_eq!(fallback_profile_arn_value(&cred), None);
        assert!(!cred.fill_missing_profile_arn());
        assert_eq!(cred.profile_arn, None);

        // 即便显式配置了 profileArn，数据面也会移除（见 provider tests），
        // 但 fill_missing_profile_arn 不应覆盖/清空用户显式值
        let mut with_arn = KiroCredentials::default();
        with_arn.auth_method = Some("api_key".to_string());
        with_arn.kiro_api_key = Some("ksk_abc".to_string());
        with_arn.profile_arn = Some("arn:aws:codewhisperer:us-east-1:1:profile/X".to_string());
        assert!(!with_arn.fill_missing_profile_arn());
    }

    #[test]
    fn test_effective_endpoints_api_key_keeps_full_rotation() {
        // 参考实现（fork）不为 API Key 凭据钉死 runtime 端点：仍走 4 端点轮询，
        // 只通过 TokenType: API_KEY 标识凭据类型。
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("api_key".to_string());
        cred.kiro_api_key = Some("ksk_abc".to_string());

        let eps = cred.effective_endpoints("us-east-1");
        assert_eq!(
            eps.iter().map(|e| e.name).collect::<Vec<_>>(),
            vec![
                EndpointName::Ide,
                EndpointName::Runtime,
                EndpointName::Codewhisperer,
                EndpointName::Amazonq,
            ],
            "API Key 凭据默认端点顺序应与 OAuth 一致"
        );

        // 账号级 endpoint 偏好同样生效（首选在前 + 其余默认序追加）
        let mut preferred = cred.clone();
        preferred.endpoint = Some(vec![EndpointName::Runtime]);
        let eps = preferred.effective_endpoints("us-east-1");
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
}
