use super::is_token_expiring_within;
use crate::http_client::{ProxyConfig, build_client};
use crate::kiro::machine_id;
use crate::kiro::model::available_models::AvailableModelsResponse;
use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::model::token_refresh::{
    IdcRefreshRequest, IdcRefreshResponse, RefreshRequest, RefreshResponse,
};
use crate::kiro::model::usage_limits::UsageLimitsResponse;
use crate::model::config::Config;
use anyhow::bail;
use chrono::{Duration, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// 检查 Token 是否已过期（提前 5 分钟判断）
pub(crate) fn is_token_expired(credentials: &KiroCredentials) -> bool {
    is_token_expiring_within(credentials, 5).unwrap_or(true)
}

/// 检查 Token 是否即将过期（10分钟内）
pub(crate) fn is_token_expiring_soon(credentials: &KiroCredentials) -> bool {
    is_token_expiring_within(credentials, 10).unwrap_or(false)
}

pub(crate) fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// 验证 refreshToken 的基本有效性
pub(crate) fn validate_refresh_token(credentials: &KiroCredentials) -> anyhow::Result<()> {
    let refresh_token = credentials
        .refresh_token
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("缺少 refreshToken"))?;

    if refresh_token.is_empty() {
        bail!("refreshToken 为空");
    }

    // external_idp（Azure AD 等）的 refresh_token 长度不受 Kiro 截断规则约束
    let is_external_idp = credentials
        .auth_method
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case("external_idp"));

    if !is_external_idp
        && (refresh_token.len() < 100
            || refresh_token.ends_with("...")
            || refresh_token.contains("..."))
    {
        bail!(
            "refreshToken 已被截断（长度: {} 字符）。\n\
             这通常是 Kiro IDE 为了防止凭证被第三方工具使用而故意截断的。",
            refresh_token.len()
        );
    }

    Ok(())
}

/// refreshToken 已被服务端永久撤销（invalid_grant），区别于瞬态刷新失败
#[derive(Debug)]
pub(crate) struct RefreshTokenInvalidError;

impl std::fmt::Display for RefreshTokenInvalidError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "refreshToken 已被服务端撤销（invalid_grant）")
    }
}

/// 账号不支持 getUsageLimits 查询（如 BuilderId 个人账号），区别于认证失败等可重试错误
#[derive(Debug)]
pub(crate) struct UsageLimitsUnsupportedError;

impl std::fmt::Display for UsageLimitsUnsupportedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "此账号不支持 getUsageLimits 查询")
    }
}

impl std::error::Error for UsageLimitsUnsupportedError {}

impl std::error::Error for RefreshTokenInvalidError {}

/// 判断刷新响应是否为服务端已撤销 refreshToken（invalid_grant）
pub(crate) fn is_invalid_grant_response(status: u16, body: &str) -> bool {
    status == 400
        && body.contains("invalid_grant")
        && body.contains("Invalid refresh token provided")
}

