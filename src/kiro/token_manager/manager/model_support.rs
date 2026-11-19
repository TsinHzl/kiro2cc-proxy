//! 按账号实际可用模型路由（Model-aware Routing）。
//!
//! 混合账号池里，不同账号在上游的模型权限可能不同（企业策略禁用部分模型等）。
//! 请求被分发到不支持目标模型的账号时，上游会直接拒绝，而该错误不会触发故障转移。
//! 这里用各账号的 `ListAvailableModels` 结果做调度前置过滤。
//!
//! # 设计取舍：宁可不过滤，也不误排除
//! - 探测结果只有「明确不含该模型」才算不支持；探测失败、返回空列表、缓存过期、
//!   `auto` 等都视为「未知」，照常参与调度。
//! - 即使有账号明确不支持，只要候选里有支持者或未知者就排除它们；
//!   候选全部「明确不支持」时不过滤（上游列表可能不完整，例如新模型尚未出现在列表里，
//!   此时保持与引入本功能前一致的行为）。
//! - 探测失败不会禁用账号，也不计入失败次数。
//!
//! # 订阅等级与 Opus（Issue #71）
//! Free / 等级未知的账号默认不承接 Opus。但部分 Free 账号带有活动赠送等额外权益，
//! 上游会在其 `ListAvailableModels` 里返回 Opus，因此探测结果明确包含目标模型时放行
//! （见 `tier_allows_model`）。此路径是反向的：探测缺失、失败、过期都**不**放行。
//!
//! # 探测不拖慢请求
//! 探测在独立后台任务里执行，请求最多等待 `MODEL_SUPPORT_MAX_WAIT`：稳态（缓存新鲜）
//! 零等待；冷启动或缓存集体过期时，请求等探测完成但不超过上限，超时则本次按「未知」
//! 调度，探测继续在后台完成。探测任务不会被取消（取消 Token 刷新可能丢失刚轮换的
//! refreshToken），所以只能「不再等待」，不能给探测本身套 timeout。
use super::super::entry::{CredentialEntry, HealthStatus};
use super::super::types::{ModelSupport, MultiTokenManager};
use crate::kiro::model::available_models::AvailableModelsResponse;
use crate::kiro::model::credentials::KiroCredentials;
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 请求为等待模型探测完成最多阻塞的时长
///
/// 只在探测结果缺失/过期时才会等待（每账号约每 30 分钟一次，或冷启动）。
pub(crate) const MODEL_SUPPORT_MAX_WAIT: Duration = Duration::from_secs(5);

/// 这些模型 ID 不是具体模型，不做可用性过滤
fn is_unfilterable_model(model: &str) -> bool {
    model.is_empty() || model.eq_ignore_ascii_case("auto")
}

/// 请求的是否 Opus 系列模型
fn is_opus_model(model: Option<&str>) -> bool {
    model.is_some_and(|m| m.to_ascii_lowercase().contains("opus"))
}

impl MultiTokenManager {
    /// 账号是否「明确支持」该模型（探测结果有效且包含该模型）
    ///
    /// 探测结果缺失、过期、失败时返回 `false`（未知）。与 [`Self::is_known_unsupported`]
    /// 不是互补关系：未知状态下两者都为 `false`。
    ///
    /// 锁序同 `is_known_unsupported`：`model_support` 是叶子锁。
    pub(crate) fn is_known_supported(&self, id: u64, model: Option<&str>) -> bool {
        let Some(model) = model.filter(|m| !is_unfilterable_model(m)) else {
            return false;
        };
        let model_lower = model.to_ascii_lowercase();
        let cache = self.model_support.lock();
        Self::supported_in(cache.get(&id), &model_lower)
    }

    /// 账号的订阅等级是否允许承接该模型（目前只有 Opus 受订阅等级限制）
    ///
    /// - 非 Opus 请求：不受限制
    /// - 订阅等级支持 Opus（付费账号、企业版 IdC 租户账号）：允许
    /// - 订阅等级不支持（Free 或等级未知）：只有上游 `ListAvailableModels` 的有效探测
    ///   结果明确包含该模型时才放行。部分 Free 账号带有活动赠送等额外权益，上游会返回
    ///   Opus；探测缺失、过期、失败或不含该模型时仍然排除，避免请求被上游拒绝（该错误不会
    ///   触发故障转移）。
    ///
    /// 锁序：调用方可以持有 `entries` 锁再进来（`model_support` 是叶子锁）。
    pub(crate) fn tier_allows_model(
        &self,
        id: u64,
        credentials: &KiroCredentials,
        model: Option<&str>,
    ) -> bool {
        !is_opus_model(model) || credentials.supports_opus() || self.is_known_supported(id, model)
    }

