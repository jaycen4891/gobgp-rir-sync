mod config;
pub mod gobgp;
mod models;
mod utils;

use crate::config::Settings;
use crate::models::scheduler::RouteScheduler;
use crate::utils::logger::Logger;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 加载运行配置，命令行参数会覆盖 TOML 配置文件
    let settings = Settings::load()?;

    // 日志依赖配置中的输出路径，必须在其它业务日志之前初始化
    Logger::setup(&settings)?;

    log::info!("路由同步服务启动");
    log::info!("配置文件:");
    log::info!("  - 国家代码: {}", settings.country_code);
    if settings.country_code == "NONECN" {
        log::info!("  - 特殊模式: 过滤中国(CN)路由");
    } else if settings.country_code == "ALL" {
        log::info!("  - 特殊模式: 处理所有国家路由");
    }
    log::info!("  - IP版本: {:?}", settings.ip_version);
    log::info!("  - 同步时间: {}", settings.sync_time);
    log::info!("  - GoBGP API: {}", settings.gobgp_api_addr());
    log::info!(
        "  - GoBGP 下一跳: IPv4={}, IPv6={}",
        settings.gobgp_nexthop_ipv4,
        settings.gobgp_nexthop_ipv6
    );
    log::info!("  - 日志文件: {}", settings.log_file);
    log::info!("  - 快照目录: {}", settings.snapshot_dir);

    // 调度器会先立即同步一次，随后按配置的每日时间循环执行
    let scheduler = RouteScheduler::new(settings);
    scheduler.run().await;

    Ok(())
}
