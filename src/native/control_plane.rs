// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Native 控制面（control plane）端点实现。
//!
//! Kiro 客户端启动/交互时会调用 AWS JSON 1.0 控制面 RPC（`x-amz-target`），
//! 不属于推理/MCP。本模块把这些 RPC 映射到 kiro2cc 已经具备的能力：
//!
//! | 原生 op | 数据来源 |
//! | --- | --- |
//! | `GetProfile` | 选中账号的凭据（profileArn/email/nickname/authMethod） |
//! | `ListAvailableModels` | `MultiTokenManager::list_available_models()`（真实上游调用） |
//! | `GetUsageLimits` | `MultiTokenManager::get_usage_limits_for(id)`（真实上游调用） |
//! | `ListAvailableProfiles` | 选中账号 profileArn；缺失时 `discover_profile_arn()` 真实发现 |
//! | `SendTelemetryEvent` | 吞掉（`{}`） |
//! | `GetEffectiveEnterpriseManagementResourceWithToken` / `GetFeatureConfiguration` | 无对应数据源，返回 `{}`（POC 限制，见 docs/native-inbound.md） |
//!
//! 账号选择统一走 `acquire_context(None)`：与推理共用同一账号池与负载均衡策略，
//! 但不写入 sticky 绑定，因此不会影响推理请求的会话粘性。

use axum::http::StatusCode;
use axum::response::Response;
use serde_json::{Value, json};

use crate::kiro::model::available_models::AvailableModelsResponse;
use crate::kiro::model::credentials::{KiroCredentials, fallback_profile_arn_value};
use crate::kiro::model::usage_limits::UsageLimitsResponse;
use crate::kiro::provider::KiroProvider;
use crate::kiro::token_manager::CallContext;

use super::aws_json::{self, json_response, ok_empty};

/// 支持映射的原生控制面操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlOp {
    GetProfile,
    ListAvailableModels,
    GetUsageLimits,
    ListAvailableProfiles,
    GetEffectiveEnterpriseManagementResourceWithToken,
    GetFeatureConfiguration,
    SendTelemetryEvent,
}

impl ControlOp {
    /// 从完整的 `x-amz-target` 解析操作名。
    ///
    /// 服务前缀可能有多种（`AmazonCodeWhispererService` /
    /// `KiroControlPlaneBearerService` / `CodeWhispererRuntime`），
    /// 因此只匹配最后一段操作名；完整 target 由调用方记录日志。
    pub(crate) fn from_target(target: &str) -> Option<Self> {
        match target.rsplit('.').next().unwrap_or_default() {
            "GetProfile" => Some(Self::GetProfile),
            "ListAvailableModels" => Some(Self::ListAvailableModels),
            "GetUsageLimits" => Some(Self::GetUsageLimits),
            "ListAvailableProfiles" => Some(Self::ListAvailableProfiles),
            "GetEffectiveEnterpriseManagementResourceWithToken" => {
                Some(Self::GetEffectiveEnterpriseManagementResourceWithToken)
            }
            "GetFeatureConfiguration" => Some(Self::GetFeatureConfiguration),
            "SendTelemetryEvent" => Some(Self::SendTelemetryEvent),
            _ => None,
        }
    }
}

/// 分发控制面操作。
pub(crate) async fn handle(provider: &KiroProvider, op: ControlOp) -> Response {
    match op {
        ControlOp::GetProfile => get_profile(provider).await,
        ControlOp::ListAvailableModels => list_available_models(provider).await,
        ControlOp::GetUsageLimits => get_usage_limits(provider).await,
        ControlOp::ListAvailableProfiles => list_available_profiles(provider).await,
        ControlOp::SendTelemetryEvent => ok_empty(),
        ControlOp::GetEffectiveEnterpriseManagementResourceWithToken => {
            // kiro2cc 没有企业管控资源（企业策略）数据源；POC 返回空对象。
            // 现有客户端把空对象当作"无企业覆盖"，可继续使用默认配置。
            tracing::debug!(
                "native control-plane: GetEffectiveEnterpriseManagementResourceWithToken 返回空对象（POC 限制）"
            );
            ok_empty()
        }
        ControlOp::GetFeatureConfiguration => {
            // 功能开关仅影响客户端 UI 特性（埋点/预览功能），不影响推理；
            // kiro2cc 无对应数据源，返回空对象（POC 限制）。
            tracing::debug!("native control-plane: GetFeatureConfiguration 返回空对象（POC 限制）");
            ok_empty()
        }
    }
}