    /// 账号是否「明确不支持」该模型（探测结果有效且不含该模型）
    ///
    /// 探测结果缺失、过期、失败时返回 `false`（未知）。
    ///
    /// 锁序：调用方可以持有 `entries` 锁再进来；`model_support` 是叶子锁，
    /// 持有期间不会再获取任何其它锁，因此不存在反向路径。
    pub(crate) fn is_known_unsupported(&self, id: u64, model: Option<&str>) -> bool {
        let Some(model) = model.filter(|m| !is_unfilterable_model(m)) else {
            return false;
        };
        let model_lower = model.to_ascii_lowercase();
        let cache = self.model_support.lock();
        Self::unsupported_in(cache.get(&id), &model_lower)
    }

    /// `model_lower` 必须已小写化（集合中存的就是小写）；与 [`Self::unsupported_in`] 对偶，
    /// 两者共用同一份「结果新鲜且探测成功」的判定
    fn supported_in(support: Option<&ModelSupport>, model_lower: &str) -> bool {
        support
            .filter(|s| s.is_fresh())
            .and_then(|s| s.models.as_ref())
            .is_some_and(|models| models.contains(model_lower))
    }

    /// `model_lower` 必须已小写化（集合中存的就是小写）
    fn unsupported_in(support: Option<&ModelSupport>, model_lower: &str) -> bool {
        support
            .filter(|s| s.is_fresh())
            .and_then(|s| s.models.as_ref())
            .is_some_and(|models| !models.contains(model_lower))
    }

