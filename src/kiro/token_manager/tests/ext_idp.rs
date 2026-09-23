// Copyright (c) 2026 Harllan He. Licensed under MIT.
// token_manager 测试（自 tests.rs 拆出，纯代码搬移）
#[cfg(test)]
pub(crate) mod tests {
    
    
    use super::super::super::refresh::RefreshTokenInvalidError;
    
    
    use super::super::super::refresh_token;
    use crate::kiro::model::credentials::KiroCredentials;
    
    use crate::model::config::Config;
    

    pub(crate) async fn spawn_single_response_server(status: u16, body: &'static str) -> String {
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
    pub(crate) struct TempDirGuard(std::path::PathBuf);

    impl TempDirGuard {
        pub(crate) fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        pub(crate) fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
