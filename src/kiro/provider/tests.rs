//! provider 测试（原内联测试整体迁移）
#![cfg(test)]

#[cfg(test)]
mod tests {

    use std::sync::Arc;

    use crate::kiro::endpoint::{BUCKET_THROTTLE_DURATION, Endpoint, EndpointName};
    use crate::kiro::model::credentials::{
        BUILDER_ID_PLACEHOLDER_PROFILE_ARN, KiroCredentials, SOCIAL_PROFILE_ARN,
    };
    use crate::kiro::provider::KiroProvider;
    use crate::kiro::token_manager::{CallContext, MultiTokenManager};
    use crate::model::config::Config;
    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

    fn create_test_provider(config: Config, credentials: KiroCredentials) -> KiroProvider {
        let tm = MultiTokenManager::new(config, vec![credentials], None, None, false).unwrap();
        KiroProvider::new(Arc::new(tm))
    }

    #[test]
    fn test_base_url() {
        let config = Config::default();
        let credentials = KiroCredentials::default();
        let provider = create_test_provider(config, credentials);
        assert!(provider.base_url().contains("amazonaws.com"));
        assert!(provider.base_url().contains("generateAssistantResponse"));
    }

    #[test]
    fn test_base_domain() {
        let mut config = Config::default();
        config.region = "us-east-1".to_string();
        let credentials = KiroCredentials::default();
        let provider = create_test_provider(config, credentials);
        assert_eq!(provider.base_domain(), "q.us-east-1.amazonaws.com");
    }

    #[test]
    fn test_build_headers() {
        let mut config = Config::default();
        config.region = "us-east-1".to_string();
        config.kiro_version = "0.8.0".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.profile_arn = Some("arn:aws:sso::123456789:profile/test".to_string());
        credentials.refresh_token = Some("a".repeat(150));

        let provider = create_test_provider(config, credentials.clone());
        let ctx = CallContext {
            id: 1,
            credentials,
            token: "test_token".to_string(),
        };
        let endpoint = Endpoint::by_name(EndpointName::Ide, "us-east-1");
        let headers = provider.build_headers(&ctx, "{}", 0, &endpoint).unwrap();

        assert_eq!(headers.get(CONTENT_TYPE).unwrap(), "application/json");
        assert_eq!(headers.get("x-amzn-codewhisperer-optout").unwrap(), "true");
        assert_eq!(headers.get("x-amzn-kiro-agent-mode").unwrap(), "vibe");
        assert!(
            headers
                .get(AUTHORIZATION)
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("Bearer ")
        );
        // Connection: close 已移除，启用 keep-alive 连接复用
        assert!(headers.get("connection").is_none());
    }

    #[test]
    fn test_is_monthly_request_limit_detects_reason() {
        let body = r#"{"message":"You have reached the limit.","reason":"MONTHLY_REQUEST_COUNT"}"#;
        assert!(KiroProvider::is_monthly_request_limit(body));
    }

    #[test]
    fn test_is_monthly_request_limit_nested_reason() {
        let body = r#"{"error":{"reason":"MONTHLY_REQUEST_COUNT"}}"#;
        assert!(KiroProvider::is_monthly_request_limit(body));
    }

    #[test]
    fn test_is_monthly_request_limit_false() {
        let body = r#"{"message":"nope","reason":"DAILY_REQUEST_COUNT"}"#;
        assert!(!KiroProvider::is_monthly_request_limit(body));
    }

    #[test]
    fn test_is_profile_arn_required_error_matches_upstream_message() {
        // 真实上游返回体（issue 场景：BuilderId 账号缺 profileArn）
        assert!(KiroProvider::is_profile_arn_required_error(
            r#"{"message":"profileArn is required for this request.","reason":null}"#
        ));
        assert!(KiroProvider::is_profile_arn_required_error(
            "400 Bad Request {\"message\":\"profileArn is required for this request.\"}"
        ));
        // 其他 400 不误判
        assert!(!KiroProvider::is_profile_arn_required_error(
            r#"{"message":"Invalid profileArn.","reason":null}"#
        ));
        assert!(!KiroProvider::is_profile_arn_required_error(""));
    }