/// 刷新 Token
pub(crate) async fn refresh_token(
    credentials: &KiroCredentials,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<KiroCredentials> {
    validate_refresh_token(credentials)?;

    // 根据 auth_method 选择刷新方式
    // 如果未指定 auth_method，根据是否有 clientId/clientSecret 自动判断
    let auth_method = credentials.auth_method.as_deref().unwrap_or_else(|| {
        if credentials.client_id.is_some() && credentials.client_secret.is_some() {
            "idc"
        } else {
            "social"
        }
    });

    if auth_method.eq_ignore_ascii_case("idc")
        || auth_method.eq_ignore_ascii_case("builder-id")
        || auth_method.eq_ignore_ascii_case("iam")
    {
        refresh_idc_token(credentials, config, proxy).await
    } else if auth_method.eq_ignore_ascii_case("external_idp") {
        refresh_external_idp_token(credentials, config, proxy).await
    } else {
        refresh_social_token(credentials, config, proxy).await
    }
}

/// 刷新 Social Token
async fn refresh_social_token(
    credentials: &KiroCredentials,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<KiroCredentials> {
    tracing::info!("正在刷新 Social Token...");

    let refresh_token = credentials.refresh_token.as_ref().unwrap();
    // 优先级：账号.auth_region > 账号.region > config.auth_region > config.region
    let region = credentials.effective_auth_region(config);

    let refresh_url = format!("https://prod.{}.auth.desktop.kiro.dev/refreshToken", region);
    let refresh_domain = format!("prod.{}.auth.desktop.kiro.dev", region);
    let machine_id = machine_id::generate_from_credentials(credentials, config);
    let kiro_version = &config.kiro_version;

    let client = build_client(proxy, 60, config.tls_backend)?;
    let body = RefreshRequest {
        refresh_token: refresh_token.to_string(),
    };

    let response = client
        .post(&refresh_url)
        .header("Accept", "application/json, text/plain, */*")
        .header("Content-Type", "application/json")
        .header(
            "User-Agent",
            format!("KiroIDE-{}-{}", kiro_version, machine_id),
        )
        .header("Accept-Encoding", "gzip, compress, deflate, br")
        .header("host", &refresh_domain)
        .header("Connection", "close")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        if is_invalid_grant_response(status.as_u16(), &body_text) {
            return Err(RefreshTokenInvalidError.into());
        }
        let error_msg = match status.as_u16() {
            401 => "OAuth 凭证已过期或无效，需要重新认证",
            403 => "权限不足，无法刷新 Token",
            429 => "请求过于频繁，已被限流",
            500..=599 => "服务器错误，AWS OAuth 服务暂时不可用",
            _ => "Token 刷新失败",
        };
        bail!("{}: {} {}", error_msg, status, body_text);
    }

    let data: RefreshResponse = response.json().await?;

    let mut new_credentials = credentials.clone();
    new_credentials.access_token = Some(data.access_token);

    if let Some(new_refresh_token) = data.refresh_token {
        new_credentials.refresh_token = Some(new_refresh_token);
    }

    if let Some(profile_arn) = data.profile_arn {
        new_credentials.profile_arn = Some(profile_arn);
    }

    if let Some(expires_in) = data.expires_in {
        let expires_at = Utc::now() + Duration::seconds(expires_in);
        new_credentials.expires_at = Some(expires_at.to_rfc3339());
    }

    Ok(new_credentials)
}

const IDC_AMZ_USER_AGENT: &str = "aws-sdk-js/3.738.0 ua/2.1 os/other lang/js md/browser#unknown_unknown api/sso-oidc#3.738.0 m/E KiroIDE";

/// Kiro auth token 文件的 region 字段结构
#[derive(Debug, Deserialize)]
struct KiroAuthTokenFile {
    #[serde(default)]
    region: Option<String>,
}

/// 从 ~/.aws/sso/cache/kiro-auth-token.json 读取 region 字段
fn read_region_from_kiro_auth_token() -> Option<String> {
    let home = dirs::home_dir()?;
    let path = home.join(".aws/sso/cache/kiro-auth-token.json");
    let content = std::fs::read_to_string(&path).ok()?;
    let token_file: KiroAuthTokenFile = serde_json::from_str(&content).ok()?;
    let region = token_file.region.filter(|r| !r.is_empty());
    if let Some(ref r) = region {
        tracing::debug!("从 kiro-auth-token.json 读取到 region: {}", r);
    }
    region
}

/// 刷新 IdC Token (AWS SSO OIDC)
async fn refresh_idc_token(
    credentials: &KiroCredentials,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<KiroCredentials> {
    tracing::info!("正在刷新 IdC Token...");

    let refresh_token = credentials.refresh_token.as_ref().unwrap();
    let client_id = credentials
        .client_id
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("IdC 刷新需要 clientId"))?;
    let client_secret = credentials
        .client_secret
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("IdC 刷新需要 clientSecret"))?;

    // Region 优先级：账号.auth_region > 账号.region > config.auth_region > config.region > kiro-auth-token.json.region
    // 先尝试账号/配置链，如果最终是默认的 us-east-1 则再看 token 文件
    let region_from_chain = credentials.effective_auth_region(config);
    let token_file_region = read_region_from_kiro_auth_token();
    let region = if let Some(ref file_region) = token_file_region {
        // 如果账号/配置链中有显式配置（非默认值），优先使用；否则用 token 文件的 region
        if credentials.auth_region.is_some()
            || credentials.region.is_some()
            || config.auth_region.is_some()
        {
            region_from_chain
        } else {
            tracing::info!("使用 kiro-auth-token.json 的 region: {}", file_region);
            file_region.as_str()
        }
    } else {
        region_from_chain
    };
    let refresh_url = format!("https://oidc.{}.amazonaws.com/token", region);

    let client = build_client(proxy, 60, config.tls_backend)?;
    let body = IdcRefreshRequest {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
        refresh_token: refresh_token.to_string(),
        grant_type: "refresh_token".to_string(),
    };

    let response = client
        .post(&refresh_url)
        .header("Content-Type", "application/json")
        .header("Host", format!("oidc.{}.amazonaws.com", region))
        .header("Connection", "keep-alive")
        .header("x-amz-user-agent", IDC_AMZ_USER_AGENT)
        .header("Accept", "*/*")
        .header("Accept-Language", "*")
        .header("sec-fetch-mode", "cors")
        .header("User-Agent", "node")
        .header("Accept-Encoding", "br, gzip, deflate")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        if is_invalid_grant_response(status.as_u16(), &body_text) {
            return Err(RefreshTokenInvalidError.into());
        }
        let error_msg = match status.as_u16() {
            401 => "IdC 凭证已过期或无效，需要重新认证",
            403 => "权限不足，无法刷新 Token",
            429 => "请求过于频繁，已被限流",
            500..=599 => "服务器错误，AWS OIDC 服务暂时不可用",
            _ => "IdC Token 刷新失败",
        };
        bail!("{}: {} {}", error_msg, status, body_text);
    }

    let data: IdcRefreshResponse = response.json().await?;
    let mut new_credentials = credentials.clone();
    apply_idc_refresh_response(&mut new_credentials, data);

    Ok(new_credentials)
}

