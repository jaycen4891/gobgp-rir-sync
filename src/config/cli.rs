use std::path::PathBuf;

use clap::Parser;

/// 路由同步服务命令行参数。
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

    /// 按 RIR 地区覆盖团体字前缀，格式: RIPE=65167
    #[arg(long = "region-community-prefix", value_name = "RIR=PREFIX")]
    pub region_community_prefix: Vec<String>,

    /// 按 RIR 地区覆盖 IPv4 下一跳，格式: RIPE=198.19.1.254
    #[arg(long = "region-nexthop-ipv4", value_name = "RIR=NEXTHOP")]
    pub region_nexthop_ipv4: Vec<String>,

    /// 按 RIR 地区覆盖 IPv6 下一跳，格式: RIPE=2001:db8:1::fe
    #[arg(long = "region-nexthop-ipv6", value_name = "RIR=NEXTHOP")]
    pub region_nexthop_ipv6: Vec<String>,

    /// 日志文件路径 (默认: ./gobgp_sync.log)
    #[arg(short = 'l', long = "log-file")]
    pub log_file: Option<String>,

    /// 快照文件目录 (默认: /tmp)
    #[arg(short = 'd', long = "snapshot-dir")]
    pub snapshot_dir: Option<String>,

    /// 团体字前缀 (默认: 3166)
    #[arg(long = "community-prefix")]
    pub community_prefix: Option<String>,

    /// 并发添加/删除路由的任务数 (默认: 100)
    #[arg(long = "concurrency")]
    pub concurrency: Option<usize>,
}
