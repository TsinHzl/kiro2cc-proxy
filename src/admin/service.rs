// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! Admin API 业务逻辑服务

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::token_manager::MultiTokenManager;

use super::error::AdminServiceError;
use super::types::{
    AddCredentialRequest, AddCredentialResponse, BalanceResponse, CredentialStatusItem,
    CredentialsStatusResponse, LoadBalancingModeResponse, SetLoadBalancingModeRequest,
    UpdateCredentialRequest,
};

/// 余额缓存过期时间（秒），5 分钟
const BALANCE_CACHE_TTL_SECS: i64 = 300;

/// 缓存的余额条目（含时间戳）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedBalance {
    /// 缓存时间（Unix 秒）
    cached_at: f64,
    /// 缓存的余额数据
    data: BalanceResponse,
}

/// 根据模型 ID 前缀推断提供方（ListAvailableModels 响应不含厂商归属字段）
///
/// 命名规则与 `build_model_list()` 中手工维护的 `owned_by` 保持一致；未知前缀返回 `"unknown"`。
fn guess_owned_by(model_id: &str) -> &'static str {
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

/// 将上游 `ListAvailableModels` 返回的单条模型映射为 `AdminModelItem`（成功路径）
///
/// 纯函数，不涉及网络调用，可直接用 fake `AvailableModelInfo` 单测。
fn live_model_to_admin_item(
    info: &crate::kiro::model::available_models::AvailableModelInfo,
) -> super::types::AdminModelItem {
    super::types::AdminModelItem {
        model: crate::anthropic::types::Model {
            id: info.model_id.clone(),
            object: "model".to_string(),
            created: 0,
            owned_by: guess_owned_by(&info.model_id).to_string(),
            display_name: info.model_name.clone(),
            model_type: "chat".to_string(),
            max_tokens: info.token_limits.max_output_tokens as i32,
        },
        rate_multiplier: info.rate_multiplier,
    }
}

/// 将本地静态模型条目映射为 `AdminModelItem`（上游调用失败时的回退路径）
///
/// 纯函数，不涉及网络调用，可直接用 fake `Model` 单测。
fn fallback_model_to_admin_item(
    model: crate::anthropic::types::Model,
) -> super::types::AdminModelItem {
    super::types::AdminModelItem {
        model,
        rate_multiplier: None,
    }
}

/// Admin 服务
///
/// 封装所有 Admin API 的业务逻辑
pub struct AdminService {
    token_manager: Arc<MultiTokenManager>,
    balance_cache: Mutex<HashMap<u64, CachedBalance>>,
    cache_path: Option<PathBuf>,
}

impl AdminService {
    pub fn new(token_manager: Arc<MultiTokenManager>) -> Self {
        let cache_path = token_manager
            .cache_dir()
            .map(|d| d.join("kiro_balance_cache.json"));

        let balance_cache = Self::load_balance_cache_from(&cache_path);

        Self {
            token_manager,
            balance_cache: Mutex::new(balance_cache),
            cache_path,
        }
    }

    /// 获取所有账号状态
    pub fn get_all_credentials(&self) -> CredentialsStatusResponse {
        let snapshot = self.token_manager.snapshot();

        let mut credentials: Vec<CredentialStatusItem> = snapshot
            .entries
            .into_iter()
            .map(|entry| CredentialStatusItem {
                id: entry.id,
                priority: entry.priority,
                disabled: entry.disabled,
                failure_count: entry.failure_count,
                is_current: entry.id == snapshot.current_id,
                expires_at: entry.expires_at,
                auth_method: entry.auth_method,
                has_profile_arn: entry.has_profile_arn,
                refresh_token_hash: entry.refresh_token_hash,
                email: entry.email,
                nickname: entry.nickname,
                success_count: entry.success_count,
                last_used_at: entry.last_used_at.clone(),
                has_proxy: entry.has_proxy,
                proxy_url: entry.proxy_url,
                health_status: entry.health_status,
                throttle_count: entry.throttle_count,
            })
            .collect();

        // 按优先级排序（数字越小优先级越高）
        credentials.sort_by_key(|c| c.priority);

        CredentialsStatusResponse {
            total: snapshot.total,
            available: snapshot.available,
            current_id: snapshot.current_id,
            credentials,
        }
    }

    /// 设置账号禁用状态
    pub fn set_disabled(&self, id: u64, disabled: bool) -> Result<(), AdminServiceError> {
        // 先获取当前账号 ID，用于判断是否需要切换
        let snapshot = self.token_manager.snapshot();
        let current_id = snapshot.current_id;

        self.token_manager
            .set_disabled(id, disabled)
            .map_err(|e| self.classify_error(e, id))?;

        // 只有禁用的是当前账号时才尝试切换到下一个
        if disabled && id == current_id {
            let _ = self.token_manager.switch_to_next();
        }
        Ok(())
    }

