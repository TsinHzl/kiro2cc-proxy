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
//! 请求必须带内部令牌头 `X-Kiro-Native-Token`，值等于环境变量 `KIRO_NATIVE_TOKEN`。
//! 未设置 `KIRO_NATIVE_TOKEN` 时（POC/本地模式）放行，生产必须设置，并保证
//! `/native` 仅由集群内 sub2api 访问（ClusterIP + NetworkPolicy/ingress 限制）。
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

use crate::kiro::provider::KiroProvider;

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
pub fn create_router(provider: Arc<KiroProvider>) -> Router {
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

    let state = NativeState { provider, token };

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
            let result = state
                .provider
                .call_api_stream(body_str, false, false, &[])
                .await;
            forward_upstream_stream(result, target, started)
        }
        NativeKind::Mcp => {
            let result = state.provider.call_mcp(body_str, &[]).await;
            forward_upstream_stream(result, target, started)
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

/// 把 KiroProvider 的上游响应原样转发给原生客户端。
///
/// - 状态码原样保留（包括上游 4xx/5xx 的非流式错误体）；
/// - 响应头按黑名单过滤 hop-by-hop / 长度 / 编码类头，其余（含
///   `x-amzn-codewhisperer-conversation-id`、`x-amzn-kiro-*`、`retry-after`）透传；
/// - body 用 `reqwest::Response::bytes_stream()` 逐块转发，**不缓冲**：AWS EventStream
///   的帧边界/时序与直连 Kiro 一致，客户端可实时解析 tool/thinking/credit 事件；
/// - 客户端断开时 axum 丢弃 body stream，reqwest 随之关闭上游连接（天然取消传播）。
fn forward_upstream_stream(
    result: anyhow::Result<(reqwest::Response, u64)>,
    target: &str,
    started: Instant,
) -> Response {
    match result {
        Ok((upstream, credential_id)) => {
            let upstream_status = upstream.status();
            let header_map = upstream.headers().clone();
            let builder = Response::builder().status(
                StatusCode::from_u16(upstream_status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            );
            let builder = aws_json::copy_response_headers(builder, &header_map);
            let body = Body::from_stream(upstream.bytes_stream());
            tracing::info!(
                target = %target,
                credential_id,
                status = %upstream_status,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "native inbound 上游流已建立，开始原样转发"
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
}
