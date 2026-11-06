//! MultiTokenManager 管理操作：手动切换账号、可用模型列表查询等面向 Admin API 的操作。
use anyhow::Context;

use super::super::entry::{CredentialEntry, DisabledReason};
use super::super::refresh::{
    UsageLimitsUnsupportedError, get_usage_limits, get_usage_limits_without_profile_arn,
    is_token_expired, is_token_expiring_soon, is_transient_usage_limits_error,
    list_available_models, list_available_profiles, pick_profile_arn, refresh_token, sha256_hex,
    validate_refresh_token,
};
use super::super::types::{
    CredentialEntrySnapshot, ManagerSnapshot, MultiTokenManager, TOKEN_REFRESH_COOLDOWN,
};
use crate::kiro::model::available_models::AvailableModelsResponse;
use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::model::credentials::canonicalize_auth_method_value;
use crate::kiro::model::usage_limits::UsageLimitsResponse;
use std::sync::atomic::Ordering;
use std::time::Instant;

impl MultiTokenManager {
    /// 切换到优先级最高的可用账号
    ///
    /// 返回是否成功切换
    pub fn switch_to_next(&self) -> bool {
        let entries = self.entries.lock();
        let mut current_id = self.current_id.lock();

        // 选择优先级最高的未禁用账号（排除当前账号）
        if let Some(next) = entries
            .iter()
            .filter(|e| !e.disabled && e.id != *current_id)
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
            // 没有其他可用账号，检查当前账号是否可用
            entries.iter().any(|e| e.id == *current_id && !e.disabled)
        }
    }

    /// 获取当前支持的模型列表（含官方费率倍率），取任意可用账号
    pub async fn list_available_models(&self) -> anyhow::Result<AvailableModelsResponse> {
        let ctx = self.acquire_context(None).await?;
        let effective_proxy = ctx.credentials.effective_proxy(self.proxy.as_ref());
        list_available_models(
            &ctx.credentials,
            &self.config,
            &ctx.token,
            effective_proxy.as_ref(),
        )
        .await
    }

    /// 获取指定账号支持的模型列表（含官方费率倍率）
    ///
    /// 与 list_available_models（取任意可用账号）不同，此方法按 id 指定账号查询，
    /// 用于 Admin API 展示单账号支持的模型。上游调用失败时直接返回错误，不回退静态表。
    pub async fn list_available_models_for(
        &self,
        id: u64,
    ) -> anyhow::Result<AvailableModelsResponse> {
        let (credentials, token) = self.acquire_token_for_id(id).await?;
        let effective_proxy = credentials.effective_proxy(self.proxy.as_ref());
        list_available_models(&credentials, &self.config, &token, effective_proxy.as_ref()).await
    }

    // ========================================================================
    // Admin API 方法
    // ========================================================================

    /// 获取管理器状态快照（用于 Admin API）
    pub fn snapshot(&self) -> ManagerSnapshot {
        let entries = self.entries.lock();
        let current_id = *self.current_id.lock();
        let available = entries.iter().filter(|e| !e.disabled).count();

        ManagerSnapshot {
            entries: entries
                .iter()
                .map(|e| CredentialEntrySnapshot {
                    id: e.id,
                    priority: e.credentials.priority,
                    disabled: e.disabled,
                    failure_count: e.failure_count,
                    auth_method: e
                        .credentials
                        .auth_method
                        .as_deref()
                        .map(|m| canonicalize_auth_method_value(m).to_string()),
                    has_profile_arn: e.credentials.profile_arn.is_some(),
                    client_id: e.credentials.client_id.clone(),
                    profile_arn: e.credentials.profile_arn.clone(),
                    machine_id: e.credentials.machine_id.clone(),
                    expires_at: e.credentials.expires_at.clone(),
                    refresh_token_hash: e.credentials.refresh_token.as_deref().map(sha256_hex),
                    email: e.credentials.email.clone(),
                    nickname: e.credentials.nickname.clone(),
                    provider: e.credentials.provider.clone(),
                    auth_region: e.credentials.auth_region.clone(),
                    api_region: e.credentials.api_region.clone(),
                    success_count: e.success_count,
                    last_used_at: e.last_used_at.clone(),
                    refresh_failure_count: e.refresh_failure_count,
                    has_proxy: e.credentials.proxy_url.is_some(),
                    proxy_url: e.credentials.proxy_url.clone(),
                    health_status: Self::compute_health(e),
                    throttle_count: e.throttle_count,
                    disabled_reason: e.disabled_reason,
                    thinking_adaptive: e.credentials.thinking_adaptive,
                    created_at: e.credentials.created_at.clone(),
                })
                .collect(),
            current_id,
            total: entries.len(),
            available,
        }
    }

    fn has_known_subscription(credentials: &KiroCredentials) -> bool {
        credentials
            .subscription_title
            .as_deref()
            .is_some_and(|title| !title.trim().is_empty())
    }

    fn update_changes_subscription_identity(
        update: &crate::admin::types::UpdateCredentialRequest,
    ) -> bool {
        update.refresh_token.is_some()
            || update.auth_method.is_some()
            || update.client_id.is_some()
            || update.client_secret.is_some()
            || update.profile_arn.is_some()
            || update.auth_region.is_some()
            || update.api_region.is_some()
            || update.machine_id.is_some()
    }

    /// 更新请求是否需要先用新凭据试刷新一次再写入。
    ///
    /// `effective` 是应用本次更新之后的账号（认证方式以更新后的为准）。除 `refreshToken` 外，
    /// `clientId` / `clientSecret` 设为非空新值也必须验证：它们写错后账号当前的 accessToken
    /// 仍然有效、面板上显示「健康」，要等 token 到期刷新才报 `invalid_client` 并累计失败次数
    /// 被禁用，故障很难与那次编辑联系起来（issue #58：浏览器把管理密码自动填进 Client Secret
    /// 框，保存后被当作 clientSecret 写入）。
    ///
    /// `clientSecret` 只在 IdC（OIDC）刷新里参与请求；external_idp 是公共客户端、social 不用
    /// 它，对这两类账号试刷新验证不到新值，只会空转，故仅对 IdC 触发。
    /// 空串表示清除，不涉及新凭据，不需要验证。
    pub(crate) fn update_requires_revalidation(
        update: &crate::admin::types::UpdateCredentialRequest,
        effective: &KiroCredentials,
    ) -> bool {
        let non_empty = |v: &Option<String>| v.as_deref().is_some_and(|s| !s.is_empty());
        update.refresh_token.is_some()
            || non_empty(&update.client_id)
            || (effective.is_idc() && non_empty(&update.client_secret))
    }

    /// 本次更新涉及的字段名（驼峰，与 API 字段一致），仅用于日志，不含任何字段值。
    ///
    /// 对 `UpdateCredentialRequest` 做穷尽解构（不写 `..`）：结构体新增字段而这里没同步时
    /// 编译直接报错，不会静默漏掉该字段的审计日志。
    pub(crate) fn updated_field_names(
        update: &crate::admin::types::UpdateCredentialRequest,
    ) -> Vec<&'static str> {
        let crate::admin::types::UpdateCredentialRequest {
            refresh_token,
            auth_method,
            client_id,
            client_secret,
            profile_arn,
            auth_region,
            api_region,
            machine_id,
            email,
            nickname,
            proxy_url,
            proxy_username,
            proxy_password,
            provider,
            thinking_adaptive,
        } = update;
        [
            ("refreshToken", refresh_token.is_some()),
            ("authMethod", auth_method.is_some()),
            ("clientId", client_id.is_some()),
            ("clientSecret", client_secret.is_some()),
            ("profileArn", profile_arn.is_some()),
            ("authRegion", auth_region.is_some()),
            ("apiRegion", api_region.is_some()),
            ("machineId", machine_id.is_some()),
            ("email", email.is_some()),
            ("nickname", nickname.is_some()),
            ("proxyUrl", proxy_url.is_some()),
            ("proxyUsername", proxy_username.is_some()),
            ("proxyPassword", proxy_password.is_some()),
            ("provider", provider.is_some()),
            ("thinkingAdaptive", thinking_adaptive.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, present)| present.then_some(name))
        .collect()
    }

    /// 比对「会影响上游返回内容」的身份字段。
    ///
    /// 不包含 `provider`：它除被 `fallback_profile_arn_value` 用于判定 Enterprise 备用 ARN 外，
    /// 既不参与 token 刷新，也不参与 API 请求，更不影响上游返回的订阅等级。若纳入比对，则
    /// 「仅改登录来源 / 补充 provider」的更新会让在途刷新/额度查询以「身份已变更」失败，
    /// 进而累计 `refresh_failure_count` 误禁用账号。
    fn has_same_subscription_identity(
        current: &KiroCredentials,
        queried: &KiroCredentials,
    ) -> bool {
        current.refresh_token == queried.refresh_token
            && current.auth_method == queried.auth_method
            && current.client_id == queried.client_id
            && current.client_secret == queried.client_secret
            && current.profile_arn == queried.profile_arn
            && current.token_endpoint == queried.token_endpoint
            && current.scopes == queried.scopes
            && current.region == queried.region
            && current.auth_region == queried.auth_region
            && current.api_region == queried.api_region
            && current.machine_id == queried.machine_id
    }

    pub(crate) fn commit_refreshed_credentials_if_identity_matches(
        &self,
        id: u64,
        refreshing_credentials: &KiroCredentials,
        refreshed_credentials: &KiroCredentials,
    ) -> anyhow::Result<KiroCredentials> {
        let mut entries = self.entries.lock();
        let entry = entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?;

        anyhow::ensure!(
            Self::has_same_subscription_identity(&entry.credentials, refreshing_credentials,),
            "账号 #{} 在 Token 刷新期间身份已变更，请重试",
            id
        );

        entry.credentials.access_token = refreshed_credentials.access_token.clone();
        entry.credentials.sso_access_token = refreshed_credentials.sso_access_token.clone();
        entry.credentials.refresh_token = refreshed_credentials.refresh_token.clone();
        entry.credentials.profile_arn = refreshed_credentials.profile_arn.clone();
        entry.credentials.expires_at = refreshed_credentials.expires_at.clone();
        entry.last_refreshed_at = Some(Instant::now());
        entry.refresh_failure_count = 0;

        Ok(entry.credentials.clone())
    }

    pub(crate) fn commit_subscription_title_if_identity_matches(
        &self,
        id: u64,
        queried_credentials: &KiroCredentials,
        subscription_title: Option<&str>,
    ) -> anyhow::Result<bool> {
        let mut entries = self.entries.lock();
        let entry = entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?;

        anyhow::ensure!(
            Self::has_same_subscription_identity(&entry.credentials, queried_credentials),
            "账号 #{} 在订阅查询期间身份已变更，请重试",
            id
        );

        if entry.credentials.subscription_title.as_deref() == subscription_title {
            return Ok(false);
        }

        let old_title = entry.credentials.subscription_title.clone();
        entry.credentials.subscription_title = subscription_title.map(str::to_string);
        tracing::info!(
            "账号 #{} 订阅等级已更新: {:?} -> {:?}",
            id,
            old_title,
            subscription_title
        );
        Ok(true)
    }

    /// 确保账号进入可用池前已获取订阅等级
    async fn ensure_subscription_before_enable(&self, id: u64) -> anyhow::Result<()> {
        let needs_lookup = {
            let entries = self.entries.lock();
            let entry = entries
                .iter()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?;
            !Self::has_known_subscription(&entry.credentials)
        };

        if !needs_lookup {
            return Ok(());
        }

        self.get_usage_limits_for(id)
            .await
            .with_context(|| format!("启用账号 #{} 前获取订阅/额度失败，账号保持禁用", id))?;

        let has_subscription = {
            let entries = self.entries.lock();
            let entry = entries
                .iter()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?;
            // 企业版 IdC 租户账号上游可能既无标题也无类型，凭真实 profileArn 视为已知
            Self::has_known_subscription(&entry.credentials) || entry.credentials.is_tenant_idc()
        };

        anyhow::ensure!(
            has_subscription,
            "账号 #{} 的订阅等级为空，账号保持禁用",
            id
        );
        Ok(())
    }

    /// 设置账号禁用状态（Admin API）
    pub async fn set_disabled(&self, id: u64, disabled: bool) -> anyhow::Result<()> {
        let _operation_guard = self.credential_admin_lock.lock().await;

        if !disabled {
            self.ensure_subscription_before_enable(id).await?;
        }

        {
            let mut entries = self.entries.lock();
            let entry = entries
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?;
            entry.disabled = disabled;
            if !disabled {
                // 启用时重置失败计数
                entry.failure_count = 0;
                entry.refresh_failure_count = 0;
                entry.disabled_reason = None;
                entry.quota_exhausted_at = None;
            } else {
                entry.disabled_reason = Some(DisabledReason::Manual);
                entry.quota_exhausted_at = None;
            }
        }
        // 持久化更改（stats 承载自动禁用原因，必须同步落盘避免重启后复活）
        self.persist_credentials()?;
        self.save_stats();
        Ok(())
    }

    /// 设置账号优先级（Admin API）
    ///
    /// 修改优先级后会立即按新优先级重新选择当前账号。
    /// 即使持久化失败，内存中的优先级和当前账号选择也会生效。
    pub fn set_priority(&self, id: u64, priority: u32) -> anyhow::Result<()> {
        {
            let mut entries = self.entries.lock();
            let entry = entries
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?;
            entry.credentials.priority = priority;
        }
        // 立即按新优先级重新选择当前账号（无论持久化是否成功）
        self.select_highest_priority();
        // 持久化更改
        self.persist_credentials()?;
        Ok(())
    }

    /// 重置账号失败计数并重新启用（Admin API）
    pub async fn reset_and_enable(&self, id: u64) -> anyhow::Result<()> {
        let _operation_guard = self.credential_admin_lock.lock().await;
        self.ensure_subscription_before_enable(id).await?;

        {
            let mut entries = self.entries.lock();
            let entry = entries
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?;
            entry.failure_count = 0;
            entry.refresh_failure_count = 0;
            entry.disabled = false;
            entry.disabled_reason = None;
            entry.quota_exhausted_at = None;
        }
        // 持久化更改（stats 承载自动禁用原因，必须同步落盘避免重启后复活）
        self.persist_credentials()?;
        self.save_stats();
        Ok(())
    }

    /// 按账号 id 取最新 credentials 与有效 access_token
    ///
    /// 封装 token 刷新逻辑（含冷却期、double-check、持久化），供 get_usage_limits_for
    /// 与 list_available_models_for 复用，消除按 id 查询时的刷新逻辑重复。
    async fn acquire_token_for_id(&self, id: u64) -> anyhow::Result<(KiroCredentials, String)> {
        let credentials = {
            let entries = self.entries.lock();
            entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.credentials.clone())
                .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?
        };

        // 检查是否需要刷新 token
        let needs_refresh = is_token_expired(&credentials) || is_token_expiring_soon(&credentials);

        let token = if needs_refresh {
            let _guard = self.refresh_lock.lock().await;
            let _identity_guard = self.credential_identity_lock.lock().await;
            let current_creds = {
                let entries = self.entries.lock();
                entries
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| e.credentials.clone())
                    .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?
            };

            if is_token_expired(&current_creds) || is_token_expiring_soon(&current_creds) {
                // 冷却期检查：仅对"即将过期"生效，已过期必须立即刷新
                let skip_for_cooldown = !is_token_expired(&current_creds) && {
                    let entries = self.entries.lock();
                    entries
                        .iter()
                        .find(|e| e.id == id)
                        .and_then(|e| e.last_refreshed_at)
                        .map(|t| t.elapsed() < TOKEN_REFRESH_COOLDOWN)
                        .unwrap_or(false)
                };
                if skip_for_cooldown {
                    tracing::debug!("Token 即将过期但在冷却期内（30s），跳过刷新");
                    current_creds
                        .access_token
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("冷却期内无 access_token"))?
                } else {
                    let effective_proxy = current_creds.effective_proxy(self.proxy.as_ref());
                    let refreshed_creds =
                        refresh_token(&current_creds, &self.config, effective_proxy.as_ref())
                            .await?;
                    let committed_creds = self.commit_refreshed_credentials_if_identity_matches(
                        id,
                        &current_creds,
                        &refreshed_creds,
                    )?;
                    // 持久化失败只记录警告，不影响本次请求
                    if let Err(e) = self.persist_credentials() {
                        tracing::warn!("Token 刷新后持久化失败（不影响本次请求）: {}", e);
                    }
                    committed_creds
                        .access_token
                        .ok_or_else(|| anyhow::anyhow!("刷新后无 access_token"))?
                }
            } else {
                current_creds
                    .access_token
                    .ok_or_else(|| anyhow::anyhow!("账号无 access_token"))?
            }
        } else {
            credentials
                .access_token
                .ok_or_else(|| anyhow::anyhow!("账号无 access_token"))?
        };

        // 返回最新 credentials（刷新后从 entries 重新取，保证 subscription_title 等字段最新）
        let credentials = {
            let entries = self.entries.lock();
            entries
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.credentials.clone())
                .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?
        };

        Ok((credentials, token))
    }

    /// 获取指定账号的使用额度（Admin API）
    pub async fn get_usage_limits_for(&self, id: u64) -> anyhow::Result<UsageLimitsResponse> {
        let (mut credentials, token) = self.acquire_token_for_id(id).await?;

        let effective_proxy = credentials.effective_proxy(self.proxy.as_ref());
        let usage_limits =
            match get_usage_limits(&credentials, &self.config, &token, effective_proxy.as_ref())
                .await
            {
                Ok(usage) => usage,
                // idc 账号没有真实 profileArn 时的自愈（issue #43）。
                // BuilderId 个人账号的“不支持额度查询”不是该问题，不触发。
                // 429 限流 / 5xx / 网络错误与 profileArn 无关，直接返回，避免自愈链路
                //（ListAvailableProfiles → 重试 → 不带 ARN 重试）在故障期间放大请求。
                Err(e)
                    if e.downcast_ref::<UsageLimitsUnsupportedError>().is_none()
                        && credentials.is_idc()
                        && credentials.has_no_real_profile_arn()
                        && !is_transient_usage_limits_error(&e) =>
                {
                    self.heal_and_retry_usage_limits(id, &mut credentials, &token, e)
                        .await?
                }
                Err(e) => return Err(e),
            };

        // 仅允许查询期间身份未变化的结果回写，防止旧凭据恢复过期订阅等级
        let subscription_title = usage_limits.resolved_subscription_title();
        let changed = self.commit_subscription_title_if_identity_matches(
            id,
            &credentials,
            subscription_title,
        )?;

        if changed && let Err(e) = self.persist_credentials() {
            tracing::warn!("订阅等级更新后持久化失败（不影响本次请求）: {}", e);
        }

        Ok(usage_limits)
    }

    /// 企业版 IdC 账号额度查询失败后的补救（issue #43），均借鉴 Kiro-account-manager：
    /// - 先用 `ListAvailableProfiles` 发现真实 ARN，写回后带它重试（对话也靠它）；
    /// - 仍不行则不带 profileArn 查询（KAM 对所有账号类型的做法）。
    ///
    /// 全部失败时返回 `original`（首次查询的错误）。
    ///
    /// 发现到新 ARN 时会更新调用方持有的 `credentials.profile_arn`：调用方随后的
    /// `commit_subscription_title_if_identity_matches` 通过 profile_arn 比较账号身份，
    /// 本地副本必须与已写回 entries 的值一致，否则订阅等级会因“身份已变更”而无法回写。
    async fn heal_and_retry_usage_limits(
        &self,
        id: u64,
        credentials: &mut KiroCredentials,
        token: &str,
        original: anyhow::Error,
    ) -> anyhow::Result<UsageLimitsResponse> {
        let Some(arn) = self
            .discover_and_commit_profile_arn(id, credentials, token)
            .await
        else {
            return self
                .usage_limits_without_arn(credentials, token, original)
                .await;
        };
        tracing::info!("账号 #{} 已自动发现真实 profileArn，重试额度查询", id);
        credentials.profile_arn = Some(arn);

        let proxy = credentials.effective_proxy(self.proxy.as_ref());
        match get_usage_limits(credentials, &self.config, token, proxy.as_ref()).await {
            Ok(usage) => Ok(usage),
            Err(e) => {
                tracing::warn!("带真实 profileArn 的额度查询仍失败: {}", e);
                self.usage_limits_without_arn(credentials, token, e).await
            }
        }
    }

    /// 发现与当前不同的真实 profileArn 并写回账号；仅在“发现且写回成功”时返回 `Some`。
    ///
    /// 发现只是尽力而为：写回失败（如账号在此期间被改动或删除）只记日志并返回 `None`，
    /// 让调用方降级，不中断整个额度查询。持久化失败同样只记日志（内存中已生效）。
    async fn discover_and_commit_profile_arn(
        &self,
        id: u64,
        credentials: &KiroCredentials,
        token: &str,
    ) -> Option<String> {
        let arn = self
            .discover_profile_arn(credentials, token)
            .await
            .filter(|arn| Some(arn.as_str()) != credentials.profile_arn.as_deref())?;

        if let Err(e) = self.commit_discovered_profile_arn(id, credentials, &arn) {
            tracing::warn!(
                "账号 #{} 写回自动发现的 profileArn 失败，降级为不带 ARN 查询: {}",
                id,
                e
            );
            return None;
        }
        if let Err(pe) = self.persist_credentials() {
            tracing::warn!("profileArn 自动发现后持久化失败（不影响本次请求）: {}", pe);
        }
        Some(arn)
    }

    /// 不带 profileArn 重试额度查询；仍失败时返回原始错误 `original`。
    async fn usage_limits_without_arn(
        &self,
        credentials: &KiroCredentials,
        token: &str,
        original: anyhow::Error,
    ) -> anyhow::Result<UsageLimitsResponse> {
        let proxy = credentials.effective_proxy(self.proxy.as_ref());
        match get_usage_limits_without_profile_arn(credentials, &self.config, token, proxy.as_ref())
            .await
        {
            Ok(usage) => {
                tracing::info!("不带 profileArn 的额度查询成功");
                Ok(usage)
            }
            Err(e) => {
                tracing::warn!("不带 profileArn 的额度查询也失败: {}", e);
                Err(original)
            }
        }
    }

    /// 用 `ListAvailableProfiles` 发现账号的真实 profileArn（尽力而为，失败只记日志）。
    ///
    /// 依次尝试 SSO portal token 与数据面 token（两者在 IdC 场景下不同，上游接受哪一个
    /// 未经真实企业账号验证）。返回与 API region 最匹配的 ARN。
    pub(crate) async fn discover_profile_arn(
        &self,
        credentials: &KiroCredentials,
        token: &str,
    ) -> Option<String> {
        // KAM 用 OIDC 的 accessToken（即本项目的 sso_access_token）调用；
        // 本项目数据面 token 为 idToken。两者都试，SSO token 优先。
        let mut tokens = Vec::with_capacity(2);
        if let Some(sso) = credentials.sso_access_token.as_deref()
            && !sso.is_empty()
        {
            tokens.push(sso);
        }
        if tokens.first() != Some(&token) {
            tokens.push(token);
        }

        let region = credentials.effective_api_region(&self.config);
        let proxy = credentials.effective_proxy(self.proxy.as_ref());
        for t in tokens {
            match list_available_profiles(credentials, &self.config, t, proxy.as_ref()).await {
                Ok(arns) => {
                    if let Some(arn) = pick_profile_arn(&arns, region) {
                        tracing::info!("已通过 ListAvailableProfiles 发现 profileArn");
                        return Some(arn);
                    }
                    tracing::debug!("ListAvailableProfiles 返回空列表");
                }
                Err(e) => tracing::warn!("ListAvailableProfiles 调用失败: {}", e),
            }
        }
        None
    }

    /// 写回自动发现的 profileArn；仅当账号身份在查询期间未变化时生效。
    pub(crate) fn commit_discovered_profile_arn(
        &self,
        id: u64,
        queried: &KiroCredentials,
        arn: &str,
    ) -> anyhow::Result<()> {
        let mut entries = self.entries.lock();
        let entry = entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?;
        anyhow::ensure!(
            Self::has_same_subscription_identity(&entry.credentials, queried),
            "账号 #{} 在 profileArn 发现期间身份已变更，请重试",
            id
        );
        entry.credentials.profile_arn = Some(arn.to_string());
        Ok(())
    }

    /// 添加新账号（Admin API）
    ///
    /// # 流程
    /// 1. 验证账号基本字段（refresh_token 不为空）
    /// 2. 基于 refreshToken 的 SHA-256 哈希检测重复
    /// 3. 尝试刷新 Token 验证账号有效性
    /// 4. 分配新 ID（当前最大 ID + 1）
    /// 5. 添加到 entries 列表
    /// 6. 持久化到配置文件
    ///
    /// # 返回
    /// - `Ok(u64)` - 新账号 ID
    /// - `Err(_)` - 验证失败或添加失败
    pub async fn add_credential(&self, new_cred: KiroCredentials) -> anyhow::Result<u64> {
        let _operation_guard = self.credential_admin_lock.lock().await;

        // 1. 基本验证
        validate_refresh_token(&new_cred)?;
        let initially_disabled = new_cred.disabled;

        // 2. 基于 refreshToken 的 SHA-256 哈希检测重复
        let new_refresh_token = new_cred
            .refresh_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("缺少 refreshToken"))?;
        let new_refresh_token_hash = sha256_hex(new_refresh_token);
        let duplicate_exists = {
            let entries = self.entries.lock();
            entries.iter().any(|entry| {
                entry
                    .credentials
                    .refresh_token
                    .as_deref()
                    .map(sha256_hex)
                    .as_deref()
                    == Some(new_refresh_token_hash.as_str())
            })
        };
        if duplicate_exists {
            anyhow::bail!("账号已存在（refreshToken 重复）");
        }

        // 3. 尝试刷新 Token 验证账号有效性
        let effective_proxy = new_cred.effective_proxy(self.proxy.as_ref());
        let mut validated_cred =
            refresh_token(&new_cred, &self.config, effective_proxy.as_ref()).await?;

        // 4. 分配新 ID
        //
        // 使用持久化的单调计数器而非"当前列表最大值 + 1"，避免账号删除后 ID 被复用，
        // 导致新账号在 usage/failure/throttle 日志中"继承"已删除旧账号的历史记录。
        let new_id = self.allocate_new_id();

        // 5. 设置 ID 并保留用户输入的元数据
        validated_cred.id = Some(new_id);
        // 用户显式填写的 profileArn 优先；否则保留刷新响应中自动获取到的值
        // （企业版 IdC 刷新通常不返回 profileArn，必须由用户手动提供）
        if new_cred.profile_arn.is_some() {
            validated_cred.profile_arn = new_cred.profile_arn;
        }
        validated_cred.priority = new_cred.priority;
        validated_cred.auth_method = new_cred
            .auth_method
            .map(|m| canonicalize_auth_method_value(&m).to_string());
        validated_cred.client_id = new_cred.client_id;
        validated_cred.client_secret = new_cred.client_secret;
        validated_cred.region = new_cred.region;
        validated_cred.auth_region = new_cred.auth_region;
        validated_cred.api_region = new_cred.api_region;
        validated_cred.machine_id = new_cred.machine_id;
        validated_cred.email = new_cred.email;
        validated_cred.nickname = new_cred.nickname;
        validated_cred.proxy_url = new_cred.proxy_url;
        validated_cred.proxy_username = new_cred.proxy_username;
        validated_cred.proxy_password = new_cred.proxy_password;
        // 添加时间：本函数是账号入库的唯一入口，在此统一写入
        validated_cred.created_at = Some(chrono::Utc::now().to_rfc3339());

        // idc 账号（BuilderId 与企业版共用 OIDC，刷新响应均不带 profileArn）：先尝试发现真实 ARN。
        // 企业版必须有租户自己的 ARN，BuilderId 占位符会被上游拒绝（issue #43）；
        // 发现失败（含 BuilderId 个人账号无 profile）时回退到下面的按类型补全，行为不变。
        if validated_cred.is_idc()
            && validated_cred.has_no_real_profile_arn()
            && let Some(token) = validated_cred.access_token.clone()
            && let Some(arn) = self.discover_profile_arn(&validated_cred, &token).await
        {
            validated_cred.profile_arn = Some(arn);
        }

        // 无 profile_arn 的 social/idc 账号按类型自动补全 fallback ARN 并随凭据持久化，
        // 后续对话、额度查询、订阅展示、模型列表均直接使用该持久化值
        // （external_idp/未知类型不补全，真实 ARN 因租户而异）
        if validated_cred.fill_missing_profile_arn() {
            tracing::info!(
                "账号无 profileArn，已按账号类型（{}）自动补全 fallback ARN",
                validated_cred.auth_method.as_deref().unwrap_or("unknown")
            );
        }

        {
            let mut entries = self.entries.lock();
            entries.push(CredentialEntry {
                id: new_id,
                credentials: validated_cred,
                failure_count: 0,
                refresh_failure_count: 0,
                disabled: initially_disabled,
                disabled_reason: initially_disabled.then_some(DisabledReason::Manual),
                success_count: 0,
                last_used_at: None,
                throttle_count: 0,
                last_throttled_at: None,
                last_throttled_wall: None,
                last_refreshed_at: None,
                rotation_bias: 0,
                quota_exhausted_at: None,
            });
        }

        // 6. 自动升级为多账号格式（添加账号后必须能持久化）
        if !self.is_multiple_format.load(Ordering::Relaxed) {
            self.is_multiple_format.store(true, Ordering::Relaxed);
            tracing::info!("已自动升级为多账号格式以支持持久化");
        }

        // 7. 持久化（失败不阻塞，账号已在内存中生效）
        match self.persist_credentials() {
            Ok(true) => tracing::info!(
                "账号 #{} 已持久化到文件（共 {} 个账号）",
                new_id,
                { self.entries.lock().len() }
            ),
            Ok(false) => tracing::warn!("账号 #{} 未持久化（非多账号格式或路径未设置）", new_id),
            Err(e) => tracing::error!("账号 #{} 持久化失败: {}", new_id, e),
        }

        tracing::info!("成功添加账号 #{}", new_id);
        Ok(new_id)
    }

    /// 更新账号配置（Admin API）
    ///
    /// 只更新提供的字段，不会触发 token 刷新验证（除非 refreshToken 变更，或 clientId 设为非空新值、IdC 账号的 clientSecret 设为非空新值；验证失败则不写入）
    pub async fn update_credential(
        &self,
        id: u64,
        update: crate::admin::types::UpdateCredentialRequest,
    ) -> anyhow::Result<()> {
        let _operation_guard = self.credential_admin_lock.lock().await;
        let _identity_guard = if Self::update_changes_subscription_identity(&update) {
            Some(self.credential_identity_lock.lock().await)
        } else {
            None
        };

        // 检查账号是否存在
        let exists = {
            let entries = self.entries.lock();
            entries.iter().any(|e| e.id == id)
        };
        if !exists {
            anyhow::bail!("账号不存在: {}", id);
        }
        // 应用本次更新后的账号预览：试刷新必须按「将要落库的配置」发起（认证方式、区域、
        // 代理等都以本次更新为准），否则同时换代理 / 区域时会拿旧配置得出与最终配置无关的结论
        let temp_cred = {
            let entries = self.entries.lock();
            let entry = entries.iter().find(|e| e.id == id).unwrap();
            let mut cred = entry.credentials.clone();
            if let Some(ref rt) = update.refresh_token {
                cred.refresh_token = Some(rt.clone());
            }
            Self::apply_update_fields(&mut cred, &update);
            cred
        };
        // refreshToken / clientId / IdC 的 clientSecret 变更，需要先试刷新验证，失败则不写入
        let needs_revalidation = Self::update_requires_revalidation(&update, &temp_cred);

        if needs_revalidation {
            let effective_proxy = temp_cred.effective_proxy(self.proxy.as_ref());
            let validated =
                refresh_token(&temp_cred, &self.config, effective_proxy.as_ref()).await?;

            // 更新账号（保留验证后的 access_token 和 expires_at）
            let mut entries = self.entries.lock();
            if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                entry.credentials.access_token = validated.access_token;
                entry.credentials.expires_at = validated.expires_at;
                if let Some(profile_arn) = validated.profile_arn {
                    entry.credentials.profile_arn = Some(profile_arn);
                }
                if let Some(rt) = validated.refresh_token {
                    entry.credentials.refresh_token = Some(rt);
                }
                // 应用用户更新的字段
                Self::apply_update_fields(&mut entry.credentials, &update);
                // 重置失败计数
                entry.failure_count = 0;
            }
        } else {
            // 不涉及需验证的凭据字段，直接更新配置字段
            let mut entries = self.entries.lock();
            if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                Self::apply_update_fields(&mut entry.credentials, &update);
            }
        }

        // 账号配置（如 profileArn、refreshToken）可能改变其模型权限，旧探测结果作废
        self.invalidate_model_support(id);

        self.persist_credentials()?;
        // 只记录字段名不记录值：clientSecret / refreshToken / proxyPassword 等是敏感信息，
        // 事后排查「是否有人改过这些字段」时靠这行日志
        tracing::info!(
            "成功更新账号 #{}，涉及字段: [{}]{}",
            id,
            Self::updated_field_names(&update).join(", "),
            if needs_revalidation {
                "（已用新凭据试刷新验证）"
            } else {
                ""
            }
        );
        Ok(())
    }

    /// 将 UpdateCredentialRequest 中的非 None 字段应用到账号
    pub(crate) fn apply_update_fields(
        cred: &mut KiroCredentials,
        update: &crate::admin::types::UpdateCredentialRequest,
    ) {
        let identity_updated = Self::update_changes_subscription_identity(update);

        if let Some(ref am) = update.auth_method {
            cred.auth_method = Some(canonicalize_auth_method_value(am).to_string());
        }
        if let Some(ref ci) = update.client_id {
            cred.client_id = if ci.is_empty() {
                None
            } else {
                Some(ci.clone())
            };
        }
        if let Some(ref pa) = update.profile_arn {
            cred.profile_arn = if pa.is_empty() {
                None
            } else {
                Some(pa.clone())
            };
        }
        if let Some(ref cs) = update.client_secret {
            cred.client_secret = if cs.is_empty() {
                None
            } else {
                Some(cs.clone())
            };
        }
        if let Some(ref ar) = update.auth_region {
            cred.auth_region = if ar.is_empty() {
                None
            } else {
                Some(ar.clone())
            };
        }
        if let Some(ref ar) = update.api_region {
            cred.api_region = if ar.is_empty() {
                None
            } else {
                Some(ar.clone())
            };
        }
        if let Some(ref mi) = update.machine_id {
            cred.machine_id = if mi.is_empty() {
                None
            } else {
                Some(mi.clone())
            };
        }
        if let Some(ref em) = update.email {
            cred.email = if em.is_empty() {
                None
            } else {
                Some(em.clone())
            };
        }
        if let Some(ref nn) = update.nickname {
            cred.nickname = if nn.is_empty() {
                None
            } else {
                Some(nn.clone())
            };
        }
        if let Some(ref pv) = update.provider {
            cred.provider = if pv.is_empty() {
                None
            } else {
                Some(pv.clone())
            };
        }
        if let Some(ref pu) = update.proxy_url {
            cred.proxy_url = if pu.is_empty() {
                None
            } else {
                Some(pu.clone())
            };
        }
        if let Some(ref pu) = update.proxy_username {
            cred.proxy_username = if pu.is_empty() {
                None
            } else {
                Some(pu.clone())
            };
        }
        if let Some(ref pp) = update.proxy_password {
            cred.proxy_password = if pp.is_empty() {
                None
            } else {
                Some(pp.clone())
            };
        }
        if let Some(ta) = update.thinking_adaptive {
            cred.thinking_adaptive = ta;
        }
        if identity_updated {
            cred.subscription_title = None;
        }
    }

    /// 删除账号（Admin API）
    ///
    /// # 行为
    /// 1. 验证账号存在
    /// 2. 从 entries 移除
    /// 3. 如果删除的是当前账号，切换到优先级最高的可用账号
    /// 4. 没有可用账号时，将 current_id 重置为 0
    /// 5. 持久化到文件
    ///
    /// # 返回
    /// - `Ok(())` - 删除成功
    /// - `Err(_)` - 账号不存在或持久化失败
    pub async fn delete_credential(&self, id: u64) -> anyhow::Result<()> {
        let _operation_guard = self.credential_admin_lock.lock().await;

        {
            let mut entries = self.entries.lock();
            let mut current_id = self.current_id.lock();
            let index = entries
                .iter()
                .position(|entry| entry.id == id)
                .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?;

            entries.remove(index);

            if *current_id == id {
                *current_id = entries
                    .iter()
                    .filter(|entry| !entry.disabled)
                    .min_by_key(|entry| entry.credentials.priority)
                    .map(|entry| entry.id)
                    .unwrap_or(0);
            }
        }

        self.invalidate_model_support(id);

        // 持久化更改
        self.persist_credentials()?;

        tracing::info!("已删除账号 #{}", id);
        Ok(())
    }

    /// 获取负载均衡模式（Admin API）
    pub fn get_load_balancing_mode(&self) -> String {
        self.load_balancing_mode.lock().clone()
    }

    fn persist_load_balancing_mode(&self, mode: &str) -> anyhow::Result<()> {
        use anyhow::Context;

        let config_path = match self.config.config_path() {
            Some(path) => path.to_path_buf(),
            None => {
                tracing::warn!("配置文件路径未知，负载均衡模式仅在当前进程生效: {}", mode);
                return Ok(());
            }
        };

        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("读取配置文件失败: {}", config_path.display()))?;
        let mut json: serde_json::Value = serde_json::from_str(&content)
            .with_context(|| format!("解析配置文件失败: {}", config_path.display()))?;
        json["loadBalancingMode"] = serde_json::Value::String(mode.to_string());
        let output = serde_json::to_string_pretty(&json)?;
        std::fs::write(&config_path, output)
            .with_context(|| format!("持久化负载均衡模式失败: {}", config_path.display()))?;

        Ok(())
    }

    /// 设置负载均衡模式（Admin API）
    pub fn set_load_balancing_mode(&self, mode: String) -> anyhow::Result<()> {
        // 验证模式值
        if mode != "priority" && mode != "balanced" {
            anyhow::bail!("无效的负载均衡模式: {}", mode);
        }

        let previous_mode = self.get_load_balancing_mode();
        if previous_mode == mode {
            return Ok(());
        }

        *self.load_balancing_mode.lock() = mode.clone();

        if let Err(err) = self.persist_load_balancing_mode(&mode) {
            tracing::warn!("负载均衡模式持久化失败，仅当前进程生效: {}", err);
        }

        tracing::info!("负载均衡模式已设置为: {}", mode);
        Ok(())
    }

    /// 测试辅助：向 sticky_cache 写入一条已过期的条目（模拟 TTL 已超出）
    #[cfg(test)]
    pub(crate) fn insert_expired_sticky_entry(&self, key: &str, credential_id: u64) {
        use super::super::types::{STICKY_CACHE_TTL, StickyCacheEntry};
        use std::time::Duration as StdDuration;

        let mut cache = self.sticky_cache.lock();
        cache.insert(
            key.to_string(),
            StickyCacheEntry {
                credential_id,
                inserted_at: Instant::now() - STICKY_CACHE_TTL - StdDuration::from_secs(1),
                consecutive_throttles: 0,
            },
        );
    }
}

impl Drop for MultiTokenManager {
    fn drop(&mut self) {
        if self.stats_dirty.load(Ordering::Relaxed) {
            self.save_stats();
        }
    }
}
