// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! KiroCredentials 行为方法：region/端点/代理解析、ARN 补全、订阅判断

use std::fs;
use std::path::Path;

use crate::http_client::ProxyConfig;
use crate::model::config::Config;

use super::model::{
    BUILDER_ID_PLACEHOLDER_PROFILE_ARN, ENTERPRISE_FALLBACK_PROFILE_ARN_EU,
    ENTERPRISE_FALLBACK_PROFILE_ARN_US, KiroCredentials, canonicalize_auth_method_value,
    fallback_profile_arn_value,
};

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
    ///
    /// 注意：API Key 凭据**不**在这里特殊限制端点。参考实现（fork）与上游
    /// v3.4.33 一样保留 4 端点轮询，只通过 `TokenType: API_KEY` 标识凭据类型；
    /// kiro-go 的 CLI runtime 协议是 ROOT path + `x-amz-target`，与本项目的
    /// Runtime 端点（`/generateAssistantResponse`、无 target）不同，
    /// 强行钉死 Runtime 反而可能破坏请求。
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

    /// 是否为 API Key 凭据
    ///
    /// API Key 凭据直接把 `kiroApiKey` 作为 Bearer Token 使用，无需 `refreshToken`，
    /// 也不参与 token 刷新。只要带了 `kiroApiKey` 字段或声明了 `authMethod: api_key`
    /// 就算（容忍 `apikey` 别名），便于只填一个字段就能接入。
    pub fn is_api_key_credential(&self) -> bool {
        self.kiro_api_key.is_some()
            || self
                .auth_method
                .as_deref()
                .is_some_and(|m| canonicalize_auth_method_value(m).eq_ignore_ascii_case("api_key"))
    }

    /// 是否为企业 SSO（external_idp）凭据
    ///
    /// 容忍未规范化的别名（azuread / entraid），统一按规范值判断。
    pub fn is_external_idp_credential(&self) -> bool {
        self.auth_method
            .as_deref()
            .is_some_and(|m| canonicalize_auth_method_value(m).eq_ignore_ascii_case("external_idp"))
    }

    /// 返回脱敏后的 API Key，供管理面板展示（无 key 则 None）
    ///
    /// 格式：前 4 + `...` + 后 4，例如 `ksk_...a1b2`。长度不足 16 或含非 ASCII
    /// 字符时回退为 `***` —— 短 key 保留首尾会暴露过高比例，非 ASCII 按字节切片
    /// 可能落在 UTF-8 字符边界内导致 panic。
    ///
    /// API Key 账号没有邮箱/昵称可标识，卡片上只能显示"账号 #N"，
    /// 多个 API Key 账号无法区分，故需要这个展示值。
    pub fn masked_api_key(&self) -> Option<String> {
        let key = self.kiro_api_key.as_deref()?;
        if key.is_ascii() && key.len() > 16 {
            Some(format!("{}...{}", &key[..4], &key[key.len() - 4..]))
        } else {
            Some("***".to_string())
        }
    }

    /// 返回调用上游时应携带的 `TokenType` 头值（无则 None）
    ///
    /// - API Key 凭据 → `API_KEY`
    /// - 企业 SSO 凭据 → `EXTERNAL_IDP`
    /// - social / idc → None
    ///
    /// API Key 判定优先于 external_idp：两者同时命中时按 API Key 处理，
    /// 因为此时实际发出的 Bearer Token 来自 `kiroApiKey`。
    pub fn token_type_header(&self) -> Option<&'static str> {
        if self.is_api_key_credential() {
            Some("API_KEY")
        } else if self.is_external_idp_credential() {
            Some("EXTERNAL_IDP")
        } else {
            None
        }
    }

    /// 调用上游数据面时实际要发的 Bearer Token
    ///
    /// - API Key 凭据 → `kiroApiKey`（原样，不 trim：kiro-go 也原样使用；
    ///   仅在添加/启动校验时按 trim 后是否为空判断有效性）
    /// - 其余凭据 → None（由 OAuth 刷新流程提供 access_token）
    pub fn api_key_bearer_token(&self) -> Option<&str> {
        if !self.is_api_key_credential() {
            return None;
        }
        self.kiro_api_key.as_deref().filter(|k| !k.trim().is_empty())
    }

    /// 缺失或为空字符串（脏数据）时按账号类型自动补全 fallback ARN（social → 固定
    /// Social ARN、idc/builder_id → BuilderId 占位符），随凭据持久化后供数据面与
    /// 控制面（额度查询/订阅展示/模型列表）所有路径统一使用。
    ///
    /// external_idp（企业 IdC）及未知类型不补全：真实 ARN 因租户而异，
    /// 缺失属确定性配置缺陷，保留 None 交由数据面 400 触发 ProfileArnMissing 禁用。
    /// API Key 凭据同样不补全：其语义（Kiro CLI）不携带任何 profileArn。
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

    /// 是否 idc 账号（含 BuilderId 与企业版 IdC，二者共用 OIDC clientId/clientSecret 刷新）
    ///
    /// 与 `refresh_token` 的判定保持一致：显式声明的 `auth_method` 优先；
    /// 缺失（`None`）时，clientId 与 clientSecret 均存在则按 idc 处理（配置文件允许省略 authMethod）。
    pub fn is_idc(&self) -> bool {
        match self.auth_method.as_deref() {
            Some(m) => canonicalize_auth_method_value(m.trim()) == "idc",
            None => self.client_id.is_some() && self.client_secret.is_some(),
        }
    }

    /// profileArn 缺失、为空，或只是代理自带的占位 / 备用 ARN（BuilderId 占位符、企业备用 ARN），
    /// 不是该账号租户自己的真实 ARN。
    ///
    /// 企业版 IdC 账号在这种状态下可能无法通过额度/对话请求，需要先用
    /// `ListAvailableProfiles` 发现真实 ARN（issue #43）。
    pub fn has_no_real_profile_arn(&self) -> bool {
        match self.profile_arn.as_deref().map(str::trim) {
            None | Some("") => true,
            Some(arn) => [
                BUILDER_ID_PLACEHOLDER_PROFILE_ARN,
                ENTERPRISE_FALLBACK_PROFILE_ARN_US,
                ENTERPRISE_FALLBACK_PROFILE_ARN_EU,
            ]
            .contains(&arn),
        }
    }

    /// 是否企业版 IdC 租户账号：IdC 登录且持有租户自己的真实 profileArn
    ///
    /// 这类账号由组织授权，不会是 Free；个人 BuilderId 使用的是代理占位 ARN，不在此列。
    /// 企业版的 `getUsageLimits` 可能不返回订阅标题，此时用它判断账号等级。
    pub fn is_tenant_idc(&self) -> bool {
        self.is_idc() && !self.has_no_real_profile_arn()
    }

    /// 检查账号是否支持 Opus 模型
    ///
    /// - 有订阅标题：非 Free 才支持（标题含 FREE 一律不支持）
    /// - 无订阅标题：仅企业版 IdC 租户账号（见 [`Self::is_tenant_idc`]）支持，
    ///   其余等级未知的账号（含个人 BuilderId）仍不支持
    pub fn supports_opus(&self) -> bool {
        match self
            .subscription_title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
        {
            Some(title) => !title.to_uppercase().contains("FREE"),
            None => self.is_tenant_idc(),
        }
    }
}