    /// 在候选账号中排除「明确不支持」目标模型的账号
    ///
    /// 若排除后一个都不剩（候选全部明确不支持），原样返回，见模块文档。
    pub(crate) fn prefer_model_supporters<'a>(
        &self,
        candidates: Vec<&'a CredentialEntry>,
        model: Option<&str>,
    ) -> Vec<&'a CredentialEntry> {
        let Some(model) = model.filter(|m| !is_unfilterable_model(m)) else {
            return candidates;
        };
        let model_lower = model.to_ascii_lowercase();
        let cache = self.model_support.lock();
        let kept: Vec<&CredentialEntry> = candidates
            .iter()
            .filter(|e| !Self::unsupported_in(cache.get(&e.id), &model_lower))
            .copied()
            .collect();
        if kept.is_empty() {
            if !candidates.is_empty() {
                // 这是排查「上游列表不准 / 探测结果过期」的一手线索
                tracing::warn!(
                    "模型感知路由: 全部 {} 个候选账号都被判定不支持 {}，回退为不过滤",
                    candidates.len(),
                    model
                );
            }
            return candidates;
        }
        if kept.len() < candidates.len() {
            tracing::debug!(
                "模型感知路由: 为 {} 排除 {}/{} 个不支持的账号",
                model,
                candidates.len() - kept.len(),
                candidates.len()
            );
        }
        kept
    }

    /// 需要探测的账号：范围内、可调度（未禁用且非 Unhealthy）、探测结果缺失或过期
    ///
    /// 范围内可调度账号不足 2 个时没有路由可言，一般返回空。例外：请求 Opus 时，订阅等级
    /// 不支持 Opus 的账号（Free / 等级未知）即使是池里唯一的账号也要探测，因为探测结果
    /// 是它能否承接 Opus 的唯一依据（见 [`Self::tier_allows_model`]）。
    pub(crate) fn model_support_probe_targets(
        &self,
        allowed_ids: &[u64],
        model: Option<&str>,
    ) -> Vec<u64> {
        let is_opus = is_opus_model(model);
        // (账号 ID, 是否需要靠探测结果才能承接本次 Opus 请求)
        let schedulable: Vec<(u64, bool)> = {
            let entries = self.entries.lock();
            entries
                .iter()
                .filter(|e| {
                    (allowed_ids.is_empty() || allowed_ids.contains(&e.id))
                        && !matches!(
                            Self::compute_health(e),
                            HealthStatus::Unhealthy | HealthStatus::Disabled
                        )
                })
                .map(|e| (e.id, is_opus && !e.credentials.supports_opus()))
                .collect()
        };
        let routable = schedulable.len() >= 2;
        let cache = self.model_support.lock();
        schedulable
            .into_iter()
            .filter(|(id, tier_gated)| {
                (routable || *tier_gated) && !cache.get(id).is_some_and(ModelSupport::is_fresh)
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// 确保范围内账号的模型可用性探测结果是新的（过期或缺失才会发起探测）
    ///
    /// 在选账号之前调用。稳态下只是几次内存查找；每个账号每 30 分钟最多多一次
    /// `ListAvailableModels`。最多等待 `MODEL_SUPPORT_MAX_WAIT`，见模块文档。
    pub async fn refresh_model_support_if_stale(
        self: &Arc<Self>,
        model: Option<&str>,
        allowed_ids: &[u64],
    ) {
        self.refresh_model_support_with(
            model,
            allowed_ids,
            MODEL_SUPPORT_MAX_WAIT,
            |manager, id| async move { manager.list_available_models_for(id).await },
        )
        .await;
    }

    /// `refresh_model_support_if_stale` 的可注入实现（探测动作与最长等待时间可替换，便于测试）
    pub(crate) async fn refresh_model_support_with<P, Fut>(
        self: &Arc<Self>,
        model: Option<&str>,
        allowed_ids: &[u64],
        max_wait: Duration,
        probe: P,
    ) where
        P: Fn(Arc<Self>, u64) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<AvailableModelsResponse>> + Send + 'static,
    {
        if model.is_none_or(is_unfilterable_model) {
            return;
        }
        // 锁外快速路径：稳态下（无过期账号）零成本返回，不触碰串行锁
        if self
            .model_support_probe_targets(allowed_ids, model)
            .is_empty()
        {
            return;
        }

        let this = Arc::clone(self);
        let ids = allowed_ids.to_vec();
        let model = model.map(str::to_owned);
        let probe_task =
            async move { this.probe_stale_models(&ids, model.as_deref(), probe).await };
        let task = tokio::spawn(probe_task);
        // 超时只是不再等待：丢弃 JoinHandle 不会取消任务
        if tokio::time::timeout(max_wait, task).await.is_err() {
            tracing::debug!(
                "模型探测超过 {:?} 仍未完成，本次请求按「未知」调度，探测在后台继续",
                max_wait
            );
        }
    }

    async fn probe_stale_models<P, Fut>(
        self: &Arc<Self>,
        allowed_ids: &[u64],
        model: Option<&str>,
        probe: P,
    ) where
        P: Fn(Arc<Self>, u64) -> Fut,
        Fut: Future<Output = anyhow::Result<AvailableModelsResponse>>,
    {
        // 串行化：并发请求同时发现过期时，只有第一个真正探测，其余在这里排队
        let _guard = self.model_support_refresh_lock.lock().await;
        // 锁内二次确认：排队期间前一个任务可能已完成本批探测，此时直接复用结果
        let targets = self.model_support_probe_targets(allowed_ids, model);
        if targets.is_empty() {
            return;
        }

        let results = futures::future::join_all(targets.iter().map(|&id| {
            let fut = probe(Arc::clone(self), id);
            async move { (id, fut.await) }
        }))
        .await;

        let mut cache = self.model_support.lock();
        for (id, result) in results {
            let models: Option<HashSet<String>> = match result {
                Ok(resp) if !resp.models.is_empty() => Some(
                    resp.models
                        .iter()
                        .map(|m| m.model_id.to_ascii_lowercase())
                        .collect(),
                ),
                Ok(_) => {
                    tracing::warn!("账号 #{} 的 ListAvailableModels 返回空列表，忽略该结果", id);
                    None
                }
                Err(e) => {
                    tracing::warn!("账号 #{} 探测可用模型失败，暂不按模型过滤该账号: {}", id, e);
                    None
                }
            };
            cache.insert(
                id,
                ModelSupport {
                    models,
                    checked_at: Instant::now(),
                },
            );
        }
    }

    /// 丢弃某账号的探测结果（账号被修改或删除后，旧结果不再可信）
    pub(crate) fn invalidate_model_support(&self, id: u64) {
        self.model_support.lock().remove(&id);
    }
}
