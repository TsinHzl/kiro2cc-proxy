// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Kiro OAuth 凭证数据模型
//!
//! 支持从 Kiro IDE 的凭证文件加载，使用 Social 认证方式
//! 支持单账号和多账号配置格式

use serde::{Deserialize, Serialize};

/// Kiro OAuth 凭证
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KiroCredentials {
    /// 账号唯一标识符（自增 ID）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,

    /// 访问令牌
    ///
    /// - Social / external_idp 账号：即为上游返回的 accessToken
    /// - IdC 账号：存放的是 AWS SSO OIDC `/token` 响应中的 `idToken`（JWT），
    ///   因为 Q 数据面接口（`generateAssistantResponse` 等）只接受 idToken。
    ///   控制面接口（如 `getUsageLimits`）需要的是 `sso_access_token` 字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,

    /// IdC 账号专用：AWS SSO OIDC `/token` 响应中的 `accessToken`（SSO portal session token）
    ///
    /// 仅 IdC 认证方式会填充此字段。用于 `getUsageLimits` 等 Q 控制面接口的鉴权，
    /// 与 `access_token`（此场景下存放的是 idToken，用于数据面接口）区分。
    /// 旧版本 credentials.json 不含此字段，反序列化时为 `None`，向后兼容。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sso_access_token: Option<String>,

    /// 刷新令牌
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// Profile ARN
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_arn: Option<String>,

    /// 过期时间 (RFC3339 格式)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,

    /// 认证方式 (social / idc)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,

    /// OIDC Client ID (IdC / external_idp 认证需要)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// OIDC Client Secret (IdC 认证需要；external_idp 公共客户端不填)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,

    /// IdP 标识（如 "AzureAD"），仅 external_idp 使用
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// IdP token 刷新端点（external_idp 必填）
    /// 示例：https://login.microsoftonline.com/<tenant>/oauth2/v2.0/token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<String>,

    /// OAuth2 scope（external_idp 使用；需含 offline_access）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<String>,

    /// 账号优先级（数字越小优先级越高，默认为 0）
    #[serde(default)]
    #[serde(skip_serializing_if = "is_zero")]
    pub priority: u32,

    /// 账号级 Region 配置（用于 OIDC token 刷新）
    /// 未配置时回退到 config.json 的全局 region
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    /// 账号级 Auth Region（用于 Token 刷新）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_region: Option<String>,

    /// 账号级 API Region（用于 API 请求）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_region: Option<String>,

    /// 账号级 Machine ID 配置（可选）
    /// 未配置时回退到 config.json 的 machineId；都未配置时由 refreshToken 派生
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,

    /// 用户邮箱（从 Anthropic API 获取）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// 用户昵称/备注名（用于前端显示）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,

    /// 订阅等级（KIRO PRO+ / KIRO FREE 等）
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub subscription_title: Option<String>,

    /// 账号级代理 URL（可选）
    /// 支持 http/https/socks5 协议
    /// 特殊值 "direct" 表示显式不使用代理（即使全局配置了代理）
    /// 未配置时回退到全局代理配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,

    /// 账号级代理认证用户名（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_username: Option<String>,

    /// 账号级代理认证密码（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_password: Option<String>,

    /// 账号是否被禁用（默认为 false）
    #[serde(default)]
    pub disabled: bool,

    /// 账号级端点首选项（多端点 LB 使用）
    ///
    /// - 未配置 / 空 → 使用全部 4 个端点（按 `Endpoint::default_order`）
    /// - 仅声明首选端点 → 首选在首 + 剩余端点按默认顺序去重追加
    /// - 非法值（如 `"invalid_endpoint"`）→ serde 反序列化时忽略该项，等价于未配置
    ///
    /// 示例：`["runtime", "codewhisperer"]` → `[Runtime, Codewhisperer, Ide, Amazonq]`
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub endpoint: Option<Vec<crate::kiro::endpoint::EndpointName>>,

    /// 账号级 thinking adaptive 注入开关（默认为 false）
    ///
    /// 开启后，当客户端请求携带 `thinking: {"type": "adaptive"}` 且路由到该账号时，
    /// provider 会向 Kiro 上游的 `additionalModelRequestFields` 注入
    /// `thinking: {"type": "adaptive"}`（恢复该账号的 thinking 调度，响应变慢）。
    /// 关闭时与 v3.3.0 以来"不发 thinking 字段"的行为完全一致。
    #[serde(default)]
    pub thinking_adaptive: bool,
}

