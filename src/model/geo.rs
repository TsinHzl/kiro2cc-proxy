// Copyright (c) 2026 Harllan He. Licensed under MIT.
//! IP 归属地离线解析（基于 ip2region 本地库）

use ip2region::Searcher;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::Path;

/// 内嵌的官方 `ip2region_v4.xdb` 数据文件（Apache-2.0，来自 lionsoul2014/ip2region）
static XDB_BYTES: &[u8] = include_bytes!("../../assets/ip2region_v4.xdb");

/// IP 归属地信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoInfo {
    pub country: String,
    pub region_name: String,
    pub city: String,
}

/// IP 归属地解析器：启动时将内嵌的 xdb 数据写出到运行时数据目录，随后常驻内存查表
pub struct GeoResolver {
    searcher: Searcher,
}

impl GeoResolver {
    /// `data_dir`：运行时数据目录（与 `throttle_log.json` 等文件同级）
    pub fn new(data_dir: &Path) -> std::io::Result<Self> {
        let xdb_path = data_dir.join("ip2region_v4.xdb");
        if !xdb_path.exists() {
            std::fs::write(&xdb_path, XDB_BYTES)?;
        }
        let searcher = Searcher::new(&xdb_path)
            .map_err(|e| std::io::Error::other(format!("加载 ip2region 数据失败: {e}")))?;
        Ok(Self { searcher })
    }

    /// 解析 IP 归属地；私有/环回/非法地址返回 `None`
    pub fn resolve(&self, ip: &str) -> Option<GeoInfo> {
        let addr: IpAddr = ip.parse().ok()?;
        let IpAddr::V4(v4) = addr else {
            return None;
        };
        if is_private_or_loopback(&v4) {
            return None;
        }
        let raw = self.searcher.search(ip).ok()?;
        let mut parts = raw.split('|');
        let country = normalize_field(parts.next().unwrap_or_default());
        let region_name = normalize_field(parts.next().unwrap_or_default());
        let city = normalize_field(parts.next().unwrap_or_default());
        if country.is_empty() && region_name.is_empty() && city.is_empty() {
            return None;
        }
        Some(GeoInfo {
            country,
            region_name,
            city,
        })
    }
}

fn is_private_or_loopback(ip: &std::net::Ipv4Addr) -> bool {
    ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
}

/// ip2region 数据用字面量 `"0"` 表示字段缺失（如无城市颗粒度的数据），归一化为空字符串
fn normalize_field(field: &str) -> String {
    if field.is_empty() || field == "0" {
        String::new()
    } else {
        field.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver() -> GeoResolver {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "kiro2cc_proxy_geo_test_{}_{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        GeoResolver::new(&dir).unwrap()
    }

    #[test]
    fn resolves_public_ipv4() {
        let geo = resolver().resolve("222.35.11.141");
        assert!(geo.is_some());
        assert!(!geo.unwrap().country.is_empty());
    }

    #[test]
    fn returns_none_for_private_address() {
        assert!(resolver().resolve("127.0.0.1").is_none());
        assert!(resolver().resolve("192.168.1.1").is_none());
        assert!(resolver().resolve("10.0.0.1").is_none());
    }

    #[test]
    fn returns_none_for_invalid_format() {
        assert!(resolver().resolve("not-an-ip").is_none());
        assert!(resolver().resolve("::1").is_none());
    }

    #[test]
    fn filters_placeholder_zero_city_field() {
        // 8.8.8.8 在该 xdb 数据源中城市字段缺失，raw 为 "United States|California|0|..."
        let geo = resolver().resolve("8.8.8.8").unwrap();
        assert_ne!(geo.city, "0");
    }
}
