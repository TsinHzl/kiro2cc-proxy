// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! KiroProvider 结构体、常量与基础方法
//!
//! 核心组件，负责与 Kiro API 通信
//! 支持流式和非流式请求
//! 支持多账号故障转移和重试

use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;

use crate::http_client::{ProxyConfig, build_client};
use crate::kiro::endpoint::{Endpoint, EndpointBucketRegistry, EndpointName};
use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::token_manager::MultiTokenManager;
use crate::model::config::TlsBackend;
use crate::model::failure_log::FailureLogStore;
use crate::model::rpm::RpmTracker;
use crate::model::throttle_log::ThrottleLogStore;
use parking_lot::Mutex;
use tokio::sync::Semaphore;

/// 每个账号的最大重试次数
pub(crate) const MAX_RETRIES_PER_CREDENTIAL: usize = 3;

/// 总重试次数硬上限（避免无限重试）
pub(crate) const MAX_TOTAL_RETRIES: usize = 9;

/// 最大并发请求数（同时发往上游的请求上限）
pub(crate) const MAX_CONCURRENT_REQUESTS: usize = 50;

/// 单账号最大并发请求数
pub(crate) const MAX_CONCURRENT_PER_CREDENTIAL: usize = 20;

/// 所有上游 API 请求统一使用的 HTTP 总超时（秒）。
///
/// 历史上曾按请求类型分档：压缩请求 1000s（历史修复 commit 9338888，大上下文
/// 非流式 502），普通请求 180s（commit d669fb6，意图是让网络异常时报错更快）。
/// 但 `reqwest::Client::timeout` 是覆盖「发请求到读完整个响应体」的总超时，
/// 流式响应的持续生成阶段同样受它约束——100K+ 输入的长生成请求实测可超过 180s，
/// 会被该超时在中途掐断（表现为 `error decoding response body` / 502），
/// 生产日志已实证（三次流式失败 + 一次非流式 502 耗时全部 ≈180s）。
/// 故统一回 1000s：上游正常生成耗时不受影响（超时只是上限），
/// 真正挂死的连接由 TCP keepalive 与客户端自身重试兜底。
pub(crate) const UPSTREAM_TIMEOUT_SECS: u64 = 1000;

/// Kiro API Provider
///
/// 核心组件，负责与 Kiro API 通信
/// 支持多账号故障转移和重试机制
pub struct KiroProvider {
    pub(crate) token_manager: Arc<MultiTokenManager>,
    /// 全局代理配置（用于账号无自定义代理时的回退）
    pub(crate) global_proxy: Option<ProxyConfig>,
    /// Client 缓存：key = (effective proxy config, use_long_timeout)，value = reqwest::Client
    /// 不同代理配置的账号使用不同的 Client，共享相同代理的账号复用 Client。
    /// 历史上按请求类型分两档超时（压缩/流式 1000s vs 普通 180s），现已统一为
    /// UPSTREAM_TIMEOUT_SECS=1000s，但缓存结构保留 bool 维度以兼容既有调用点
    /// （见 `client_for`）。
    pub(crate) client_cache: Mutex<HashMap<(Option<ProxyConfig>, bool), Client>>,
    /// TLS 后端配置
    pub(crate) tls_backend: TlsBackend,
    /// 并发控制信号量，限制同时发往上游的请求数
    pub(crate) concurrency_limit: Arc<Semaphore>,
    /// 单账号并发信号量：限制每个账号的同时请求数
    pub(crate) credential_semaphores: Mutex<HashMap<u64, Arc<tokio::sync::Semaphore>>>,
    /// RPM 追踪器（可选，用于记录账号维度的 RPM）
    pub(crate) rpm_tracker: Option<Arc<RpmTracker>>,
    /// 限流日志存储（可选）
    pub(crate) throttle_log_store: Option<Arc<ThrottleLogStore>>,
    /// 失败日志存储（可选）
    pub(crate) failure_log_store: Option<Arc<FailureLogStore>>,
    /// 端点级 429 状态注册表（多端点 LB 使用）
    pub(crate) endpoint_registry: Arc<EndpointBucketRegistry>,
}

#[allow(dead_code)] // 拆分自原 provider.rs：部分方法仅测试/预留使用
impl KiroProvider {
    /// 创建新的 KiroProvider 实例
    pub fn new(token_manager: Arc<MultiTokenManager>) -> Self {
        Self::with_proxy(token_manager, None)
    }

