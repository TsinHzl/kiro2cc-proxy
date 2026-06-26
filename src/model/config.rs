// Copyright (c) 2026 Harllan He. Licensed under MIT.
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum TlsBackend {
    #[default]
    Rustls,
    NativeTls,
}

/// Prompt cache 模拟与指纹追踪配置
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheSimulationConfig {
    /// 是否启用指纹追踪（替代 from_ratio_config 末层兜底）
    #[serde(default = "default_fingerprint_enabled")]
    pub fingerprint_enabled: bool,

    /// 5m ephemeral TTL（秒）
    #[serde(default = "default_fingerprint_ttl_5m")]
    pub fingerprint_ttl_5m: u64,

    /// 1h ephemeral TTL（秒）
    #[serde(default = "default_fingerprint_ttl_1h")]
    pub fingerprint_ttl_1h: u64,

    /// 新建 cache_creation 中 1h tier 占比（0.0~1.0，默认 0.0 全部 5m）
    #[serde(default = "default_ephemeral_1h_ratio")]
    pub ephemeral_1h_ratio: f64,

    /// 单账号指纹断点上限（超出按 LRU 淘汰）
    #[serde(default = "default_fingerprint_max_breakpoints")]
    pub fingerprint_max_breakpoints_per_account: usize,
}

fn default_fingerprint_enabled() -> bool {
    true
}
fn default_fingerprint_ttl_5m() -> u64 {
    300
}
fn default_fingerprint_ttl_1h() -> u64 {
    3600
}
fn default_ephemeral_1h_ratio() -> f64 {
    0.0
}
fn default_fingerprint_max_breakpoints() -> usize {
    256
}

impl Default for CacheSimulationConfig {
    fn default() -> Self {
        Self {
            fingerprint_enabled: default_fingerprint_enabled(),
            fingerprint_ttl_5m: default_fingerprint_ttl_5m(),
            fingerprint_ttl_1h: default_fingerprint_ttl_1h(),
            ephemeral_1h_ratio: default_ephemeral_1h_ratio(),
            fingerprint_max_breakpoints_per_account: default_fingerprint_max_breakpoints(),
        }
    }
}

/// KNA 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_region")]
    pub region: String,

    /// Auth Region（用于 Token 刷新），未配置时回退到 region
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_region: Option<String>,

    /// API Region（用于 API 请求），未配置时回退到 region
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_region: Option<String>,

    #[serde(default = "default_kiro_version")]
    pub kiro_version: String,

    #[serde(default)]
    pub machine_id: Option<String>,

    #[serde(default)]
    pub api_key: Option<String>,

    #[serde(default = "default_system_version")]
    pub system_version: String,

    #[serde(default = "default_node_version")]
    pub node_version: String,

    #[serde(default = "default_tls_backend")]
    pub tls_backend: TlsBackend,

    /// 外部 count_tokens API 地址（可选）
    #[serde(default)]
    pub count_tokens_api_url: Option<String>,

    /// count_tokens API 密钥（可选）
    #[serde(default)]
    pub count_tokens_api_key: Option<String>,

    /// count_tokens API 认证类型（可选，"x-api-key" 或 "bearer"，默认 "x-api-key"）
    #[serde(default = "default_count_tokens_auth_type")]
    pub count_tokens_auth_type: String,

    /// HTTP 代理地址（可选）
    /// 支持格式: http://host:port, https://host:port, socks5://host:port
    #[serde(default)]
    pub proxy_url: Option<String>,

    /// 代理认证用户名（可选）
    #[serde(default)]
    pub proxy_username: Option<String>,

    /// 代理认证密码（可选）
    #[serde(default)]
    pub proxy_password: Option<String>,

    /// Admin API 密钥（可选，启用 Admin API 功能）
    #[serde(default)]
    pub admin_api_key: Option<String>,

    /// 负载均衡模式（"priority" 或 "balanced"）
    #[serde(default = "default_load_balancing_mode")]
    pub load_balancing_mode: String,

    /// 单账号每分钟最大请求数（超出时排队等待），0 表示不限制
    #[serde(default = "default_max_rpm_per_credential")]
    pub max_rpm_per_credential: u32,

    /// Prompt cache 模拟与指纹追踪配置
    #[serde(default)]
    pub cache_simulation: CacheSimulationConfig,

    /// 配置文件路径（运行时元数据，不写入 JSON）
    #[serde(skip)]
    config_path: Option<PathBuf>,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_region() -> String {
    "us-east-1".to_string()
}

fn default_kiro_version() -> String {
    "2.2.2".to_string()
}

// 0 = 关闭单账号 RPM 硬限（仅依赖上游 429 + throttle_delay 兜底）
fn default_max_rpm_per_credential() -> u32 {
    0
}

fn default_system_version() -> String {
    "darwin#24.6.0".to_string()
}

fn default_node_version() -> String {
    "22.21.1".to_string()
}

fn default_count_tokens_auth_type() -> String {
    "x-api-key".to_string()
}

fn default_tls_backend() -> TlsBackend {
    TlsBackend::Rustls
}

