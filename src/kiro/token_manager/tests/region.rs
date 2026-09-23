// Copyright (c) 2026 Harllan He. Licensed under MIT.
// token_manager 测试（自 tests.rs 拆出，纯代码搬移）
#[cfg(test)]
pub(crate) mod tests {
    
    
    
    use super::super::super::refresh::{
        apply_idc_refresh_response, select_usage_limits_token,
    };
    
    
    use crate::kiro::model::credentials::KiroCredentials;
    use crate::kiro::model::token_refresh::IdcRefreshResponse;
    
    use crate::model::config::Config;
    

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
}
