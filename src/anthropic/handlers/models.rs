// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Anthropic API Handler 函数

use crate::anthropic::middleware::AppState;
use crate::anthropic::types::{ErrorResponse, Model, ModelsResponse};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use std::time::Duration;

/// GET /v1/models
///
/// 返回可用的模型列表（动态来源，带 TTL 缓存 + 静态表回退）
pub async fn get_models(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("Received GET /v1/models request");

    Json(ModelsResponse {
        object: "list".to_string(),
        data: fetch_models_dynamic(&state).await,
    })
}

/// 模型缓存别名，简化签名
pub(crate) type ModelCache =
    std::sync::Arc<parking_lot::RwLock<Option<super::super::middleware::CachedModels>>>;

/// 动态获取模型列表：优先上游实时响应（带 TTL 缓存），失败按顺序回退。
///
/// 分支：
/// 1. 缓存存在且未过期 → 直接返回缓存，不打上游
/// 2. 缓存过期/缺失且上游成功返回非空模型集 → 映射、写缓存、返回
/// 3. 上游失败但存在旧缓存 → 续用旧缓存（warn）
/// 4. 上游失败且无缓存 → 回退静态表 `build_model_list()`（warn）
/// 5. 未配置 `kiro_provider` → 直接回退静态表（无上游可查）
pub(crate) async fn fetch_models_dynamic(state: &AppState) -> Vec<Model> {
    // 分支 5：无上游 provider，直接静态表
    let Some(provider) = state.kiro_provider.as_ref() else {
        return build_model_list();
    };

    let ttl = Duration::from_secs(provider.token_manager().config().model_cache_ttl_secs);

    // 分支 1：缓存命中且未过期
    if let Some(hit) = cached_if_fresh(&state.model_cache, ttl) {
        return hit;
    }

    // 缓存缺失/过期，尝试刷新上游（仅此处涉及网络；结果归一化为 Option<Vec<Model>>）
    let refreshed: Option<Vec<Model>> = match provider.token_manager().list_available_models().await
    {
        Ok(resp) if !resp.models.is_empty() => {
            Some(resp.models.iter().map(available_model_to_model).collect())
        }
        Ok(_) => {
            tracing::warn!("上游模型列表为空，回退缓存/静态表");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "刷新上游模型列表失败，回退缓存/静态表");
            None
        }
    };

    resolve_after_refresh(&state.model_cache, refreshed)
}

/// 缓存命中判定（纯逻辑，无网络）：存在且未超过 TTL 时返回克隆的模型列表。
pub(crate) fn cached_if_fresh(cache: &ModelCache, ttl: Duration) -> Option<Vec<Model>> {
    let guard = cache.read();
    guard
        .as_ref()
        .filter(|cached| cached.fetched_at.elapsed() < ttl)
        .map(|cached| cached.models.clone())
}

/// 刷新结果落地（纯逻辑，无网络）：
/// - `Some(非空)` → 写缓存并返回（分支 2）
/// - `None` 且有旧缓存 → 续用旧缓存（分支 3）
/// - `None` 且无缓存 → 静态表（分支 4）
pub(crate) fn resolve_after_refresh(
    cache: &ModelCache,
    refreshed: Option<Vec<Model>>,
) -> Vec<Model> {
    if let Some(models) = refreshed {
        *cache.write() = Some(super::super::middleware::CachedModels {
            models: models.clone(),
            fetched_at: std::time::Instant::now(),
        });
        return models;
    }
    if let Some(cached) = cache.read().as_ref() {
        return cached.models.clone();
    }
    build_model_list()
}

