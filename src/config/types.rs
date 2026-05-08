use std::collections::HashMap;

use serde::Deserialize;

/// TOML 配置文件结构。
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigFile {
    pub settings: Option<SettingsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SettingsConfig {
    pub ip_version: Option<String>,
    pub country_code: Option<String>,
    pub sync_time: Option<String>,
    pub gobgp_api_host: Option<String>,
    pub gobgp_api_port: Option<u16>,
    pub gobgp_nexthop_ipv4: Option<String>,
    pub gobgp_nexthop_ipv6: Option<String>,
    pub community_nexthop_ipv4: Option<HashMap<String, String>>,
    pub community_nexthop_ipv6: Option<HashMap<String, String>>,
    pub region_community_prefix: Option<HashMap<String, String>>,
    pub region_nexthop_ipv4: Option<HashMap<String, String>>,
    pub region_nexthop_ipv6: Option<HashMap<String, String>>,
    pub log_file: Option<String>,
    pub snapshot_dir: Option<String>,
    pub community_prefix: Option<String>,
    pub concurrency: Option<usize>,
}

/// 运行时配置。
#[derive(Debug, Clone)]
pub struct Settings {
    pub ip_version: IpVersion,
    pub country_code: String,
    pub sync_time: String,
    pub gobgp_api_host: String,
    pub gobgp_api_port: u16,
    pub gobgp_nexthop_ipv4: String,
    pub gobgp_nexthop_ipv6: String,
    pub community_nexthop_ipv4: HashMap<String, String>,
    pub community_nexthop_ipv6: HashMap<String, String>,
    pub region_nexthop_ipv4: HashMap<String, String>,
    pub region_nexthop_ipv6: HashMap<String, String>,
    pub log_file: String,
    pub snapshot_dir: String,
    pub snapshot_ipv4_file: String,
    pub snapshot_ipv6_file: String,
    pub community_prefix: String,
    pub region_community_prefix: HashMap<String, String>,
    pub concurrency: usize,
    pub rir_urls: HashMap<String, String>,
    pub country_rir_map: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IpVersion {
    Ipv4,
    Ipv6,
    Dual,
}

impl IpVersion {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "IPV4" => IpVersion::Ipv4,
            "IPV6" => IpVersion::Ipv6,
            "DUAL" => IpVersion::Dual,
            _ => {
                log::warn!("无效的IP_VERSION: {}, 使用默认值DUAL", s);
                IpVersion::Dual
            }
        }
    }

    pub fn should_process_ipv4(&self) -> bool {
        matches!(self, IpVersion::Ipv4 | IpVersion::Dual)
    }

    pub fn should_process_ipv6(&self) -> bool {
        matches!(self, IpVersion::Ipv6 | IpVersion::Dual)
    }
}
