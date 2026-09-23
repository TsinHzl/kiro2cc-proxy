// Copyright (c) 2026 Harllan He. Licensed under MIT.
// token_manager 测试（自 tests.rs 拆出，纯代码搬移）
#[cfg(test)]
pub(crate) mod tests {
    
    
    
    
    use super::super::super::types::MultiTokenManager;
    
    use crate::kiro::model::credentials::{
        BUILDER_ID_PLACEHOLDER_PROFILE_ARN, KiroCredentials,
    };
    
    use crate::kiro::token_manager::tests::ext_idp::tests::{
        TempDirGuard, spawn_single_response_server,
    };
    use crate::model::config::Config;
    

    #[tokio::test]
    async fn test_add_credential_reject_duplicate_refresh_token() {
        let config = Config::default();

        let mut existing = KiroCredentials::default();
        existing.refresh_token = Some("a".repeat(150));

        let manager = MultiTokenManager::new(config, vec![existing], None, None, false).unwrap();

        let mut duplicate = KiroCredentials::default();
        duplicate.refresh_token = Some("a".repeat(150));

        let result = manager.add_credential(duplicate).await;
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("账号已存在"));
    }

    /// 回归测试：账号删除后其 ID 不得被复用，否则新账号会"继承"已删除旧账号在
    /// usage/failure/throttle 日志中按 credential_id 存储的历史记录（表现为管理面板
    /// 中一个从未被调用过的新账号却显示历史请求日志）。
    ///
    /// 场景：账号 #1、#2 存在 -> 删除 #2 -> 通过 add_credential 新增一个账号，
    /// 新账号必须获得 #3，而不是复用刚被释放的 #2。
    #[tokio::test]
    async fn test_add_credential_never_reuses_deleted_id_within_same_process() {
        let body = r#"{"access_token":"new-access-token","expires_in":3600}"#;
        let endpoint = spawn_single_response_server(200, body).await;

        let mut cred1 = KiroCredentials::default();
        cred1.id = Some(1);
        cred1.refresh_token = Some("a".repeat(150));

        let mut cred2 = KiroCredentials::default();
        cred2.id = Some(2);
        cred2.refresh_token = Some("b".repeat(150));

        let config = Config::default();
        let manager =
            MultiTokenManager::new(config, vec![cred1, cred2], None, None, false).unwrap();

        // 删除账号必须先禁用
        manager.set_disabled(2, true).unwrap();
        manager.delete_credential(2).unwrap();
        assert_eq!(manager.total_count(), 1);

        // 新增账号：走 external_idp 刷新路径，指向本地 mock server
        let new_cred = KiroCredentials {
            auth_method: Some("external_idp".to_string()),
            refresh_token: Some("c".repeat(150)),
            client_id: Some("client-id".to_string()),
            token_endpoint: Some(endpoint),
            ..Default::default()
        };

        let new_id = manager.add_credential(new_cred).await.unwrap();
        assert_eq!(
            new_id, 3,
            "已删除账号 #2 的 ID 不应被复用，新账号应分配 #3，实际: {}",
            new_id
        );
    }

    /// 回归测试：账号删除后重启进程（重新构造 MultiTokenManager），新增账号仍不得
    /// 复用已删除账号曾经使用过的 ID —— 覆盖"仅按当前 credentials 列表最大值 + 1"
    /// 分配 ID 在重启场景下的复用漏洞（持久化计数器文件是本次修复的核心）。
    #[tokio::test(flavor = "multi_thread")]
    async fn test_add_credential_never_reuses_deleted_id_after_restart() {
        let dir_guard = TempDirGuard::new(&format!("k2cc_id_reuse_restart_{}", std::process::id()));
        let cred_path = dir_guard.path().join("credentials.json");

        let mut cred1 = KiroCredentials::default();
        cred1.id = Some(1);
        cred1.refresh_token = Some("a".repeat(150));

        let mut cred2 = KiroCredentials::default();
        cred2.id = Some(2);
        cred2.refresh_token = Some("b".repeat(150));

        let config = Config::default();
        let manager = MultiTokenManager::new(
            config.clone(),
            vec![cred1, cred2],
            None,
            Some(cred_path.clone()),
            true,
        )
        .unwrap();

        // 删除账号 #2（必须先禁用），并持久化
        manager.set_disabled(2, true).unwrap();
        manager.delete_credential(2).unwrap();
        manager.persist_credentials().unwrap();
        assert_eq!(manager.total_count(), 1);

        // 模拟进程重启：从磁盘重新读取 credentials.json（此时只剩 #1），
        // 重新构造一个全新的 MultiTokenManager 实例
        let persisted: Vec<KiroCredentials> =
            serde_json::from_str(&std::fs::read_to_string(&cred_path).unwrap()).unwrap();
        assert_eq!(persisted.len(), 1, "磁盘上应只剩 1 个账号");

        let reloaded =
            MultiTokenManager::new(config, persisted, None, Some(cred_path.clone()), true).unwrap();
        assert_eq!(reloaded.total_count(), 1);

        // "重启"后新增账号：若仅按当前列表 max(1) + 1 = 2 分配，将复用已删除账号 #2 的 ID
        let body = r#"{"access_token":"new-access-token","expires_in":3600}"#;
        let endpoint = spawn_single_response_server(200, body).await;
        let new_cred = KiroCredentials {
            auth_method: Some("external_idp".to_string()),
            refresh_token: Some("d".repeat(150)),
            client_id: Some("client-id".to_string()),
            token_endpoint: Some(endpoint),
            ..Default::default()
        };

        let new_id = reloaded.add_credential(new_cred).await.unwrap();
        assert_eq!(
            new_id, 3,
            "重启后新增账号仍不应复用已删除账号 #2 的 ID，实际: {}",
            new_id
        );
    }

    /// 回归测试：启动加载时为无 profileArn 的 social/idc 存量账号自动补全 fallback ARN
    /// 并写回配置文件（覆盖旧版本添加的账号，如 BuilderId 注册但 authMethod 标 idc 的形态）。
    #[test]
    fn test_new_fills_missing_profile_arn_and_persists() {
        let dir_guard = TempDirGuard::new(&format!("k2cc_fill_arn_load_{}", std::process::id()));
        let cred_path = dir_guard.path().join("credentials.json");

        let mut cred = KiroCredentials::default();
        cred.id = Some(14);
        cred.auth_method = Some("idc".to_string());
        cred.client_id = Some("client-id".to_string());
        cred.client_secret = Some("client-secret".to_string());
        cred.refresh_token = Some("r".repeat(150));

        let config = Config::default();
        let manager =
            MultiTokenManager::new(config, vec![cred], None, Some(cred_path.clone()), true)
                .unwrap();

        // 内存中已补全
        let ids = manager.credential_ids();
        assert_eq!(ids, vec![14]);
        // 触发写回：磁盘上的 credentials.json 应包含补全后的占位符 ARN
        let persisted: Vec<KiroCredentials> =
            serde_json::from_str(&std::fs::read_to_string(&cred_path).unwrap()).unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(
            persisted[0].profile_arn.as_deref(),
            Some(BUILDER_ID_PLACEHOLDER_PROFILE_ARN),
            "存量账号的 profileArn 应已补全并持久化"
        );
    }

    /// 回归测试：add_credential 添加链路会调用 fill_missing_profile_arn，
    /// 对 external_idp（不补全类型）保守跳过。
    ///
    /// 注：idc/social 形态的刷新分别硬编码请求真实 AWS OIDC / Kiro OAuth 端点
    /// （无 tokenEndpoint 注入点，external_idp 的 tokenEndpoint 仅 external_idp
    /// 刷新路径消费），单测无法 mock，故正向补全覆盖落在：
    /// - credentials.rs 纯函数单测（fill_missing_profile_arn 各分支）
    /// - MultiTokenManager::new 加载路径持久化测试（test_new_fills_missing_profile_arn_and_persists）
    /// 此处用可 mock 的 external_idp 形态验证添加链路走通且对不补全类型保守跳过。
    #[tokio::test]
    async fn test_add_credential_skips_profile_arn_fill_for_external_idp() {
        let body = r#"{"access_token":"new-access-token","expires_in":3600}"#;
        let endpoint = spawn_single_response_server(200, body).await;

        let mut cred = KiroCredentials::default();
        cred.auth_method = Some("external_idp".to_string());
        cred.refresh_token = Some("c".repeat(150));
        cred.client_id = Some("client-id".to_string());
        cred.token_endpoint = Some(endpoint);

        let config = Config::default();
        let manager = MultiTokenManager::new(config, vec![], None, None, false).unwrap();
        let new_id = manager.add_credential(cred).await.unwrap();

        let entries = manager.entries.lock();
        let added = entries.iter().find(|e| e.id == new_id).unwrap();
        assert!(
            added.credentials.profile_arn.is_none(),
            "external_idp 不应补全 ARN（真实 ARN 因租户而异）"
        );
    }

    /// 回归测试：并发调用 `allocate_new_id` 时分配的 ID 必须两两不同。
    ///
    /// `allocate_new_id` 依赖 `AtomicU64::fetch_add` 保证内存中的分配互斥唯一，但落盘
    /// 由 `save_id_counter_at_least` 负责——本测试只验证内存分配层的并发唯一性（磁盘落盘
    /// 的单调性由 `save_id_counter_at_least` 内部锁内重新读取磁盘取 max 后写入来保证，
    /// 已在实现中处理，不依赖此测试）。
    #[tokio::test(flavor = "multi_thread")]
    async fn test_allocate_new_id_concurrent_calls_never_collide() {
        let config = Config::default();
        let manager =
            std::sync::Arc::new(MultiTokenManager::new(config, vec![], None, None, false).unwrap());

        let mut handles = Vec::new();
        for _ in 0..20 {
            let m = manager.clone();
            handles.push(tokio::spawn(async move { m.allocate_new_id() }));
        }
        let mut ids: Vec<u64> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();
        ids.sort_unstable();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(
            unique.len(),
            ids.len(),
            "并发分配的 ID 不应出现重复，实际: {:?}",
            ids
        );
    }

    // MultiTokenManager 测试
}
