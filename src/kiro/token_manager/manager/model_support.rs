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
//! # 探测不拖慢请求
//! 探测在独立后台任务里执行，请求最多等待 `MODEL_SUPPORT_MAX_WAIT`：稳态（缓存新鲜）
//! 零等待；冷启动或缓存集体过期时，请求等探测完成但不超过上限，超时则本次按「未知」
//! 调度，探测继续在后台完成。探测任务不会被取消（取消 Token 刷新可能丢失刚轮换的
//! refreshToken），所以只能「不再等待」，不能给探测本身套 timeout。
use super::super::entry::{CredentialEntry, HealthStatus};
use super::super::types::{ModelSupport, MultiTokenManager};
use crate::kiro::model::available_models::AvailableModelsResponse;
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// 请求为等待模型探测完成最多阻塞的时长
///
/// 只在探测结果缺失/过期时才会等待（成功结果缓存 24 小时，或冷启动）。
pub(crate) const MODEL_SUPPORT_MAX_WAIT: Duration = Duration::from_secs(5);

/// 这些模型 ID 不是具体模型，不做可用性过滤
fn is_unfilterable_model(model: &str) -> bool {
    model.is_empty() || model.eq_ignore_ascii_case("auto")
}

impl MultiTokenManager {
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
    /// 范围内可调度账号不足 2 个时没有路由可言，返回空。
    pub(crate) fn model_support_probe_targets(&self, allowed_ids: &[u64]) -> Vec<u64> {
        let schedulable: Vec<u64> = {
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
                .map(|e| e.id)
                .collect()
        };
        if schedulable.len() < 2 {
            return Vec::new();
        }
        let cache = self.model_support.lock();
        schedulable
            .into_iter()
            .filter(|id| !cache.get(id).is_some_and(ModelSupport::is_fresh))
            .collect()
    }

    /// 确保范围内账号的模型可用性探测结果是新的（过期或缺失才会发起探测）
    ///
    /// 在选账号之前调用。稳态下只是几次内存查找；成功结果缓存 24 小时。
    /// 过期后查询 `ListAvailableModels`，最多等待 `MODEL_SUPPORT_MAX_WAIT`。
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
        if self.model_support_probe_targets(allowed_ids).is_empty() {
            return;
        }

        let this = Arc::clone(self);
        let ids = allowed_ids.to_vec();
        let task = tokio::spawn(async move { this.probe_stale_models(&ids, probe).await });
        // 超时只是不再等待：丢弃 JoinHandle 不会取消任务
        if tokio::time::timeout(max_wait, task).await.is_err() {
            tracing::debug!(
                "模型探测超过 {:?} 仍未完成，本次请求按「未知」调度，探测在后台继续",
                max_wait
            );
        }
    }

    async fn probe_stale_models<P, Fut>(self: &Arc<Self>, allowed_ids: &[u64], probe: P)
    where
        P: Fn(Arc<Self>, u64) -> Fut,
        Fut: Future<Output = anyhow::Result<AvailableModelsResponse>>,
    {
        // 串行化：并发请求同时发现过期时，只有第一个真正探测，其余在这里排队
        let _guard = self.model_support_refresh_lock.lock().await;
        // 锁内二次确认：排队期间前一个任务可能已完成本批探测，此时直接复用结果
        let targets = self.model_support_probe_targets(allowed_ids);
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
