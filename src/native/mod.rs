// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Native Kiro inbound 直通（Option A）。
//!
//! # 背景
//!
//! kiro2cc 现有下游是 Anthropic / OpenAI 兼容协议；本模块在同一端口上额外暴露
//! 一个**原生 Kiro 协议**入口 `POST /native`，供 sub2api（或本地 Kiro CLI/IDE 的
//! 转发层）直接调用：
//!
//! ```text
//! Kiro CLI/IDE --native--> sub2api thin native route --> kiro2cc POST /native
//!                                                          |
//!                                                          v
//!                                              KiroProvider（现有账号池/故障转移）
//!                                                          |
//!                                                          v
//!                                                        Kiro/AWS
//! ```
//!
//! 与 Anthropic 路由不同，这里**不做协议转换**：
//!
//! - 推理请求体（`GenerateAssistantResponse` / `SendMessage`）本身就是 kiro2cc 的
//!   `KiroRequest` JSON，直接交给 `KiroProvider::call_api_stream`；
//! - 上游返回的 AWS EventStream 按原始字节流式转发（不解析、不缓冲、不重新编码），
//!   因而 tools / MCP / images / thinking / metering / contextUsage 等全部能力与
//!   直连 Kiro 完全一致；
//! - MCP（`InvokeMCP`）请求体是 JSON-RPC，直接交给 `KiroProvider::call_mcp`；
//! - 控制面（`GetProfile` / `ListAvailableModels` / `GetUsageLimits` / ...）
//!   映射到 kiro2cc 已有的账号/模型/额度能力，见 `control_plane` 模块。
//!
//! # 认证
//!
//! 两层认证：
//!
//! 1. 内部令牌头 `X-Kiro-Native-Token`，值等于环境变量 `KIRO_NATIVE_TOKEN`。
//!    未设置时（POC/本地模式）放行，生产必须设置，并保证 `/native` 仅由集群内
//!    sub2api 访问（ClusterIP + NetworkPolicy/ingress 限制）。
//! 2. kiro2cc API Key（`Authorization: Bearer <key>` / `x-api-key`），仅在
//!    ApiKeyManager 可用时启用：认证/懒激活/额度检查与 Anthropic 路由一致，
//!    并把 key 的 `bound_credential_ids` 传给 provider 限定账号池。
//!
//! # 用量记录
//!
//! 推理请求在流结束时按 API Key 记录一次 usage（token 估算 + 真实 metering
//! credits/cache），见 `billing::UsageRecordingConfig`；MCP/控制面不记录。
//!
//! # 请求格式
//!
//! `POST /native`（也接受 `/native/<任意子路径>`，便于 sub2api 原样转发客户端路径），
//! `content-type: application/x-amz-json-1.0`，`x-amz-target: <Service>.<Operation>`。
//! `x-amz-target` 缺失返回 400；令牌不匹配返回 403。
//!
//! # 限制
//!
//! POC 阶段控制面中 `GetEffectiveEnterpriseManagementResourceWithToken` 与
//! `GetFeatureConfiguration` 没有数据源，返回 `{}`；`GetProfile` 为凭据合成，
//! 不是真实上游 profile。详见 `docs/native-inbound.md`。

mod aws_json;
mod billing;
mod control_plane;

use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use serde_json::json;

use crate::common::auth;
use crate::kiro::provider::KiroProvider;
use crate::model::api_key::{ApiKeyAuthResult, ApiKeyManager};
use crate::model::usage::UsageTracker;

use control_plane::ControlOp;

/// 内部令牌头名称（与 sub2api 原生转发层约定）。
const NATIVE_TOKEN_HEADER: &str = "x-kiro-native-token";

/// 默认请求体上限：64 MiB。原生请求可能内联 base64 图片/文档，但不应无限大。
const DEFAULT_MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

