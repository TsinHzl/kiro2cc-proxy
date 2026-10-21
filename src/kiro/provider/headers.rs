//! 请求头构建：普通请求与 MCP 请求的 HTTP 头（Authorization、Host、Content-Type 等）。
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HOST, HeaderMap, HeaderValue};
use uuid::Uuid;

use crate::kiro::endpoint::Endpoint;
use crate::kiro::machine_id;
use crate::kiro::token_manager::CallContext;

use super::core::{KiroProvider, MAX_RETRIES_PER_CREDENTIAL};

const KIRO_AWS_SDK_JS_VERSION: &str = "1.0.27";
const AWS_SDK_UA_PROTOCOL_VERSION: &str = "2.1";
const CODEWHISPERER_STREAMING_API_VERSION: &str = "1.0.27";

impl KiroProvider {
    pub(crate) fn build_headers(
        &self,
        ctx: &CallContext,
        request_body: &str,
        attempt: usize,
        endpoint: &Endpoint,
    ) -> anyhow::Result<HeaderMap> {
        let config = self.token_manager.config();

        let machine_id = machine_id::generate_from_credentials(&ctx.credentials, config);

        let kiro_version = &config.kiro_version;
        let os_name = &config.system_version;
        let node_version = &config.node_version;

        let x_amz_user_agent = format!(
            "aws-sdk-js/{} KiroIDE-{}-{}",
            KIRO_AWS_SDK_JS_VERSION, kiro_version, machine_id
        );

        let user_agent = format!(
            "aws-sdk-js/{} ua/{} os/{} lang/js md/nodejs#{} api/codewhispererstreaming#{} m/E KiroIDE-{}-{}",
            KIRO_AWS_SDK_JS_VERSION,
            AWS_SDK_UA_PROTOCOL_VERSION,
            os_name,
            node_version,
            CODEWHISPERER_STREAMING_API_VERSION,
            kiro_version,
            machine_id
        );

        let agent_mode = Self::extract_agent_task_type_from_request(request_body);

        let mut headers = HeaderMap::new();

        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-amzn-codewhisperer-optout",
            HeaderValue::from_static("true"),
        );
        headers.insert(
            "x-amzn-kiro-agent-mode",
            HeaderValue::from_static(agent_mode),
        );
        headers.insert(
            "x-amz-user-agent",
            HeaderValue::from_str(&x_amz_user_agent).unwrap(),
        );
        headers.insert(
            reqwest::header::USER_AGENT,
            HeaderValue::from_str(&user_agent).unwrap(),
        );
        headers.insert(
            HOST,
            HeaderValue::from_str(&self.base_domain_for(&ctx.credentials, endpoint)).unwrap(),
        );
        headers.insert(
            "amz-sdk-invocation-id",
            HeaderValue::from_str(&Uuid::new_v4().to_string()).unwrap(),
        );
        headers.insert(
            "amz-sdk-request",
            HeaderValue::from_str(&format!(
                "attempt={}; max={}",
                attempt + 1,
                MAX_RETRIES_PER_CREDENTIAL
            ))
            .unwrap(),
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", ctx.token)).unwrap(),
        );

        // codewhisperer / amazonq 端点需要 x-amz-target 头路由到对应后端服务
        if let Some(target) = endpoint.amz_target {
            headers.insert("x-amz-target", HeaderValue::from_static(target));
        }

        // API Key → API_KEY；企业 SSO → EXTERNAL_IDP；social / idc 不带该头。
        // （Kiro 要求 API Key 与企业 IdP token 显式标识类型，否则按 social 校验必失败）
        if let Some(token_type) = ctx.credentials.token_type_header() {
            headers.insert("TokenType", HeaderValue::from_static(token_type));
        }

        Ok(headers)
    }

    /// 构建 MCP 请求头
    pub(crate) fn build_mcp_headers(
        &self,
        ctx: &CallContext,
        attempt: usize,
    ) -> anyhow::Result<HeaderMap> {
        let config = self.token_manager.config();

        let machine_id = machine_id::generate_from_credentials(&ctx.credentials, config);

        let kiro_version = &config.kiro_version;
        let os_name = &config.system_version;
        let node_version = &config.node_version;

        let x_amz_user_agent = format!(
            "aws-sdk-js/{} KiroIDE-{}-{}",
            KIRO_AWS_SDK_JS_VERSION, kiro_version, machine_id
        );

        let user_agent = format!(
            "aws-sdk-js/{} ua/{} os/{} lang/js md/nodejs#{} api/codewhispererstreaming#{} m/E KiroIDE-{}-{}",
            KIRO_AWS_SDK_JS_VERSION,
            AWS_SDK_UA_PROTOCOL_VERSION,
            os_name,
            node_version,
            CODEWHISPERER_STREAMING_API_VERSION,
            kiro_version,
            machine_id
        );

        let mut headers = HeaderMap::new();

        // 按照严格顺序添加请求头
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert(
            "x-amz-user-agent",
            HeaderValue::from_str(&x_amz_user_agent).unwrap(),
        );
        headers.insert("user-agent", HeaderValue::from_str(&user_agent).unwrap());
        // MCP 端点独立于多端点 LB，沿用 q.{region}.amazonaws.com
        headers.insert(
            "host",
            HeaderValue::from_str(&format!(
                "q.{}.amazonaws.com",
                ctx.credentials
                    .effective_api_region(self.token_manager.config())
            ))
            .unwrap(),
        );
        headers.insert(
            "amz-sdk-invocation-id",
            HeaderValue::from_str(&Uuid::new_v4().to_string()).unwrap(),
        );
        headers.insert(
            "amz-sdk-request",
            HeaderValue::from_str(&format!(
                "attempt={}; max={}",
                attempt + 1,
                MAX_RETRIES_PER_CREDENTIAL
            ))
            .unwrap(),
        );
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {}", ctx.token)).unwrap(),
        );
        // API Key 凭据的 MCP（WebSearch）调用同样需要 `TokenType: API_KEY`，
        // 否则上游会按 social 凭据校验该 Bearer Token 并失败。
        // 仅对 API Key 生效：不改变既有 social / idc / external_idp 的 MCP 行为。
        if ctx.credentials.is_api_key_credential() {
            headers.insert("TokenType", HeaderValue::from_static("API_KEY"));
        }
        Ok(headers)
    }
}