/// 取得一个可用账号上下文（与推理共用账号池，不写 sticky 绑定）。
async fn acquire_context(provider: &KiroProvider) -> Result<CallContext, Response> {
    provider
        .token_manager()
        .acquire_context(None)
        .await
        .map_err(|err| {
            tracing::warn!("native control-plane 无法获取可用账号: {err:#}");
            aws_json::provider_error(&err)
        })
}

/// `GetProfile`：从选中账号的凭据合成 `ProfileInfo`。
async fn get_profile(provider: &KiroProvider) -> Response {
    let ctx = match acquire_context(provider).await {
        Ok(ctx) => ctx,
        Err(response) => return response,
    };
    let mut credentials = ctx.credentials.clone();
    let arn = resolve_profile_arn(provider, &mut credentials, &ctx.token).await;
    tracing::debug!(
        credential_id = ctx.id,
        profile_arn = arn.as_deref().unwrap_or("<none>"),
        "native control-plane: GetProfile"
    );
    let body = aws_json::prune_nulls(profile_response(&credentials, arn));
    json_response(StatusCode::OK, &body)
}

/// 返回账号的 profileArn：优先真实 ARN，其次类型占位 ARN，最后尝试真实发现。
async fn resolve_profile_arn(
    provider: &KiroProvider,
    credentials: &mut KiroCredentials,
    token: &str,
) -> Option<String> {
    if let Some(arn) = credentials
        .profile_arn
        .as_deref()
        .map(str::trim)
        .filter(|arn| !arn.is_empty())
    {
        return Some(arn.to_string());
    }
    // social / idc(IdC/BuilderId) 账号使用 Kiro IDE 同款固定/占位 ARN；
    // API Key 账号没有 profile 语义（fallback 返回 None），保持缺失。
    if let Some(arn) = fallback_profile_arn_value(credentials) {
        return Some(arn.to_string());
    }
    // 企业版 IdC 等真实 ARN 因租户而异：尽力向真实控制面发现一次。
    let discovered = provider
        .token_manager()
        .discover_profile_arn(credentials, token)
        .await;
    if let Some(arn) = &discovered {
        credentials.profile_arn = Some(arn.clone());
    }
    discovered
}

/// 合成 `GetProfile` 响应体。
///
/// 新/旧两代客户端都按 `{"profile": {...}}` 解析（KAS 的 `ProfileInfo`），
/// 同时保留 `profileArn` 顶层字段兼容旧实现。
fn profile_response(credentials: &KiroCredentials, arn: Option<String>) -> Value {
    let profile = json!({
        "arn": arn,
        "profileName": credentials
            .nickname
            .clone()
            .or_else(|| credentials.email.clone())
            .unwrap_or_else(|| "kiro-account".to_string()),
        "profileType": profile_type(credentials),
        "status": "ACTIVE",
        "identityDetails": {
            "email": credentials.email,
            "provider": credentials.provider,
        },
    });
    json!({
        "profile": profile,
        "profileArn": arn,
        "downloadEnabled": true,
    })
}