/// 构建可用模型列表（供 get_models 和 get_model 共用）
pub(crate) fn build_model_list() -> Vec<Model> {
    vec![
        // === 旧版模型 ID（兼容旧版 Claude Code 客户端） ===
        // 这些旧 ID 在 map_model() 中会被正确映射到对应的 Kiro 模型
        Model {
            id: "claude-3-5-sonnet-20241022".to_string(),
            object: "model".to_string(),
            created: 1729555200,
            owned_by: "anthropic".to_string(),
            display_name: "Claude 3.5 Sonnet".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 8192,
        },
        Model {
            id: "claude-3-5-haiku-20241022".to_string(),
            object: "model".to_string(),
            created: 1729555200,
            owned_by: "anthropic".to_string(),
            display_name: "Claude 3.5 Haiku".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 8192,
        },
        Model {
            id: "claude-3-opus-20240229".to_string(),
            object: "model".to_string(),
            created: 1709164800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude 3 Opus".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 4096,
        },
        Model {
            id: "claude-3-haiku-20240307".to_string(),
            object: "model".to_string(),
            created: 1709769600,
            owned_by: "anthropic".to_string(),
            display_name: "Claude 3 Haiku".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 4096,
        },
        Model {
            id: "claude-3-sonnet-20240229".to_string(),
            object: "model".to_string(),
            created: 1709164800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude 3 Sonnet".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 4096,
        },
        // === Claude 4.x 过渡期模型 ID ===
        Model {
            id: "claude-sonnet-4".to_string(),
            object: "model".to_string(),
            created: 1747180800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-20250514".to_string(),
            object: "model".to_string(),
            created: 1747180800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-20250514".to_string(),
            object: "model".to_string(),
            created: 1747180800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        // === 当前主力模型 ===
        Model {
            id: "claude-sonnet-4-5-20250929".to_string(),
            object: "model".to_string(),
            created: 1727568000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-5-20250929-thinking".to_string(),
            object: "model".to_string(),
            created: 1727568000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-5-20251101".to_string(),
            object: "model".to_string(),
            created: 1730419200,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-5-20251101-thinking".to_string(),
            object: "model".to_string(),
            created: 1730419200,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-6".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.6".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-4-6-thinking".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 4.6 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-5".to_string(),
            object: "model".to_string(),
            created: 1775600000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-sonnet-5-thinking".to_string(),
            object: "model".to_string(),
            created: 1775600000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Sonnet 5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-opus-4-6".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.6".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-4-6-thinking".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.6 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-4-7".to_string(),
            object: "model".to_string(),
            created: 1773000000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.7".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-4-7-thinking".to_string(),
            object: "model".to_string(),
            created: 1773000000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.7 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-4-8".to_string(),
            object: "model".to_string(),
            created: 1775600000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.8".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-4-8-thinking".to_string(),
            object: "model".to_string(),
            created: 1775600000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 4.8 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-5".to_string(),
            object: "model".to_string(),
            created: 1777500000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-opus-5-thinking".to_string(),
            object: "model".to_string(),
            created: 1777500000,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Opus 5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-fable-5".to_string(),
            object: "model".to_string(),
            created: 1772582400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Fable 5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-fable-5-thinking".to_string(),
            object: "model".to_string(),
            created: 1772582400,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Fable 5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 128000,
        },
        Model {
            id: "claude-haiku-4-5-20251001".to_string(),
            object: "model".to_string(),
            created: 1727740800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Haiku 4.5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        Model {
            id: "claude-haiku-4-5-20251001-thinking".to_string(),
            object: "model".to_string(),
            created: 1727740800,
            owned_by: "anthropic".to_string(),
            display_name: "Claude Haiku 4.5 (Thinking)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 64000,
        },
        // === 非 Claude 模型 ===
        Model {
            id: "auto".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "kiro".to_string(),
            display_name: "Auto (智能路由)".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "deepseek-3.2".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "deepseek".to_string(),
            display_name: "DeepSeek 3.2".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "glm-5".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "glm".to_string(),
            display_name: "GLM-5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "minimax-m2.5".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "minimax".to_string(),
            display_name: "MiniMax M2.5".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "minimax-m2.1".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "minimax".to_string(),
            display_name: "MiniMax M2.1".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "qwen3-coder-next".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "qwen".to_string(),
            display_name: "Qwen3 Coder Next".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "gpt-5.6-sol".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "openai".to_string(),
            display_name: "GPT-5.6 Sol".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "gpt-5.6-terra".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "openai".to_string(),
            display_name: "GPT-5.6 Terra".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
        Model {
            id: "gpt-5.6-luna".to_string(),
            object: "model".to_string(),
            created: 1770314400,
            owned_by: "openai".to_string(),
            display_name: "GPT-5.6 Luna".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 32000,
        },
    ]
}

/// 根据模型 ID 前缀推断提供方（ListAvailableModels 响应不含厂商归属字段）
///
/// 命名规则与 `build_model_list()` 中手工维护的 `owned_by` 保持一致；未知前缀返回 `"unknown"`。
/// 供 `/v1/models` 动态映射与 Admin 端共享，避免两份实现漂移。
pub(crate) fn guess_owned_by(model_id: &str) -> &'static str {
    let id = model_id.to_lowercase();
    if id.contains("claude") {
        "anthropic"
    } else if id.contains("gpt") {
        "openai"
    } else if id == "auto" {
        "kiro"
    } else if id.contains("deepseek") {
        "deepseek"
    } else if id.contains("minimax") {
        "minimax"
    } else if id.contains("glm") {
        "glm"
    } else if id.contains("qwen") {
        "qwen"
    } else {
        "unknown"
    }
}

/// 将上游 `ListAvailableModels` 返回的单条模型映射为 Anthropic `Model`
///
/// 纯函数，不涉及网络调用，可直接用 fake `AvailableModelInfo` 单测。
pub(crate) fn available_model_to_model(
    info: &crate::kiro::model::available_models::AvailableModelInfo,
) -> Model {
    Model {
        id: info.model_id.clone(),
        object: "model".to_string(),
        created: 0,
        owned_by: guess_owned_by(&info.model_id).to_string(),
        display_name: info.model_name.clone(),
        model_type: "chat".to_string(),
        max_tokens: info.token_limits.max_output_tokens as i32,
    }
}

/// GET /v1/models/:model_id
///
/// 返回指定模型的信息
pub async fn get_model(
    State(state): State<AppState>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> Response {
    tracing::info!(model_id = %model_id, "Received GET /v1/models/:model_id request");

    // 与 /v1/models 相同的动态来源，查找匹配的模型
    let models = fetch_models_dynamic(&state).await;
    if let Some(model) = models.into_iter().find(|m| m.id == model_id) {
        Json(model).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                "not_found_error",
                format!("Model '{}' not found", model_id),
            )),
        )
            .into_response()
    }
}
