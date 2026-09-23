use super::super::entry::{CredentialEntry, DisabledReason};
use super::super::types::{
    MAX_FAILURES_PER_CREDENTIAL, MultiTokenManager, QUOTA_EXHAUSTED_ALL_MARKER,
};

use chrono::{Datelike, Utc};
use std::sync::atomic::Ordering;
use std::time::Instant;

impl MultiTokenManager {
    /// 报告指定账号被限流（429 响应）
    pub fn report_throttled(&self, id: u64) {
        let mut entries = self.entries.lock();
        if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
            entry.throttle_count += 1;
            entry.last_throttled_at = Some(Instant::now());
            entry.last_throttled_wall = Some(Utc::now());
            tracing::debug!("账号 #{} 被限流（累计 {} 次）", id, entry.throttle_count);
        }
        // throttle_count 在下次 success/failure 时随 debounce 一起落盘
        self.stats_dirty.store(true, Ordering::Relaxed);
    }

    /// 报告指定账号被限流并增加轮转偏移量
    ///
    /// 用于 429 场景：增加 rotation_bias 使选择算法优先选择其他账号，
    /// 不影响 success_count 和 failure_count
    pub fn report_throttled_for_rotation(&self, id: u64) {
        let mut entries = self.entries.lock();
        if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
            entry.rotation_bias = entry.rotation_bias.saturating_add(1);
            tracing::debug!("账号 #{} rotation_bias 递增至 {}", id, entry.rotation_bias);
        }
    }

    /// 报告指定账号 API 调用成功
    ///
    /// 重置该账号的失败计数
    ///
    /// # Arguments
    /// * `id` - 账号 ID（来自 CallContext）
    pub fn report_success(&self, id: u64) {
        {
            let mut entries = self.entries.lock();
            if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                entry.failure_count = 0;
                entry.success_count += 1;
                entry.rotation_bias = 0;
                entry.last_used_at = Some(Utc::now().to_rfc3339());
                tracing::debug!(
                    "账号 #{} API 调用成功（累计 {} 次）",
                    id,
                    entry.success_count
                );
            }
        }
        self.save_stats_debounced();
    }

    /// 报告指定账号 API 调用失败
    ///
    /// 增加失败计数，达到阈值时禁用账号并切换到优先级最高的可用账号
    /// 返回是否还有可用账号可以重试
    ///
    /// # Arguments
    /// * `id` - 账号 ID（来自 CallContext）
    pub fn report_failure(&self, id: u64) -> bool {
        let result = {
            let mut entries = self.entries.lock();
            let mut current_id = self.current_id.lock();

            let entry = match entries.iter_mut().find(|e| e.id == id) {
                Some(e) => e,
                None => return entries.iter().any(|e| !e.disabled),
            };

            // 已禁用的账号直接返回，避免覆盖其原始禁用原因（如 QuotaExceeded/Manual）。
            // 并发下若该账号已被 report_quota_exhausted 等禁用，这里不应再累加失败计数。
            if entry.disabled {
                return entries.iter().any(|e| !e.disabled);
            }

            entry.failure_count += 1;
            entry.last_used_at = Some(Utc::now().to_rfc3339());
            let failure_count = entry.failure_count;

            tracing::warn!(
                "账号 #{} API 调用失败（{}/{}）",
                id,
                failure_count,
                MAX_FAILURES_PER_CREDENTIAL
            );

            if failure_count >= MAX_FAILURES_PER_CREDENTIAL {
                entry.disabled = true;
                entry.disabled_reason = Some(DisabledReason::TooManyFailures);
                tracing::error!("账号 #{} 已连续失败 {} 次，已被禁用", id, failure_count);

                // 切换到优先级最高的可用账号
                if let Some(next) = entries
                    .iter()
                    .filter(|e| !e.disabled)
                    .min_by_key(|e| e.credentials.priority)
                {
                    *current_id = next.id;
                    tracing::info!(
                        "已切换到账号 #{}（优先级 {}）",
                        next.id,
                        next.credentials.priority
                    );
                } else {
                    tracing::error!("所有账号均已禁用！");
                }
            }

            (
                entries.iter().any(|e| !e.disabled),
                failure_count >= MAX_FAILURES_PER_CREDENTIAL,
            )
        };
        let (result, just_disabled) = result;
        if just_disabled {
            // 禁用原因是关键状态，跳过防抖立即落盘
            self.save_stats();
        } else {
            self.save_stats_debounced();
        }
        result
    }

    /// 报告指定账号 refreshToken 已被服务端撤销（invalid_grant）
    ///
    /// 立即禁用该账号，不计入 `refresh_failure_count`（永久性失效，需人工更换凭证，
    /// 与瞬态刷新失败区分，因此不参与全灭自愈）
    /// 返回是否还有可用账号
    pub fn report_refresh_token_invalid(&self, id: u64) -> bool {
        let result = {
            let mut entries = self.entries.lock();

            let entry = match entries.iter_mut().find(|e| e.id == id) {
                Some(e) => e,
                None => return entries.iter().any(|e| !e.disabled),
            };

            if entry.disabled {
                return entries.iter().any(|e| !e.disabled);
            }

            entry.disabled = true;
            entry.disabled_reason = Some(DisabledReason::InvalidRefreshToken);

            tracing::error!(
                "账号 #{} 的 refreshToken 已被服务端撤销（invalid_grant），已被禁用，需人工更换凭证",
                id
            );

            entries.iter().any(|e| !e.disabled)
        };
        // 禁用原因是关键状态，跳过防抖立即落盘
        self.save_stats();
        result
    }

    /// 报告指定账号 Token 刷新失败（非 invalid_grant 的瞬态失败）
    ///
    /// 增加 `refresh_failure_count`，达到 `MAX_FAILURES_PER_CREDENTIAL` 阈值后禁用账号
    /// （`disabled_reason = TooManyRefreshFailures`）
    /// 返回是否还有可用账号
    pub fn report_refresh_failure(&self, id: u64) -> bool {
        let result = {
            let mut entries = self.entries.lock();

            let entry = match entries.iter_mut().find(|e| e.id == id) {
                Some(e) => e,
                None => return entries.iter().any(|e| !e.disabled),
            };

            if entry.disabled {
                return entries.iter().any(|e| !e.disabled);
            }

            entry.refresh_failure_count += 1;
            let refresh_failure_count = entry.refresh_failure_count;

            tracing::warn!(
                "账号 #{} Token 刷新失败（{}/{}）",
                id,
                refresh_failure_count,
                MAX_FAILURES_PER_CREDENTIAL
            );

            if refresh_failure_count >= MAX_FAILURES_PER_CREDENTIAL {
                entry.disabled = true;
                entry.disabled_reason = Some(DisabledReason::TooManyRefreshFailures);
                tracing::error!(
                    "账号 #{} 已连续刷新失败 {} 次，已被禁用",
                    id,
                    refresh_failure_count
                );
            }

            (
                entries.iter().any(|e| !e.disabled),
                refresh_failure_count >= MAX_FAILURES_PER_CREDENTIAL,
            )
        };
        let (result, just_disabled) = result;
        if just_disabled {
            self.save_stats();
        } else {
            self.save_stats_debounced();
        }
        result
    }

    /// 报告指定账号缺少 profileArn（企业 IdC 账号必需字段）
    ///
    /// 用于数据面/MCP 接口返回 400 "profileArn is required" 的场景：
    /// - 立即禁用该账号（确定性配置缺陷，重试与继续失败计数均无意义）
    /// - 切换到下一个可用账号继续重试
    /// - 返回是否还有可用账号
    pub fn report_profile_arn_missing(&self, id: u64) -> bool {
        let result = {
            let mut entries = self.entries.lock();
            let mut current_id = self.current_id.lock();

            let entry = match entries.iter_mut().find(|e| e.id == id) {
                Some(e) => e,
                None => return entries.iter().any(|e| !e.disabled),
            };

            // 已禁用的账号直接返回，避免覆盖其原始禁用原因
            if entry.disabled {
                return entries.iter().any(|e| !e.disabled);
            }

            entry.disabled = true;
            entry.disabled_reason = Some(DisabledReason::ProfileArnMissing);
            entry.last_used_at = Some(Utc::now().to_rfc3339());
            // 设为阈值，便于在管理面板中直观看到该账号已不可用
            entry.failure_count = MAX_FAILURES_PER_CREDENTIAL;

            tracing::error!(
                "账号 #{} 缺少 profileArn（企业 IdC 账号必需），已被禁用，需在账号配置中补充该字段",
                id
            );

            // 切换到优先级最高的可用账号
            if let Some(next) = entries
                .iter()
                .filter(|e| !e.disabled)
                .min_by_key(|e| e.credentials.priority)
            {
                *current_id = next.id;
                tracing::info!(
                    "已切换到账号 #{}（优先级 {}）",
                    next.id,
                    next.credentials.priority
                );
                true
            } else {
                tracing::error!("所有账号均已禁用！");
                false
            }
        };
        // 禁用原因是关键状态，跳过 30s 防抖立即落盘，避免进程重启丢失
        self.save_stats();
        result
    }

    /// 报告指定账号额度已用尽
    ///
    /// 用于处理 402 Payment Required 且 reason 为 `MONTHLY_REQUEST_COUNT` 的场景：
    /// - 立即禁用该账号（不等待连续失败阈值）
    /// - 切换到下一个可用账号继续重试
    /// - 返回是否还有可用账号
    pub fn report_quota_exhausted(&self, id: u64) -> bool {
        let result = {
            let mut entries = self.entries.lock();
            let mut current_id = self.current_id.lock();

            let entry = match entries.iter_mut().find(|e| e.id == id) {
                Some(e) => e,
                None => return entries.iter().any(|e| !e.disabled),
            };

            if entry.disabled {
                return entries.iter().any(|e| !e.disabled);
            }

            let now = Utc::now();
            entry.disabled = true;
            entry.disabled_reason = Some(DisabledReason::QuotaExceeded);
            entry.last_used_at = Some(now.to_rfc3339());
            entry.quota_exhausted_at = Some(now);
            // 设为阈值，便于在管理面板中直观看到该账号已不可用
            entry.failure_count = MAX_FAILURES_PER_CREDENTIAL;

            tracing::error!(
                "账号 #{} 额度已用尽（MONTHLY_REQUEST_COUNT），已被禁用（{} 后的自然月将自动恢复）",
                id,
                now.format("%Y-%m")
            );

            // 切换到优先级最高的可用账号
            if let Some(next) = entries
                .iter()
                .filter(|e| !e.disabled)
                .min_by_key(|e| e.credentials.priority)
            {
                *current_id = next.id;
                tracing::info!(
                    "已切换到账号 #{}（优先级 {}）",
                    next.id,
                    next.credentials.priority
                );
                true
            } else {
                tracing::error!("所有账号均已禁用！");
                false
            }
        };
        // 禁用原因是关键状态，跳过 30s 防抖立即落盘，避免进程重启丢失
        self.save_stats();
        result
    }

    /// 跨自然月后自动解除因额度用尽而禁用的账号
    ///
    /// Kiro 的 MONTHLY_REQUEST_COUNT 按自然月重置，因此只要当前月份不同于
    /// 被判定耗尽时的月份，就应重新放回可用池；若下个月仍无额度，
    /// 上游会再次返回 402 并重新禁用，代价仅一次请求。
    ///
    /// 返回被恢复的账号数量。
    pub(crate) fn recover_expired_quota_disables(&self) -> usize {
        let now = Utc::now();
        let now_year_month = (now.year(), now.month());
        let recovered = {
            let mut entries = self.entries.lock();
            let mut ids = Vec::new();
            for e in entries.iter_mut() {
                if !e.disabled || e.disabled_reason != Some(DisabledReason::QuotaExceeded) {
                    continue;
                }
                // 缺失时间戳（旧版本数据）视为可恢复，避免账号被永久钉死
                let same_month = e
                    .quota_exhausted_at
                    .is_some_and(|t| (t.year(), t.month()) == now_year_month);
                if same_month {
                    continue;
                }
                e.disabled = false;
                e.disabled_reason = None;
                e.quota_exhausted_at = None;
                e.failure_count = 0;
                ids.push(e.id);
            }
            ids
        };

        if !recovered.is_empty() {
            tracing::info!(
                "已跨自然月，自动恢复 {} 个额度耗尽的账号: {:?}",
                recovered.len(),
                recovered
            );
            if let Err(e) = self.persist_credentials() {
                tracing::warn!("恢复额度耗尽账号后持久化失败: {}", e);
            }
            self.save_stats();
        }
        recovered.len()
    }

    /// 生成"无可用账号"的诊断文案
    ///
    /// 直接读取 disabled_reason 而非仅 disabled，避免额度耗尽/连续失败/手动禁用
    /// 三种完全不同的原因在错误消息中塌缩成同一句"均已禁用"。
    /// `scope_ids` 为空表示全局账号池，否则为绑定的账号白名单。
    ///
    /// `model` 与 `select_next_credential` 保持一致的过滤逻辑：不支持该模型
    /// （如非付费订阅账号请求 opus）的账号本就不会被纳入候选，必须先排除，
    /// 否则"该模型专属账号全部额度耗尽、其余模型账号健康"时 quota 计数会被
    /// 无关账号稀释，导致 `QUOTA_EXHAUSTED_ALL_MARKER` 永远不会触发。
    pub(crate) fn describe_unavailable(&self, model: Option<&str>, scope_ids: &[u64]) -> String {
        let entries = self.entries.lock();
        let bound: Vec<&CredentialEntry> = entries
            .iter()
            .filter(|e| scope_ids.is_empty() || scope_ids.contains(&e.id))
            .collect();
        let scope_label = if scope_ids.is_empty() {
            "账号"
        } else {
            "绑定的账号"
        };

        let is_opus = model
            .map(|m| m.to_lowercase().contains("opus"))
            .unwrap_or(false);
        let model_mismatch = if is_opus {
            bound
                .iter()
                .filter(|e| !e.credentials.supports_opus())
                .count()
        } else {
            0
        };
        let in_scope: Vec<&CredentialEntry> = bound
            .iter()
            .filter(|e| !is_opus || e.credentials.supports_opus())
            .copied()
            .collect();
        let total = in_scope.len();

        if total == 0 {
            return format!(
                "{}中没有支持该模型的账号（共 {} 个）",
                scope_label,
                bound.len()
            );
        }

        let quota = in_scope
            .iter()
            .filter(|e| e.disabled_reason == Some(DisabledReason::QuotaExceeded))
            .count();
        let failures = in_scope
            .iter()
            .filter(|e| e.disabled_reason == Some(DisabledReason::TooManyFailures))
            .count();
        let profile_arn_missing = in_scope
            .iter()
            .filter(|e| e.disabled_reason == Some(DisabledReason::ProfileArnMissing))
            .count();
        let manual = in_scope
            .iter()
            .filter(|e| e.disabled_reason == Some(DisabledReason::Manual))
            .count();

        // 全部因额度耗尽而不可用：附带机器可识别标记，供上层映射为 402
        if quota == total {
            return format!(
                "{}{}（共 {} 个）[{}]",
                scope_label,
                DisabledReason::QuotaExceeded.describe(),
                total,
                QUOTA_EXHAUSTED_ALL_MARKER
            );
        }

        let mut parts = Vec::new();
        if quota > 0 {
            parts.push(format!("{} 个额度用尽", quota));
        }
        if failures > 0 {
            parts.push(format!("{} 个连续认证失败", failures));
        }
        if profile_arn_missing > 0 {
            parts.push(format!("{} 个缺少 profileArn", profile_arn_missing));
        }
        if manual > 0 {
            parts.push(format!("{} 个手动禁用", manual));
        }
        let others = total.saturating_sub(quota + failures + profile_arn_missing + manual);
        if others > 0 {
            parts.push(format!("{} 个无有效 Token", others));
        }
        if model_mismatch > 0 {
            parts.push(format!("{} 个不支持该模型", model_mismatch));
        }

        if parts.is_empty() {
            format!("{}均不可用（共 {} 个）", scope_label, total)
        } else {
            format!(
                "{}均不可用（共 {} 个：{}）",
                scope_label,
                total,
                parts.join("，")
            )
        }
    }
}
