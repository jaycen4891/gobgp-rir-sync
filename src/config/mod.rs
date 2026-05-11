use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;

use clap::Parser;

use crate::models::country::CountryCodeMap;

mod cli;
mod types;

pub use cli::CliArgs;
pub use types::{ConfigFile, IpVersion, Settings};

impl Settings {
    /// 从命令行参数和可选配置文件构建配置
    /// 优先级: CLI 参数 > 配置文件 > 代码默认值
    pub fn load() -> anyhow::Result<Self> {
        let args = CliArgs::parse();

        // 优先级: CLI > 配置文件 > 程序默认值
        // 先取程序默认值
        let mut config = Settings {
            ip_version: IpVersion::Dual,
            country_code: "CN".to_string(),
            sync_time: "02:00".to_string(),
            gobgp_api_host: "127.0.0.1".to_string(),
            gobgp_api_port: 50051,
            gobgp_nexthop_ipv4: "0.0.0.0".to_string(),
            gobgp_nexthop_ipv6: "::".to_string(),
            community_nexthop_ipv4: HashMap::new(),
            community_nexthop_ipv6: HashMap::new(),
            region_nexthop_ipv4: HashMap::new(),
            region_nexthop_ipv6: HashMap::new(),
            log_file: format!("{}/gobgp_sync.log", Self::exe_dir()),
            snapshot_dir: Self::default_snapshot_dir(),
            community_prefix: "3166".to_string(),
            region_community_prefix: HashMap::new(),
            concurrency: 100,
            snapshot_ipv4_file: String::new(),
            snapshot_ipv6_file: String::new(),
            rir_urls: Self::default_rir_urls(),
            country_rir_map: CountryCodeMap::country_rir_map(),
        };

        // 配置文件覆盖默认值
        if let Some(config_path) = &args.config {
            if config_path.exists() {
                let content = std::fs::read_to_string(config_path)?;
                let cfg_file: ConfigFile = toml::from_str(&content)?;
                if let Some(s) = cfg_file.settings {
                    if let Some(v) = s.ip_version {
                        config.ip_version = IpVersion::from_str(&v);
                    }
                    if let Some(v) = s.country_code {
                        config.country_code = v.to_uppercase();
                    }
                    if let Some(v) = s.sync_time {
                        config.sync_time = v;
                    }
                    if let Some(v) = s.gobgp_api_host {
                        config.gobgp_api_host = v;
                    }
                    if let Some(v) = s.gobgp_api_port {
                        config.gobgp_api_port = v;
                    }
                    if let Some(v) = s.gobgp_nexthop_ipv4 {
                        config.gobgp_nexthop_ipv4 = v;
                    }
                    if let Some(v) = s.gobgp_nexthop_ipv6 {
                        config.gobgp_nexthop_ipv6 = v;
                    }
                    if let Some(v) = s.community_nexthop_ipv4 {
                        config.community_nexthop_ipv4 =
                            config.convert_country_next_hop_map(v, "IPv4");
                    }
                    if let Some(v) = s.community_nexthop_ipv6 {
                        config.community_nexthop_ipv6 =
                            config.convert_country_next_hop_map(v, "IPv6");
                    }
                    if let Some(v) = s.region_community_prefix {
                        config.region_community_prefix =
                            Self::normalize_region_string_map(v, "团体字前缀", |value| {
                                value.parse::<u16>().is_ok()
                            });
                    }
                    if let Some(v) = s.region_nexthop_ipv4 {
                        config.region_nexthop_ipv4 = Self::normalize_region_next_hop_map(v, false);
                    }
                    if let Some(v) = s.region_nexthop_ipv6 {
                        config.region_nexthop_ipv6 = Self::normalize_region_next_hop_map(v, true);
                    }
                    if let Some(v) = s.log_file {
                        config.log_file = v;
                    }
                    if let Some(v) = s.snapshot_dir {
                        config.snapshot_dir = v;
                    }
                    if let Some(v) = s.community_prefix {
                        config.community_prefix = v;
                    }
                    if let Some(v) = s.concurrency {
                        config.concurrency = v;
                    }
                }
            } else {
                log::warn!("配置文件不存在: {:?}", config_path);
            }
        }

        // 命令行参数覆盖配置文件（优先级最高）
        if let Some(v) = &args.ip_version {
            config.ip_version = IpVersion::from_str(v);
        }
        if let Some(v) = &args.country_code {
            config.country_code = v.to_uppercase();
        }
        if let Some(v) = &args.sync_time {
            config.sync_time = v.clone();
        }
        if let Some(v) = &args.gobgp_api_host {
            config.gobgp_api_host = v.clone();
        }
        if let Some(v) = args.gobgp_api_port {
            config.gobgp_api_port = v;
        }
        if let Some(v) = &args.gobgp_nexthop_ipv4 {
            config.gobgp_nexthop_ipv4 = v.clone();
        }
        if let Some(v) = &args.gobgp_nexthop_ipv6 {
            config.gobgp_nexthop_ipv6 = v.clone();
        }
        for item in &args.community_nexthop_ipv4 {
            if let Some((code, next_hop)) = config.parse_country_next_hop(item, "IPv4") {
                config.community_nexthop_ipv4.insert(code, next_hop);
            }
        }
        for item in &args.community_nexthop_ipv6 {
            if let Some((code, next_hop)) = config.parse_country_next_hop(item, "IPv6") {
                config.community_nexthop_ipv6.insert(code, next_hop);
            }
        }
        for item in &args.region_community_prefix {
            if let Some((region, prefix)) = Self::parse_region_item(item, "团体字前缀") {
                let mut map = HashMap::new();
                map.insert(region, prefix);
                config
                    .region_community_prefix
                    .extend(Self::normalize_region_string_map(
                        map,
                        "团体字前缀",
                        |value| value.parse::<u16>().is_ok(),
                    ));
            }
        }
        for item in &args.region_nexthop_ipv4 {
            if let Some((region, next_hop)) = Self::parse_region_item(item, "IPv4 下一跳") {
                let mut map = HashMap::new();
                map.insert(region, next_hop);
                config
                    .region_nexthop_ipv4
                    .extend(Self::normalize_region_next_hop_map(map, false));
            }
        }
        for item in &args.region_nexthop_ipv6 {
            if let Some((region, next_hop)) = Self::parse_region_item(item, "IPv6 下一跳") {
                let mut map = HashMap::new();
                map.insert(region, next_hop);
                config
                    .region_nexthop_ipv6
                    .extend(Self::normalize_region_next_hop_map(map, true));
            }
        }
        if let Some(v) = &args.log_file {
            config.log_file = v.clone();
        }
        if let Some(v) = &args.snapshot_dir {
            config.snapshot_dir = v.clone();
        }
        if let Some(v) = &args.community_prefix {
            config.community_prefix = v.clone();
        }
        if let Some(v) = args.concurrency {
            config.concurrency = v;
        }

        // 验证国家代码
        config.validate_country_code();
        config.validate_next_hops();

        // 设置文件路径
        let snap_dir = config.snapshot_dir.trim_end_matches('/');
        config.snapshot_ipv4_file = format!("{}/snapshot_ipv4_routing.prefix", snap_dir);
        config.snapshot_ipv6_file = format!("{}/snapshot_ipv6_routing.prefix", snap_dir);

        Ok(config)
    }

