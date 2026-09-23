// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! KiroCredentials 行为方法：region/端点/代理解析、ARN 补全、订阅判断

use std::fs;
use std::path::Path;

use crate::http_client::ProxyConfig;
use crate::model::config::Config;

use super::model::{KiroCredentials, canonicalize_auth_method_value, fallback_profile_arn_value};

impl KiroCredentials {
    /// 特殊值：显式不使用代理
    pub const PROXY_DIRECT: &'static str = "direct";

    /// 获取默认凭证文件路径
    pub fn default_credentials_path() -> &'static str {
        "credentials.json"
    }

    /// 获取有效的 Auth Region（用于 Token 刷新）
    /// 优先级：账号.auth_region > 账号.region > config.auth_region > config.region
    pub fn effective_auth_region<'a>(&'a self, config: &'a Config) -> &'a str {
        self.auth_region
            .as_deref()
            .or(self.region.as_deref())
            .unwrap_or(config.effective_auth_region())
    }

    /// 获取有效的 API Region（用于 API 请求）
    /// 优先级：账号.api_region > config.api_region > config.region
    pub fn effective_api_region<'a>(&'a self, config: &'a Config) -> &'a str {
        self.api_region
            .as_deref()
            .unwrap_or(config.effective_api_region())
    }

    /// 获取账号的多端点列表（多端点 LB 使用）
    ///
    /// - 未配置 / `None` / 空 Vec → 全部 4 个端点（按 `Endpoint::default_order`）
    /// - 配置 → 首选端点在前 + 剩余端点按默认顺序去重追加
    pub fn effective_endpoints(&self, region: &str) -> Vec<crate::kiro::endpoint::Endpoint> {
        use crate::kiro::endpoint::{Endpoint, EndpointName};

        let defaults: Vec<EndpointName> = Endpoint::default_order().to_vec();

        let preferred = match &self.endpoint {
            Some(v) if !v.is_empty() => v.clone(),
            _ => return Endpoint::all(region).to_vec(),
        };

        // 去重（保留首次出现顺序）+ 过滤未知 enum 变体（serde 已保证，理论不可达）
        let mut seen = std::collections::HashSet::new();
        let mut result: Vec<EndpointName> = Vec::with_capacity(4);
        for name in preferred.into_iter().chain(defaults) {
            if seen.insert(name) {
                result.push(name);
            }
        }
        result
            .into_iter()
            .map(|n| Endpoint::by_name(n, region))
            .collect()
    }

    /// 获取有效的代理配置
    /// 优先级：账号代理 > 全局代理 > 无代理
    /// 特殊值 "direct" 表示显式不使用代理（即使全局配置了代理）
    pub fn effective_proxy(&self, global_proxy: Option<&ProxyConfig>) -> Option<ProxyConfig> {
        match self.proxy_url.as_deref() {
            Some(url) if url.eq_ignore_ascii_case(Self::PROXY_DIRECT) => None,
            Some(url) => {
                let mut proxy = ProxyConfig::new(url);
                if let (Some(username), Some(password)) =
                    (&self.proxy_username, &self.proxy_password)
                {
                    proxy = proxy.with_auth(username, password);
                }
                Some(proxy)
            }
            None => global_proxy.cloned(),
        }
    }

    /// 从 JSON 字符串解析凭证
    #[allow(dead_code)]
    pub fn from_json(json_string: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_string)
    }

    /// 从文件加载凭证
    #[allow(dead_code)]
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        if content.is_empty() {
            anyhow::bail!("凭证文件为空: {:?}", path.as_ref());
        }
        let credentials = Self::from_json(&content)?;
        Ok(credentials)
    }

    /// 序列化为格式化的 JSON 字符串
    #[allow(dead_code)]
    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn canonicalize_auth_method(&mut self) {
        let auth_method = match &self.auth_method {
            Some(m) => m,
            None => return,
        };

        let canonical = canonicalize_auth_method_value(auth_method);
        if canonical != auth_method {
            self.auth_method = Some(canonical.to_string());
        }
    }

    /// 缺失或为空字符串（脏数据）时按账号类型自动补全 fallback ARN（social → 固定
    /// Social ARN、idc/builder_id → BuilderId 占位符），随凭据持久化后供数据面与
    /// 控制面（额度查询/订阅展示/模型列表）所有路径统一使用。
    ///
    /// external_idp（企业 IdC）及未知类型不补全：真实 ARN 因租户而异，
    /// 缺失属确定性配置缺陷，保留 None 交由数据面 400 触发 ProfileArnMissing 禁用。
    /// 已有非空 profile_arn（含用户显式填写）时不覆盖，返回 false。
    pub fn fill_missing_profile_arn(&mut self) -> bool {
        if self.profile_arn.as_deref().is_some_and(|s| !s.is_empty()) {
            return false;
        }
        match fallback_profile_arn_value(self) {
            Some(arn) => {
                self.profile_arn = Some(arn.to_string());
                true
            }
            None => false,
        }
    }

    /// 检查账号是否支持 Opus 模型
    ///
    /// Free 账号不支持 Opus 模型，需要 PRO 或更高等级订阅
    pub fn supports_opus(&self) -> bool {
        match &self.subscription_title {
            Some(title) => {
                let title_upper = title.to_uppercase();
                // 如果包含 FREE，则不支持 Opus
                !title_upper.contains("FREE")
            }
            // 如果还没有获取订阅信息，暂时允许（首次使用时会获取）
            None => true,
        }
    }
}
