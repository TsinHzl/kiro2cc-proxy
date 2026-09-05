// Copyright (c) 2026 Harllan He. Licensed under MIT.
use crate::common::fs::atomic_write;
use chrono::{DateTime, Utc};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// 单个 API Key
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKey {
    pub id: u32,
    pub key: String,
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// 额度限制数值，None 表示不限额（按日期模式）
    /// 单位由 `limit_unit` 决定：`"usd"`（默认，estimated_cost 累加）或 `"credits"`（真实 credits 累加）
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spending_limit: Option<f64>,
    /// 额度计量单位（"usd" | "credits"），默认 "usd" 保持向后兼容
    #[serde(default = "default_limit_unit")]
    pub limit_unit: String,
    /// 有效期天数（懒激活模式），首次使用后才开始计时
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_days: Option<f64>,
    /// 首次使用激活时间
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<DateTime<Utc>>,
    /// 绑定的账号 ID 列表，None 或空列表表示不限制（使用全局策略）
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bound_credential_ids: Option<Vec<u64>>,
}

fn default_enabled() -> bool {
    true
}

fn default_limit_unit() -> String {
    "usd".to_string()
}

impl ApiKey {
    /// 生成新的 API Key
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u32,
        name: String,
        expires_at: Option<DateTime<Utc>>,
        spending_limit: Option<f64>,
        limit_unit: String,
        duration_days: Option<f64>,
        bound_credential_ids: Option<Vec<u64>>,
    ) -> Self {
        Self {
            id,
            key: generate_api_key(),
            name,
            enabled: true,
            created_at: Utc::now(),
            expires_at,
            spending_limit,
            limit_unit,
            duration_days,
            activated_at: None,
            bound_credential_ids,
        }
    }

    /// 检查 key 是否有效（启用且未过期）
    #[allow(dead_code)]
    pub fn is_valid(&self) -> bool {
        if !self.enabled {
            return false;
        }
        if let Some(expires_at) = self.expires_at {
            return Utc::now() < expires_at;
        }
        true
    }

    /// 检查是否已过期
    /// 待激活状态（duration_days 有值但 activated_at 为 None）返回 false
    pub fn is_expired(&self) -> bool {
        if self.duration_days.is_some() && self.activated_at.is_none() {
            return false;
        }
        self.expires_at
            .map(|exp| Utc::now() >= exp)
            .unwrap_or(false)
    }

    /// 检查是否为活跃状态（已激活且未过期）
    pub fn is_active(&self) -> bool {
        self.activated_at.is_some() && !self.is_expired()
    }

    /// 激活 key：设置 activated_at 并计算 expires_at
    /// 幂等操作，已激活的 key 直接跳过
    pub fn activate(&mut self) -> bool {
        if self.activated_at.is_some() || self.duration_days.is_none() {
            return false;
        }
        let now = Utc::now();
        let days = self.duration_days.unwrap();
        let duration = chrono::Duration::milliseconds((days * 86_400_000.0) as i64);
        self.activated_at = Some(now);
        self.expires_at = Some(now + duration);
        true
    }
}
/// 生成 sk- 前缀的随机 API Key
fn generate_api_key() -> String {
    let id = uuid::Uuid::new_v4();
    format!("sk-{}", id.simple())
}

/// API Key 认证结果
pub enum ApiKeyAuthResult {
    /// 认证通过，携带 key ID 和名称
    Valid {
        id: u32,
        name: String,
        spending_limit: Option<f64>,
        limit_unit: String,
        bound_credential_ids: Option<Vec<u64>>,
    },
    /// Key 已被禁用
    Disabled,
    /// Key 已过期
    Expired,
    /// Key 不存在
    NotFound,
}

/// API Key 管理器（线程安全）
pub struct ApiKeyManager {
    keys: RwLock<Vec<ApiKey>>,
    file_path: PathBuf,
    /// 历史最大 API Key ID（单调递增，跨删除/重启持久化）
    ///
    /// key 删除后其 id 会从 `api_keys.json` 中消失，但 `api_key_usage.json` 中按
    /// `api_key_id` 存储的用量记录并不清理。若新 key 仍按"当前列表最大值 + 1"分配，
    /// 会复用已删除 key 的 id，从而继承其历史用量与累计消费额——后者是消费上限的
    /// 判定依据，新 key 可能一次请求都没发就被判超额。
    next_id_counter: AtomicU32,
    /// 计数器落盘串行锁：串行化"重读磁盘 → 取 max → 写盘"
    counter_lock: Mutex<()>,
}

