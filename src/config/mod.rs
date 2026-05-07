use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;

use clap::Parser;
use serde::Deserialize;

/// 路由同步服务配置
/// 支持通过命令行参数和配置文件两种方式配置
#[derive(Parser, Debug, Clone)]
#[command(name = "gobgp-sync", version, about = "GoBGP 路由同步服务")]
pub struct CliArgs {
    /// 配置文件路径 (TOML格式)
    #[arg(short = 'c', long = "config", value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// IP协议版本: ipv4, ipv6, dual (默认: dual)
    #[arg(short = 'i', long = "ip-version")]
    pub ip_version: Option<String>,

    /// 国家代码: CN, JP, US, ALL, NONECN (默认: CN)
    #[arg(short = 'C', long = "country")]
    pub country_code: Option<String>,

    /// 每日同步时间 (格式: HH:MM, 默认: 02:00)
    #[arg(short = 's', long = "sync-time")]
    pub sync_time: Option<String>,

    /// GoBGP gRPC API 地址
    #[arg(long = "gobgp-api-host")]
    pub gobgp_api_host: Option<String>,

    /// GoBGP gRPC API 端口
    #[arg(long = "gobgp-api-port")]
    pub gobgp_api_port: Option<u16>,

    /// GoBGP 注入 IPv4 路由时使用的下一跳
    #[arg(long = "gobgp-nexthop-ipv4")]
    pub gobgp_nexthop_ipv4: Option<String>,

    /// GoBGP 注入 IPv6 路由时使用的下一跳
    #[arg(long = "gobgp-nexthop-ipv6")]
    pub gobgp_nexthop_ipv6: Option<String>,

    /// 按国家/地区简写覆盖 IPv4 下一跳，格式: CN=198.19.0.254
    #[arg(long = "community-nexthop-ipv4", value_name = "COUNTRY=NEXTHOP")]
    pub community_nexthop_ipv4: Vec<String>,

    /// 按国家/地区简写覆盖 IPv6 下一跳，格式: CN=2001:db8::fe
    #[arg(long = "community-nexthop-ipv6", value_name = "COUNTRY=NEXTHOP")]
    pub community_nexthop_ipv6: Vec<String>,

    /// 日志文件路径 (默认: ./gobgp_sync.log)
    #[arg(short = 'l', long = "log-file")]
    pub log_file: Option<String>,

    /// 快照文件目录 (默认: ./)
    #[arg(short = 'd', long = "snapshot-dir")]
    pub snapshot_dir: Option<String>,

    /// 团体字前缀 (默认: 3166)
    #[arg(long = "community-prefix")]
    pub community_prefix: Option<String>,

    /// 并发添加/删除路由的任务数 (默认: 100)
    #[arg(long = "concurrency")]
    pub concurrency: Option<usize>,
}

/// TOML配置文件结构
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
    pub log_file: Option<String>,
    pub snapshot_dir: Option<String>,
    pub community_prefix: Option<String>,
    pub concurrency: Option<usize>,
}

