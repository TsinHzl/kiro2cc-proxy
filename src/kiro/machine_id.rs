// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! 设备指纹生成器
//!

use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::kiro::model::credentials::KiroCredentials;
use crate::model::config::Config;

/// 兜底 machineId 缓存（按凭据 id 分桶，进程生命周期内稳定）
///
/// key 为 `credentials.id`；无 id 的凭据共享同一个兜底值（正常流程不会出现）。
static FALLBACK_MACHINE_IDS: OnceLock<Mutex<HashMap<Option<u64>, String>>> = OnceLock::new();

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
pub fn generate_from_credentials(credentials: &KiroCredentials, config: &Config) -> String {
    // 如果配置了账号级 machineId，优先使用
    if let Some(ref machine_id) = credentials.machine_id
        && let Some(normalized) = normalize_machine_id(machine_id)
    {
        return normalized;
    }

    // 如果配置了全局 machineId，作为默认值
    if let Some(ref machine_id) = config.machine_id
        && let Some(normalized) = normalize_machine_id(machine_id)
    {
        return normalized;
    }

    // 按凭据类型派生。两条路径互斥、不互相回落：
    // API Key 凭据没有 refreshToken，OAuth 凭据没有 kiroApiKey，
    // 若允许回落会让同一账号在不同调用间产生不同 machineId。
    if credentials.is_api_key_credential() {
        if let Some(ref api_key) = credentials.kiro_api_key
            && !api_key.is_empty()
        {
            // 与 kiro-go `config.MachineIdFromAPIKey` 完全一致：
            // sha256 hex of "KiroAPIKey/<api_key>"（Kiro CLI/API-key 客户端设备指纹）
            return sha256_hex(&format!("KiroAPIKey/{}", api_key));
        }
        // 声明 api_key 但 key 为空：不得回落到 refreshToken 派生，否则同一账号在
        // 配置修好前后会得到不同 machineId。启动时该账号已按 InvalidConfig 自动禁用，
        // 这里只兜底生成进程内稳定的随机值，避免整条请求硬失败。
        tracing::warn!(
            credential_id = ?credentials.id,
            "API Key 凭据缺少 kiroApiKey，不回落到 refreshToken 派生 machineId"
        );
        return fallback_machine_id(credentials);
    }

    // 使用 refreshToken 生成
    if let Some(ref refresh_token) = credentials.refresh_token
        && !refresh_token.is_empty()
    {
        return sha256_hex(&format!("KotlinNativeAPI/{}", refresh_token));
    }

    // 没有有效的派生材料：走兜底流程，避免整条请求硬失败
    fallback_machine_id(credentials)
}

/// 为缺失派生材料的凭据生成兜底 machineId
///
/// - 经 `sha256("KiroFallback/<uuid>")` 派生，输出格式与正常路径一致（64 字符十六进制）
/// - 按 `credentials.id` 在进程内缓存；同一凭据多次调用返回同一值
/// - 数组格式凭据会被 `MultiTokenManager::new` 写回 `cred.machine_id` 并持久化到磁盘，
///   重启后视为已配置值直接复用，不再重新随机；仅单账号对象格式（不落盘）才会在每次
///   进程启动时重新随机
fn fallback_machine_id(credentials: &KiroCredentials) -> String {
    let cache = FALLBACK_MACHINE_IDS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock();
    if let Some(existing) = map.get(&credentials.id) {
        return existing.clone();
    }

    let seed = Uuid::new_v4();
    let derived = sha256_hex(&format!("KiroFallback/{}", seed));
    tracing::warn!(
        credential_id = ?credentials.id,
        "凭据缺少派生材料（machineId/refreshToken 均不可用），使用随机兜底 machineId（进程内稳定）"
    );
    map.insert(credentials.id, derived.clone());
    derived
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
        assert_eq!(result, "a".repeat(64));
    }

    #[test]
    fn test_generate_with_credential_machine_id_overrides_config() {
        let mut credentials = KiroCredentials::default();
        credentials.machine_id = Some("b".repeat(64));

        let mut config = Config::default();
        config.machine_id = Some("a".repeat(64));

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result, "b".repeat(64));
    }

    #[test]
    fn test_generate_with_refresh_token() {
        let mut credentials = KiroCredentials::default();
        credentials.refresh_token = Some("test_refresh_token".to_string());
        let config = Config::default();

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result.len(), 64);
    }

    #[test]
    fn test_generate_from_api_key_uses_kiro_go_seed() {
        // 与 kiro-go config.MachineIdFromAPIKey 对齐：sha256 hex of "KiroAPIKey/<key>"
        let mut credentials = KiroCredentials::default();
        credentials.auth_method = Some("api_key".to_string());
        credentials.kiro_api_key = Some("ksk_test".to_string());
        let config = Config::default();

        let derived = generate_from_credentials(&credentials, &config);
        let expected = sha256_hex("KiroAPIKey/ksk_test");
        assert_eq!(derived, expected);
        assert_eq!(derived.len(), 64);
        // 稳定性：同一 key 多次派生一致
        assert_eq!(generate_from_credentials(&credentials, &config), derived);
    }

    #[test]
    fn test_generate_from_api_key_differs_from_refresh_token_seed() {
        let mut api = KiroCredentials::default();
        api.kiro_api_key = Some("ksk_test".to_string());
        let mut oauth = KiroCredentials::default();
        oauth.refresh_token = Some("ksk_test".to_string());
        let config = Config::default();

        assert_ne!(
            generate_from_credentials(&api, &config),
            generate_from_credentials(&oauth, &config),
            "API Key 与 refreshToken 派生必须使用不同种子"
        );
    }

    #[test]
    fn test_api_key_credential_does_not_fall_back_to_refresh_token() {
        // 两条路径互斥：声明 api_key 但 key 为空时，不得回落到 refreshToken 派生，
        // 否则同一账号在配置修好前后会得到不同 machineId
        let mut cred = KiroCredentials::default();
        cred.id = Some(u64::MAX - 30);
        cred.auth_method = Some("api_key".to_string());
        cred.refresh_token = Some("a".repeat(150));
        let config = Config::default();

        let first = generate_from_credentials(&cred, &config);
        assert_eq!(first.len(), 64);
        assert_ne!(first, sha256_hex(&format!("KotlinNativeAPI/{}", "a".repeat(150))));
        // 兜底值进程内稳定
        assert_eq!(first, generate_from_credentials(&cred, &config));
    }

    #[test]
    fn test_generate_without_credentials_uses_fallback() {
        let credentials = KiroCredentials::default();
        let config = Config::default();

        let result = generate_from_credentials(&credentials, &config);
        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_fallback_is_stable_per_credential() {
        let mut credentials = KiroCredentials::default();
        credentials.id = Some(u64::MAX - 10);
        let config = Config::default();

        let first = generate_from_credentials(&credentials, &config);
        let second = generate_from_credentials(&credentials, &config);
        assert_eq!(first, second);
    }

    #[test]
    fn test_fallback_differs_across_credentials() {
        let mut cred_a = KiroCredentials::default();
        cred_a.id = Some(u64::MAX - 20);
        let mut cred_b = KiroCredentials::default();
        cred_b.id = Some(u64::MAX - 21);
        let config = Config::default();

        let id_a = generate_from_credentials(&cred_a, &config);
        let id_b = generate_from_credentials(&cred_b, &config);
        assert_ne!(id_a, id_b);
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
        assert_eq!(result.len(), 64);
    }
}