impl ApiKeyManager {
    /// 从文件加载，文件不存在则创建空列表
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let keys: Vec<ApiKey> = if path.exists() {
            let content = fs::read_to_string(&path)?;
            if content.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&content)?
            }
        } else {
            Vec::new()
        };

        // ID 计数器种子 = 当前列表最大 id 与 持久化计数器 的较大值。
        // 二者缺一不可：列表为空时靠计数器兜底；列表非空但 id 均小于计数器时
        // （如重启后只剩 #1，而计数器已因此前存在过 #5 记为 5），仍需取计数器，
        // 否则会丢失磁盘上的历史高位 ID，导致新 key 复用已删除 key 的 id。
        let max_existing = keys.iter().map(|k| k.id).max().unwrap_or(0);
        let persisted = Self::load_id_counter_from_path(&path);
        let seed = max_existing.max(persisted);

        let manager = Self {
            keys: RwLock::new(keys),
            file_path: path,
            next_id_counter: AtomicU32::new(seed),
            counter_lock: Mutex::new(()),
        };

        // 无条件落盘，自愈缺失/损坏的计数器文件（存量部署首次运行即走此路径）
        manager.save_id_counter_at_least(seed);

        Ok(manager)
    }

    /// ID 计数器文件路径（与 `api_keys.json` 同目录）
    ///
    /// 独立于 key 列表本身持久化，确保 key 被删除后该文件仍保留曾分配过的最大 id。
    fn id_counter_path_for(api_keys_path: &Path) -> Option<PathBuf> {
        api_keys_path
            .parent()
            .map(|d| d.join("api_key_id_counter.json"))
    }

    /// 读取历史最大 ID 计数器（静态版本，供 `load()` 在实例构造前调用）；
    /// 文件缺失或解析失败均返回 0
    fn load_id_counter_from_path(api_keys_path: &Path) -> u32 {
        let Some(path) = Self::id_counter_path_for(api_keys_path) else {
            return 0;
        };
        let Ok(content) = fs::read_to_string(&path) else {
            return 0;
        };
        #[derive(Deserialize)]
        struct IdCounterFile {
            #[serde(rename = "maxId")]
            max_id: u32,
        }
        serde_json::from_str::<IdCounterFile>(&content)
            .map(|c| c.max_id)
            .unwrap_or(0)
    }

    /// 将 ID 计数器持久化到磁盘（写入不小于 `min_value` 的值）
    ///
    /// 锁内重新读取磁盘现有值，与 `min_value`、内存计数器三者取最大值再写入：否则
    /// 并发调用时可能较大值先落盘、较小值后落盘将其覆盖，进程若在该窗口崩溃重启，
    /// 会从磁盘读到被覆盖的较小值，削弱单调性保证（进而复用已分配过的 id）。
    ///
    /// 写失败仅告警不阻断——内存计数器仍然单调，本进程内不会分配出重复 id。
    fn save_id_counter_at_least(&self, min_value: u32) {
        let Some(path) = Self::id_counter_path_for(&self.file_path) else {
            return;
        };
        let _guard = self.counter_lock.lock();
        let in_memory = self.next_id_counter.load(Ordering::Relaxed);
        let on_disk = Self::load_id_counter_from_path(&self.file_path);
        let target = on_disk.max(min_value).max(in_memory);
        let json = serde_json::json!({ "maxId": target }).to_string();

        // 相对路径且无目录部分时 parent() 返回空路径，create_dir_all("") 会报错
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
        if let Some(parent) = parent
            && let Err(e) = fs::create_dir_all(parent)
        {
            tracing::warn!("创建 API Key ID 计数器目录失败: {}", e);
            return;
        }
        if let Err(e) = atomic_write(&path, json.as_bytes()) {
            tracing::warn!("保存 API Key ID 计数器失败: {}", e);
        }
    }

    /// 分配一个从未被使用过的 API Key ID（线程安全，单调递增，持久化）
    ///
    /// 计数器从 0 起，首次 `fetch_add` 返回 0、分配出 1 —— id 0 是保留给主密钥的
    /// （见 `UsageRecord::api_key_id` 注释），分配路径不能吐出 0。
    fn allocate_new_id(&self) -> u32 {
        let new_id = self.next_id_counter.fetch_add(1, Ordering::SeqCst) + 1;
        self.save_id_counter_at_least(new_id);
        new_id
    }

    /// 用外部已知的历史最大 id 补齐计数器种子（只增不减）
    ///
    /// 计数器文件首次不存在时（所有存量部署），`load()` 只能按当前 key 列表推算，
    /// 会漏掉已删除 key 曾用过的高位 id。启动时由 `main` 传入 `api_key_usage.json`
    /// 中出现过的最大 `api_key_id` 兜住这个窗口。
    pub fn seed_id_counter_at_least(&self, min_id: u32) {
        self.next_id_counter.fetch_max(min_id, Ordering::SeqCst);
        self.save_id_counter_at_least(min_id);
    }

    /// 持久化到文件
    fn save(&self) -> anyhow::Result<()> {
        let keys = self.keys.read();
        let content = serde_json::to_string_pretty(&*keys)?;
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.file_path, content)?;
        Ok(())
    }

    /// 验证请求中的 key
    pub fn authenticate(&self, key: &str) -> ApiKeyAuthResult {
        let keys = self.keys.read();
        match keys.iter().find(|k| k.key == key) {
            Some(api_key) => {
                if !api_key.enabled {
                    ApiKeyAuthResult::Disabled
                } else if api_key.is_expired() {
                    ApiKeyAuthResult::Expired
                } else {
                    ApiKeyAuthResult::Valid {
                        id: api_key.id,
                        name: api_key.name.clone(),
                        spending_limit: api_key.spending_limit,
                        limit_unit: api_key.limit_unit.clone(),
                        bound_credential_ids: api_key.bound_credential_ids.clone(),
                    }
                }
            }
            None => ApiKeyAuthResult::NotFound,
        }
    }

    /// 只读认证：只要 key 存在就放行（不检查过期/禁用/额度）
    /// 用于用户查询用量等只读场景
    pub fn authenticate_readonly(&self, key: &str) -> ApiKeyAuthResult {
        let keys = self.keys.read();
        match keys.iter().find(|k| k.key == key) {
            Some(api_key) => ApiKeyAuthResult::Valid {
                id: api_key.id,
                name: api_key.name.clone(),
                spending_limit: api_key.spending_limit,
                limit_unit: api_key.limit_unit.clone(),
                bound_credential_ids: api_key.bound_credential_ids.clone(),
            },
            None => ApiKeyAuthResult::NotFound,
        }
    }
    /// 获取所有 key（克隆）
    pub fn list(&self) -> Vec<ApiKey> {
        self.keys.read().clone()
    }

    /// 创建新 key
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        name: String,
        expires_at: Option<DateTime<Utc>>,
        spending_limit: Option<f64>,
        limit_unit: Option<String>,
        duration_days: Option<f64>,
        bound_credential_ids: Option<Vec<u64>>,
    ) -> anyhow::Result<ApiKey> {
        // 先分配 id 再取写锁：allocate_new_id 内含 fsync 磁盘写，不应持写锁跨越
        let next_id = self.allocate_new_id();
        let api_key = ApiKey::new(
            next_id,
            name,
            expires_at,
            spending_limit,
            limit_unit.unwrap_or_else(default_limit_unit),
            duration_days,
            bound_credential_ids,
        );
        self.keys.write().push(api_key.clone());
        self.save()?;
        Ok(api_key)
    }

    /// 更新 key（name, enabled, expires_at, spending_limit, limit_unit, duration_days）
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &self,
        id: u32,
        name: Option<String>,
        enabled: Option<bool>,
        expires_at: Option<Option<DateTime<Utc>>>,
        spending_limit: Option<Option<f64>>,
        limit_unit: Option<String>,
        duration_days: Option<Option<f64>>,
        bound_credential_ids: Option<Option<Vec<u64>>>,
    ) -> anyhow::Result<Option<ApiKey>> {
        let mut keys = self.keys.write();
        let Some(api_key) = keys.iter_mut().find(|k| k.id == id) else {
            return Ok(None);
        };
        if let Some(name) = name {
            api_key.name = name;
        }
        if let Some(enabled) = enabled {
            api_key.enabled = enabled;
        }
        if let Some(expires_at) = expires_at {
            api_key.expires_at = expires_at;
        }
        if let Some(spending_limit) = spending_limit {
            api_key.spending_limit = spending_limit;
        }
        if let Some(limit_unit) = limit_unit {
            api_key.limit_unit = limit_unit;
        }
        if let Some(duration_days) = duration_days {
            match duration_days {
                Some(new_days) => {
                    if api_key.is_active() && api_key.expires_at.is_some() {
                        // 活跃 Key（有到期时间）：在当前到期时间上增量续期
                        let extension =
                            chrono::Duration::milliseconds((new_days * 86_400_000.0) as i64);
                        let new_expires = api_key.expires_at.unwrap() + extension;
                        api_key.expires_at = Some(new_expires);
                        // 重算 duration_days 为从激活到新到期的总天数
                        let total_ms =
                            (new_expires - api_key.activated_at.unwrap()).num_milliseconds();
                        api_key.duration_days = Some(total_ms as f64 / 86_400_000.0);
                    } else {
                        // 已过期或待激活：重置为待激活状态
                        api_key.duration_days = Some(new_days);
                        api_key.activated_at = None;
                        api_key.expires_at = None;
                    }
                }
                None => {
                    // 切换为"永不过期"模式
                    api_key.duration_days = None;
                    api_key.activated_at = None;
                }
            }
        }
        if let Some(ids) = bound_credential_ids {
            api_key.bound_credential_ids = ids;
        }
        let updated = api_key.clone();
        drop(keys);
        self.save()?;
        Ok(Some(updated))
    }

    /// 删除 key
    pub fn delete(&self, id: u32) -> anyhow::Result<bool> {
        let mut keys = self.keys.write();
        let len_before = keys.len();
        keys.retain(|k| k.id != id);
        let deleted = keys.len() < len_before;
        drop(keys);
        if deleted {
            self.save()?;
        }
        Ok(deleted)
    }

    /// 获取文件路径
    #[allow(dead_code)]
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// 激活指定 key（幂等操作）
    /// 已激活或非懒激活模式的 key 直接跳过
    pub fn activate_key(&self, id: u32) -> anyhow::Result<()> {
        let mut keys = self.keys.write();
        let Some(api_key) = keys.iter_mut().find(|k| k.id == id) else {
            return Ok(());
        };
        if api_key.activate() {
            drop(keys);
            self.save()?;
        }
        Ok(())
    }
    // APPEND_MARKER2
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试专用临时目录：Drop 时清理，即使中途 assert! panic 也不残留
    /// （标准库 unwind 会执行 Drop），避免 CI 机器上堆积垂悬目录。
    struct TempDirGuard(PathBuf);

    impl TempDirGuard {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("k2cc_apikey_{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn keys_path(&self) -> PathBuf {
            self.0.join("api_keys.json")
        }

        fn counter_path(&self) -> PathBuf {
            self.0.join("api_key_id_counter.json")
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn create_key(mgr: &ApiKeyManager, name: &str) -> ApiKey {
        mgr.create(name.to_string(), None, None, None, None, None)
            .unwrap()
    }

    /// 回归测试：同进程内删除 key 后，新建的 key 不得复用被删除的 id。
    /// 复用会让新 key 继承 api_key_usage.json 中该 id 的用量与累计消费额，
    /// 后者是消费上限的判定依据。
    #[test]
    fn test_create_never_reuses_deleted_id_within_same_process() {
        let dir = TempDirGuard::new();
        let mgr = ApiKeyManager::load(dir.keys_path()).unwrap();

        assert_eq!(create_key(&mgr, "k1").id, 1);
        assert_eq!(create_key(&mgr, "k2").id, 2);
        assert!(mgr.delete(2).unwrap());

        // 旧逻辑 max(id) + 1 会在此再次分配出 2
        assert_eq!(create_key(&mgr, "k3").id, 3);
    }

    /// 回归测试：进程重启（从磁盘重新 load）后，新建的 key 仍不得复用已删除的 id
    #[test]
    fn test_create_never_reuses_deleted_id_after_restart() {
        let dir = TempDirGuard::new();
        {
            let mgr = ApiKeyManager::load(dir.keys_path()).unwrap();
            create_key(&mgr, "k1");
            create_key(&mgr, "k2");
            assert!(mgr.delete(2).unwrap());
        }

        // 模拟重启：api_keys.json 中只剩 #1，高位 id 只存在于计数器文件里
        let mgr = ApiKeyManager::load(dir.keys_path()).unwrap();
        assert_eq!(mgr.list().len(), 1);
        assert_eq!(create_key(&mgr, "k3").id, 3);
    }

    /// 回归测试：计数器文件尚不存在的存量部署，靠 usage 历史补种子避免首次复用。
    /// 这是仅凭 api_keys.json 无法覆盖的窗口——已删除 key 的高位 id 只在
    /// api_key_usage.json 中留有痕迹。
    #[test]
    fn test_seed_from_usage_history_prevents_first_run_reuse() {
        let dir = TempDirGuard::new();

        // 构造存量现场：api_keys.json 中只有 #1，且没有计数器文件
        let mgr = ApiKeyManager::load(dir.keys_path()).unwrap();
        create_key(&mgr, "k1");
        drop(mgr);
        std::fs::remove_file(dir.counter_path()).unwrap();

        let mgr = ApiKeyManager::load(dir.keys_path()).unwrap();
        // usage 历史中出现过 #5（其用量记录仍留在 api_key_usage.json 中）
        mgr.seed_id_counter_at_least(5);

        assert_eq!(create_key(&mgr, "k2").id, 6);
    }

    /// 回归测试：并发分配的 id 必须两两不同，且不得吐出保留给主密钥的 0
    #[test]
    fn test_allocate_new_id_concurrent_calls_never_collide() {
        let dir = TempDirGuard::new();
        let mgr = std::sync::Arc::new(ApiKeyManager::load(dir.keys_path()).unwrap());

        let handles: Vec<_> = (0..20)
            .map(|_| {
                let m = mgr.clone();
                std::thread::spawn(move || m.allocate_new_id())
            })
            .collect();

        let mut ids: Vec<u32> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "并发分配出现重复 id");
        assert!(ids.iter().all(|&id| id > 0), "id 0 保留给主密钥");
    }
}