/// 将 IdC token 刷新响应写入凭据（纯逻辑，便于单测，不涉及网络）。
///
/// - `access_token`（此结构体字段用于承载最终 Bearer token）保存 idToken：
///   Amazon Q 数据面接口（`generateAssistantResponse` 等）只接受 idToken。
/// - `sso_access_token` 保存 AWS SSO OIDC 原始返回的 accessToken（SSO portal
///   session token）：`getUsageLimits` 等 Q 控制面接口需要它，见 issue #31。
/// - 若响应未返回 `id_token`（部分环境/旧行为），回退用 accessToken 顶替，
///   保持与历史行为一致（commit 4f39dd7 之前的 fallback 语义）。
pub(crate) fn apply_idc_refresh_response(
    credentials: &mut KiroCredentials,
    data: IdcRefreshResponse,
) {
    credentials.sso_access_token = Some(data.access_token.clone());
    credentials.access_token = Some(data.id_token.unwrap_or(data.access_token));

    if let Some(new_refresh_token) = data.refresh_token {
        credentials.refresh_token = Some(new_refresh_token);
    }

    if let Some(expires_in) = data.expires_in {
        let expires_at = Utc::now() + Duration::seconds(expires_in);
        credentials.expires_at = Some(expires_at.to_rfc3339());
    }
}

