//! 诊断日志辅助：上游响应头筛选、机器码脱敏、代理出口标签。
//!
//! 这里的函数只服务于 DEBUG / WARN 诊断日志。输出内容不得包含 token、
//! 代理账号密码、原始 machine id，因此代理地址只保留 scheme，机器码只保留哈希。
use reqwest::header::HeaderMap;
use sha2::{Digest, Sha256};

use crate::http_client::ProxyConfig;
use crate::kiro::model::credentials::KiroCredentials;

/// 单个响应头值在日志中的最大字符数，防止异常长的头撑爆日志行
const HEADER_VALUE_MAX_CHARS: usize = 120;

/// 是否为诊断相关的上游响应头：AWS 请求 ID、错误类型、限流与重试提示。
fn is_diagnostic_header(name: &str) -> bool {
    name.starts_with("x-amzn-")
        || name.starts_with("x-amz-")
        || name.starts_with("x-ratelimit-")
        || name == "retry-after"
}

/// 把上游响应头整理为 `name=value, name=value`（按名称排序），只保留诊断相关的头。
///
/// 没有可记录的头时返回 `<none>`，便于在日志里区分"上游没给"和"没记录"。
pub(crate) fn format_upstream_headers(headers: &HeaderMap) -> String {
    let mut parts: Vec<String> = headers
        .iter()
        .filter(|(name, _)| is_diagnostic_header(name.as_str()))
        .map(|(name, value)| {
            let raw = value.to_str().unwrap_or("<non-ascii>");
            let shown: String = raw.chars().take(HEADER_VALUE_MAX_CHARS).collect();
            format!("{}={}", name.as_str(), shown)
        })
        .collect();
    if parts.is_empty() {
        return "<none>".to_string();
    }
    parts.sort();
    parts.join(", ")
}

/// 机器码的短哈希（12 位十六进制）：同一账号的指纹在日志中保持一致，但不暴露原值。
pub(crate) fn hash_machine_id(machine_id: &str) -> String {
    Sha256::digest(machine_id.as_bytes())
        .iter()
        .take(6)
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// 把文本中出现的原始机器码替换为哈希。机器码为空时原样返回，避免空串替换。
pub(crate) fn redact_machine_id(text: &str, machine_id: &str) -> String {
    if machine_id.is_empty() {
        return text.to_string();
    }
    text.replace(machine_id, &hash_machine_id(machine_id))
}

/// 代理出口标签，形如 `account-direct:none`、`account:socks5`、`global:http`。
///
/// 只输出来源（账号级 / 全局）和 scheme，不输出地址、用户名或密码。
pub(crate) fn proxy_label(credentials: &KiroCredentials, global: Option<&ProxyConfig>) -> String {
    let source = match credentials.proxy_url.as_deref() {
        Some(url) if url.eq_ignore_ascii_case(KiroCredentials::PROXY_DIRECT) => "account-direct",
        Some(_) => "account",
        None => "global",
    };
    match credentials.effective_proxy(global) {
        Some(proxy) => format!("{}:{}", source, proxy_scheme(&proxy.url)),
        None => format!("{}:none", source),
    }
}

fn proxy_scheme(url: &str) -> &str {
    url.split_once("://")
        .map(|(scheme, _)| scheme)
        .unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderName, HeaderValue};

    fn insert(map: &mut HeaderMap, name: &str, value: &str) {
        map.insert(
            name.parse::<HeaderName>().unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }

    #[test]
    fn upstream_headers_keep_only_diagnostic_ones() {
        let mut map = HeaderMap::new();
        insert(&mut map, "x-amzn-requestid", "req-123");
        insert(&mut map, "x-amzn-errortype", "ThrottlingException");
        insert(&mut map, "retry-after", "30");
        insert(&mut map, "set-cookie", "session=secret");
        insert(&mut map, "content-type", "application/json");

        let text = format_upstream_headers(&map);
        assert!(text.contains("x-amzn-requestid=req-123"));
        assert!(text.contains("x-amzn-errortype=ThrottlingException"));
        assert!(text.contains("retry-after=30"));
        assert!(!text.contains("set-cookie"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("content-type"));
    }

    #[test]
    fn upstream_headers_truncate_long_values() {
        let mut map = HeaderMap::new();
        let long = "a".repeat(500);
        insert(&mut map, "x-amzn-requestid", &long);
        let text = format_upstream_headers(&map);
        assert_eq!(
            text.len(),
            "x-amzn-requestid=".len() + HEADER_VALUE_MAX_CHARS
        );
    }

    #[test]
    fn upstream_headers_report_none_when_empty() {
        assert_eq!(format_upstream_headers(&HeaderMap::new()), "<none>");
    }

    #[test]
    fn machine_id_hash_is_stable_and_hides_original() {
        let id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let hash = hash_machine_id(id);
        assert_eq!(hash.len(), 12);
        assert_eq!(hash, hash_machine_id(id));
        assert!(!hash.contains("0123456789abcdef"));
    }

    #[test]
    fn redact_machine_id_replaces_raw_value() {
        let id = "machine-raw-value";
        let ua = format!("aws-sdk-js/1.0.27 KiroIDE-1.4.121-{}", id);
        let redacted = redact_machine_id(&ua, id);
        assert!(!redacted.contains(id));
        assert!(redacted.contains(&hash_machine_id(id)));
        assert_eq!(redact_machine_id("abc", ""), "abc");
    }

    #[test]
    fn proxy_label_shows_scheme_without_credentials() {
        let creds = KiroCredentials {
            proxy_url: Some("socks5://user:pw@proxy.example.com:1080".to_string()),
            ..Default::default()
        };
        let label = proxy_label(&creds, None);
        assert_eq!(label, "account:socks5");
        assert!(!label.contains("pw"));
        assert!(!label.contains("proxy.example.com"));
    }

    #[test]
    fn proxy_label_marks_direct_and_global() {
        let direct = KiroCredentials {
            proxy_url: Some("direct".to_string()),
            ..Default::default()
        };
        assert_eq!(proxy_label(&direct, None), "account-direct:none");

        let global = ProxyConfig::new("http://127.0.0.1:7890");
        assert_eq!(
            proxy_label(&KiroCredentials::default(), Some(&global)),
            "global:http"
        );
        assert_eq!(
            proxy_label(&KiroCredentials::default(), None),
            "global:none"
        );
    }
}
