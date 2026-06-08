// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 设备指纹生成器
//!

use sha2::{Digest, Sha256};

use crate::kiro::model::credentials::KiroCredentials;
use crate::model::config::Config;

const OS_VERSIONS: &[&str] = &[
    "darwin#24.6.0",
    "darwin#23.6.0",
    "win32#10.0.22631",
    "win32#10.0.19045",
];

const NODE_VERSIONS: &[&str] = &[
    "20.11.1",
    "20.18.0",
    "22.11.0",
    "22.14.0",
    "22.21.1",
];

fn sha256_bytes(input: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hasher.finalize().to_vec()
}

/// 标准化 machineId 格式
///
/// 支持以下格式：
/// - 64 字符十六进制字符串（直接返回）
/// - UUID 格式（如 "2582956e-cc88-4669-b546-07adbffcb894"，移除连字符后补齐到 64 字符）
fn normalize_machine_id(machine_id: &str) -> Option<String> {
    let trimmed = machine_id.trim();

    // 如果已经是 64 字符，直接返回
    if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(trimmed.to_string());
    }

    // 尝试解析 UUID 格式（移除连字符）
    let without_dashes: String = trimmed.chars().filter(|c| *c != '-').collect();

    // UUID 去掉连字符后是 32 字符
    if without_dashes.len() == 32 && without_dashes.chars().all(|c| c.is_ascii_hexdigit()) {
        // 补齐到 64 字符（重复一次）
        return Some(format!("{}{}", without_dashes, without_dashes));
    }

    // 无法识别的格式
    None
}

/// 根据凭证信息生成唯一的 Machine ID
///
/// 优先使用账号级 machineId，其次使用 config.machineId，然后使用 refreshToken 生成
pub fn generate_from_credentials(credentials: &KiroCredentials, config: &Config) -> Option<String> {
    // 如果配置了账号级 machineId，优先使用
    if let Some(ref machine_id) = credentials.machine_id {
        if let Some(normalized) = normalize_machine_id(machine_id) {
            return Some(normalized);
        }
    }

    // 如果配置了全局 machineId，作为默认值
    if let Some(ref machine_id) = config.machine_id {
        if let Some(normalized) = normalize_machine_id(machine_id) {
            return Some(normalized);
        }
    }

    // 使用 refreshToken 生成
    if let Some(ref refresh_token) = credentials.refresh_token {
        if !refresh_token.is_empty() {
            return Some(sha256_hex(&format!("KotlinNativeAPI/{}", refresh_token)));
        }
    }

    // 没有有效的凭证
    None
}

/// 根据账号 refreshToken 确定性地派生 OS 版本字符串
///
/// 保证同账号跨会话稳定，跨账号各不相同。
/// 无 refreshToken 时回退到 config.system_version。
pub fn derive_os_fingerprint(credentials: &KiroCredentials, config: &Config) -> String {
    if let Some(ref rt) = credentials.refresh_token {
        if !rt.is_empty() {
            let hash = sha256_bytes(rt);
            let idx = (hash[0] as usize) % OS_VERSIONS.len();
            return OS_VERSIONS[idx].to_string();
        }
    }
    config.system_version.clone()
}

/// 根据账号 refreshToken 确定性地派生 Node 版本字符串
///
/// 使用 hash[1] 与 OS 指纹使用 hash[0] 区分，保证两者独立。
/// 无 refreshToken 时回退到 config.node_version。
pub fn derive_node_version(credentials: &KiroCredentials, config: &Config) -> String {
    if let Some(ref rt) = credentials.refresh_token {
        if !rt.is_empty() {
            let hash = sha256_bytes(rt);
            let idx = (hash[1] as usize) % NODE_VERSIONS.len();
            return NODE_VERSIONS[idx].to_string();
        }
    }
    config.node_version.clone()
}

