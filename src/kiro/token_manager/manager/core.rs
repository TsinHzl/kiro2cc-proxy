use super::super::entry::{CredentialEntry, DisabledReason, HealthStatus};
use super::super::refresh::{
    RefreshTokenInvalidError, is_token_expired, is_token_expiring_soon, refresh_token,
};
use super::super::types::{
    CallContext, MultiTokenManager, STICKY_CACHE_TTL, STICKY_THROTTLE_EVICT_THRESHOLD,
    StickyCacheEntry, TOKEN_REFRESH_COOLDOWN,
};

use crate::http_client::ProxyConfig;
use crate::kiro::machine_id;
use crate::kiro::model::credentials::KiroCredentials;
use crate::model::config::Config;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
use tokio::sync::Mutex as TokioMutex;

impl MultiTokenManager {
    /// 创建多账号 Token 管理器
    ///
    /// # Arguments
    /// * `config` - 应用配置
    /// * `credentials` - 账号列表
    /// * `proxy` - 可选的代理配置
    /// * `credentials_path` - 账号文件路径（用于回写）
    /// * `is_multiple_format` - 是否为多账号格式（决定回写 JSON 形状：数组 vs 单对象，不再影响是否回写）
    pub fn new(
        config: Config,
        credentials: Vec<KiroCredentials>,
        proxy: Option<ProxyConfig>,
        credentials_path: Option<PathBuf>,
        is_multiple_format: bool,
    ) -> anyhow::Result<Self> {
        // 计算当前最大 ID，为没有 ID 的账号分配新 ID
        //
        // 注意：不能只看当前 credentials 列表的最大 ID —— 账号删除后其 ID 会从列表中消失，
        // 若后续新增账号仅按“当前列表最大值 + 1”分配，会复用已删除账号曾用过的 ID，
        // 导致新账号继承该 ID 下遗留的历史用量/失败/限流日志（表现为“从未使用的新账号却有历史请求记录”）。
        // 因此需要额外加载持久化的历史最大 ID 计数器，取二者较大值，确保 ID 只增不减、永不复用。
        let max_existing_id = credentials.iter().filter_map(|c| c.id).max().unwrap_or(0);
        let persisted_max_id = Self::load_id_counter_from_path(credentials_path.as_deref());
        let starting_max_id = max_existing_id.max(persisted_max_id);
        // checked_add 防御：ID 计数器逼近 u64::MAX 时报错而非静默回绕（cr-result C7）
        let mut next_id = starting_max_id
            .checked_add(1)
            .expect("credential id 计数器溢出（u64::MAX），credentials.json 异常");
        let mut has_new_ids = false;
        let mut has_new_machine_ids = false;
        let mut has_new_profile_arns = false;
        let config_ref = &config;

        let entries: Vec<CredentialEntry> = credentials
            .into_iter()
            .map(|mut cred| {
                cred.canonicalize_auth_method();
                let id = cred.id.unwrap_or_else(|| {
                    let id = next_id;
                    next_id = next_id
                        .checked_add(1)
                        .expect("credential id 计数器溢出（u64::MAX），请检查 credentials.json 中的 id 字段");
                    cred.id = Some(id);
                    has_new_ids = true;
                    id
                });
                if cred.machine_id.is_none() {
                    cred.machine_id =
                        Some(machine_id::generate_from_credentials(&cred, config_ref));
                    has_new_machine_ids = true;
                }
                // 存量账号补全：旧版本添加的 social/idc 账号可能没有 profileArn，
                // 启动加载时按账号类型补全并标记写回配置文件
                if cred.fill_missing_profile_arn() {
                    has_new_profile_arns = true;
                }
                CredentialEntry {
                    id,
                    credentials: cred.clone(),
                    failure_count: 0,
                    refresh_failure_count: 0,
                    disabled: cred.disabled, // 从配置文件读取 disabled 状态
                    // 暂定 Manual；load_stats() 会用持久化的真实原因覆盖
                    // （额度耗尽/连续失败也会被 persist_credentials 写成 disabled: true，
                    //   若在此直接认定 Manual，自愈逻辑将永远跳过它们）
                    disabled_reason: if cred.disabled {
                        Some(DisabledReason::Manual)
                    } else {
                        None
                    },
                    success_count: 0,
                    last_used_at: None,
                    throttle_count: 0,
                    last_throttled_at: None,
                    last_throttled_wall: None,
                    last_refreshed_at: None,
                    rotation_bias: 0,
                    quota_exhausted_at: None,
                }
            })
            .collect();

        // 检测重复 ID
        let mut seen_ids = std::collections::HashSet::new();
        let mut duplicate_ids = Vec::new();
        for entry in &entries {
            if !seen_ids.insert(entry.id) {
                duplicate_ids.push(entry.id);
            }
        }
        if !duplicate_ids.is_empty() {
            anyhow::bail!("检测到重复的账号 ID: {:?}", duplicate_ids);
        }

        // 选择初始账号：优先级最高（priority 最小）的账号，无账号时为 0
        let initial_id = entries
            .iter()
            .min_by_key(|e| e.credentials.priority)
            .map(|e| e.id)
            .unwrap_or(0);

        // 历史最大 ID = 本次结束后 entries 中的最大值 与 持久化计数器 的较大值。
        // 二者缺一不可：entries 为空时需兜底 starting_max_id；entries 非空但其账号
        // 均携带小于 persisted_max_id 的显式 id 时（如本次重启后仅剩 #1，但磁盘计数器
        // 已因此前存在过 #2 而记录为 2），entries.max() 本身小于 starting_max_id，
        // 仍需与其取较大值，否则会丢失磁盘上记录的历史高位 ID，导致 ID 复用。
        let final_max_id = entries
            .iter()
            .map(|e| e.id)
            .max()
            .unwrap_or(0)
            .max(starting_max_id);

        let load_balancing_mode = config.load_balancing_mode.clone();
        let manager = Self {
            config,
            proxy,
            entries: Mutex::new(entries),
            current_id: Mutex::new(initial_id),
            refresh_lock: TokioMutex::new(()),
            credentials_path,
            is_multiple_format: AtomicBool::new(is_multiple_format),
            load_balancing_mode: Mutex::new(load_balancing_mode),
            last_stats_save_at: Mutex::new(None),
            stats_dirty: AtomicBool::new(false),
            rr_counter: AtomicU64::new(0),
            sticky_cache: Mutex::new(HashMap::new()),
            sticky_hits: AtomicU64::new(0),
            sticky_misses: AtomicU64::new(0),
            persist_lock: Mutex::new(()),
            next_id_counter: AtomicU64::new(final_max_id),
        };

        // 持久化历史最大 ID 计数器（即使本次没有新增账号，也要确保计数器文件与内存一致，
        // 避免文件丢失/首次运行时缺失导致回退到仅按当前列表推算）
        manager.save_id_counter_at_least(final_max_id);

        // 如果有新分配的 ID、新生成的 machineId 或补全的 profileArn，立即持久化到配置文件
        if has_new_ids || has_new_machine_ids || has_new_profile_arns {
            if let Err(e) = manager.persist_credentials() {
                tracing::warn!("补全账号 ID/machineId/profileArn 后持久化失败: {}", e);
            } else {
                tracing::info!("已补全账号 ID/machineId/profileArn 并写回配置文件");
            }
        }

        // 加载持久化的统计数据（success_count, last_used_at, disabled_reason）
        manager.load_stats();

        // 启动即检查：跨自然月后自动恢复因额度用尽被禁用的账号
        manager.recover_expired_quota_disables();

        Ok(manager)
    }