/// `/native` 路由共享状态。
#[derive(Clone)]
struct NativeState {
    provider: Arc<KiroProvider>,
    /// `KIRO_NATIVE_TOKEN`；None = POC 模式不校验。
    token: Option<String>,
    /// kiro2cc API Key 管理器（Admin 启用时存在）。None = 仅内部令牌鉴权（POC/本地）。
    api_key_manager: Option<Arc<ApiKeyManager>>,
    /// per-API-key 用量追踪器（与 Anthropic 路由共享）。None = 不落库用量。
    usage_tracker: Option<Arc<UsageTracker>>,
}

/// 原生入口对 API Key 的认证结果。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NativeAuthOutcome {
    /// 未配置 ApiKeyManager：仅内部令牌鉴权（POC/本地模式）。
    Unmanaged,
    /// 认证通过。
    Authorized {
        api_key_id: u32,
        bound_credential_ids: Vec<u64>,
    },
    /// 认证失败，需返回给客户端的原生 JSON 错误。
    Rejected(NativeAuthError),
}

/// 原生错误响应描述（不依赖 axum Response，便于单元测试）。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NativeAuthError {
    pub status: StatusCode,
    pub error_type: &'static str,
    pub message: String,
}

impl NativeAuthError {
    fn into_response(self) -> Response {
        aws_json::error(self.status, self.error_type, &self.message)
    }
}

/// 认证 native 请求携带的 kiro2cc API Key。
///
/// 语义与 `anthropic::middleware::auth_middleware` 保持一致：
/// - 支持 `Authorization: Bearer <key>` 与 `x-api-key: <key>`；
/// - 校验启用/过期/额度（spending_limit，credits 或 USD）；
/// - 懒激活（activate_key）；
/// - 返回 key 绑定的账号列表（bound_credential_ids），供 provider 限定账号池。
///
/// `manager=None` 时返回 [`NativeAuthOutcome::Unmanaged`]：保持 POC/本地
/// （admin_psw 未配置、无 API Key 体系）行为不变，仅由 X-Kiro-Native-Token 保护。
pub(crate) fn authenticate_native_api_key(
    manager: Option<&ApiKeyManager>,
    usage_tracker: Option<&UsageTracker>,
    headers: &HeaderMap,
) -> NativeAuthOutcome {
    let Some(manager) = manager else {
        return NativeAuthOutcome::Unmanaged;
    };
    let Some(key) = auth::extract_api_key_from_headers(headers) else {
        return NativeAuthOutcome::Rejected(NativeAuthError {
            status: StatusCode::UNAUTHORIZED,
            error_type: "AuthenticationException",
            message: "Missing API key. Provide Authorization: Bearer <key> or x-api-key."
                .to_string(),
        });
    };
    match manager.authenticate(key) {
        ApiKeyAuthResult::Valid {
            id,
            name,
            spending_limit,
            limit_unit,
            bound_credential_ids,
        } => {
            if let Err(err) = manager.activate_key(id) {
                tracing::warn!(api_key_id = id, error = %err, "激活 API Key 失败");
            }
            if let (Some(limit), Some(tracker)) = (spending_limit, usage_tracker) {
                let is_credits = limit_unit.eq_ignore_ascii_case("credits");
                let used = if is_credits {
                    tracker.get_total_credits(id)
                } else {
                    tracker.get_total_cost(id)
                };
                if used >= limit {
                    tracing::warn!(
                        api_key_id = id,
                        api_key_name = %name,
                        used,
                        spending_limit = limit,
                        limit_unit = %limit_unit,
                        "native API Key 额度已用尽"
                    );
                    let message = if is_credits {
                        format!(
                            "API key spending limit exceeded. Used: {used:.2} credits, Limit: {limit:.2} credits"
                        )
                    } else {
                        format!(
                            "API key spending limit exceeded. Used: ${used:.2}, Limit: ${limit:.2}"
                        )
                    };
                    return NativeAuthOutcome::Rejected(NativeAuthError {
                        status: StatusCode::FORBIDDEN,
                        error_type: "ForbiddenException",
                        message,
                    });
                }
            }
            tracing::debug!(
                api_key_id = id,
                api_key_name = %name,
                bound_credential_ids = bound_credential_ids.as_ref().map(Vec::len),
                "native API Key 认证通过"
            );
            NativeAuthOutcome::Authorized {
                api_key_id: id,
                bound_credential_ids: bound_credential_ids.unwrap_or_default(),
            }
        }
        ApiKeyAuthResult::Expired => NativeAuthOutcome::Rejected(NativeAuthError {
            status: StatusCode::FORBIDDEN,
            error_type: "ForbiddenException",
            message: "API key has expired. Please contact the administrator to renew it."
                .to_string(),
        }),
        ApiKeyAuthResult::Disabled => NativeAuthOutcome::Rejected(NativeAuthError {
            status: StatusCode::FORBIDDEN,
            error_type: "ForbiddenException",
            message: "API key has been disabled. Please contact the administrator.".to_string(),
        }),
        ApiKeyAuthResult::NotFound => NativeAuthOutcome::Rejected(NativeAuthError {
            status: StatusCode::UNAUTHORIZED,
            error_type: "AuthenticationException",
            message: "Invalid API key.".to_string(),
        }),
    }
}