/// 对邮箱做部分掩码（保留首字符与域名，如 u***@example.com）
fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) => match local.chars().next() {
            Some(first) => format!("{first}***@{domain}"),
            None => format!("***@{domain}"),
        },
        None => "[REDACTED]".to_string(),
    }
}

impl std::fmt::Debug for KiroCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redact = |v: &Option<String>| v.as_ref().map(|_| "[REDACTED]");
        let masked_email = self.email.as_deref().map(mask_email);
        f.debug_struct("KiroCredentials")
            .field("id", &self.id)
            .field("access_token", &redact(&self.access_token))
            .field("sso_access_token", &redact(&self.sso_access_token))
            .field("refresh_token", &redact(&self.refresh_token))
            .field("profile_arn", &self.profile_arn)
            .field("expires_at", &self.expires_at)
            .field("auth_method", &self.auth_method)
            .field("client_id", &self.client_id)
            .field("client_secret", &redact(&self.client_secret))
            .field("priority", &self.priority)
            .field("region", &self.region)
            .field("auth_region", &self.auth_region)
            .field("api_region", &self.api_region)
            .field("machine_id", &self.machine_id)
            .field("email", &masked_email)
            .field("nickname", &self.nickname)
            .field("subscription_title", &self.subscription_title)
            .field("proxy_url", &self.proxy_url)
            .field("proxy_username", &self.proxy_username)
            .field("proxy_password", &redact(&self.proxy_password))
            .field("disabled", &self.disabled)
            .field("thinking_adaptive", &self.thinking_adaptive)
            .finish()
    }
}

/// 判断是否为零（用于跳过序列化）
fn is_zero(value: &u32) -> bool {
    *value == 0
}

pub(crate) fn canonicalize_auth_method_value(value: &str) -> &str {
    if value.eq_ignore_ascii_case("idc")
        || value.eq_ignore_ascii_case("builder-id")
        || value.eq_ignore_ascii_case("iam")
    {
        "idc"
    } else if value.eq_ignore_ascii_case("azuread") || value.eq_ignore_ascii_case("entraid") {
        "external_idp"
    } else {
        value
    }
}

/// Kiro IDE 源码 FixedProfileArns 给 BuilderId 账号硬编码的占位 profileArn。
/// 上游对几乎所有请求都要求 profileArn 字段存在，BuilderId 用此固定值即可。
pub(crate) const BUILDER_ID_PLACEHOLDER_PROFILE_ARN: &str =
    "arn:aws:codewhisperer:us-east-1:638616132270:profile/AAAACCCCXXXX";

/// Social 登录（Github/Google）账号共用的固定 profileArn。
pub(crate) const SOCIAL_PROFILE_ARN: &str =
    "arn:aws:codewhisperer:us-east-1:699475941385:profile/EHGA3GRVQMUK";

/// 无 profile_arn 账号按账号类型推导的 fallback ARN（对齐 Kiro IDE FixedProfileArns 行为）。
///
/// - social → 固定 Social ARN
/// - idc → BuilderId 占位符（本仓库将 builder-id 归一化为 idc，且
///   BuilderId 账号同样以 OIDC clientId/clientSecret 刷新，无法进一步区分；
///   Kiro IDE 对该类账号即使用此占位符）
/// - external_idp（企业 IdC）及未知/未归一化值（含空字符串，视为未知类型）
///   → None：真实 ARN 因租户而异
pub(crate) fn fallback_profile_arn_value(credentials: &KiroCredentials) -> Option<&'static str> {
    let auth_method = credentials
        .auth_method
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if credentials.client_id.is_some() && credentials.client_secret.is_some() {
                "idc"
            } else {
                "social"
            }
        });
    match auth_method.trim().to_ascii_lowercase().as_str() {
        "social" => Some(SOCIAL_PROFILE_ARN),
        "idc" | "builder_id" => Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN),
        _ => None,
    }
}