    /// 获取配置的引用
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// 获取当前活动账号的克隆（仅测试使用）
    #[cfg(test)]
    pub fn credentials(&self) -> KiroCredentials {
        let entries = self.entries.lock();
        let current_id = *self.current_id.lock();
        entries
            .iter()
            .find(|e| e.id == current_id)
            .map(|e| e.credentials.clone())
            .unwrap_or_default()
    }

    /// 获取账号总数
    pub fn total_count(&self) -> usize {
        self.entries.lock().len()
    }

    /// 获取可用账号数量
    pub fn available_count(&self) -> usize {
        self.entries.lock().iter().filter(|e| !e.disabled).count()
    }

    /// 返回当前未禁用账号的 id 列表
    pub fn credential_ids(&self) -> Vec<u64> {
        self.entries
            .lock()
            .iter()
            .filter(|e| !e.disabled)
            .map(|e| e.id)
            .collect()
    }

    /// 返回 (sticky_hits, sticky_misses) 累计计数
    pub fn sticky_metrics(&self) -> (u64, u64) {
        (
            self.sticky_hits.load(Ordering::Relaxed),
            self.sticky_misses.load(Ordering::Relaxed),
        )
    }

    /// 根据负载均衡模式选择下一个账号
    ///
    /// - priority 模式：选择优先级最高（priority 最小）的可用账号
    /// - balanced 模式：轮询选择可用账号
    ///
    /// # 参数
    /// - `model`: 可选的模型名称，用于过滤支持该模型的账号（如 opus 模型需要付费订阅）
    fn select_next_credential(
        &self,
        model: Option<&str>,
        allowed_ids: &[u64],
    ) -> Option<(u64, KiroCredentials)> {
        let entries = self.entries.lock();

        // 检查是否是 opus 模型
        let is_opus = model
            .map(|m| m.to_lowercase().contains("opus"))
            .unwrap_or(false);

        // 过滤可用账号
        let available: Vec<_> = entries
            .iter()
            .filter(|e| {
                if e.disabled {
                    return false;
                }
                // 账号 ID 白名单过滤（空列表表示不限制）
                if !allowed_ids.is_empty() && !allowed_ids.contains(&e.id) {
                    return false;
                }
                // 如果是 opus 模型，需要检查订阅等级
                if is_opus && !e.credentials.supports_opus() {
                    return false;
                }
                true
            })
            .collect();

        if available.is_empty() {
            return None;
        }

        // 优先选择健康状态不为 Unhealthy 的账号；全部不健康时才 fallback 避免完全不可用
        let preferred: Vec<_> = available
            .iter()
            .filter(|e| Self::compute_health(e) != HealthStatus::Unhealthy)
            .copied()
            .collect();
        let pool: &[&CredentialEntry] = if preferred.is_empty() {
            &available
        } else {
            &preferred
        };

        let mode = self.load_balancing_mode.lock().clone();
        let mode = mode.as_str();

        match mode {
            "balanced" => {
                // Round-Robin + rotation_bias：优先选 bias 最小的子集，再 round-robin
                let min_bias = pool.iter().map(|e| e.rotation_bias).min().unwrap_or(0);
                let low_bias: Vec<&CredentialEntry> = pool
                    .iter()
                    .filter(|e| e.rotation_bias == min_bias)
                    .copied()
                    .collect();
                let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed) as usize;
                let entry = low_bias[idx % low_bias.len()];
                Some((entry.id, entry.credentials.clone()))
            }
            _ => {
                // priority 模式：同优先级内按 rotation_bias 排序后 round-robin
                let min_priority = pool.iter().map(|e| e.credentials.priority).min()?;
                let top_tier: Vec<&CredentialEntry> = pool
                    .iter()
                    .filter(|e| e.credentials.priority == min_priority)
                    .copied()
                    .collect();
                if top_tier.len() == 1 {
                    Some((top_tier[0].id, top_tier[0].credentials.clone()))
                } else {
                    let min_bias = top_tier.iter().map(|e| e.rotation_bias).min().unwrap_or(0);
                    let low_bias: Vec<&CredentialEntry> = top_tier
                        .iter()
                        .filter(|e| e.rotation_bias == min_bias)
                        .copied()
                        .collect();
                    let idx = self.rr_counter.fetch_add(1, Ordering::Relaxed) as usize;
                    let entry = low_bias[idx % low_bias.len()];
                    Some((entry.id, entry.credentials.clone()))
                }
            }
        }
    }

    /// 获取 API 调用上下文
    ///
    /// 返回绑定了 id、credentials 和 token 的调用上下文
    /// 确保整个 API 调用过程中使用一致的账号信息
    ///
    /// 如果 Token 过期或即将过期，会自动刷新
    /// Token 刷新失败时会尝试下一个可用账号（不计入失败次数）
    ///
    /// # 参数
    /// - `model`: 可选的模型名称，用于过滤支持该模型的账号（如 opus 模型需要付费订阅）
    pub async fn acquire_context(&self, model: Option<&str>) -> anyhow::Result<CallContext> {
        // 跨自然月后，额度已重置，先把此前判定耗尽的账号放回可用池
        self.recover_expired_quota_disables();

        let total = self.total_count();
        let mut tried_count = 0;

        loop {
            if tried_count >= total {
                anyhow::bail!(
                    "所有账号均无法获取有效 Token（可用: {}/{}）",
                    self.available_count(),
                    total
                );
            }

            let (id, credentials) = {
                let is_balanced = self.load_balancing_mode.lock().as_str() == "balanced";

                // balanced 模式：每次请求都轮询选择，不固定 current_id
                // priority 模式：优先使用 current_id 指向的账号
                let current_hit = if is_balanced {
                    None
                } else {
                    let entries = self.entries.lock();
                    let current_id = *self.current_id.lock();
                    entries
                        .iter()
                        .find(|e| {
                            e.id == current_id
                                && !e.disabled
                                && Self::compute_health(e) != HealthStatus::Unhealthy
                        })
                        .map(|e| (e.id, e.credentials.clone()))
                };

                if let Some(hit) = current_hit {
                    hit
                } else {
                    // 当前账号不可用或 balanced 模式，根据负载均衡策略选择
                    let mut best = self.select_next_credential(model, &[]);

                    // 没有可用账号：如果是"自动禁用导致全灭"，做一次类似重启的自愈
                    if best.is_none() {
                        let mut entries = self.entries.lock();
                        if entries.iter().any(|e| {
                            e.disabled
                                && matches!(
                                    e.disabled_reason,
                                    Some(DisabledReason::TooManyFailures)
                                        | Some(DisabledReason::TooManyRefreshFailures)
                                )
                        }) {
                            tracing::warn!(
                                "所有账号均已被自动禁用，执行自愈：重置失败计数并重新启用（等价于重启）"
                            );
                            for e in entries.iter_mut() {
                                match e.disabled_reason {
                                    Some(DisabledReason::TooManyFailures) => {
                                        e.disabled = false;
                                        e.disabled_reason = None;
                                        e.failure_count = 0;
                                    }
                                    Some(DisabledReason::TooManyRefreshFailures) => {
                                        e.disabled = false;
                                        e.disabled_reason = None;
                                        e.refresh_failure_count = 0;
                                    }
                                    _ => {}
                                }
                            }
                            drop(entries);
                            // 落盘清除的禁用原因，否则重启后 load_stats 会让禁用态复活
                            self.save_stats();
                            best = self.select_next_credential(model, &[]);
                        }
                    }

                    if let Some((new_id, new_creds)) = best {
                        // 更新 current_id
                        let mut current_id = self.current_id.lock();
                        *current_id = new_id;
                        (new_id, new_creds)
                    } else {
                        // describe_unavailable 内部会获取 entries 锁，
                        // 因此必须在任何 entries 锁作用域之外调用，否则死锁
                        anyhow::bail!("{}", self.describe_unavailable(model, &[]));
                    }
                }
            };

            // 尝试获取/刷新 Token
            match self.try_ensure_token(id, &credentials).await {
                Ok(ctx) => {
                    return Ok(ctx);
                }
                Err(e) => {
                    tracing::warn!("账号 #{} Token 刷新失败，尝试下一个账号: {}", id, e);

                    if e.downcast_ref::<RefreshTokenInvalidError>().is_some() {
                        self.report_refresh_token_invalid(id);
                    } else {
                        self.report_refresh_failure(id);
                    }

                    // 切换到下一个优先级的账号
                    self.switch_to_next_by_priority();
                    tried_count += 1;
                }
            }
        }
    }

    /// 带账号 ID 白名单的调用上下文获取
    ///
    /// 与 acquire_context 逻辑相同，但只在 allowed_ids 指定的账号中选择。
    /// 白名单内所有账号均不可用时直接返回错误，不回退到全局池。
    pub async fn acquire_context_filtered(
        &self,
        model: Option<&str>,
        allowed_ids: &[u64],
    ) -> anyhow::Result<CallContext> {
        if allowed_ids.is_empty() {
            return self.acquire_context(model).await;
        }

        // 跨自然月后，额度已重置，先把此前判定耗尽的账号放回可用池
        self.recover_expired_quota_disables();

        let mut tried_ids: Vec<u64> = Vec::new();

        loop {
            if tried_ids.len() >= allowed_ids.len() {
                anyhow::bail!("{}", self.describe_unavailable(model, allowed_ids));
            }

            // 从白名单中排除已尝试过的账号
            let effective_ids: Vec<u64> = allowed_ids
                .iter()
                .filter(|id| !tried_ids.contains(id))
                .copied()
                .collect();

            let (id, credentials) = {
                match self.select_next_credential(model, &effective_ids) {
                    Some((new_id, new_creds)) => (new_id, new_creds),
                    None => {
                        anyhow::bail!("{}", self.describe_unavailable(model, allowed_ids));
                    }
                }
            };

            match self.try_ensure_token(id, &credentials).await {
                Ok(ctx) => return Ok(ctx),
                Err(e) => {
                    tracing::warn!("绑定账号 #{} Token 刷新失败，尝试下一个: {}", id, e);

                    if e.downcast_ref::<RefreshTokenInvalidError>().is_some() {
                        self.report_refresh_token_invalid(id);
                    } else {
                        self.report_refresh_failure(id);
                    }

                    tried_ids.push(id);
                }
            }
        }
    }

    /// 在排除 `avoid_ids` 的前提下选账号
    ///
    /// 用于同一请求内的重试：绑定账号刚被限流时换个账号完成本次调用。
    /// 排除后无账号可用时回退到不排除的选择逻辑，保证不会因避让而彻底失败。
    async fn acquire_context_avoiding(
        &self,
        model: Option<&str>,
        allowed_ids: &[u64],
        avoid_ids: &[u64],
    ) -> anyhow::Result<CallContext> {
        let base_ids: Vec<u64> = if allowed_ids.is_empty() {
            self.entries.lock().iter().map(|e| e.id).collect()
        } else {
            allowed_ids.to_vec()
        };
        let remaining: Vec<u64> = base_ids
            .iter()
            .filter(|id| !avoid_ids.contains(id))
            .copied()
            .collect();

        if remaining.is_empty() {
            // 所有候选账号都已限流：回到原逻辑，由上层重试与退避处理
            return self.acquire_context_filtered(model, allowed_ids).await;
        }

        match self.acquire_context_filtered(model, &remaining).await {
            Ok(ctx) => Ok(ctx),
            Err(_) => self.acquire_context_filtered(model, allowed_ids).await,
        }
    }

    /// 基于 agentContinuationId 的 sticky 路由
    ///
    /// 同一会话优先路由到缓存中的同一账号，保证 Kiro prompt cache 命中率。
    /// 缓存条目 TTL 60 分钟（每次命中续期），不健康时自动驱逐并重选。
    ///
    /// `avoid_ids` 是本次请求内已经限流过的账号：绑定命中这些账号时跳过复用改选其它
    /// 账号，但**不删除绑定关系**，下一次请求仍可回到原账号继续命中 prompt cache。
    pub async fn acquire_context_sticky(
        &self,
        model: Option<&str>,
        allowed_ids: &[u64],
        continuation_id: Option<&str>,
        avoid_ids: &[u64],
    ) -> anyhow::Result<CallContext> {
        let Some(cid) = continuation_id else {
            // 新会话无 continuation_id 是正常流程，不计入 miss，避免稀释真实掉线率
            return self.acquire_context_filtered(model, allowed_ids).await;
        };

        // 绑定是否指向本次请求内已限流的账号：决定后续是否保留绑定
        let bound_to_avoided = {
            let cache = self.sticky_cache.lock();
            cache
                .get(cid)
                .is_some_and(|e| avoid_ids.contains(&e.credential_id))
        };

        // 步骤 ①②：从 sticky_cache 查找，验证 TTL + 健康状态
        let cached = {
            let cache = self.sticky_cache.lock();
            if let Some(entry) = cache.get(cid) {
                if entry.inserted_at.elapsed() < STICKY_CACHE_TTL {
                    // TTL 未过期，检查账号健康状态
                    let entries = self.entries.lock();
                    // 健康度门槛：仅 Unhealthy/Disabled 才放弃绑定。
                    // Degraded/Warning 表示账号近期有限流但仍可服务，此时保留绑定
                    // 更有利于 prompt cache 命中；真正不可用时下面的调用链会重选。
                    entries
                        .iter()
                        .find(|e| {
                            e.id == entry.credential_id
                                && !avoid_ids.contains(&e.id)
                                && !e.disabled
                                && !matches!(
                                    Self::compute_health(e),
                                    HealthStatus::Unhealthy | HealthStatus::Disabled
                                )
                                && (allowed_ids.is_empty() || allowed_ids.contains(&e.id))
                        })
                        .map(|e| (e.id, e.credentials.clone()))
                } else {
                    None
                }
            } else {
                None
            }
        };

        // 步骤 ③：尝试使用缓存账号
        if let Some((id, credentials)) = cached {
            match self.try_ensure_token(id, &credentials).await {
                Ok(ctx) => {
                    // 命中成功，续期
                    self.sticky_hits.fetch_add(1, Ordering::Relaxed);
                    self.sticky_cache
                        .lock()
                        .entry(cid.to_string())
                        .and_modify(|e| {
                            e.inserted_at = Instant::now();
                            // 成功命中说明该账号仍可承载此会话，清零连续限流计数
                            e.consecutive_throttles = 0;
                        });
                    return Ok(ctx);
                }
                Err(e) => {
                    tracing::warn!(
                        "sticky cache 账号 #{} token 刷新失败，驱逐并重选: {}",
                        id,
                        e
                    );

                    if e.downcast_ref::<RefreshTokenInvalidError>().is_some() {
                        self.report_refresh_token_invalid(id);
                    } else {
                        self.report_refresh_failure(id);
                    }

                    self.sticky_cache.lock().remove(cid);
                    self.sticky_misses.fetch_add(1, Ordering::Relaxed);
                }
            }
        } else if bound_to_avoided {
            // 本次请求内该账号已限流：换账号完成这次调用，但保留绑定关系，
            // 让后续请求仍能回到原账号命中 prompt cache。不计 miss，避免稀释掉线率。
        } else {
            // TTL 过期或不健康，清理旧条目
            self.sticky_cache.lock().remove(cid);
            self.sticky_misses.fetch_add(1, Ordering::Relaxed);
        }

        // 步骤 ④：走原有选择逻辑（排除本次请求内已限流的账号）
        let ctx = if avoid_ids.is_empty() {
            self.acquire_context_filtered(model, allowed_ids).await?
        } else {
            self.acquire_context_avoiding(model, allowed_ids, avoid_ids)
                .await?
        };

        // 步骤 ⑤⑥：写入 sticky_cache，懒惰 GC
        {
            let mut cache = self.sticky_cache.lock();
            // 绑定仍指向本次请求内被避让的账号时保留原绑定，不要改写为临时替补账号
            if !bound_to_avoided {
                cache.insert(
                    cid.to_string(),
                    StickyCacheEntry {
                        credential_id: ctx.id,
                        inserted_at: Instant::now(),
                        consecutive_throttles: 0,
                    },
                );
            }
            // 懒惰 GC：清理所有过期条目
            cache.retain(|_, v| v.inserted_at.elapsed() < STICKY_CACHE_TTL);
        }

        Ok(ctx)
    }

    /// 记录一次 429 并按阈值决定是否解除 sticky 绑定
    ///
    /// Kiro 的 429 多为端点级瞬时限流，此时 `rotation_bias` 递增与端点桶封禁已能让
    /// 新会话避让该账号；若同时立刻解绑，长会话会反复丢失 prompt cache，触发更多
    /// 输入 token 重算，反而加剧限流。因此仅当同一会话在同一账号上连续限流达到
    /// `STICKY_THROTTLE_EVICT_THRESHOLD` 次，才认为该账号确实无法承载此会话。
    ///
    /// 返回 `true` 表示本次已解除绑定。
    pub fn report_sticky_throttled(&self, continuation_id: &str, credential_id: u64) -> bool {
        let mut cache = self.sticky_cache.lock();
        let Some(entry) = cache.get_mut(continuation_id) else {
            return false;
        };
        // 绑定已指向其它账号：本次限流与当前绑定无关，保留绑定
        if entry.credential_id != credential_id {
            return false;
        }
        entry.consecutive_throttles = entry.consecutive_throttles.saturating_add(1);
        if entry.consecutive_throttles < STICKY_THROTTLE_EVICT_THRESHOLD {
            tracing::debug!(
                "sticky 绑定保留: continuation_id={} 账号 #{} 连续限流 {}/{}",
                continuation_id,
                credential_id,
                entry.consecutive_throttles,
                STICKY_THROTTLE_EVICT_THRESHOLD
            );
            return false;
        }
        cache.remove(continuation_id);
        tracing::info!(
            "sticky 绑定已解除: continuation_id={} 账号 #{} 连续限流达到 {} 次",
            continuation_id,
            credential_id,
            STICKY_THROTTLE_EVICT_THRESHOLD
        );
        true
    }

    /// 切换到下一个优先级最高的可用账号（内部方法）
    fn switch_to_next_by_priority(&self) {
        let entries = self.entries.lock();
        let mut current_id = self.current_id.lock();

        // 选择优先级最高的未禁用账号（排除当前账号）
        if let Some(entry) = entries
            .iter()
            .filter(|e| !e.disabled && e.id != *current_id)
            .min_by_key(|e| e.credentials.priority)
        {
            *current_id = entry.id;
            tracing::info!(
                "已切换到账号 #{}（优先级 {}）",
                entry.id,
                entry.credentials.priority
            );
        }
    }

    /// 选择优先级最高的未禁用账号作为当前账号（内部方法）
    ///
    /// 与 `switch_to_next_by_priority` 不同，此方法不排除当前账号，
    /// 纯粹按优先级选择，用于优先级变更后立即生效
    pub(crate) fn select_highest_priority(&self) {
        let entries = self.entries.lock();
        let mut current_id = self.current_id.lock();

        // 选择优先级最高的未禁用账号（不排除当前账号）
        if let Some(best) = entries
            .iter()
            .filter(|e| !e.disabled)
            .min_by_key(|e| e.credentials.priority)
            && best.id != *current_id
        {
            tracing::info!(
                "优先级变更后切换账号: #{} -> #{}（优先级 {}）",
                *current_id,
                best.id,
                best.credentials.priority
            );
            *current_id = best.id;
        }
    }

    /// 尝试使用指定账号获取有效 Token
    ///
    /// 使用双重检查锁定模式，确保同一时间只有一个刷新操作
    ///
    /// # Arguments
    /// * `id` - 账号 ID，用于更新正确的条目
    /// * `credentials` - 账号信息
    async fn try_ensure_token(
        &self,
        id: u64,
        credentials: &KiroCredentials,
    ) -> anyhow::Result<CallContext> {
        // 第一次检查（无锁）：快速判断是否需要刷新
        let needs_refresh = is_token_expired(credentials) || is_token_expiring_soon(credentials);

        let creds = if needs_refresh {
            // 获取刷新锁，确保同一时间只有一个刷新操作
            let _guard = self.refresh_lock.lock().await;

            // 第二次检查：获取锁后重新读取账号，因为其他请求可能已经完成刷新
            let current_creds = {
                let entries = self.entries.lock();
                entries
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| e.credentials.clone())
                    .ok_or_else(|| anyhow::anyhow!("账号 #{} 不存在", id))?
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
                } else {
                    // 确实需要刷新
                    let effective_proxy = current_creds.effective_proxy(self.proxy.as_ref());
                    let new_creds =
                        refresh_token(&current_creds, &self.config, effective_proxy.as_ref())
                            .await?;

                    if is_token_expired(&new_creds) {
                        anyhow::bail!("刷新后的 Token 仍然无效或已过期");
                    }

                    // 更新账号 + 记录刷新时间
                    {
                        let mut entries = self.entries.lock();
                        if let Some(entry) = entries.iter_mut().find(|e| e.id == id) {
                            entry.credentials = new_creds.clone();
                            entry.last_refreshed_at = Some(Instant::now());
                            entry.refresh_failure_count = 0;
                        }
                    }

                    // 回写账号到文件（仅多账号格式），失败只记录警告
                    if let Err(e) = self.persist_credentials() {
                        tracing::warn!("Token 刷新后持久化失败（不影响本次请求）: {}", e);
                    }

                    new_creds
                }
            } else {
                // 其他请求已经完成刷新，直接使用新账号
                tracing::debug!("Token 已被其他请求刷新，跳过刷新");
                current_creds
            }
        } else {
            credentials.clone()
        };

        let token = creds
            .access_token
            .clone()
            .ok_or_else(|| anyhow::anyhow!("没有可用的 accessToken"))?;

        Ok(CallContext {
            id,
            credentials: creds,
            token,
        })
    }
}