/// 刷新 external_idp Token（Microsoft Entra ID / Azure AD 等 OIDC IdP）
///
/// 使用公共客户端 refresh_token grant（无 client_secret），
/// 向凭据中指定的 token_endpoint 发送 application/x-www-form-urlencoded 请求。
async fn refresh_external_idp_token(
    credentials: &KiroCredentials,
    config: &Config,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<KiroCredentials> {
    tracing::info!("正在刷新 external_idp Token...");

    let refresh_token = credentials.refresh_token.as_ref().unwrap();
    let client_id = credentials
        .client_id
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("external_idp 刷新需要 clientId"))?;
    let token_endpoint = credentials
        .token_endpoint
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("external_idp 刷新需要 tokenEndpoint"))?;

    let mut params = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token.as_str()),
        ("client_id", client_id.as_str()),
    ];
    let scopes_owned;
    if let Some(ref s) = credentials.scopes {
        if !s.is_empty() {
            scopes_owned = s.clone();
            params.push(("scope", scopes_owned.as_str()));
        }
    }

    let client = build_client(proxy, 60, config.tls_backend)?;
    let response = client
        .post(token_endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .form(&params)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        if is_invalid_grant_response(status.as_u16(), &body_text) {
            return Err(RefreshTokenInvalidError.into());
        }
        let error_msg = match status.as_u16() {
            400 => "external_idp token 请求参数错误（400）",
            401 => "external_idp 凭证已过期或无效，需要重新认证（401）",
            403 => "权限不足，无法刷新 Token（403）",
            429 => "请求过于频繁，已被限流（429）",
            500..=599 => "IdP 服务器错误，暂时不可用",
            _ => "external_idp Token 刷新失败",
        };
        bail!("{}: {} {}", error_msg, status, body_text);
    }

    let data: serde_json::Value = response.json().await?;

    let access_token = data["access_token"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("external_idp 响应缺少 access_token"))?
        .to_string();

    let mut new_credentials = credentials.clone();
    new_credentials.access_token = Some(access_token);

    if let Some(new_rt) = data["refresh_token"].as_str() {
        new_credentials.refresh_token = Some(new_rt.to_string());
    }

    let expires_in = data["expires_in"].as_i64().unwrap_or(3600);
    let expires_at = Utc::now() + Duration::seconds(expires_in);
    new_credentials.expires_at = Some(expires_at.to_rfc3339());

    Ok(new_credentials)
}

const USAGE_LIMITS_AMZ_USER_AGENT_PREFIX: &str = "aws-sdk-js/1.0.0";

/// 为 `getUsageLimits`（Q 控制面接口）选择合适的 Bearer token。
///
/// IdC 账号的 `credentials.access_token` 存放的是 idToken（供数据面接口使用，见
/// `refresh_idc_token`），`getUsageLimits` 需要的是 SSO portal 的原始 accessToken
/// （`credentials.sso_access_token`）。若该字段缺失（如旧版 credentials.json 尚未
/// 刷新过、或反序列化自旧格式文件），回退到传入的 `token` 参数以保持向后兼容。
/// 非 IdC 账号不受影响，始终使用传入的 `token`。
pub(crate) fn select_usage_limits_token<'a>(
    credentials: &'a KiroCredentials,
    token: &'a str,
) -> &'a str {
    if credentials
        .auth_method
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case("idc"))
    {
        credentials.sso_access_token.as_deref().unwrap_or(token)
    } else {
        token
    }
}