    /// 创建带代理配置的 KiroProvider 实例
    pub fn with_proxy(token_manager: Arc<MultiTokenManager>, proxy: Option<ProxyConfig>) -> Self {
        let tls_backend = token_manager.config().tls_backend;
        // 预热：为全局代理配置构建普通超时 Client（长超时 Client 按需懒创建）
        let initial_client = build_client(proxy.as_ref(), UPSTREAM_TIMEOUT_SECS, tls_backend)
            .expect("创建 HTTP 客户端失败");
        let mut cache = HashMap::new();
        cache.insert((proxy.clone(), false), initial_client);

        Self {
            token_manager,
            global_proxy: proxy,
            client_cache: Mutex::new(cache),
            tls_backend,
            concurrency_limit: Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)),
            credential_semaphores: Mutex::new(HashMap::new()),
            rpm_tracker: None,
            throttle_log_store: None,
            failure_log_store: None,
            endpoint_registry: Arc::new(EndpointBucketRegistry::new()),
        }
    }

    /// 注入外部 endpoint_registry（多 provider 共享桶状态时使用）
    pub fn with_endpoint_registry(mut self, registry: Arc<EndpointBucketRegistry>) -> Self {
        self.endpoint_registry = registry;
        self
    }

    /// 设置 RPM 追踪器
    pub fn with_rpm_tracker(mut self, tracker: Arc<RpmTracker>) -> Self {
        self.rpm_tracker = Some(tracker);
        self
    }

    /// 设置限流日志存储
    pub fn with_throttle_log_store(mut self, store: Arc<ThrottleLogStore>) -> Self {
        self.throttle_log_store = Some(store);
        self
    }

    /// 设置失败日志存储
    pub fn with_failure_log_store(mut self, store: Arc<FailureLogStore>) -> Self {
        self.failure_log_store = Some(store);
        self
    }

    /// 根据账号的代理配置获取（或创建并缓存）对应的 reqwest::Client
    ///
    /// 历史上按请求类型分两档超时（参数 `use_long_timeout` 区分），现统一为
    /// `UPSTREAM_TIMEOUT_SECS`。参数保留以兼容既有调用点，两档构建的 Client
    /// 超时一致，仅缓存 key 隔离。
    pub(crate) fn client_for(
        &self,
        credentials: &KiroCredentials,
        use_long_timeout: bool,
    ) -> anyhow::Result<Client> {
        let effective = credentials.effective_proxy(self.global_proxy.as_ref());
        let key = (effective.clone(), use_long_timeout);
        let mut cache = self.client_cache.lock();
        if let Some(client) = cache.get(&key) {
            return Ok(client.clone());
        }
        let timeout_secs = UPSTREAM_TIMEOUT_SECS;
        tracing::debug!(
            "[CLIENT] 创建新 Client：use_long_timeout={} timeout_secs={}",
            use_long_timeout,
            timeout_secs
        );
        let client = build_client(effective.as_ref(), timeout_secs, self.tls_backend)?;
        cache.insert(key, client.clone());
        Ok(client)
    }

    /// 获取指定账号的并发信号量（懒初始化）
    pub(crate) fn semaphore_for(&self, credential_id: u64) -> Arc<tokio::sync::Semaphore> {
        let mut map = self.credential_semaphores.lock();
        map.entry(credential_id)
            .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_PER_CREDENTIAL)))
            .clone()
    }

    /// 获取 token_manager 的引用
    pub fn token_manager(&self) -> &MultiTokenManager {
        &self.token_manager
    }

    /// 获取 API 基础 URL（使用 config 级 api_region + 默认 Ide 端点）
    pub fn base_url(&self) -> String {
        let region = self.token_manager.config().effective_api_region();
        let endpoint = Endpoint::by_name(EndpointName::Ide, region);
        format!("https://{}/generateAssistantResponse", endpoint.host)
    }

    /// 获取 MCP API URL（使用 config 级 api_region，MCP 端点独立于多端点 LB）
    pub fn mcp_url(&self) -> String {
        format!(
            "https://q.{}.amazonaws.com/mcp",
            self.token_manager.config().effective_api_region()
        )
    }

    /// 获取 API 基础域名（使用 config 级 api_region + 默认 Ide 端点）
    pub fn base_domain(&self) -> String {
        Endpoint::by_name(
            EndpointName::Ide,
            self.token_manager.config().effective_api_region(),
        )
        .host
    }

    /// 获取账号级 API 基础 URL（按指定 endpoint）
    pub(crate) fn base_url_for(
        &self,
        _credentials: &KiroCredentials,
        endpoint: &Endpoint,
    ) -> String {
        format!("https://{}/generateAssistantResponse", endpoint.host)
    }

    /// 获取账号级 MCP API URL（MCP 端点不走多端点 LB）
    pub(crate) fn mcp_url_for(&self, credentials: &KiroCredentials) -> String {
        format!(
            "https://q.{}.amazonaws.com/mcp",
            credentials.effective_api_region(self.token_manager.config())
        )
    }

    /// 获取账号级 API 基础域名（按指定 endpoint）
    pub(crate) fn base_domain_for(
        &self,
        _credentials: &KiroCredentials,
        endpoint: &Endpoint,
    ) -> String {
        endpoint.host.clone()
    }

    /// 从请求体中提取模型信息
    ///
    /// 尝试解析 JSON 请求体，提取 conversationState.currentMessage.userInputMessage.modelId
    pub(crate) fn extract_model_from_request(request_body: &str) -> Option<String> {
        use serde_json::Value;

        let json: Value = serde_json::from_str(request_body).ok()?;

        // 尝试提取 conversationState.currentMessage.userInputMessage.modelId
        json.get("conversationState")?
            .get("currentMessage")?
            .get("userInputMessage")?
            .get("modelId")?
            .as_str()
            .map(|s| s.to_string())
    }

    /// 从请求体中提取 agentTaskType
    ///
    /// 提取 conversationState.agentTaskType，用于设置 x-amzn-kiro-agent-mode 请求头
    pub(crate) fn extract_agent_task_type_from_request(request_body: &str) -> &'static str {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(request_body) else {
            return "vibe";
        };
        match json
            .get("conversationState")
            .and_then(|s| s.get("agentTaskType"))
            .and_then(|v| v.as_str())
        {
            Some("spectask") => "spectask",
            _ => "vibe",
        }
    }

    /// 提取 conversationState.agentContinuationId，用于 sticky cache 路由
    pub(crate) fn extract_continuation_id_from_request(request_body: &str) -> Option<String> {
        let json: serde_json::Value = serde_json::from_str(request_body).ok()?;
        json.get("conversationState")?
            .get("agentContinuationId")?
            .as_str()
            .map(|s| s.to_string())
    }
}