    /// 默认快照目录。使用系统临时目录，避免服务目录不可写或污染部署目录。
    fn default_snapshot_dir() -> String {
        "/tmp".to_string()
    }

    /// 获取二进制文件所在目录
    fn exe_dir() -> String {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .to_string()
    }

    fn validate_country_code(&mut self) {
        let valid_codes: Vec<String> = self
            .country_rir_map
            .keys()
            .cloned()
            .chain(vec!["ALL".to_string(), "NONECN".to_string()])
            .collect();

        if !valid_codes.contains(&self.country_code) {
            log::warn!("无效的COUNTRY_CODE: {}, 使用默认值CN", self.country_code);
            self.country_code = "CN".to_string();
        }
    }

    /// 判断是否需要过滤中国路由
    pub fn should_filter_cn(&self) -> bool {
        self.country_code == "NONECN"
    }

    /// 生成 tonic 需要的 GoBGP API 连接地址
    pub fn gobgp_api_addr(&self) -> String {
        format!("http://{}:{}", self.gobgp_api_host, self.gobgp_api_port)
    }

    /// 根据团体字中的国家/地区数字码选择下一跳；未命中覆盖表时使用默认下一跳
    pub fn next_hop_for_community(&self, community: &str, is_ipv6: bool) -> String {
        let code = community
            .split_once(':')
            .map(|(_, code)| code)
            .unwrap_or_default();
        let overrides = if is_ipv6 {
            &self.community_nexthop_ipv6
        } else {
            &self.community_nexthop_ipv4
        };

        if let Some(next_hop) = overrides.get(code) {
            return next_hop.clone();
        }

        let region_overrides = if is_ipv6 {
            &self.region_nexthop_ipv6
        } else {
            &self.region_nexthop_ipv4
        };

        let country_map = CountryCodeMap::default();
        if let Ok(numeric) = code.parse::<u16>() {
            if let Some(rir) = country_map.rir_for_numeric(numeric) {
                if let Some(next_hop) = region_overrides.get(rir) {
                    return next_hop.clone();
                }
            }
        }

        if is_ipv6 {
            self.gobgp_nexthop_ipv6.clone()
        } else {
            self.gobgp_nexthop_ipv4.clone()
        }
    }

