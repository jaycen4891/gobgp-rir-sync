use std::collections::HashMap;
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

    /// gobgp 可执行文件路径
    #[arg(short = 'g', long = "gobgp-path")]
    pub gobgp_path: Option<String>,

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
    pub gobgp_path: Option<String>,
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
    pub gobgp_path: String,
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
            gobgp_path: "/usr/local/bin/gobgp".to_string(),
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
                    if let Some(v) = s.gobgp_path {
                        config.gobgp_path = v;
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

        // CLI 参数覆盖配置文件（优先级最高）
        if let Some(v) = &args.ip_version {
            config.ip_version = IpVersion::from_str(v);
        }
        if let Some(v) = &args.country_code {
            config.country_code = v.to_uppercase();
        }
        if let Some(v) = &args.sync_time {
            config.sync_time = v.clone();
        }
        if let Some(v) = &args.gobgp_path {
            config.gobgp_path = v.clone();
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