/// 从请求头提取客户端 IP（x-forwarded-for 首个非空段，其次 x-real-ip）。
pub(crate) fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok())
        && let Some(first) = value
            .split(',')
            .map(str::trim)
            .find(|part| !part.is_empty())
    {
        return Some(first.to_string());
    }
    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 从请求头提取 User-Agent（截断至 256 字符，避免落库超长字段）。
pub(crate) fn extract_user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(256).collect())
}

/// inference 请求的 per-API-key 用量记录上下文（credential_id 在拿到上游响应后补全）。
struct PendingNativeUsage {
    api_key_id: u32,
    usage_tracker: Arc<UsageTracker>,
    model: String,
    client_ip: Option<String>,
    user_agent: Option<String>,
    request_started: Instant,
}

/// 原生请求分类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeKind {
    /// 推理：`*StreamingService.GenerateAssistantResponse` / `*.SendMessage`
    Inference,
    /// MCP：`*.InvokeMCP` / `*.InvokeMCPStream`
    Mcp,
    /// 控制面 op
    Control(ControlOp),
    /// 未识别的 target（POC 兜底返回 `{}`，保证客户端启动不失败）
    Unknown,
}

/// 根据 `x-amz-target` 分类原生请求。
///
/// 选择"按操作名后缀"而不是按完整 service 名匹配：Kiro 2.36 同时存在旧
/// `AmazonCodeWhispererService` / `AmazonCodeWhispererStreamingService` 与
/// KAS 新 `KiroRuntimeService` / `KiroControlPlaneBearerService` 两代 target。
pub(crate) fn classify_target(target: &str) -> NativeKind {
    let target = target.trim();
    let operation = target.rsplit('.').next().unwrap_or_default();
    match operation {
        "GenerateAssistantResponse" | "SendMessage" => NativeKind::Inference,
        "InvokeMCP" | "InvokeMCPStream" => NativeKind::Mcp,
        _ => match ControlOp::from_target(target) {
            Some(op) => NativeKind::Control(op),
            None => NativeKind::Unknown,
        },
    }
}

/// 内部令牌校验。
///
/// - `expected` 为 None/空：POC 模式，放行；
/// - 否则要求 `provided` 与 `expected` 常量时间相等。
pub(crate) fn check_native_token(expected: Option<&str>, provided: Option<&str>) -> bool {
    match expected.map(str::trim).filter(|token| !token.is_empty()) {
        None => true,
        Some(expected) => provided
            .is_some_and(|provided| crate::common::auth::constant_time_eq(provided, expected)),
    }
}