    /// 使用国家/地区所属 RIR 选择 community 前缀，未配置地区前缀时回退到默认前缀。
    pub fn community_for_country(&self, country: &str) -> Option<String> {
        let country_map = CountryCodeMap::default();
        let region = country_map.rir_for_country(country);
        let prefix = region
            .and_then(|rir| self.region_community_prefix.get(rir))
            .unwrap_or(&self.community_prefix);
        country_map.community(country, prefix)
    }

    /// 校验默认下一跳和按国家/地区覆盖的下一跳，非法值会被忽略或回退
    fn validate_next_hops(&mut self) {
        if !matches!(self.gobgp_nexthop_ipv4.parse::<IpAddr>(), Ok(IpAddr::V4(_))) {
            log::warn!(
                "无效的 IPv4 下一跳: {}, 使用默认值 0.0.0.0",
                self.gobgp_nexthop_ipv4
            );
            self.gobgp_nexthop_ipv4 = Ipv4Addr::UNSPECIFIED.to_string();
        }

        if !matches!(self.gobgp_nexthop_ipv6.parse::<IpAddr>(), Ok(IpAddr::V6(_))) {
            log::warn!(
                "无效的 IPv6 下一跳: {}, 使用默认值 ::",
                self.gobgp_nexthop_ipv6
            );
            self.gobgp_nexthop_ipv6 = Ipv6Addr::UNSPECIFIED.to_string();
        }

        Self::validate_community_next_hops(&mut self.community_nexthop_ipv4, false);
        Self::validate_community_next_hops(&mut self.community_nexthop_ipv6, true);
    }

    /// 将 TOML 中 `CN = "下一跳"` 形式的覆盖表转换为内部数字码 key
    fn convert_country_next_hop_map(
        &self,
        map: HashMap<String, String>,
        family: &str,
    ) -> HashMap<String, String> {
        map.into_iter()
            .filter_map(|(country, next_hop)| {
                self.country_to_numeric_code(&country, family)
                    .map(|code| (code, next_hop))
            })
            .collect()
    }

    /// 解析 CLI 中 `CN=下一跳` 形式的单条覆盖配置
    fn parse_country_next_hop(&self, item: &str, family: &str) -> Option<(String, String)> {
        let (country, next_hop) = match item.split_once('=') {
            Some(v) => v,
            None => {
                log::warn!("无效的团体字下一跳配置: {}，应为 COUNTRY=NEXTHOP", item);
                return None;
            }
        };

        self.country_to_numeric_code(country, family)
            .map(|code| (code, next_hop.trim().to_string()))
    }

    /// 使用 ISO 3166-1 二位字母简写查找对应的三位数字码
    fn country_to_numeric_code(&self, country: &str, family: &str) -> Option<String> {
        let country = country.trim().to_uppercase();
        crate::models::country::CountryCodeMap::default()
            .get(&country)
            .map(|code| code.to_string())
            .or_else(|| {
                log::warn!("{} 下一跳覆盖忽略未知国家/地区简写: {}", family, country);
                None
            })
    }