    #[test]
    fn test_extract_agent_task_type_vibe_default() {
        assert_eq!(
            KiroProvider::extract_agent_task_type_from_request("{}"),
            "vibe"
        );
        assert_eq!(
            KiroProvider::extract_agent_task_type_from_request("invalid json"),
            "vibe"
        );
    }

    #[test]
    fn test_extract_agent_task_type_spectask() {
        let body = r#"{"conversationState":{"agentTaskType":"spectask","conversationId":"abc"}}"#;
        assert_eq!(
            KiroProvider::extract_agent_task_type_from_request(body),
            "spectask"
        );
    }

    #[test]
    fn test_extract_agent_task_type_vibe_explicit() {
        let body = r#"{"conversationState":{"agentTaskType":"vibe","conversationId":"abc"}}"#;
        assert_eq!(
            KiroProvider::extract_agent_task_type_from_request(body),
            "vibe"
        );
    }

    #[test]
    fn test_build_headers_spectask_mode() {
        let mut config = Config::default();
        config.region = "us-east-1".to_string();
        config.kiro_version = "0.8.0".to_string();

        let mut credentials = KiroCredentials::default();
        credentials.profile_arn = Some("arn:aws:sso::123456789:profile/test".to_string());
        credentials.refresh_token = Some("a".repeat(150));

        let provider = create_test_provider(config, credentials.clone());
        let ctx = CallContext {
            id: 1,
            credentials,
            token: "test_token".to_string(),
        };
        let spectask_body = r#"{"conversationState":{"agentTaskType":"spectask"}}"#;
        let endpoint = Endpoint::by_name(EndpointName::Ide, "us-east-1");
        let headers = provider
            .build_headers(&ctx, spectask_body, 0, &endpoint)
            .unwrap();
        assert_eq!(headers.get("x-amzn-kiro-agent-mode").unwrap(), "spectask");
    }