/// 构造 `/native` 路由。
///
/// 由 `main.rs` 挂载到现有 axum 服务的 `/native` 前缀；与 Anthropic/OpenAI/Admin
/// 路由共享同一端口与进程，但路由/状态完全独立，不改变既有协议行为。
pub fn create_router(
    provider: Arc<KiroProvider>,
    api_key_manager: Option<Arc<ApiKeyManager>>,
    usage_tracker: Option<Arc<UsageTracker>>,
) -> Router {
    let token = std::env::var("KIRO_NATIVE_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let max_body_bytes = std::env::var("KIRO_NATIVE_MAX_BODY_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_BODY_BYTES);

    match &token {
        Some(_) => tracing::info!(
            "Native inbound 已启用: POST /native（X-Kiro-Native-Token 校验开启，body 上限 {} MiB）",
            max_body_bytes / 1024 / 1024
        ),
        None => tracing::warn!(
            "Native inbound 已启用: POST /native（KIRO_NATIVE_TOKEN 未设置，POC 模式不校验内部令牌！生产必须设置该环境变量并限制 /native 仅由 sub2api 访问；body 上限 {} MiB）",
            max_body_bytes / 1024 / 1024
        ),
    }
    tracing::info!(
        "Native inbound 目标: 推理直通(GenerateAssistantResponse/SendMessage)、MCP 直通(InvokeMCP*)、控制面映射(GetProfile/ListAvailableModels/GetUsageLimits/ListAvailableProfiles/...)；未知 target 返回 {{}}"
    );

    match (&api_key_manager, &usage_tracker) {
        (Some(_), Some(_)) => tracing::info!(
            "Native inbound API Key 认证已启用（Authorization: Bearer <kiro2cc key> / x-api-key；额度检查与 per-key 用量记录已启用）"
        ),
        (Some(_), None) => tracing::warn!(
            "Native inbound API Key 认证已启用，但 usage_tracker 不可用：不落库 per-key 用量、不检查 spending_limit"
        ),
        (None, _) => tracing::warn!(
            "Native inbound 未启用 API Key 认证（ApiKeyManager 不可用，admin_psw 未配置？）：仅 X-Kiro-Native-Token 保护，POC/本地模式"
        ),
    }

    let state = NativeState {
        provider,
        token,
        api_key_manager,
        usage_tracker,
    };

    // 路由以绝对路径定义并由 main.rs merge 到根路由（不用 nest）：
    // axum 的 nest 对 `/native/`（CLI 会在 endpoint 末尾追加 `/`）匹配不稳定，
    // 显式列出无斜杠、带斜杠、子路径和健康检查四种形式最稳妥。
    Router::new()
        .route("/native", post(handle_native))
        .route("/native/", post(handle_native))
        // 容忍 sub2api / 客户端在 /native 后追加路径（如 /native/generateAssistantResponse）；
        // 分发只依赖 x-amz-target。
        .route("/native/{*path}", post(handle_native))
        .route("/native/healthz", get(healthz))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .with_state(state)
}

/// `GET /native/healthz`：供 sub2api/探针检查原生入口可用性（无需令牌）。
async fn healthz() -> Response {
    aws_json::json_response(
        StatusCode::OK,
        &json!({"ok": true, "service": "kiro-native-inbound"}),
    )
}