fn default_load_balancing_mode() -> String {
    "priority".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            region: default_region(),
            auth_region: None,
            api_region: None,
            kiro_version: default_kiro_version(),
            machine_id: None,
            api_key: None,
            system_version: default_system_version(),
            node_version: default_node_version(),
            tls_backend: default_tls_backend(),
            count_tokens_api_url: None,
            count_tokens_api_key: None,
            count_tokens_auth_type: default_count_tokens_auth_type(),
            proxy_url: None,
            proxy_username: None,
            proxy_password: None,
            admin_api_key: None,
            load_balancing_mode: default_load_balancing_mode(),
            max_rpm_per_credential: default_max_rpm_per_credential(),
            cache_simulation: CacheSimulationConfig::default(),
            config_path: None,
        }
    }
}

impl Config {
    /// 获取默认配置文件路径
    pub fn default_config_path() -> &'static str {
        "config.json"
    }

    /// 获取有效的 Auth Region（用于 Token 刷新）
    /// 优先使用 auth_region，未配置时回退到 region
    pub fn effective_auth_region(&self) -> &str {
        self.auth_region.as_deref().unwrap_or(&self.region)
    }

    /// 获取有效的 API Region（用于 API 请求）
    /// 优先使用 api_region，未配置时回退到 region
    pub fn effective_api_region(&self) -> &str {
        self.api_region.as_deref().unwrap_or(&self.region)
    }

    /// 从文件加载配置
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self {
                config_path: Some(path.to_path_buf()),
                ..Self::default()
            });
        }

        let content = fs::read_to_string(path)?;
        let mut config: Config = serde_json::from_str(&content)?;
        config.config_path = Some(path.to_path_buf());
        Ok(config)
    }

    /// 获取配置文件路径（如果有）
    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    /// 将当前配置写回原始配置文件
    #[allow(dead_code)]
    pub fn save(&self) -> anyhow::Result<()> {
        let path = self
            .config_path
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("配置文件路径未知，无法保存配置"))?;

        let content = serde_json::to_string_pretty(self).context("序列化配置失败")?;
        fs::write(path, content)
            .with_context(|| format!("写入配置文件失败: {}", path.display()))?;
        Ok(())
    }

    /// 从环境变量覆盖配置项（用于容器化部署，如 Zeabur）
    ///
    /// 支持的环境变量:
    /// - `API_KEY`: apiKey
    /// - `HOST`: 监听地址
    /// - `PORT`: 监听端口
    /// - `REGION`: AWS 区域
    /// - `AUTH_REGION`: Token 刷新区域
    /// - `API_REGION`: API 请求区域
    /// - `ADMIN_API_KEY`: Admin API 密钥
    /// - `PROXY_URL`: HTTP 代理地址
    /// - `PROXY_USERNAME`: 代理用户名
    /// - `PROXY_PASSWORD`: 代理密码
    /// - `LOAD_BALANCING_MODE`: 负载均衡模式
    pub fn apply_env_overrides(&mut self) {
        if let Ok(v) = env::var("API_KEY") {
            self.api_key = Some(v);
        }
        if let Ok(v) = env::var("HOST") {
            self.host = v;
        }
        if let Ok(v) = env::var("PORT")
            && let Ok(p) = v.parse::<u16>()
        {
            self.port = p;
        }
        if let Ok(v) = env::var("REGION") {
            self.region = v;
        }
        if let Ok(v) = env::var("AUTH_REGION") {
            self.auth_region = Some(v);
        }
        if let Ok(v) = env::var("API_REGION") {
            self.api_region = Some(v);
        }
        if let Ok(v) = env::var("ADMIN_API_KEY") {
            self.admin_api_key = Some(v);
        }
        if let Ok(v) = env::var("PROXY_URL") {
            self.proxy_url = Some(v);
        }
        if let Ok(v) = env::var("PROXY_USERNAME") {
            self.proxy_username = Some(v);
        }
        if let Ok(v) = env::var("PROXY_PASSWORD") {
            self.proxy_password = Some(v);
        }
        if let Ok(v) = env::var("LOAD_BALANCING_MODE") {
            self.load_balancing_mode = v;
        }

        // CacheSimulationConfig 嵌套字段覆盖
        if let Ok(v) = env::var("CACHE_SIMULATION_FINGERPRINT_ENABLED")
            && let Ok(b) = v.parse::<bool>()
        {
            self.cache_simulation.fingerprint_enabled = b;
        }
        if let Ok(v) = env::var("CACHE_SIMULATION_FINGERPRINT_TTL_5M")
            && let Ok(n) = v.parse::<u64>()
        {
            self.cache_simulation.fingerprint_ttl_5m = n;
        }
        if let Ok(v) = env::var("CACHE_SIMULATION_FINGERPRINT_TTL_1H")
            && let Ok(n) = v.parse::<u64>()
        {
            self.cache_simulation.fingerprint_ttl_1h = n;
        }
        if let Ok(v) = env::var("CACHE_SIMULATION_EPHEMERAL_1H_RATIO")
            && let Ok(f) = v.parse::<f64>()
        {
            self.cache_simulation.ephemeral_1h_ratio = f.clamp(0.0, 1.0);
        }
        if let Ok(v) = env::var("CACHE_SIMULATION_FINGERPRINT_MAX_BREAKPOINTS")
            && let Ok(n) = v.parse::<usize>()
        {
            self.cache_simulation
                .fingerprint_max_breakpoints_per_account = n;
        }
    }
}