/// 账号 profile 类型（尽力映射；无真实上游字段可用）。
fn profile_type(credentials: &KiroCredentials) -> &'static str {
    if credentials
        .provider
        .as_deref()
        .is_some_and(|provider| provider.trim().eq_ignore_ascii_case("enterprise"))
    {
        "ENTERPRISE"
    } else if credentials.is_api_key_credential() {
        // KAS 的 API Key 模式：Kiro CLI 语义下的个人 profile。
        "KIRO"
    } else if credentials.is_idc() {
        "BUILDER_ID"
    } else {
        "SOCIAL"
    }
}

/// `ListAvailableModels`：真实上游模型目录 → 原生响应。
async fn list_available_models(provider: &KiroProvider) -> Response {
    match provider.token_manager().list_available_models().await {
        Ok(models) => {
            tracing::debug!(
                model_count = models.models.len(),
                "native control-plane: ListAvailableModels"
            );
            json_response(
                StatusCode::OK,
                &aws_json::prune_nulls(models_to_native(&models)),
            )
        }
        Err(err) => {
            tracing::warn!("native control-plane ListAvailableModels 失败: {err:#}");
            aws_json::provider_error(&err)
        }
    }
}

/// `GetUsageLimits`：真实上游额度 → 原生响应。
async fn get_usage_limits(provider: &KiroProvider) -> Response {
    let ctx = match acquire_context(provider).await {
        Ok(ctx) => ctx,
        Err(response) => return response,
    };
    match provider.token_manager().get_usage_limits_for(ctx.id).await {
        Ok(usage) => {
            tracing::debug!(
                credential_id = ctx.id,
                "native control-plane: GetUsageLimits"
            );
            json_response(
                StatusCode::OK,
                &aws_json::prune_nulls(usage_to_native(
                    &usage,
                    chrono::Utc::now().timestamp() as f64,
                )),
            )
        }
        Err(err) => {
            tracing::warn!(
                credential_id = ctx.id,
                "native control-plane GetUsageLimits 失败: {err:#}"
            );
            aws_json::provider_error(&err)
        }
    }
}

/// `ListAvailableProfiles`：选中账号的 profile 列表（缺失时真实发现）。
async fn list_available_profiles(provider: &KiroProvider) -> Response {
    let ctx = match acquire_context(provider).await {
        Ok(ctx) => ctx,
        Err(response) => return response,
    };
    let mut credentials = ctx.credentials.clone();
    let arn = resolve_profile_arn(provider, &mut credentials, &ctx.token).await;
    let profiles: Vec<Value> = arn
        .iter()
        .map(|arn| {
            json!({
                "arn": arn,
                "profileName": credentials
                    .nickname
                    .clone()
                    .or_else(|| credentials.email.clone())
                    .unwrap_or_else(|| "kiro-account".to_string()),
                "profileType": profile_type(&credentials),
            })
        })
        .collect();
    let body = aws_json::prune_nulls(json!({
        "profiles": profiles,
        "nextToken": serde_json::Value::Null,
    }));
    let profile_count = body
        .get("profiles")
        .and_then(|profiles| profiles.as_array())
        .map(Vec::len)
        .unwrap_or(0);
    tracing::debug!(
        credential_id = ctx.id,
        profile_count,
        "native control-plane: ListAvailableProfiles"
    );
    json_response(StatusCode::OK, &body)
}

/// 把 kiro2cc 的模型列表转换为原生 `ListAvailableModels` 响应。
///
/// 新客户端（KAS）把 `defaultModel` 解析为 `Model` 对象、`models` 为 `Model[]`；
/// 旧客户端只读取 `models`，额外的 `defaultModel`/`nextToken` 字段被忽略。
pub(crate) fn models_to_native(models: &AvailableModelsResponse) -> Value {
    let models_json: Vec<Value> = models
        .models
        .iter()
        .map(|model| {
            json!({
                "modelId": model.model_id,
                "modelName": model.model_name,
                "description": model.description,
                "rateMultiplier": model.rate_multiplier,
                "tokenLimits": {
                    "maxInputTokens": model.token_limits.max_input_tokens,
                    "maxOutputTokens": model.token_limits.max_output_tokens,
                },
                "additionalModelRequestFieldsSchema": model.additional_model_request_fields_schema,
            })
        })
        .collect();
    let default_model = models_json.first().cloned().unwrap_or(Value::Null);
    json!({
        "models": models_json,
        "defaultModel": default_model,
        "nextToken": serde_json::Value::Null,
    })
}