/// 原生入口唯一 POST handler：认证 → 按 `x-amz-target` 分发。
async fn handle_native(
    State(state): State<NativeState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();

    let Some(target) = headers
        .get("x-amz-target")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return aws_json::error(
            StatusCode::BAD_REQUEST,
            "MissingRequiredHeaderException",
            "x-amz-target header is required",
        );
    };

    // 认证先于解析/转发：未授权请求不接触账号池。
    let provided_token = headers
        .get(NATIVE_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok());
    if !check_native_token(state.token.as_deref(), provided_token) {
        tracing::warn!(target = %target, "native inbound 令牌校验失败（X-Kiro-Native-Token 不匹配）");
        return aws_json::error(
            StatusCode::FORBIDDEN,
            "AccessDeniedException",
            "invalid X-Kiro-Native-Token",
        );
    }

    // API Key 认证（kiro2cc per-user key）：未配置 ApiKeyManager 时保持 POC 行为。
    // 认证先于解析/转发：未授权请求不接触账号池，也不产生用量。
    let auth_outcome = authenticate_native_api_key(
        state.api_key_manager.as_deref(),
        state.usage_tracker.as_deref(),
        &headers,
    );
    let authorized = match auth_outcome {
        NativeAuthOutcome::Unmanaged => None,
        NativeAuthOutcome::Authorized {
            api_key_id,
            bound_credential_ids,
        } => Some((api_key_id, bound_credential_ids)),
        NativeAuthOutcome::Rejected(error) => {
            tracing::warn!(target = %target, status = %error.status, "native API Key 认证失败");
            return error.into_response();
        }
    };
    let bound_ids: Vec<u64> = authorized
        .as_ref()
        .map(|(_, bound)| bound.clone())
        .unwrap_or_default();

    let Ok(body_str) = std::str::from_utf8(&body) else {
        return aws_json::error(
            StatusCode::BAD_REQUEST,
            "ValidationException",
            "request body is not valid UTF-8 JSON",
        );
    };

    let kind = classify_target(target);
    // 不记录请求体（可能含用户 prompt / 图片）；只记录 target、大小与分类。
    tracing::debug!(
        target = %target,
        kind = ?kind,
        body_bytes = body.len(),
        "native inbound request"
    );

    match kind {
        NativeKind::Inference => {
            // 计费口径：原生上游流没有 tokenUsage，这里用 tiktoken 估算输入 tokens，
            // 在 metadataEvent 帧中注入 tokenUsage 供 sub2api 记账（输出 tokens 在
            // 流式转发过程中累计）；流结束后按 API Key 落库一条 usage 记录。
            let input_tokens = billing::estimate_input_tokens(body_str);
            let result = state
                .provider
                .call_api_stream(body_str, false, false, &bound_ids)
                .await;
            // 仅当 API Key 认证通过且 usage tracker 可用时记录（MCP/控制面不记录）。
            let pending_usage = authorized.as_ref().and_then(|(api_key_id, _)| {
                state
                    .usage_tracker
                    .as_ref()
                    .map(|tracker| PendingNativeUsage {
                        api_key_id: *api_key_id,
                        usage_tracker: tracker.clone(),
                        model: KiroProvider::extract_model_from_request(body_str)
                            .unwrap_or_else(|| "auto".to_string()),
                        client_ip: extract_client_ip(&headers),
                        user_agent: extract_user_agent(&headers),
                        request_started: started,
                    })
            });
            forward_upstream_stream(result, target, started, Some(input_tokens), pending_usage)
        }
        NativeKind::Mcp => {
            // MCP 是 JSON-RPC 工具调用，不属于按 token 计费的推理请求（不记录用量）。
            let result = state.provider.call_mcp(body_str, &bound_ids).await;
            forward_upstream_stream(result, target, started, None, None)
        }
        NativeKind::Control(op) => control_plane::handle(&state.provider, op).await,
        NativeKind::Unknown => {
            // POC 兜底：未识别的控制面 op 返回空对象，避免客户端启动/交互失败。
            tracing::debug!(
                target = %target,
                "native inbound: 未识别的 x-amz-target，返回 {{}} 兜底"
            );
            aws_json::ok_empty()
        }
    }
}