    /// 校验内部覆盖表；这里的 key 已经是团体字后半段的数字码
    fn validate_community_next_hops(overrides: &mut HashMap<String, String>, is_ipv6: bool) {
        overrides.retain(|code, next_hop| {
            let valid_code = code.parse::<u16>().is_ok();
            let valid_next_hop = matches!(
                (next_hop.parse::<IpAddr>(), is_ipv6),
                (Ok(IpAddr::V4(_)), false) | (Ok(IpAddr::V6(_)), true)
            );

            if !valid_code {
                log::warn!("忽略无效的国家/地区数字码下一跳覆盖: {}", code);
            }
            if !valid_next_hop {
                log::warn!("忽略无效的下一跳覆盖: {}={}", code, next_hop);
            }

            valid_code && valid_next_hop
        });
    }

    fn normalize_region_next_hop_map(
        map: HashMap<String, String>,
        is_ipv6: bool,
    ) -> HashMap<String, String> {
        Self::normalize_region_string_map(map, "下一跳", |value| {
            matches!(
                (value.parse::<IpAddr>(), is_ipv6),
                (Ok(IpAddr::V4(_)), false) | (Ok(IpAddr::V6(_)), true)
            )
        })
    }

    fn parse_region_item(item: &str, value_name: &str) -> Option<(String, String)> {
        let (region, value) = match item.split_once('=') {
            Some(v) => v,
            None => {
                log::warn!(
                    "无效的 RIR 地区{}配置: {}，应为 RIR=VALUE",
                    value_name,
                    item
                );
                return None;
            }
        };

        Some((region.trim().to_string(), value.trim().to_string()))
    }

    fn normalize_region_string_map<F>(
        map: HashMap<String, String>,
        value_name: &str,
        valid_value: F,
    ) -> HashMap<String, String>
    where
        F: Fn(&str) -> bool,
    {
        let valid_regions: std::collections::HashSet<String> =
            CountryCodeMap::country_rir_map().into_values().collect();

        map.into_iter()
            .filter_map(|(region, value)| {
                let region = region.trim().to_uppercase();
                let value = value.trim().to_string();

                if !valid_regions.contains(&region) {
                    log::warn!("忽略未知 RIR 地区配置: {}", region);
                    return None;
                }

                if !valid_value(&value) {
                    log::warn!(
                        "忽略无效的 RIR 地区{}配置: {}={}",
                        value_name,
                        region,
                        value
                    );
                    return None;
                }

                Some((region, value))
            })
            .collect()
    }

    /// 获取需要处理的RIR列表
    pub fn get_rir_list(&self) -> Vec<String> {
        match self.country_code.as_str() {
            "ALL" | "NONECN" => self.rir_urls.keys().cloned().collect(),
            _ => {
                if let Some(rir) = self.country_rir_map.get(&self.country_code) {
                    vec![rir.clone()]
                } else {
                    log::warn!("未知国家代码: {}, 将下载所有RIR数据", self.country_code);
                    self.rir_urls.keys().cloned().collect()
                }
            }
        }
    }

    /// 默认 RIR delegated 数据源地址
    fn default_rir_urls() -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert(
            "APNIC".to_string(),
            "http://ftp.apnic.net/stats/apnic/delegated-apnic-extended-latest".to_string(),
            // "http://ftp.apnic.net/stats/apnic/delegated-apnic-extended-latest".to_string(),
        );
        map.insert(
            "ARIN".to_string(),
            "http://ftp.apnic.net/stats/arin/delegated-arin-extended-latest".to_string(),
            // "http://ftp.arin.net/pub/stats/arin/delegated-arin-extended-latest".to_string(),
        );
        map.insert(
            "RIPE".to_string(),
            "http://ftp.apnic.net/stats/ripe-ncc/delegated-ripencc-extended-latest".to_string(),
            // "http://ftp.ripe.net/ripe/stats/delegated-ripencc-extended-latest".to_string(),
        );
        map.insert(
            "LACNIC".to_string(),
            "http://ftp.apnic.net/stats/lacnic/delegated-lacnic-extended-latestt".to_string(),
            // "http://ftp.lacnic.net/pub/stats/lacnic/delegated-lacnic-extended-latest".to_string(),
        );
        map.insert(
            "AFRINIC".to_string(),
            "http://ftp.apnic.net/stats/afrinic/delegated-afrinic-latest"
            // "http://ftp.afrinic.net/pub/stats/afrinic/delegated-afrinic-extended-latest"
                .to_string(),
        );
        map
    }
}