/// 运行时配置
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
    pub log_file: String,
    pub snapshot_dir: String,
    pub snapshot_ipv4_file: String,
    pub snapshot_ipv6_file: String,
    pub community_prefix: String,
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
            log_file: format!("{}/gobgp_sync.log", Self::exe_dir()),
            snapshot_dir: Self::exe_dir(),
            community_prefix: "3166".to_string(),
            concurrency: 100,
            snapshot_ipv4_file: String::new(),
            snapshot_ipv6_file: String::new(),
            rir_urls: Self::default_rir_urls(),
            country_rir_map: Self::default_country_rir_map(),
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

        overrides.get(code).cloned().unwrap_or_else(|| {
            if is_ipv6 {
                self.gobgp_nexthop_ipv6.clone()
            } else {
                self.gobgp_nexthop_ipv4.clone()
            }
        })
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
        );
        map.insert(
            "ARIN".to_string(),
            "http://ftp.arin.net/pub/stats/arin/delegated-arin-extended-latest".to_string(),
        );
        map.insert(
            "RIPE".to_string(),
            "http://ftp.ripe.net/ripe/stats/delegated-ripencc-extended-latest".to_string(),
        );
        map.insert(
            "LACNIC".to_string(),
            "http://ftp.lacnic.net/pub/stats/lacnic/delegated-lacnic-extended-latest".to_string(),
        );
        map.insert(
            "AFRINIC".to_string(),
            "http://ftp.afrinic.net/pub/stats/afrinic/delegated-afrinic-extended-latest"
                .to_string(),
        );
        map
    }

    /// 国家/地区简写到所属 RIR 的映射，用于决定需要下载哪些 delegated 文件
    fn default_country_rir_map() -> HashMap<String, String> {
        let mut map = HashMap::new();

        // AFRINIC
        let afrinic_countries = [
            "AO", "BF", "BI", "BJ", "BW", "CD", "CF", "CG", "CI", "CM", "CV", "DJ", "DZ", "EG",
            "ER", "ET", "GA", "GH", "GM", "GN", "GQ", "GW", "KE", "KM", "LR", "LS", "LY", "MA",
            "MG", "ML", "MR", "MU", "MW", "MZ", "NA", "NE", "NG", "RE", "RW", "SC", "SD", "SL",
            "SN", "SO", "SS", "ST", "SZ", "TD", "TG", "TN", "TZ", "UG", "YT", "ZA", "ZM", "ZW",
            "ZZ",
        ];
        for c in &afrinic_countries {
            map.insert(c.to_string(), "AFRINIC".to_string());
        }

        // APNIC
        let apnic_countries = [
            "AE", "AF", "AL", "AP", "AS", "AU", "BD", "BN", "BR", "BT", "BZ", "CA", "CH", "CK",
            "CN", "CO", "CY", "DE", "DK", "EE", "ES", "FJ", "FM", "FR", "GB", "GU", "HK", "ID",
            "IE", "IM", "IN", "IO", "IT", "JP", "KH", "KI", "KP", "KR", "LA", "LK", "LT", "LU",
            "MH", "MM", "MN", "MO", "MP", "MT", "MV", "MX", "MY", "NC", "NF", "NL", "NO", "NP",
            "NR", "NU", "NZ", "PA", "PF", "PG", "PH", "PK", "PT", "PW", "RO", "RU", "SB", "SE",
            "SG", "SI", "TH", "TK", "TL", "TO", "TR", "TV", "TW", "US", "VG", "VN", "VU", "WF",
            "WS",
        ];
        for c in &apnic_countries {
            map.insert(c.to_string(), "APNIC".to_string());
        }

        // ARIN
        let arin_countries = [
            "AG", "AI", "BB", "BE", "BL", "BM", "BS", "CZ", "DM", "DO", "FI", "GD", "GP", "IL",
            "IS", "JE", "JM", "KN", "KY", "LC", "MF", "MQ", "MS", "PM", "PR", "SG", "TC", "UG",
            "VC", "VI",
        ];
        for c in &arin_countries {
            map.insert(c.to_string(), "ARIN".to_string());
        }

        // LACNIC
        let lacnic_countries = [
            "AR", "AW", "BO", "BQ", "CL", "CR", "CU", "CW", "EC", "GF", "GT", "GY", "HN", "HT",
            "NI", "PE", "PY", "SR", "SV", "SX", "TT", "UY", "VE",
        ];
        for c in &lacnic_countries {
            map.insert(c.to_string(), "LACNIC".to_string());
        }

        // RIPE
        let ripe_countries = [
            "AD", "AM", "AT", "AX", "AZ", "BA", "BG", "BH", "BY", "GE", "GI", "GL", "GR", "HR",
            "HU", "IQ", "IR", "JO", "KG", "KW", "KZ", "LB", "LI", "LV", "MC", "MD", "ME", "MK",
            "OM", "PL", "PS", "QA", "RS", "SA", "SK", "SM", "SY", "TJ", "TM", "UA", "UZ", "VA",
            "YE",
        ];
        for c in &ripe_countries {
            map.insert(c.to_string(), "RIPE".to_string());
        }

        map
    }
}