/// 把 kiro2cc 的额度响应转换为原生 `GetUsageLimits` 响应。
///
/// 上游（KAM/REST 形态）与原生（Q 控制面形态）字段高度重合；这里补齐
/// 原生客户端额外读取的 `limits`（空数组，额度主要看 usageBreakdownList）
/// 与 `daysUntilReset`（由 nextDateReset 计算）。
pub(crate) fn usage_to_native(usage: &UsageLimitsResponse, now_epoch_secs: f64) -> Value {
    let breakdowns: Vec<Value> = usage
        .usage_breakdown_list
        .iter()
        .map(|breakdown| {
            json!({
                "resourceType": breakdown.resource_type,
                "displayName": breakdown.display_name,
                "displayNamePlural": breakdown.display_name_plural,
                "currency": breakdown.currency,
                "unit": breakdown.unit,
                "overageRate": breakdown.overage_rate,
                "overageCap": breakdown.overage_cap,
                "currentUsage": breakdown.current_usage,
                "currentUsageWithPrecision": breakdown.current_usage_with_precision,
                "usageLimit": breakdown.usage_limit,
                "usageLimitWithPrecision": breakdown.usage_limit_with_precision,
                "nextDateReset": breakdown.next_date_reset,
                "freeTrialInfo": breakdown.free_trial_info.as_ref().map(|trial| json!({
                    "currentUsage": trial.current_usage,
                    "currentUsageWithPrecision": trial.current_usage_with_precision,
                    "usageLimit": trial.usage_limit,
                    "usageLimitWithPrecision": trial.usage_limit_with_precision,
                    "freeTrialStatus": trial.free_trial_status,
                    "freeTrialExpiry": trial.free_trial_expiry,
                })),
                "bonuses": breakdown.bonuses.iter().map(|bonus| json!({
                    "bonusCode": bonus.bonus_code,
                    "displayName": bonus.display_name,
                    "currentUsage": bonus.current_usage,
                    "usageLimit": bonus.usage_limit,
                    "expiresAt": bonus.expires_at,
                    "status": bonus.status,
                })).collect::<Vec<Value>>(),
            })
        })
        .collect();

    json!({
        // 原生客户端把 limits 当作"按维度限流列表"；kiro2cc 没有该维度数据，
        // 返回空数组，客户端回退到 usageBreakdownList 展示额度。
        "limits": Vec::<Value>::new(),
        "nextDateReset": usage.next_date_reset,
        "daysUntilReset": days_until_reset(usage.next_date_reset, now_epoch_secs),
        "usageBreakdownList": breakdowns,
        "subscriptionInfo": usage.subscription_info.as_ref().map(|info| json!({
            "type": info.subscription_type,
            "subscriptionTitle": info.subscription_title,
            "upgradeCapability": info.upgrade_capability,
            "overageCapability": info.overage_capability,
        })),
        "overageConfiguration": usage.overage_configuration.as_ref().map(|overage| json!({
            "overageEnabled": overage.overage_enabled,
            "overageStatus": overage.overage_status,
        })),
        "userInfo": usage.user_info.as_ref().map(|user| json!({
            "userId": user.user_id,
            "email": user.email,
        })),
    })
}

