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
}