/// 获取使用额度信息
///
/// host/resourceType 分流说明：实测（curl 验证，BuilderId Student 账号）表明
/// q 端点 + resourceType=AGENTIC_REQUEST + BuilderId 占位 ARN 组合返回 200，
/// 并非旧注释断言的"BuilderId 携带 resourceType 必 400"。分流判据因此保留
/// profile_arn 存在性：补全后 idc/social 账号恒有占位 ARN，统一走
/// q + resourceType 路径（实测正确）；codewhisperer 分支仅对极少数
/// 无 ARN 的 external_idp/未知类型账号可达。
pub(crate) async fn get_usage_limits(
    credentials: &KiroCredentials,
    config: &Config,
    token: &str,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<UsageLimitsResponse> {
    tracing::debug!("正在获取使用额度信息...");

    // 优先级：账号.api_region > config.api_region > config.region
    let region = credentials.effective_api_region(config);
    let region_lower = region.to_lowercase();
    // 企业 IdC 账号（有 profileArn）用 q.{region}.amazonaws.com；
    // BuilderId 个人账号用 codewhisperer 端点（us-east-1 专用主机，其他区域回退到 q.*）
    let host = if credentials.profile_arn.is_some() {
        format!("q.{}.amazonaws.com", region_lower)
    } else if region_lower == "us-east-1" {
        format!("codewhisperer.{}.amazonaws.com", region_lower)
    } else {
        format!("q.{}.amazonaws.com", region_lower)
    };
    let machine_id = machine_id::generate_from_credentials(credentials, config);
    let kiro_version = &config.kiro_version;

    // 构建 URL
    // resourceType=AGENTIC_REQUEST 对企业 IdC 账号（有 profileArn）有效；实测
    // BuilderId 占位 ARN + resourceType 组合同样返回 200（见函数级 doc 注释），
    // 补全后的 idc/social 账号统一走此路径
    let mut url = if credentials.profile_arn.is_some() {
        format!(
            "https://{}/getUsageLimits?origin=AI_EDITOR&resourceType=AGENTIC_REQUEST",
            host
        )
    } else {
        format!("https://{}/getUsageLimits?origin=AI_EDITOR", host)
    };

    // profileArn 是可选的
    if let Some(profile_arn) = &credentials.profile_arn {
        url.push_str(&format!("&profileArn={}", urlencoding::encode(profile_arn)));
    }

    // 构建 User-Agent headers
    let user_agent = format!(
        "aws-sdk-js/1.0.0 ua/2.1 os/darwin#24.6.0 lang/js md/nodejs#22.21.1 \
         api/codewhispererruntime#1.0.0 m/N,E KiroIDE-{}-{}",
        kiro_version, machine_id
    );
    let amz_user_agent = format!(
        "{} KiroIDE-{}-{}",
        USAGE_LIMITS_AMZ_USER_AGENT_PREFIX, kiro_version, machine_id
    );

    let client = build_client(proxy, 60, config.tls_backend)?;

    // getUsageLimits 是 Q 控制面接口，鉴权需要 SSO portal 的 accessToken；
    // 而传入的 `token` 参数在 IdC 场景下是 credentials.access_token（存放的是 idToken，
    // 用于数据面接口）。IdC 账号若已保存 sso_access_token，则优先使用它，
    // 避免复用 idToken 导致 403 Invalid token（见 issue #31）。非 IdC 账号保持原逻辑不变。
    let effective_token = select_usage_limits_token(credentials, token);

    let response = client
        .get(&url)
        .header("x-amz-user-agent", &amz_user_agent)
        .header("User-Agent", &user_agent)
        .header("host", &host)
        .header("amz-sdk-invocation-id", uuid::Uuid::new_v4().to_string())
        .header("amz-sdk-request", "attempt=1; max=1")
        .header("Authorization", format!("Bearer {}", effective_token))
        .header("Connection", "close")
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        // BuilderId 个人账号不支持 getUsageLimits，AWS 返回 400 "Invalid profileArn"
        if status.as_u16() == 400 && body_text.contains("Invalid profileArn") {
            return Err(UsageLimitsUnsupportedError.into());
        }
        let error_msg = match status.as_u16() {
            401 => "认证失败，Token 无效或已过期",
            403 => "权限不足，无法获取使用额度",
            429 => "请求过于频繁，已被限流",
            500..=599 => "服务器错误，AWS 服务暂时不可用",
            _ => "获取使用额度失败",
        };
        bail!("{}: {} {}", error_msg, status, body_text);
    }

    let data: UsageLimitsResponse = response.json().await?;
    Ok(data)
}

/// 获取当前支持的模型列表（含官方费率倍率）
///
/// 与 getUsageLimits 不同，这是 AWS JSON RPC 协议（POST + x-amz-target），
/// 而非 REST 查询，两者协议格式互不通用。
pub(crate) async fn list_available_models(
    credentials: &KiroCredentials,
    config: &Config,
    token: &str,
    proxy: Option<&ProxyConfig>,
) -> anyhow::Result<AvailableModelsResponse> {
    tracing::debug!("正在获取支持模型列表...");

    let region = credentials.effective_api_region(config);
    let host = format!("management.{}.kiro.dev", region);
    let url = format!("https://{}/?origin=KIRO_CLI", host);
    tracing::debug!("ListAvailableModels 请求 host: {}", host);

    let mut body = serde_json::json!({ "origin": "KIRO_CLI" });
    if let Some(profile_arn) = &credentials.profile_arn {
        body["profileArn"] = serde_json::Value::String(profile_arn.clone());
    }

    let client = build_client(proxy, 15, config.tls_backend)?;

    let response = client
        .post(&url)
        .header("content-type", "application/x-amz-json-1.0")
        .header(
            "x-amz-target",
            "AmazonCodeWhispererService.ListAvailableModels",
        )
        .header("Authorization", format!("Bearer {}", token))
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        bail!("获取支持模型列表失败: {} {}", status, body_text);
    }

    let data: AvailableModelsResponse = response.json().await?;
    Ok(data)
}