    /// 设置账号优先级
    pub fn set_priority(&self, id: u64, priority: u32) -> Result<(), AdminServiceError> {
        self.token_manager
            .set_priority(id, priority)
            .map_err(|e| self.classify_error(e, id))
    }

    /// 重置失败计数并重新启用
    pub fn reset_and_enable(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .reset_and_enable(id)
            .map_err(|e| self.classify_error(e, id))
    }

    /// 获取账号余额（带缓存）
    pub async fn get_balance(&self, id: u64) -> Result<BalanceResponse, AdminServiceError> {
        // 先查缓存
        {
            let cache = self.balance_cache.lock();
            if let Some(cached) = cache.get(&id) {
                let now = Utc::now().timestamp() as f64;
                if (now - cached.cached_at) < BALANCE_CACHE_TTL_SECS as f64 {
                    tracing::debug!("账号 #{} 余额命中缓存", id);
                    return Ok(cached.data.clone());
                }
            }
        }

        // 缓存未命中或已过期，从上游获取
        let balance = self.fetch_balance(id).await?;

        // 更新缓存
        {
            let mut cache = self.balance_cache.lock();
            cache.insert(
                id,
                CachedBalance {
                    cached_at: Utc::now().timestamp() as f64,
                    data: balance.clone(),
                },
            );
        }
        self.save_balance_cache();

        Ok(balance)
    }

    /// 从上游获取余额（无缓存）
    async fn fetch_balance(&self, id: u64) -> Result<BalanceResponse, AdminServiceError> {
        let usage = self
            .token_manager
            .get_usage_limits_for(id)
            .await
            .map_err(|e| self.classify_balance_error(e, id))?;

        let current_usage = usage.current_usage();
        let usage_limit = usage.usage_limit();
        let remaining = (usage_limit - current_usage).max(0.0);
        let usage_percentage = if usage_limit > 0.0 {
            (current_usage / usage_limit * 100.0).min(100.0)
        } else {
            0.0
        };

        Ok(BalanceResponse {
            id,
            subscription_title: usage.subscription_title().map(|s| s.to_string()),
            current_usage,
            usage_limit,
            remaining,
            usage_percentage,
            next_reset_at: usage.next_date_reset,
        })
    }

    /// 获取当前支持模型列表（直接来源于上游 ListAvailableModels 实时响应）
    ///
    /// 每次实时调用，不做缓存；上游调用失败（无可用账号、网络错误、非 2xx、
    /// 反序列化失败）时记录日志并回退到本地静态模型表（`rate_multiplier` 全为 `None`）。
    pub async fn list_admin_models(&self) -> Vec<super::types::AdminModelItem> {
        match self.token_manager.list_available_models().await {
            Ok(resp) => resp.models.iter().map(live_model_to_admin_item).collect(),
            Err(e) => {
                tracing::warn!("获取实时支持模型列表失败，回退到本地静态模型表: {}", e);
                crate::anthropic::handlers::build_model_list()
                    .into_iter()
                    .map(fallback_model_to_admin_item)
                    .collect()
            }
        }
    }

