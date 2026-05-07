mod config;
mod models;
mod utils;

use crate::config::Settings;
use crate::models::scheduler::RouteScheduler;
use crate::utils::logger::Logger;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 加载配置
    let settings = Settings::load()?;

    // 初始化日志
    Logger::setup(&settings)?;

    log::info!("路由同步服务启动");
    log::info!("配置文件:");
    log::info!("  - 国家代码: {}", settings.country_code);
    log::info!("  - IP版本: {:?}", settings.ip_version);
    log::info!("  - 同步时间: {}", settings.sync_time);
    log::info!("  - GoBGP路径: {}", settings.gobgp_path);
    log::info!("  - 日志文件: {}", settings.log_file);
    log::info!("  - 快照目录: {}", settings.snapshot_dir);

    // 启动调度器
    let scheduler = RouteScheduler::new(settings);
    scheduler.run().await;

    Ok(())
}
