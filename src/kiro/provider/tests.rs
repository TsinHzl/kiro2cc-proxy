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
    fn test_thinking_adaptive_switch_off_strips_field() {
        // 开关关闭 + 请求体已含 thinking（converter 注入）→ 剥离
        let body = r#"{"conversationState":{"currentMessage":{"userInputMessage":{"modelId":"claude-sonnet-4-6"}}},"additionalModelRequestFields":{"thinking":{"type":"adaptive"}}}"#;
        let cred = KiroCredentials::default(); // thinking_adaptive = false
        let result = KiroProvider::rewrite_request_body(body, &cred, true);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["additionalModelRequestFields"]["thinking"].is_null());
    }

    #[test]
    fn test_thinking_adaptive_switch_on_keeps_field() {
        // 开关开启 + 请求体已含 thinking → 保留（converter 注入，provider 不重复注入）
        let body = r#"{"conversationState":{"currentMessage":{"userInputMessage":{"modelId":"claude-sonnet-4-6"}}},"additionalModelRequestFields":{"thinking":{"type":"adaptive"}}}"#;
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), true);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["additionalModelRequestFields"]["thinking"]["type"],
            serde_json::json!("adaptive")
        );
    }

    fn body_for_model(model_id: &str, history0: Option<&str>) -> String {
        let mut v = serde_json::json!({
            "conversationState": {
                "currentMessage": {"userInputMessage": {"modelId": model_id}}
            },
            "additionalModelRequestFields": {"output_config": {"effort": "low"}}
        });
        if let Some(content) = history0 {
            v["conversationState"]["history"] = serde_json::json!([
                {"userInputMessage": {"content": content}},
                {"assistantResponseMessage": {"content": "I will follow these instructions."}}
            ]);
        }
        v.to_string()
    }

    #[test]
    fn test_thinking_switch_on_force_injects_when_client_did_not_request() {
        // 开关开启 + 客户端未请求 thinking（体内无 thinking）+ 支持原生字段的模型 → 强制注入，
        // 同时保留已有字段
        let body = body_for_model("claude-opus-5.5", Some("SYSTEM RULES"));
        let result = KiroProvider::rewrite_request_body(&body, &adaptive_cred(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["additionalModelRequestFields"]["thinking"]["type"],
            serde_json::json!("adaptive")
        );
        assert_eq!(
            v["additionalModelRequestFields"]["output_config"]["effort"],
            serde_json::json!("low")
        );
    }

    #[test]
    fn test_thinking_switch_on_force_creates_fields_object_when_missing() {
        let body = r#"{"conversationState":{"currentMessage":{"userInputMessage":{"modelId":"claude-sonnet-4.6"}}}}"#;
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["additionalModelRequestFields"]["thinking"]["type"],
            serde_json::json!("adaptive")
        );
    }

    #[test]
    fn test_thinking_switch_on_does_not_stack_on_enabled_text_tag() {
        // enabled 请求已由 history[0] 文本标签控制 thinking，不得再叠加原生字段
        let body = body_for_model("claude-opus-5.5", Some(&format!("{TAG}\nSYSTEM RULES")));
        let result = KiroProvider::rewrite_request_body(&body, &adaptive_cred(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["additionalModelRequestFields"]["thinking"].is_null());
    }

    #[test]
    fn test_thinking_switch_on_skips_unsupported_models() {
        // "4.5" 代际 / 第三方 / GPT 系上游会 400 拒绝该字段，开关只能关闭、不能强制开启
        for model in [
            "claude-sonnet-4.5",
            "deepseek-3.2",
            "glm-5",
            "gpt-5.6-terra",
            // 未被字段黑名单跳过、但并非 Claude 系：不得强制注入
            "minimax-m2.1",
            "auto",
        ] {
            let body = body_for_model(model, None);
            let result = KiroProvider::rewrite_request_body(&body, &adaptive_cred(), false);
            let v: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert!(
                v["additionalModelRequestFields"]["thinking"].is_null(),
                "{model}"
            );
        }
    }

    #[test]
    fn test_thinking_switch_on_injects_when_body_merely_quotes_tag_literal() {
        // history[0] 只是正文里引用了标签字面量（非 converter 注入的前缀）→ 仍须强制注入
        let body = body_for_model(
            "claude-opus-5.5",
            Some("请解释字符串 <thinking_mode> 的含义"),
        );
        let result = KiroProvider::rewrite_request_body(&body, &adaptive_cred(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["additionalModelRequestFields"]["thinking"]["type"],
            serde_json::json!("adaptive")
        );

        // 前缀形态相同但没有配对确认语（用户原文）→ 同样视为非注入，仍须注入
        let body = serde_json::json!({
            "conversationState": {
                "history": [
                    {"userInputMessage": {"content": format!("{TAG}\n用户自带")}},
                    {"assistantResponseMessage": {"content": "别的回复"}}
                ],
                "currentMessage": {"userInputMessage": {"modelId": "claude-opus-5.5"}}
            }
        })
        .to_string();
        let result = KiroProvider::rewrite_request_body(&body, &adaptive_cred(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["additionalModelRequestFields"]["thinking"]["type"],
            serde_json::json!("adaptive")
        );
    }

    #[test]
    fn test_thinking_switch_on_without_model_id_is_noop() {
        // MCP 等无 modelId 的请求体不触发注入
        let body = r#"{"conversationState":{}}"#;
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("additionalModelRequestFields").is_none());
    }

    #[test]
    fn test_thinking_switch_off_never_injects() {
        let body = body_for_model("claude-opus-5.5", None);
        let result = KiroProvider::rewrite_request_body(&body, &KiroCredentials::default(), true);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["additionalModelRequestFields"]["thinking"].is_null());
    }

    const TAG: &str =
        "<thinking_mode>enabled</thinking_mode><max_thinking_length>24576</max_thinking_length>";

    fn body_with_history(history: serde_json::Value) -> String {
        serde_json::json!({
            "conversationState": {
                "history": history,
                "currentMessage": {"userInputMessage": {"modelId": "claude-sonnet-4.5"}}
            }
        })
        .to_string()
    }

    #[test]
    fn test_thinking_switch_off_strips_text_tag_keeps_system_prompt() {
        // enabled 请求（4.5 代际）：标签 + "\n" + 系统提示；开关关闭 → 只去掉标签
        let body = body_with_history(serde_json::json!([
            {"userInputMessage": {"content": format!("{TAG}\nSYSTEM RULES")}},
            {"assistantResponseMessage": {"content": "I will follow these instructions."}}
        ]));
        let result = KiroProvider::rewrite_request_body(&body, &KiroCredentials::default(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let h = v["conversationState"]["history"].as_array().unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0]["userInputMessage"]["content"], "SYSTEM RULES");
    }

    #[test]
    fn test_thinking_switch_off_removes_standalone_tag_pair() {
        // 无系统消息时 converter 插入的「仅标签 user + ack assistant」配对 → 整对移除
        let body = body_with_history(serde_json::json!([
            {"userInputMessage": {"content": TAG}},
            {"assistantResponseMessage": {"content": "I will follow these instructions."}},
            {"userInputMessage": {"content": "earlier question"}},
            {"assistantResponseMessage": {"content": "earlier answer"}}
        ]));
        let result = KiroProvider::rewrite_request_body(&body, &KiroCredentials::default(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        let h = v["conversationState"]["history"].as_array().unwrap();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0]["userInputMessage"]["content"], "earlier question");
    }

    #[test]
    fn test_thinking_switch_off_keeps_user_authored_tag_with_tool_pairing() {
        // 用户原文恰为完整标签、后接带 tool_use 的真实 assistant：不得整对删除，
        // 否则 tool_use 被删而当前 tool_result 残留
        let history = serde_json::json!([
            {"userInputMessage": {"content": TAG}},
            {"assistantResponseMessage": {
                "content": "reading",
                "toolUses": [{"toolUseId": "t1", "name": "Read", "input": {}}]
            }}
        ]);
        let body = body_with_history(history.clone());
        let result = KiroProvider::rewrite_request_body(&body, &KiroCredentials::default(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["conversationState"]["history"], history);
    }

    #[test]
    fn test_thinking_switch_off_keeps_tag_when_ack_has_tool_uses_or_differs() {
        // 确认语相同但带工具调用 / 内容不是确认语：都视为用户原文，不动（含标签+正文形态）
        for (user, assistant) in [
            (
                TAG.to_string(),
                serde_json::json!({"content": "I will follow these instructions.",
                    "toolUses": [{"toolUseId": "t1", "name": "Read", "input": {}}]}),
            ),
            (
                format!("{TAG}\nuser text"),
                serde_json::json!({"content": "something else"}),
            ),
        ] {
            let history = serde_json::json!([
                {"userInputMessage": {"content": user}},
                {"assistantResponseMessage": assistant}
            ]);
            let body = body_with_history(history.clone());
            let result =
                KiroProvider::rewrite_request_body(&body, &KiroCredentials::default(), false);
            let v: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(v["conversationState"]["history"], history);
        }
    }

    #[test]
    fn test_thinking_switch_on_keeps_text_tag() {
        let content = format!("{TAG}\nSYSTEM RULES");
        let body = body_with_history(serde_json::json!([
            {"userInputMessage": {"content": content}},
            {"assistantResponseMessage": {"content": "I will follow these instructions."}}
        ]));
        let result = KiroProvider::rewrite_request_body(&body, &adaptive_cred(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            v["conversationState"]["history"][0]["userInputMessage"]["content"],
            serde_json::json!(content)
        );
    }

    #[test]
    fn test_thinking_switch_off_leaves_unrelated_history_untouched() {
        // 标签不在 history[0] 开头 / 形态不同 / 无 history → 不动
        for history in [
            serde_json::json!([{"userInputMessage": {"content": format!("SYS {TAG}")}}]),
            serde_json::json!([{"userInputMessage": {"content": "<thinking_mode>enabled</thinking_mode>\nx"}}]),
            serde_json::json!([{"userInputMessage": {"content": "plain system"}}]),
            serde_json::json!([]),
        ] {
            let body = body_with_history(history.clone());
            let result =
                KiroProvider::rewrite_request_body(&body, &KiroCredentials::default(), false);
            let v: serde_json::Value = serde_json::from_str(&result).unwrap();
            assert_eq!(v["conversationState"]["history"], history);
        }
        let no_history =
            r#"{"conversationState":{"currentMessage":{"userInputMessage":{"modelId":"m"}}}}"#;
        let result =
            KiroProvider::rewrite_request_body(no_history, &KiroCredentials::default(), false);
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["conversationState"].get("history").is_none());
    }

    #[test]
    fn test_rewrite_request_body_invalid_json_passthrough() {
        // JSON 解析失败 → 原样返回（不阻断请求）
        let body = "not-json";
        let result = KiroProvider::rewrite_request_body(body, &adaptive_cred(), true);
        assert_eq!(result, "not-json");
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