    /// 添加新账号
    pub async fn add_credential(
        &self,
        req: AddCredentialRequest,
    ) -> Result<AddCredentialResponse, AdminServiceError> {
        // 构建账号对象
        let email = req.email.clone();
        let new_cred = KiroCredentials {
            id: None,
            access_token: None,
            refresh_token: Some(req.refresh_token),
            profile_arn: req.profile_arn.filter(|s| !s.is_empty()),
            expires_at: None,
            auth_method: Some(req.auth_method),
            client_id: req.client_id,
            client_secret: req.client_secret,
            priority: req.priority,
            region: req.region,
            auth_region: req.auth_region,
            api_region: req.api_region,
            machine_id: req.machine_id,
            email: req.email,
            nickname: req.nickname,
            subscription_title: None, // 将在首次获取使用额度时自动更新
            proxy_url: req.proxy_url,
            proxy_username: req.proxy_username,
            proxy_password: req.proxy_password,
            disabled: false, // 新添加的账号默认启用
        };

        // 调用 token_manager 添加账号
        let credential_id = self
            .token_manager
            .add_credential(new_cred)
            .await
            .map_err(|e| self.classify_add_error(e))?;

        // 后台获取订阅等级，避免首次请求时 Free 账号绕过 Opus 模型过滤
        let tm = self.token_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = tm.get_usage_limits_for(credential_id).await {
                tracing::warn!("添加账号后获取订阅等级失败（不影响账号添加）: {}", e);
            }
        });

        Ok(AddCredentialResponse {
            success: true,
            message: format!("账号添加成功，ID: {}", credential_id),
            credential_id,
            email,
        })
    }

    /// 删除账号
    pub fn delete_credential(&self, id: u64) -> Result<(), AdminServiceError> {
        self.token_manager
            .delete_credential(id)
            .map_err(|e| self.classify_delete_error(e, id))?;

        // 清理已删除账号的余额缓存
        {
            let mut cache = self.balance_cache.lock();
            cache.remove(&id);
        }
        self.save_balance_cache();

        Ok(())
    }

    /// 更新账号配置
    pub async fn update_credential(
        &self,
        id: u64,
        req: UpdateCredentialRequest,
    ) -> Result<(), AdminServiceError> {
        self.token_manager
            .update_credential(id, req)
            .await
            .map_err(|e| self.classify_update_error(e, id))?;

        // 清理该账号的余额缓存（配置变更后需要重新获取）
        {
            let mut cache = self.balance_cache.lock();
            cache.remove(&id);
        }
        self.save_balance_cache();

        Ok(())
    }

    /// 构建账号 ID -> 显示标签（nickname 优先，其次 email，都没有则用 #id）的映射
    pub fn credential_labels(&self) -> std::collections::HashMap<u64, String> {
        let snapshot = self.token_manager.snapshot();
        snapshot
            .entries
            .into_iter()
            .map(|e| {
                let label = e
                    .nickname
                    .filter(|s| !s.is_empty())
                    .or_else(|| e.email.filter(|s| !s.is_empty()))
                    .unwrap_or_else(|| format!("#{}", e.id));
                (e.id, label)
            })
            .collect()
    }

    /// 获取负载均衡模式
    pub fn get_load_balancing_mode(&self) -> LoadBalancingModeResponse {
        LoadBalancingModeResponse {
            mode: self.token_manager.get_load_balancing_mode(),
        }
    }

    /// 设置负载均衡模式
    pub fn set_load_balancing_mode(
        &self,
        req: SetLoadBalancingModeRequest,
    ) -> Result<LoadBalancingModeResponse, AdminServiceError> {
        // 验证模式值
        if req.mode != "priority" && req.mode != "balanced" {
            return Err(AdminServiceError::InvalidCredential(
                "mode 必须是 'priority' 或 'balanced'".to_string(),
            ));
        }

        self.token_manager
            .set_load_balancing_mode(req.mode.clone())
            .map_err(|e| AdminServiceError::InternalError(e.to_string()))?;

        Ok(LoadBalancingModeResponse { mode: req.mode })
    }

    // ============ 余额缓存持久化 ============

    fn load_balance_cache_from(cache_path: &Option<PathBuf>) -> HashMap<u64, CachedBalance> {
        let path = match cache_path {
            Some(p) => p,
            None => return HashMap::new(),
        };

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        // 文件中使用字符串 key 以兼容 JSON 格式
        let map: HashMap<String, CachedBalance> = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("解析余额缓存失败，将忽略: {}", e);
                return HashMap::new();
            }
        };

        let now = Utc::now().timestamp() as f64;
        map.into_iter()
            .filter_map(|(k, v)| {
                let id = k.parse::<u64>().ok()?;
                // 丢弃超过 TTL 的条目
                if (now - v.cached_at) < BALANCE_CACHE_TTL_SECS as f64 {
                    Some((id, v))
                } else {
                    None
                }
            })
            .collect()
    }

    fn save_balance_cache(&self) {
        let path = match &self.cache_path {
            Some(p) => p,
            None => return,
        };

        // 持有锁期间完成序列化和写入，防止并发损坏
        let cache = self.balance_cache.lock();
        let map: HashMap<String, &CachedBalance> =
            cache.iter().map(|(k, v)| (k.to_string(), v)).collect();

        match serde_json::to_string_pretty(&map) {
            Ok(json) => {
                if let Err(e) = std::fs::write(path, json) {
                    tracing::warn!("保存余额缓存失败: {}", e);
                }
            }
            Err(e) => tracing::warn!("序列化余额缓存失败: {}", e),
        }
    }

    // ============ 错误分类 ============

    /// 分类简单操作错误（set_disabled, set_priority, reset_and_enable）
    fn classify_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();
        if msg.contains("不存在") {
            AdminServiceError::NotFound { id }
        } else {
            AdminServiceError::InternalError(msg)
        }
    }

    /// 分类余额查询错误（可能涉及上游 API 调用）
    fn classify_balance_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();

        // 1. 账号不存在
        if msg.contains("不存在") {
            return AdminServiceError::NotFound { id };
        }

        // 2. 上游服务错误特征：HTTP 响应错误或网络错误
        let is_upstream_error =
            // HTTP 响应错误（来自 refresh_*_token 的错误消息）
            msg.contains("凭证已过期或无效") ||
            msg.contains("权限不足") ||
            msg.contains("已被限流") ||
            msg.contains("服务器错误") ||
            msg.contains("Token 刷新失败") ||
            msg.contains("暂时不可用") ||
            // 网络错误（reqwest 错误）
            msg.contains("error trying to connect") ||
            msg.contains("connection") ||
            msg.contains("timeout") ||
            msg.contains("timed out");

        if is_upstream_error {
            AdminServiceError::UpstreamError(msg)
        } else {
            // 3. 默认归类为内部错误（本地验证失败、配置错误等）
            // 包括：缺少 refreshToken、refreshToken 已被截断、无法生成 machineId 等
            AdminServiceError::InternalError(msg)
        }
    }

    /// 分类添加账号错误
    fn classify_add_error(&self, e: anyhow::Error) -> AdminServiceError {
        let msg = e.to_string();

        // 账号验证失败（refreshToken 无效、格式错误等）
        let is_invalid_credential = msg.contains("缺少 refreshToken")
            || msg.contains("refreshToken 为空")
            || msg.contains("refreshToken 已被截断")
            || msg.contains("账号已存在")
            || msg.contains("refreshToken 重复")
            || msg.contains("凭证已过期或无效")
            || msg.contains("权限不足")
            || msg.contains("已被限流");

        if is_invalid_credential {
            AdminServiceError::InvalidCredential(msg)
        } else if msg.contains("error trying to connect")
            || msg.contains("connection")
            || msg.contains("timeout")
        {
            AdminServiceError::UpstreamError(msg)
        } else {
            AdminServiceError::InternalError(msg)
        }
    }

    /// 分类删除账号错误
    fn classify_delete_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();
        if msg.contains("不存在") {
            AdminServiceError::NotFound { id }
        } else if msg.contains("只能删除已禁用的账号") || msg.contains("请先禁用账号")
        {
            AdminServiceError::InvalidCredential(msg)
        } else {
            AdminServiceError::InternalError(msg)
        }
    }

    /// 转发 sticky cache 命中/未命中计数
    pub fn sticky_metrics(&self) -> (u64, u64) {
        self.token_manager.sticky_metrics()
    }

    /// 分类更新账号错误
    fn classify_update_error(&self, e: anyhow::Error, id: u64) -> AdminServiceError {
        let msg = e.to_string();
        if msg.contains("不存在") {
            AdminServiceError::NotFound { id }
        } else if msg.contains("凭证已过期或无效")
            || msg.contains("权限不足")
            || msg.contains("已被限流")
            || msg.contains("error trying to connect")
            || msg.contains("timeout")
        {
            AdminServiceError::UpstreamError(msg)
        } else {
            AdminServiceError::InvalidCredential(msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::types::Model;
    use crate::kiro::model::available_models::{
        AvailableModelInfo, AvailableModelsResponse, TokenLimits,
    };

    fn fake_info(
        model_id: &str,
        model_name: &str,
        rate: Option<f64>,
        max_in: u64,
        max_out: u64,
    ) -> AvailableModelInfo {
        AvailableModelInfo {
            model_id: model_id.to_string(),
            model_name: model_name.to_string(),
            rate_multiplier: rate,
            token_limits: TokenLimits {
                max_input_tokens: max_in,
                max_output_tokens: max_out,
            },
        }
    }

    #[test]
    fn test_guess_owned_by_known_and_unknown_prefixes() {
        assert_eq!(guess_owned_by("claude-sonnet-4.6"), "anthropic");
        assert_eq!(guess_owned_by("gpt-5.6-sol"), "openai");
        assert_eq!(guess_owned_by("auto"), "kiro");
        assert_eq!(guess_owned_by("deepseek-3.2"), "deepseek");
        assert_eq!(guess_owned_by("minimax-m2.5"), "minimax");
        assert_eq!(guess_owned_by("glm-5"), "glm");
        assert_eq!(guess_owned_by("qwen3-coder-next"), "qwen");
        assert_eq!(guess_owned_by("foo-model"), "unknown");
    }

    #[test]
    fn test_live_model_to_admin_item_maps_fields() {
        let info = fake_info(
            "claude-sonnet-4.6",
            "Claude Sonnet 4.6",
            Some(1.3),
            1_000_000,
            64_000,
        );
        let item = live_model_to_admin_item(&info);
        assert_eq!(item.model.id, "claude-sonnet-4.6");
        assert_eq!(item.model.display_name, "Claude Sonnet 4.6");
        assert_eq!(item.model.max_tokens, 64_000);
        assert_eq!(item.model.owned_by, "anthropic");
        assert_eq!(item.rate_multiplier, Some(1.3));

        let unknown = fake_info("foo-model", "Foo Model", None, 100, 50);
        let unknown_item = live_model_to_admin_item(&unknown);
        assert_eq!(unknown_item.model.owned_by, "unknown");
        assert_eq!(unknown_item.rate_multiplier, None);
    }

    #[test]
    fn test_fallback_model_to_admin_item_clears_rate_multiplier() {
        let model = Model {
            id: "claude-3-5-sonnet-20241022".to_string(),
            object: "model".to_string(),
            created: 1729555200,
            owned_by: "anthropic".to_string(),
            display_name: "Claude 3.5 Sonnet".to_string(),
            model_type: "chat".to_string(),
            max_tokens: 8192,
        };
        let expected_id = model.id.clone();
        let item = fallback_model_to_admin_item(model);
        assert_eq!(item.model.id, expected_id);
        assert_eq!(item.rate_multiplier, None);
    }

    /// 基于真实抓包响应体结构构造的 JSON 字面量（字段名/嵌套与
    /// `ListAvailableModels` 实际返回一致），验证反序列化字段映射；
    /// 第三条模型故意缺失 `modelName`/`tokenLimits`，验证 `#[serde(default)]`
    /// 容错不会导致整个 `models[]` 解析失败
    #[test]
    fn test_available_models_response_deserializes_real_capture_shape() {
        let raw = r#"{
            "defaultModel": { "modelId": "auto" },
            "models": [
                {
                    "description": "自动选择模型",
                    "modelId": "auto",
                    "modelName": "Auto",
                    "promptCaching": {
                        "maximumCacheCheckpointsPerRequest": 4,
                        "minimumTokensPerCacheCheckpoint": 1024,
                        "supportsPromptCaching": true
                    },
                    "rateMultiplier": 1.0,
                    "rateUnit": "Credit",
                    "supportedInputTypes": ["TEXT"],
                    "tokenLimits": { "maxInputTokens": 1000000, "maxOutputTokens": 64000 }
                },
                {
                    "description": "Claude Sonnet 5",
                    "modelId": "claude-sonnet-5",
                    "modelName": "Claude Sonnet 5",
                    "promptCaching": {
                        "maximumCacheCheckpointsPerRequest": 4,
                        "minimumTokensPerCacheCheckpoint": 1024,
                        "supportsPromptCaching": true
                    },
                    "rateMultiplier": 1.3,
                    "rateUnit": "Credit",
                    "supportedInputTypes": ["TEXT", "IMAGE"],
                    "tokenLimits": { "maxInputTokens": 1000000, "maxOutputTokens": 64000 },
                    "additionalModelRequestFieldsSchema": { "type": "object", "properties": {} }
                },
                {
                    "modelId": "legacy-drift-model",
                    "rateMultiplier": 0.5
                }
            ]
        }"#;

        let parsed: AvailableModelsResponse =
            serde_json::from_str(raw).expect("应能反序列化真实抓包结构");
        assert_eq!(parsed.models.len(), 3);

        let auto = &parsed.models[0];
        assert_eq!(auto.model_id, "auto");
        assert_eq!(auto.model_name, "Auto");
        assert_eq!(auto.rate_multiplier, Some(1.0));
        assert_eq!(auto.token_limits.max_input_tokens, 1_000_000);
        assert_eq!(auto.token_limits.max_output_tokens, 64_000);

        let sonnet = &parsed.models[1];
        assert_eq!(sonnet.model_id, "claude-sonnet-5");
        assert_eq!(sonnet.rate_multiplier, Some(1.3));

        // 缺失 modelName/tokenLimits 时应回退到默认值，反序列化不失败
        let drift = &parsed.models[2];
        assert_eq!(drift.model_id, "legacy-drift-model");
        assert_eq!(drift.model_name, "");
        assert_eq!(drift.token_limits.max_input_tokens, 0);
        assert_eq!(drift.token_limits.max_output_tokens, 0);
        assert_eq!(drift.rate_multiplier, Some(0.5));
    }
}
