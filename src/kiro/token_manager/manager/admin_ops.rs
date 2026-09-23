use super::super::entry::{CredentialEntry, DisabledReason};
use super::super::refresh::{
    get_usage_limits, is_token_expired, is_token_expiring_soon, list_available_models,
    refresh_token, sha256_hex, validate_refresh_token,
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

    /// 获取使用额度信息
    #[allow(dead_code)]
    pub async fn get_usage_limits(&self) -> anyhow::Result<UsageLimitsResponse> {
        let ctx = self.acquire_context(None).await?;
        let effective_proxy = ctx.credentials.effective_proxy(self.proxy.as_ref());
        get_usage_limits(
            &ctx.credentials,
            &self.config,
            &ctx.token,
            effective_proxy.as_ref(),
        )
        .await
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
                    expires_at: e.credentials.expires_at.clone(),
                    refresh_token_hash: e.credentials.refresh_token.as_deref().map(sha256_hex),
                    email: e.credentials.email.clone(),
                    nickname: e.credentials.nickname.clone(),
                    success_count: e.success_count,
                    last_used_at: e.last_used_at.clone(),
                    refresh_failure_count: e.refresh_failure_count,
                    has_proxy: e.credentials.proxy_url.is_some(),
                    proxy_url: e.credentials.proxy_url.clone(),
                    health_status: Self::compute_health(e),
                    throttle_count: e.throttle_count,
                    disabled_reason: e.disabled_reason,
                    thinking_adaptive: e.credentials.thinking_adaptive,
                })
                .collect(),
            current_id,
            total: entries.len(),
            available,
        }
    }

    /// 设置账号禁用状态（Admin API）
    pub fn set_disabled(&self, id: u64, disabled: bool) -> anyhow::Result<()> {
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
    pub fn reset_and_enable(&self, id: u64) -> anyhow::Result<()> {
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
                    let new_creds =
                        refresh_token(&current_creds, &self.config, effective_proxy.as_ref())
                            .await?;
                    {
                        let mut entries = self.entries.lock();
                        if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                            entry.credentials = new_creds.clone();
                            entry.last_refreshed_at = Some(Instant::now());
                        }
                    }
                    // 持久化失败只记录警告，不影响本次请求
                    if let Err(e) = self.persist_credentials() {
                        tracing::warn!("Token 刷新后持久化失败（不影响本次请求）: {}", e);
                    }
                    new_creds
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
        let (credentials, token) = self.acquire_token_for_id(id).await?;

        let effective_proxy = credentials.effective_proxy(self.proxy.as_ref());
        let usage_limits =
            get_usage_limits(&credentials, &self.config, &token, effective_proxy.as_ref()).await?;

        // 更新订阅等级到账号（仅在发生变化时持久化）
        if let Some(subscription_title) = usage_limits.subscription_title() {
            let changed = {
                let mut entries = self.entries.lock();
                if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                    let old_title = entry.credentials.subscription_title.clone();
                    if old_title.as_deref() != Some(subscription_title) {
                        entry.credentials.subscription_title = Some(subscription_title.to_string());
                        tracing::info!(
                            "账号 #{} 订阅等级已更新: {:?} -> {}",
                            id,
                            old_title,
                            subscription_title
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            if changed && let Err(e) = self.persist_credentials() {
                tracing::warn!("订阅等级更新后持久化失败（不影响本次请求）: {}", e);
            }
        }

        Ok(usage_limits)
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
        // 1. 基本验证
        validate_refresh_token(&new_cred)?;

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
                disabled: false,
                disabled_reason: None,
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
    /// 只更新提供的字段，不会触发 token 刷新验证（除非 refreshToken 变更）
    pub async fn update_credential(
        &self,
        id: u64,
        update: crate::admin::types::UpdateCredentialRequest,
    ) -> anyhow::Result<()> {
        // 检查账号是否存在
        let exists = {
            let entries = self.entries.lock();
            entries.iter().any(|e| e.id == id)
        };
        if !exists {
            anyhow::bail!("账号不存在: {}", id);
        }

        // 如果 refreshToken 变更，需要重新验证
        let needs_revalidation = update.refresh_token.is_some();

        if needs_revalidation {
            // 先构建临时账号用于验证
            let temp_cred = {
                let entries = self.entries.lock();
                let entry = entries.iter().find(|e| e.id == id).unwrap();
                let mut cred = entry.credentials.clone();
                if let Some(ref rt) = update.refresh_token {
                    cred.refresh_token = Some(rt.clone());
                }
                if let Some(ref am) = update.auth_method {
                    cred.auth_method = Some(am.clone());
                }
                if let Some(ref ci) = update.client_id {
                    cred.client_id = Some(ci.clone());
                }
                if let Some(ref cs) = update.client_secret {
                    cred.client_secret = Some(cs.clone());
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
                cred
            };

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
            // 不涉及 refreshToken 变更，直接更新配置字段
            let mut entries = self.entries.lock();
            if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                Self::apply_update_fields(&mut entry.credentials, &update);
            }
        }

        self.persist_credentials()?;
        tracing::info!("成功更新账号 #{}", id);
        Ok(())
    }

    /// 将 UpdateCredentialRequest 中的非 None 字段应用到账号
    pub(crate) fn apply_update_fields(
        cred: &mut KiroCredentials,
        update: &crate::admin::types::UpdateCredentialRequest,
    ) {
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
    }

    /// 删除账号（Admin API）
    ///
    /// # 前置条件
    /// - 账号必须已禁用（disabled = true）
    ///
    /// # 行为
    /// 1. 验证账号存在
    /// 2. 验证账号已禁用
    /// 3. 从 entries 移除
    /// 4. 如果删除的是当前账号，切换到优先级最高的可用账号
    /// 5. 如果删除后没有账号，将 current_id 重置为 0
    /// 6. 持久化到文件
    ///
    /// # 返回
    /// - `Ok(())` - 删除成功
    /// - `Err(_)` - 账号不存在、未禁用或持久化失败
    pub fn delete_credential(&self, id: u64) -> anyhow::Result<()> {
        let was_current = {
            let mut entries = self.entries.lock();

            // 查找账号
            let entry = entries
                .iter()
                .find(|e| e.id == id)
                .ok_or_else(|| anyhow::anyhow!("账号不存在: {}", id))?;

            // 检查是否已禁用
            if !entry.disabled {
                anyhow::bail!("只能删除已禁用的账号（请先禁用账号 #{}）", id);
            }

            // 记录是否是当前账号
            let current_id = *self.current_id.lock();
            let was_current = current_id == id;

            // 删除账号
            entries.retain(|e| e.id != id);

            was_current
        };

        // 如果删除的是当前账号，切换到优先级最高的可用账号
        if was_current {
            self.select_highest_priority();
        }

        // 如果删除后没有任何账号，将 current_id 重置为 0（与初始化行为保持一致）
        {
            let entries = self.entries.lock();
            if entries.is_empty() {
                let mut current_id = self.current_id.lock();
                *current_id = 0;
                tracing::info!("所有账号已删除，current_id 已重置为 0");
            }
        }

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
