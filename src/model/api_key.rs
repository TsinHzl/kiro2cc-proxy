use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

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
    /// 额度限制（美元），None 表示不限额（按日期模式）
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spending_limit: Option<f64>,
    /// 有效期天数（懒激活模式），首次使用后才开始计时
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_days: Option<f64>,
    /// 首次使用激活时间
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<DateTime<Utc>>,
}

fn default_enabled() -> bool {
    true
}

impl ApiKey {
    /// 生成新的 API Key
    pub fn new(
        id: u32,
        name: String,
        expires_at: Option<DateTime<Utc>>,
        spending_limit: Option<f64>,
        duration_days: Option<f64>,
    ) -> Self {
        Self {
            id,
            key: generate_api_key(),
            name,
            enabled: true,
            created_at: Utc::now(),
            expires_at,
            spending_limit,
            duration_days,
            activated_at: None,
        }
    }

    /// 检查 key 是否有效（启用且未过期）
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
    Valid { id: u32, name: String, spending_limit: Option<f64> },
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
}

impl ApiKeyManager {
    /// 从文件加载，文件不存在则创建空列表
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let keys = if path.exists() {
            let content = fs::read_to_string(&path)?;
            if content.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&content)?
            }
        } else {
            Vec::new()
        };
        Ok(Self {
            keys: RwLock::new(keys),
            file_path: path,
        })
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
            },
            None => ApiKeyAuthResult::NotFound,
        }
    }
    /// 获取所有 key（克隆）
    pub fn list(&self) -> Vec<ApiKey> {
        self.keys.read().clone()
    }

    /// 创建新 key
    pub fn create(&self, name: String, expires_at: Option<DateTime<Utc>>, spending_limit: Option<f64>, duration_days: Option<f64>) -> anyhow::Result<ApiKey> {
        let mut keys = self.keys.write();
        let next_id = keys.iter().map(|k| k.id).max().unwrap_or(0) + 1;
        let api_key = ApiKey::new(next_id, name, expires_at, spending_limit, duration_days);
        keys.push(api_key.clone());
        drop(keys);
        self.save()?;
        Ok(api_key)
    }

    /// 更新 key（name, enabled, expires_at, spending_limit, duration_days）
    pub fn update(
        &self,
        id: u32,
        name: Option<String>,
        enabled: Option<bool>,
        expires_at: Option<Option<DateTime<Utc>>>,
        spending_limit: Option<Option<f64>>,
        duration_days: Option<Option<f64>>,
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
        if let Some(duration_days) = duration_days {
            match duration_days {
                Some(new_days) => {
                    if api_key.is_active() && api_key.expires_at.is_some() {
                        // 活跃 Key（有到期时间）：在当前到期时间上增量续期
                        let extension = chrono::Duration::milliseconds((new_days * 86_400_000.0) as i64);
                        let new_expires = api_key.expires_at.unwrap() + extension;
                        api_key.expires_at = Some(new_expires);
                        // 重算 duration_days 为从激活到新到期的总天数
                        let total_ms = (new_expires - api_key.activated_at.unwrap()).num_milliseconds();
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