/// SHA256 哈希实现（返回十六进制字符串）
fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_hex() {
        let result = sha256_hex("test");
        assert_eq!(result.len(), 64);
        assert_eq!(
            result,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }

    #[test]
    fn test_generate_with_custom_machine_id() {
        let credentials = KiroCredentials::default();
        let mut config = Config::default();
        config.machine_id = Some("a".repeat(64));

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result, Some("a".repeat(64)));
    }

    #[test]
    fn test_generate_with_credential_machine_id_overrides_config() {
        let mut credentials = KiroCredentials::default();
        credentials.machine_id = Some("b".repeat(64));

        let mut config = Config::default();
        config.machine_id = Some("a".repeat(64));

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result, Some("b".repeat(64)));
    }

    #[test]
    fn test_generate_with_refresh_token() {
        let mut credentials = KiroCredentials::default();
        credentials.refresh_token = Some("test_refresh_token".to_string());
        let config = Config::default();

        let result = generate_from_credentials(&credentials, &config);
        assert!(result.is_some());
        assert_eq!(result.as_ref().unwrap().len(), 64);
    }

    #[test]
    fn test_generate_without_credentials() {
        let credentials = KiroCredentials::default();
        let config = Config::default();

        let result = generate_from_credentials(&credentials, &config);
        assert!(result.is_none());
    }

    #[test]
    fn test_normalize_uuid_format() {
        // UUID 格式应该被转换为 64 字符
        let uuid = "2582956e-cc88-4669-b546-07adbffcb894";
        let result = normalize_machine_id(uuid);
        assert!(result.is_some());
        let normalized = result.unwrap();
        assert_eq!(normalized.len(), 64);
        // UUID 去掉连字符后重复一次
        assert_eq!(
            normalized,
            "2582956ecc884669b54607adbffcb8942582956ecc884669b54607adbffcb894"
        );
    }

    #[test]
    fn test_normalize_64_char_hex() {
        // 64 字符十六进制应该直接返回
        let hex64 = "a".repeat(64);
        let result = normalize_machine_id(&hex64);
        assert_eq!(result, Some(hex64));
    }

    #[test]
    fn test_normalize_invalid_format() {
        // 无效格式应该返回 None
        assert!(normalize_machine_id("invalid").is_none());
        assert!(normalize_machine_id("too-short").is_none());
        assert!(normalize_machine_id(&"g".repeat(64)).is_none()); // 非十六进制
    }

    #[test]
    fn test_generate_with_uuid_machine_id() {
        let mut credentials = KiroCredentials::default();
        credentials.machine_id = Some("2582956e-cc88-4669-b546-07adbffcb894".to_string());

        let config = Config::default();

        let result = generate_from_credentials(&credentials, &config);
        assert!(result.is_some());
        assert_eq!(result.as_ref().unwrap().len(), 64);
    }

    #[test]
    fn test_derive_os_fingerprint_stable() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test_token_abc".to_string());
        let config = Config::default();

        let r1 = derive_os_fingerprint(&creds, &config);
        let r2 = derive_os_fingerprint(&creds, &config);
        assert_eq!(r1, r2);
        assert!(OS_VERSIONS.contains(&r1.as_str()));
    }

    #[test]
    fn test_derive_os_fingerprint_diverse() {
        let config = Config::default();
        let tokens = ["token_a", "token_b", "token_c", "token_d", "token_e", "token_f", "token_g", "token_h"];
        let results: Vec<String> = tokens.iter().map(|t| {
            let mut creds = KiroCredentials::default();
            creds.refresh_token = Some(t.to_string());
            derive_os_fingerprint(&creds, &config)
        }).collect();
        let unique: std::collections::HashSet<_> = results.iter().collect();
        assert!(unique.len() >= 2, "8 个不同 token 应产生至少 2 种不同 OS，实际: {:?}", unique);
    }

    #[test]
    fn test_derive_os_fingerprint_fallback_no_token() {
        let creds = KiroCredentials::default();
        let mut config = Config::default();
        config.system_version = "darwin#99.0.0".to_string();

        let result = derive_os_fingerprint(&creds, &config);
        assert_eq!(result, "darwin#99.0.0");
    }

    #[test]
    fn test_derive_node_version_stable() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("test_token_abc".to_string());
        let config = Config::default();

        let r1 = derive_node_version(&creds, &config);
        let r2 = derive_node_version(&creds, &config);
        assert_eq!(r1, r2);
        assert!(NODE_VERSIONS.contains(&r1.as_str()));
    }

    #[test]
    fn test_derive_node_version_fallback_no_token() {
        let creds = KiroCredentials::default();
        let mut config = Config::default();
        config.node_version = "99.0.0".to_string();

        let result = derive_node_version(&creds, &config);
        assert_eq!(result, "99.0.0");
    }

    #[test]
    fn test_os_and_node_independent() {
        let mut creds = KiroCredentials::default();
        creds.refresh_token = Some("independence_test".to_string());
        let config = Config::default();

        let os = derive_os_fingerprint(&creds, &config);
        let node = derive_node_version(&creds, &config);
        assert!(OS_VERSIONS.contains(&os.as_str()));
        assert!(NODE_VERSIONS.contains(&node.as_str()));
    }
}