/// 距离额度重置还有多少天（向上取整，最小 0）。
pub(crate) fn days_until_reset(next_date_reset: Option<f64>, now_epoch_secs: f64) -> Option<i64> {
    next_date_reset.map(|reset| {
        let days = ((reset - now_epoch_secs) / 86_400.0).ceil() as i64;
        days.max(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_ops_parse_from_all_known_service_prefixes() {
        assert_eq!(
            ControlOp::from_target("AmazonCodeWhispererService.GetProfile"),
            Some(ControlOp::GetProfile)
        );
        assert_eq!(
            ControlOp::from_target("KiroControlPlaneBearerService.GetProfile"),
            Some(ControlOp::GetProfile)
        );
        assert_eq!(
            ControlOp::from_target("KiroControlPlaneBearerService.ListAvailableModels"),
            Some(ControlOp::ListAvailableModels)
        );
        assert_eq!(
            ControlOp::from_target("AmazonCodeWhispererService.GetUsageLimits"),
            Some(ControlOp::GetUsageLimits)
        );
        assert_eq!(
            ControlOp::from_target("KiroRuntimeService.GetFeatureConfiguration"),
            Some(ControlOp::GetFeatureConfiguration)
        );
        assert_eq!(
            ControlOp::from_target(
                "KiroControlPlaneBearerService.GetEffectiveEnterpriseManagementResourceWithToken"
            ),
            Some(ControlOp::GetEffectiveEnterpriseManagementResourceWithToken)
        );
        assert_eq!(ControlOp::from_target("UnknownService.UnknownOp"), None);
        assert_eq!(ControlOp::from_target(""), None);
    }

    #[test]
    fn models_to_native_maps_real_model_catalog() {
        let upstream: AvailableModelsResponse = serde_json::from_str(
            r#"{
                "models": [
                    {
                        "modelId": "claude-sonnet-4.6",
                        "modelName": "Claude Sonnet 4.6",
                        "rateMultiplier": 1.3,
                        "tokenLimits": {"maxInputTokens": 1000000, "maxOutputTokens": 64000}
                    },
                    {"modelId": "auto", "modelName": "Auto"}
                ]
            }"#,
        )
        .unwrap();
        let native = aws_json::prune_nulls(models_to_native(&upstream));
        let models = native["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["modelId"], "claude-sonnet-4.6");
        assert_eq!(models[0]["tokenLimits"]["maxInputTokens"], 1_000_000);
        assert_eq!(models[0]["rateMultiplier"], 1.3);
        // defaultModel 取目录第一个模型（对象形态，KAS 期望 Model 而不是字符串）
        assert_eq!(native["defaultModel"]["modelId"], "claude-sonnet-4.6");
        // 缺失的可选字段被裁剪为不存在，而不是 null
        assert!(models[1].get("rateMultiplier").is_none());
    }

    #[test]
    fn usage_to_native_carries_breakdown_and_computed_days() {
        let upstream: UsageLimitsResponse = serde_json::from_str(
            r#"{
                "nextDateReset": 1799136000,
                "subscriptionInfo": {"subscriptionTitle": "KIRO PRO", "type": "Q_DEVELOPER_STANDALONE_PRO"},
                "userInfo": {"email": "u***@example.com", "userId": "d-123"},
                "overageConfiguration": {"overageEnabled": true, "overageStatus": "ENABLED"},
                "usageBreakdownList": [{
                    "resourceType": "CREDIT",
                    "displayName": "Credit",
                    "currency": "USD",
                    "unit": "INVOCATIONS",
                    "currentUsage": 38,
                    "currentUsageWithPrecision": 37.6,
                    "usageLimit": 1000,
                    "usageLimitWithPrecision": 1000,
                    "freeTrialInfo": {"freeTrialStatus": "EXPIRED", "freeTrialExpiry": 1735689600},
                    "bonuses": [{"bonusCode": "trial", "status": "ACTIVE", "currentUsage": 0, "usageLimit": 500}]
                }]
            }"#,
        )
        .unwrap();
        // 2026-11-04 附近作为 "now"，距 2027-01-05 还有约 61 天
        let native = aws_json::prune_nulls(usage_to_native(&upstream, 1_793_000_000.0));
        assert_eq!(native["nextDateReset"], 1_799_136_000.0);
        assert!(native["daysUntilReset"].as_i64().unwrap() > 0);
        assert_eq!(native["limits"].as_array().unwrap().len(), 0);
        assert_eq!(
            native["subscriptionInfo"]["type"],
            "Q_DEVELOPER_STANDALONE_PRO"
        );
        assert_eq!(native["overageConfiguration"]["overageStatus"], "ENABLED");
        let breakdown = &native["usageBreakdownList"][0];
        assert_eq!(breakdown["resourceType"], "CREDIT");
        assert_eq!(breakdown["currentUsage"], 38);
        assert_eq!(breakdown["freeTrialInfo"]["freeTrialStatus"], "EXPIRED");
        assert_eq!(breakdown["bonuses"][0]["bonusCode"], "trial");
        // 空数组仍保留（客户端按数组遍历）
        assert!(breakdown["bonuses"].is_array());
    }

    #[test]
    fn usage_to_native_tolerates_empty_upstream_response() {
        let upstream: UsageLimitsResponse = serde_json::from_str("{}").unwrap();
        let native = aws_json::prune_nulls(usage_to_native(&upstream, 1_000.0));
        assert!(native["nextDateReset"].is_null());
        assert!(native["subscriptionInfo"].is_null());
        assert_eq!(native["limits"].as_array().unwrap().len(), 0);
        assert_eq!(native["usageBreakdownList"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn days_until_reset_never_negative_and_rounds_up() {
        assert_eq!(days_until_reset(None, 0.0), None);
        assert_eq!(days_until_reset(Some(86_400.0), 0.0), Some(1));
        assert_eq!(days_until_reset(Some(86_401.0), 0.0), Some(2));
        // 已过期 → 0，而不是负数
        assert_eq!(days_until_reset(Some(1_000.0), 2_000.0), Some(0));
    }

    #[test]
    fn profile_response_uses_real_arn_and_provider_type() {
        let credentials: KiroCredentials = serde_json::from_str(
            r#"{
                "authMethod": "idc",
                "profileArn": "arn:aws:codewhisperer:us-east-1:1:profile/REAL",
                "email": "user@example.com",
                "nickname": "primary"
            }"#,
        )
        .unwrap();
        let body = aws_json::prune_nulls(profile_response(
            &credentials,
            Some("arn:aws:codewhisperer:us-east-1:1:profile/REAL".into()),
        ));
        assert_eq!(
            body["profile"]["arn"],
            "arn:aws:codewhisperer:us-east-1:1:profile/REAL"
        );
        assert_eq!(body["profile"]["profileType"], "BUILDER_ID");
        assert_eq!(
            body["profile"]["identityDetails"]["email"],
            "user@example.com"
        );
        assert_eq!(
            body["profileArn"],
            "arn:aws:codewhisperer:us-east-1:1:profile/REAL"
        );
    }

    #[test]
    fn profile_type_prefers_enterprise_then_api_key() {
        let enterprise: KiroCredentials = serde_json::from_str(
            r#"{"provider": "Enterprise", "authMethod": "idc", "clientId": "a", "clientSecret": "b"}"#,
        )
        .unwrap();
        assert_eq!(profile_type(&enterprise), "ENTERPRISE");

        let api_key: KiroCredentials =
            serde_json::from_str(r#"{"authMethod": "api_key", "kiroApiKey": "ksk_test"}"#).unwrap();
        assert_eq!(profile_type(&api_key), "KIRO");
        assert_eq!(fallback_profile_arn_value(&api_key), None);

        let social: KiroCredentials = serde_json::from_str(r#"{"authMethod": "social"}"#).unwrap();
        assert_eq!(profile_type(&social), "SOCIAL");
        assert_eq!(
            fallback_profile_arn_value(&social),
            Some(crate::kiro::model::credentials::SOCIAL_PROFILE_ARN)
        );
    }
}