    #[test]
    fn test_rewrite_profile_arn_overwrites_existing_field() {
        let body = r#"{"conversationState":{},"profileArn":"old-arn"}"#;
        let mut cred = KiroCredentials::default();
        cred.profile_arn = Some("arn:aws:sso::111:profile/new".to_string());
        let result = KiroProvider::rewrite_profile_arn(body, &cred);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["profileArn"].as_str(),
            Some("arn:aws:sso::111:profile/new")
        );
    }

    #[test]
    fn test_rewrite_profile_arn_adds_field_when_missing() {
        // refresh_token 账号首次请求：body 无 profileArn，账号有 ARN → 应新增字段
        let body = r#"{"conversationState":{}}"#;
        let mut cred = KiroCredentials::default();
        cred.profile_arn = Some("arn:aws:sso::111:profile/new".to_string());
        let result = KiroProvider::rewrite_profile_arn(body, &cred);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["profileArn"].as_str(),
            Some("arn:aws:sso::111:profile/new")
        );
    }

    #[test]
    fn test_rewrite_profile_arn_injects_social_arn_when_none() {
        // 无 profile_arn 且无 clientId/secret（视为 social）→ 注入固定 Social ARN
        let body = r#"{"conversationState":{},"profileArn":"some-arn"}"#;
        let cred = KiroCredentials::default(); // profile_arn / auth_method 均为 None
        let result = KiroProvider::rewrite_profile_arn(body, &cred);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["profileArn"].as_str(), Some(SOCIAL_PROFILE_ARN));
    }

    #[test]
    fn test_rewrite_profile_arn_injects_builder_id_placeholder_for_idc_when_none() {
        // BuilderId 账号（auth_method 归一化为 idc，带 OIDC clientId/secret）缺 ARN
        // → 注入 Kiro IDE 官方占位符 ARN，而非移除字段
        let body = r#"{"conversationState":{}}"#;
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("idc".to_string());
        cred.client_id = Some("client".to_string());
        cred.client_secret = Some("secret".to_string());
        let result = KiroProvider::rewrite_profile_arn(body, &cred);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["profileArn"].as_str(),
            Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN)
        );
    }

    /// 构造开启 thinking_adaptive 的账号
    fn adaptive_cred() -> KiroCredentials {
        let mut cred = KiroCredentials::default();
        cred.thinking_adaptive = true;
        cred
    }

    #[test]
    fn test_inject_thinking_adaptive_injects_when_enabled_and_requested() {
        // 开关开启 + 客户端请求 adaptive + 非 4.5/GPT 模型 → 注入 thinking 字段
        let body = r#"{"conversationState":{"currentMessage":{"userInputMessage":{"modelId":"claude-sonnet-4-6"}}}}"#;
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), true);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["additionalModelRequestFields"]["thinking"]["type"],
            serde_json::json!("adaptive")
        );
    }

    #[test]
    fn test_inject_thinking_adaptive_not_injected_when_switch_off() {
        // 客户端请求 adaptive 但账号开关关闭 → 不注入
        let body = r#"{"conversationState":{"currentMessage":{"userInputMessage":{"modelId":"claude-sonnet-4-6"}}}}"#;
        let cred = KiroCredentials::default(); // thinking_adaptive = false
        let result = KiroProvider::rewrite_request_body(body, &cred, true);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("additionalModelRequestFields").is_none());
    }

    #[test]
    fn test_inject_thinking_adaptive_not_injected_when_not_requested() {
        // 开关开启但客户端未请求 adaptive（enabled / 不传）→ 不注入
        let body = r#"{"conversationState":{"currentMessage":{"userInputMessage":{"modelId":"claude-sonnet-4-6"}}}}"#;
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("additionalModelRequestFields").is_none());
    }

    #[test]
    fn test_inject_thinking_adaptive_skipped_for_4_5_models() {
        // "4.5" 代际模型 → 保持与 converter 整体跳过一致，不注入
        let body = r#"{"conversationState":{"currentMessage":{"userInputMessage":{"modelId":"claude-sonnet-4.5"}}}}"#;
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), true);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("additionalModelRequestFields").is_none());
    }

    #[test]
    fn test_inject_thinking_adaptive_skipped_for_gpt_models() {
        // GPT 系模型通过 reasoning.effort 传递配置，不走 Kiro thinking 协议，
        // 所以 rewrite_request_body 不应注入 thinking.type=adaptive。
        let body = r#"{"conversationState":{"currentMessage":{"userInputMessage":{"modelId":"gpt-5.6-luna"}}}}"#;
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), true);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("additionalModelRequestFields").is_none());
    }

    #[test]
    fn test_inject_thinking_adaptive_creates_fields_when_missing() {
        // additionalModelRequestFields 不存在 → 创建新对象并插入 thinking
        let body = r#"{"conversationState":{"currentMessage":{"userInputMessage":{"modelId":"claude-opus-4-6"}}}}"#;
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), true);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["additionalModelRequestFields"].is_object());
        assert_eq!(
            v["additionalModelRequestFields"]["thinking"]["type"],
            serde_json::json!("adaptive")
        );
    }

    #[test]
    fn test_inject_thinking_adaptive_merges_into_existing_fields() {
        // additionalModelRequestFields 已存在 → 保留既有键，追加 thinking
        let body = r#"{"conversationState":{"currentMessage":{"userInputMessage":{"modelId":"claude-opus-4-6"}}},"additionalModelRequestFields":{"max_tokens":8192}}"#;
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), true);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let fields = &v["additionalModelRequestFields"];
        assert_eq!(fields["max_tokens"], serde_json::json!(8192));
        assert_eq!(fields["thinking"]["type"], serde_json::json!("adaptive"));
    }

    #[test]
    fn test_inject_thinking_adaptive_invalid_json_passthrough() {
        // JSON 解析失败 → 原样返回
        let body = "not-a-json";
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), true);
        assert_eq!(result, body);
    }

    #[test]
    fn test_inject_thinking_adaptive_skipped_when_model_id_missing() {
        // fail-closed：modelId 路径缺失（取不到模型）→ 跳过注入
        let body =
            r#"{"conversationState":{"currentMessage":{"userInputMessage":{"content":"hi"}}}}"#;
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), true);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("additionalModelRequestFields").is_none());
    }

    #[test]
    fn test_fallback_infer_idc_when_oidc_creds_present() {
        // auth_method 缺失但带 OIDC clientId/secret → 推断为 idc → BuilderId 占位符
        let mut cred = KiroCredentials::default();
        cred.client_id = Some("c".to_string());
        cred.client_secret = Some("s".to_string());
        assert_eq!(
            KiroProvider::fallback_profile_arn(&cred),
            Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN)
        );
    }

    #[test]
    fn test_fallback_infer_social_when_no_oidc_creds() {
        // auth_method 缺失且无 OIDC 凭据 → 推断为 social → 固定 Social ARN
        let cred = KiroCredentials::default();
        assert_eq!(
            KiroProvider::fallback_profile_arn(&cred),
            Some(SOCIAL_PROFILE_ARN)
        );
    }

    #[test]
    fn test_fallback_unknown_auth_method_returns_none() {
        // 未知/未归一化 auth_method（external_idp、enterprise 等）→ None
        // 移除字段交由上游 400 触发 ProfileArnMissing 禁用
        for method in ["external_idp", "enterprise", "unknown"] {
            let mut cred = KiroCredentials::default();
            cred.auth_method = Some(method.to_string());
            assert_eq!(
                KiroProvider::fallback_profile_arn(&cred),
                None,
                "auth_method={method} 应返回 None"
            );
        }
    }

    #[test]
    fn test_rewrite_profile_arn_removes_field_for_external_idp_when_none() {
        // 企业 IdC 账号缺 ARN：真实 ARN 因租户而异，仍移除字段，
        // 由上游 400 触发 ProfileArnMissing 禁用逻辑
        let body = r#"{"conversationState":{},"profileArn":"some-arn"}"#;
        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("external_idp".to_string());
        let result = KiroProvider::rewrite_profile_arn(body, &cred);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("profileArn").is_none());
    }

    #[test]
    fn test_rewrite_profile_arn_invalid_json_falls_back() {
        let body = "not-json";
        let mut cred = KiroCredentials::default();
        cred.profile_arn = Some("arn:aws:sso::111:profile/x".to_string());
        let result = KiroProvider::rewrite_profile_arn(body, &cred);
        assert_eq!(result, "not-json");
    }

    // -------- 多端点 LB: select_endpoint + amz_target 注入 --------

    #[test]
    fn test_select_endpoint_skips_throttled_buckets() {
        let mut config = Config::default();
        config.region = "us-east-1".to_string();
        let mut creds = KiroCredentials::default();
        creds.id = Some(1);
        creds.refresh_token = Some("a".repeat(150));
        let provider = create_test_provider(config, creds.clone());

        // 封禁 Ide 与 Runtime 两个桶
        provider
            .endpoint_registry
            .throttle(1, EndpointName::Ide, BUCKET_THROTTLE_DURATION);
        provider
            .endpoint_registry
            .throttle(1, EndpointName::Runtime, BUCKET_THROTTLE_DURATION);

        let picked = provider
            .select_endpoint(&creds, 0)
            .expect("应跳过封禁桶返回剩余端点");
        assert_eq!(picked.name, EndpointName::Codewhisperer);
    }

    #[test]
    fn test_select_endpoint_attempt_offset_rotates_endpoints() {
        let mut config = Config::default();
        config.region = "us-east-1".to_string();
        let mut creds = KiroCredentials::default();
        creds.id = Some(2);
        creds.refresh_token = Some("a".repeat(150));
        let provider = create_test_provider(config, creds.clone());

        // 4 桶均未封禁：attempt=0/1/2/3 应轮询 Ide/Runtime/Codewhisperer/Amazonq
        assert_eq!(
            provider.select_endpoint(&creds, 0).unwrap().name,
            EndpointName::Ide
        );
        assert_eq!(
            provider.select_endpoint(&creds, 1).unwrap().name,
            EndpointName::Runtime
        );
        assert_eq!(
            provider.select_endpoint(&creds, 2).unwrap().name,
            EndpointName::Codewhisperer
        );
        assert_eq!(
            provider.select_endpoint(&creds, 3).unwrap().name,
            EndpointName::Amazonq
        );
        // attempt=4 回卷到 Ide
        assert_eq!(
            provider.select_endpoint(&creds, 4).unwrap().name,
            EndpointName::Ide
        );
    }

    #[test]
    fn test_select_endpoint_returns_none_when_all_throttled() {
        let mut config = Config::default();
        config.region = "us-east-1".to_string();
        let mut creds = KiroCredentials::default();
        creds.id = Some(3);
        creds.refresh_token = Some("a".repeat(150));
        let provider = create_test_provider(config, creds.clone());

        // 封禁全部 4 桶
        for name in EndpointName::ALL {
            provider
                .endpoint_registry
                .throttle(3, name, BUCKET_THROTTLE_DURATION);
        }

        assert!(provider.select_endpoint(&creds, 0).is_none());
    }

    #[test]
    fn test_select_endpoint_respects_account_preferred_order() {
        let mut config = Config::default();
        config.region = "us-east-1".to_string();
        let mut creds = KiroCredentials::default();
        creds.id = Some(4);
        creds.refresh_token = Some("a".repeat(150));
        // 账号声明首选 Runtime
        creds.endpoint = Some(vec![EndpointName::Runtime]);
        let provider = create_test_provider(config, creds.clone());

        // attempt=0 应选 Runtime（首选在首）
        assert_eq!(
            provider.select_endpoint(&creds, 0).unwrap().name,
            EndpointName::Runtime
        );
        // attempt=1 跳过 Runtime → Ide（默认序下一个）
        assert_eq!(
            provider.select_endpoint(&creds, 1).unwrap().name,
            EndpointName::Ide
        );
    }

    #[test]
    fn test_build_headers_injects_amz_target_for_codewhisperer_endpoint() {
        let mut config = Config::default();
        config.region = "us-east-1".to_string();
        config.kiro_version = "0.8.0".to_string();
        let mut creds = KiroCredentials::default();
        creds.id = Some(5);
        creds.refresh_token = Some("a".repeat(150));

        let provider = create_test_provider(config, creds.clone());
        let ctx = CallContext {
            id: 5,
            credentials: creds,
            token: "tok".to_string(),
        };
        let endpoint = Endpoint::by_name(EndpointName::Codewhisperer, "us-east-1");
        let headers = provider.build_headers(&ctx, "{}", 0, &endpoint).unwrap();

        assert_eq!(
            headers.get("x-amz-target").unwrap(),
            "AmazonCodeWhispererStreamingService.GenerateAssistantResponse"
        );
        assert_eq!(
            headers.get("host").unwrap(),
            "codewhisperer.us-east-1.amazonaws.com"
        );
    }

    #[test]
    fn test_build_headers_omits_amz_target_for_ide_endpoint() {
        let mut config = Config::default();
        config.region = "us-east-1".to_string();
        let mut creds = KiroCredentials::default();
        creds.id = Some(6);
        creds.refresh_token = Some("a".repeat(150));

        let provider = create_test_provider(config, creds.clone());
        let ctx = CallContext {
            id: 6,
            credentials: creds,
            token: "tok".to_string(),
        };
        let endpoint = Endpoint::by_name(EndpointName::Ide, "us-east-1");
        let headers = provider.build_headers(&ctx, "{}", 0, &endpoint).unwrap();

        assert!(headers.get("x-amz-target").is_none());
        assert_eq!(headers.get("host").unwrap(), "q.us-east-1.amazonaws.com");
    }
}