/// 把 KiroProvider 的上游响应转发给原生客户端。
///
/// - 状态码原样保留（包括上游 4xx/5xx 的非流式错误体）；
/// - 响应头按黑名单过滤 hop-by-hop / 长度 / 编码类头，其余（含
///   `x-amzn-codewhisperer-conversation-id`、`x-amzn-kiro-*`、`retry-after`）透传；
/// - body 用 `reqwest::Response::bytes_stream()` 逐块转发，**不缓冲**：AWS EventStream
///   的帧边界/时序与直连 Kiro 一致，客户端可实时解析 tool/thinking/credit 事件；
/// - `input_tokens=Some(..)` 时（推理请求）在流中间插入计费包装器：除
///   `metadataEvent` 注入估算 tokenUsage 外，所有帧逐字节原样转发；
/// - 客户端断开时 axum 丢弃 body stream，reqwest 随之关闭上游连接（天然取消传播）。
fn forward_upstream_stream(
    result: anyhow::Result<(reqwest::Response, u64)>,
    target: &str,
    started: Instant,
    input_tokens: Option<u64>,
    pending_usage: Option<PendingNativeUsage>,
) -> Response {
    match result {
        Ok((upstream, credential_id)) => {
            let upstream_status = upstream.status();
            let header_map = upstream.headers().clone();
            let builder = Response::builder().status(
                StatusCode::from_u16(upstream_status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            );
            let builder = aws_json::copy_response_headers(builder, &header_map);
            let body = match input_tokens {
                Some(input_tokens) => {
                    let recording = pending_usage.map(|pending| billing::UsageRecordingConfig {
                        usage_tracker: pending.usage_tracker,
                        api_key_id: pending.api_key_id,
                        model: pending.model,
                        credential_id,
                        client_ip: pending.client_ip,
                        user_agent: pending.user_agent,
                        request_started: pending.request_started,
                    });
                    tracing::debug!(
                        target = %target,
                        input_tokens,
                        record_usage = recording.is_some(),
                        "native 计费：metadataEvent 将注入估算 tokenUsage"
                    );
                    Body::from_stream(billing::rewrite_event_stream(
                        upstream.bytes_stream(),
                        input_tokens,
                        recording,
                    ))
                }
                None => Body::from_stream(upstream.bytes_stream()),
            };
            tracing::info!(
                target = %target,
                credential_id,
                status = %upstream_status,
                estimated_input_tokens = input_tokens,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "native inbound 上游流已建立，开始转发"
            );
            builder.body(body).unwrap_or_else(|err| {
                aws_json::error(
                    StatusCode::BAD_GATEWAY,
                    "InternalException",
                    &format!("构建上游响应失败: {err}"),
                )
            })
        }
        Err(err) => {
            // 只记录顶层错误（provider 的 Display 已含关键状态/文案，避免打印完整
            // 上游响应体）；客户端拿到分类后的 JSON 错误与可重试状态码。
            tracing::warn!(
                target = %target,
                elapsed_ms = started.elapsed().as_millis() as u64,
                error = %err,
                "native inbound 上游请求失败"
            );
            aws_json::provider_error(&err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_inference_targets_for_both_protocol_generations() {
        assert_eq!(
            classify_target("AmazonCodeWhispererStreamingService.GenerateAssistantResponse"),
            NativeKind::Inference
        );
        assert_eq!(
            classify_target("KiroRuntimeService.GenerateAssistantResponse"),
            NativeKind::Inference
        );
        assert_eq!(
            classify_target("AmazonQDeveloperStreamingService.SendMessage"),
            NativeKind::Inference
        );
        assert_eq!(
            classify_target("AmazonCodeWhispererStreamingService.SendMessage"),
            NativeKind::Inference
        );
    }

    #[test]
    fn classifies_mcp_targets() {
        for target in [
            "KiroRuntimeService.InvokeMCP",
            "KiroRuntimeService.InvokeMCPStream",
            "AmazonCodeWhispererStreamingService.InvokeMCP",
            "AmazonCodeWhispererStreamingService.InvokeMCPStream",
        ] {
            assert_eq!(classify_target(target), NativeKind::Mcp, "{target}");
        }
    }

    #[test]
    fn classifies_control_plane_targets() {
        assert_eq!(
            classify_target("KiroControlPlaneBearerService.GetProfile"),
            NativeKind::Control(ControlOp::GetProfile)
        );
        assert_eq!(
            classify_target("AmazonCodeWhispererService.GetUsageLimits"),
            NativeKind::Control(ControlOp::GetUsageLimits)
        );
        assert_eq!(
            classify_target("KiroRuntimeService.GetFeatureConfiguration"),
            NativeKind::Control(ControlOp::GetFeatureConfiguration)
        );
        assert_eq!(
            classify_target("AmazonCodeWhispererService.SendTelemetryEvent"),
            NativeKind::Control(ControlOp::SendTelemetryEvent)
        );
    }

    #[test]
    fn unknown_targets_fall_back_to_unknown() {
        assert_eq!(classify_target("Foo.Bar"), NativeKind::Unknown);
        assert_eq!(classify_target(""), NativeKind::Unknown);
        // trim 后按最后一段解析
        assert_eq!(
            classify_target("  KiroRuntimeService.InvokeMCP  "),
            NativeKind::Mcp
        );
    }

    #[test]
    fn token_disabled_allows_everything() {
        assert!(check_native_token(None, None));
        assert!(check_native_token(None, Some("anything")));
        assert!(check_native_token(Some("   "), None));
    }

    #[test]
    fn token_must_match_exactly_when_configured() {
        assert!(check_native_token(Some("secret"), Some("secret")));
        assert!(!check_native_token(Some("secret"), Some("Secret")));
        assert!(!check_native_token(Some("secret"), Some("")));
        assert!(!check_native_token(Some("secret"), None));
    }
    // ---- API Key 认证（per-user kiro2cc key） ---------------------------

    use axum::http::HeaderValue;

    fn temp_manager() -> (ApiKeyManager, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("kiro-native-auth-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let manager = ApiKeyManager::load(dir.join("api_keys.json")).unwrap();
        (manager, dir)
    }

    fn bearer_headers(key: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {key}")).unwrap(),
        );
        headers
    }

    fn x_api_key_headers(key: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_str(key).unwrap());
        headers
    }

    fn create_key(
        manager: &ApiKeyManager,
        bound: Option<Vec<u64>>,
    ) -> crate::model::api_key::ApiKey {
        manager
            .create(
                "native-user".to_string(),
                None,
                None,
                Some("usd".to_string()),
                None,
                bound,
            )
            .unwrap()
    }

    #[test]
    fn unmanaged_auth_keeps_poc_behavior() {
        assert_eq!(
            authenticate_native_api_key(None, None, &HeaderMap::new()),
            NativeAuthOutcome::Unmanaged
        );
        // 即使带 key，无 manager 时也不拦截（仍受 X-Kiro-Native-Token 保护）
        assert_eq!(
            authenticate_native_api_key(None, None, &bearer_headers("sk-anything")),
            NativeAuthOutcome::Unmanaged
        );
    }

    #[test]
    fn valid_api_key_authorizes_and_carries_bound_credential_ids() {
        let (manager, _dir) = temp_manager();
        let api_key = create_key(&manager, Some(vec![7, 8]));

        let outcome =
            authenticate_native_api_key(Some(&manager), None, &bearer_headers(&api_key.key));
        assert_eq!(
            outcome,
            NativeAuthOutcome::Authorized {
                api_key_id: api_key.id,
                bound_credential_ids: vec![7, 8],
            }
        );

        // x-api-key 与 Authorization: Bearer 等价
        let outcome =
            authenticate_native_api_key(Some(&manager), None, &x_api_key_headers(&api_key.key));
        assert!(matches!(outcome, NativeAuthOutcome::Authorized { .. }));
    }

    #[test]
    fn missing_or_invalid_api_key_is_rejected_without_leaking_the_key() {
        let (manager, _dir) = temp_manager();

        let missing = authenticate_native_api_key(Some(&manager), None, &HeaderMap::new());
        match missing {
            NativeAuthOutcome::Rejected(error) => {
                assert_eq!(error.status, StatusCode::UNAUTHORIZED);
                assert_eq!(error.error_type, "AuthenticationException");
            }
            other => panic!("expected rejection, got {other:?}"),
        }

        let secret = "sk-super-secret-key-that-must-not-leak";
        let invalid = authenticate_native_api_key(Some(&manager), None, &bearer_headers(secret));
        match invalid {
            NativeAuthOutcome::Rejected(error) => {
                assert_eq!(error.status, StatusCode::UNAUTHORIZED);
                assert!(!error.message.contains(secret), "错误信息不得回显 API Key");
            }
            other => panic!("expected rejection, got {other:?}"),
        }
    }

    #[test]
    fn expired_api_key_is_rejected_with_403() {
        let (manager, _dir) = temp_manager();
        let api_key = manager
            .create(
                "expired".to_string(),
                Some(chrono::Utc::now() - chrono::Duration::days(1)),
                None,
                Some("usd".to_string()),
                None,
                None,
            )
            .unwrap();
        let outcome =
            authenticate_native_api_key(Some(&manager), None, &bearer_headers(&api_key.key));
        match outcome {
            NativeAuthOutcome::Rejected(error) => {
                assert_eq!(error.status, StatusCode::FORBIDDEN);
                assert!(!error.message.contains(&api_key.key));
            }
            other => panic!("expected rejection, got {other:?}"),
        }
    }

    #[test]
    fn disabled_api_key_is_rejected_with_403() {
        let (manager, _dir) = temp_manager();
        let api_key = create_key(&manager, None);
        manager
            .update(api_key.id, None, Some(false), None, None, None, None, None)
            .unwrap();
        let outcome =
            authenticate_native_api_key(Some(&manager), None, &bearer_headers(&api_key.key));
        match outcome {
            NativeAuthOutcome::Rejected(error) => {
                assert_eq!(error.status, StatusCode::FORBIDDEN);
                assert!(error.message.contains("disabled"));
            }
            other => panic!("expected rejection, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn spending_limit_credits_is_enforced_with_403() {
        let (manager, dir) = temp_manager();
        let api_key = manager
            .create(
                "limited".to_string(),
                None,
                Some(1.0),
                Some("credits".to_string()),
                None,
                None,
            )
            .unwrap();
        let tracker = UsageTracker::load(dir.join("api_key_usage.json")).unwrap();
        tracker.record(
            api_key.id,
            "claude-sonnet-4.6".to_string(),
            0,
            0,
            crate::model::usage::RecordOptions {
                credits_used: Some(2.0),
                ..Default::default()
            },
        );

        let outcome = authenticate_native_api_key(
            Some(&manager),
            Some(&tracker),
            &bearer_headers(&api_key.key),
        );
        match outcome {
            NativeAuthOutcome::Rejected(error) => {
                assert_eq!(error.status, StatusCode::FORBIDDEN);
                assert!(error.message.contains("spending limit"));
                assert!(error.message.contains("credits"));
            }
            other => panic!("expected rejection, got {other:?}"),
        }
    }

    #[test]
    fn client_ip_prefers_x_forwarded_for_then_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.9, 10.0.0.1"),
        );
        headers.insert("x-real-ip", HeaderValue::from_static("10.0.0.2"));
        assert_eq!(extract_client_ip(&headers), Some("203.0.113.9".to_string()));

        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("10.0.0.2"));
        assert_eq!(extract_client_ip(&headers), Some("10.0.0.2".to_string()));

        assert_eq!(extract_client_ip(&HeaderMap::new()), None);
    }

    #[test]
    fn user_agent_is_truncated_to_256_chars() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::USER_AGENT,
            HeaderValue::from_static("kiro-cli/2.36.0"),
        );
        assert_eq!(
            extract_user_agent(&headers),
            Some("kiro-cli/2.36.0".to_string())
        );

        let long = "a".repeat(400);
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::USER_AGENT,
            HeaderValue::from_str(&long).unwrap(),
        );
        assert_eq!(extract_user_agent(&headers).unwrap().chars().count(), 256);
        assert_eq!(extract_user_agent(&HeaderMap::new()), None);
    }
}
