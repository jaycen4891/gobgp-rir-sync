use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDate, NaiveTime};
use tokio::sync::Mutex;

use crate::config::{IpVersion, Settings};
use crate::models::rir::{PrefixExtractor, RIRDataFetcher};
use crate::models::route::RouteManager;

/// 前缀 + 团体字对
type PrefixEntry = Vec<(String, String)>;

/// 路由调度器
pub struct RouteScheduler {
    settings: Arc<Settings>,
    route_manager: Arc<RouteManager>,
    rir_fetcher: RIRDataFetcher,
    last_ipv4_prefixes: Arc<Mutex<Option<PrefixEntry>>>,
    last_ipv6_prefixes: Arc<Mutex<Option<PrefixEntry>>>,
}

impl RouteScheduler {
    pub fn new(settings: Settings) -> Self {
        let settings = Arc::new(settings);
        let route_manager = Arc::new(RouteManager::new((*settings).clone()));

        Self {
            settings,
            route_manager,
            rir_fetcher: RIRDataFetcher::new(),
            last_ipv4_prefixes: Arc::new(Mutex::new(None)),
            last_ipv6_prefixes: Arc::new(Mutex::new(None)),
        }
    }

    /// 获取快照文件的修改日期（仅日期部分）
    fn snapshot_mtime(path: &str) -> Option<NaiveDate> {
        let meta = std::fs::metadata(path).ok()?;
        let modified = meta.modified().ok()?;
        let dt: chrono::DateTime<Local> = modified.into();
        Some(dt.date_naive())
    }

    /// 启动调度
    pub async fn run(&self) {
        // 解析同步时间
        let sync_time = match NaiveTime::parse_from_str(&self.settings.sync_time, "%H:%M") {
            Ok(t) => t,
            Err(e) => {
                log::error!(
                    "解析同步时间失败 '{}': {}, 使用默认 02:00",
                    self.settings.sync_time,
                    e
                );
                NaiveTime::parse_from_str("02:00", "%H:%M").unwrap()
            }
        };

        log::info!("路由同步服务启动");
        log::info!("  国家代码: {}", self.settings.country_code);
        if self.settings.country_code == "NONECN" {
            log::info!("  特殊模式: 过滤中国(CN)路由");
        } else if self.settings.country_code == "ALL" {
            log::info!("  特殊模式: 处理所有国家路由");
        }
        log::info!("  同步时间: {}", self.settings.sync_time);
        log::info!("  IP 版本: {:?}", self.settings.ip_version);

        // 首次执行
        self.sync_operation().await;

        // 定时执行
        loop {
            let now = Local::now().time();
            let target = sync_time;

            let secs = if now <= target {
                (target - now).num_seconds()
            } else {
                let tomorrow = target + chrono::Duration::days(1);
                (tomorrow - now).num_seconds()
            };

            if secs > 0 {
                log::info!(
                    "距离下次同步还有 {} 秒 ({}h{}m{}s)",
                    secs,
                    secs / 3600,
                    (secs % 3600) / 60,
                    secs % 60
                );
            }

            tokio::time::sleep(Duration::from_secs(std::cmp::max(secs as u64, 60))).await;

            self.sync_operation().await;
        }
    }

    /// 执行同步操作 — 每个协议各自独立处理
    async fn sync_operation(&self) {
        let start = Instant::now();
        log::info!("开始执行路由同步任务");

        let need_ipv4 = self.settings.ip_version.should_process_ipv4();
        let need_ipv6 = self.settings.ip_version.should_process_ipv6();

        let mut results = Vec::new();

        if need_ipv4 {
            let result = self.sync_one("ipv4").await;
            results.push(result);
        }

        if need_ipv6 {
            let result = self.sync_one("ipv6").await;
            results.push(result);
        }

        log::info!("同步任务完成");
        for line in &results {
            for part in line.split('\n') {
                log::info!("{}", part);
            }
        }
        log::info!("总耗时 {:.2}s", start.elapsed().as_secs_f64());
    }

    /// 同步单个协议
    ///
    /// 决策逻辑（按优先级）：
    /// 1. 快照不存在 → 下载 RIR → 提取前缀 → 写入路由 → 保存快照
    /// 2. 快照存在且不是今天 → 下载 RIR → 对比快照 → 增删路由 → 更新快照
    /// 3. 快照存在且是今天 → 按快照恢复路由
    async fn sync_one(&self, protocol: &str) -> String {
        let tag = protocol.to_uppercase();

        let (snapshot_file, last_prefixes_lock) = if protocol == "ipv4" {
            (
                self.settings.snapshot_ipv4_file.clone(),
                &self.last_ipv4_prefixes,
            )
        } else {
            (
                self.settings.snapshot_ipv6_file.clone(),
                &self.last_ipv6_prefixes,
            )
        };

        let snapshot_mtime_date = Self::snapshot_mtime(&snapshot_file);
        let today = Local::now().date_naive();

        // 快照不存在 → 下载 RIR，走完整流程
        if snapshot_mtime_date.is_none() {
            log::info!("{} | 无快照文件，下载 RIR 数据", tag);
            return self
                .sync_with_rir(protocol, &snapshot_file, last_prefixes_lock)
                .await;
        }

        // 快照不是今天的 → 需要更新，下载 RIR
        if snapshot_mtime_date.unwrap() < today {
            log::info!("{} | 快照不是今天的，下载最新 RIR 数据", tag);
            return self
                .sync_with_rir(protocol, &snapshot_file, last_prefixes_lock)
                .await;
        }

        // 快照是今天的 → 按快照恢复一次
        let snapshot_prefixes = RouteManager::load_snapshot(&snapshot_file);

        if snapshot_prefixes.is_empty() {
            return format!("{} | 快照文件为空，跳过", tag);
        }

        // 加载快照到内存
        {
            let mut sorted: Vec<(String, String)> = snapshot_prefixes.clone().into_iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            let mut guard = last_prefixes_lock.lock().await;
            if guard.is_none() {
                *guard = Some(sorted);
            }
        }

        let snapshot_count = snapshot_prefixes.len();
        log::info!("{} | 从快照恢复 {} 条路由", tag, snapshot_count);

        let (ok, fail) = self
            .route_manager
            .batch_sync(&snapshot_prefixes, &[], &tag)
            .await;

        format!(
            "{} | 从快照恢复 {} 条路由\n{} | 同步成功 {}, 同步失败 {}",
            tag, snapshot_count, tag, ok, fail,
        )
    }

    /// 获取用于差异比较的上一版前缀：优先用内存缓存，服务重启后回退到快照文件
    fn previous_prefixes(
        cached_prefixes: Option<&PrefixEntry>,
        snapshot_file: &str,
    ) -> HashMap<String, String> {
        cached_prefixes
            .map(|p| p.iter().cloned().collect())
            .unwrap_or_else(|| RouteManager::load_snapshot(snapshot_file))
    }

    /// 下载 RIR 并执行完整同步（首次运行 / 定时更新）
    async fn sync_with_rir(
        &self,
        protocol: &str,
        snapshot_file: &str,
        last_prefixes_lock: &Arc<Mutex<Option<PrefixEntry>>>,
    ) -> String {
        let tag = protocol.to_uppercase();

        let rir_data = match self.rir_fetcher.download_rir_data(&self.settings).await {
            Ok(data) => data,
            Err(e) => {
                return format!("{} | 下载 RIR 数据失败: {}", tag, e);
            }
        };

        let ip_version = match protocol {
            "ipv4" => Some(&IpVersion::Ipv4),
            "ipv6" => Some(&IpVersion::Ipv6),
            _ => None,
        };

        let p =
            PrefixExtractor::get_prefixes_by_country_mode(&self.settings, &rir_data, ip_version);
        let prefixes: HashMap<String, String> = match protocol {
            "ipv4" => p.0,
            "ipv6" => p.1,
            _ => HashMap::new(),
        };

        log::info!("{} | 提取到 {} 条前缀", tag, prefixes.len());

        // 构建上一版前缀集合，用于比较差异
        let last_set: HashMap<String, String> = {
            let guard = last_prefixes_lock.lock().await;
            Self::previous_prefixes(guard.as_ref(), snapshot_file)
        };

        let total = prefixes.len();

        // 计算新增和删除的键（前缀）
        let last_keys: Vec<_> = last_set.keys().cloned().collect();
        let current_keys: Vec<_> = prefixes.keys().cloned().collect();

        let last_keys_set: std::collections::HashSet<String> = last_keys.into_iter().collect();
        let current_keys_set: std::collections::HashSet<String> =
            current_keys.into_iter().collect();

        let added_count = current_keys_set.difference(&last_keys_set).count();
        let removed_count = last_keys_set.difference(&current_keys_set).count();

        if prefixes == last_set {
            let _ = self.route_manager.save_snapshot(&prefixes, snapshot_file);
            return format!("{} | 路由总数 {}, 新增 0, 删除 0, 无变化", tag, total);
        }

        // 构建 to_add: 只取本次新增的前缀（带团体字）
        let to_add: HashMap<String, String> = prefixes
            .iter()
            .filter(|(prefix, _)| !last_set.contains_key(*prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // 构建 to_del: 取上次有但本次没有的前缀
        let to_del: Vec<String> = last_set
            .keys()
            .filter(|prefix| !prefixes.contains_key(*prefix))
            .cloned()
            .collect();

        log::info!("{} | 新增 {}, 删除 {}", tag, added_count, removed_count);

        let (ok, fail) = self.route_manager.batch_sync(&to_add, &to_del, &tag).await;

        // 保存完整的当前前缀集合到快照
        if let Err(e) = self.route_manager.save_snapshot(&prefixes, snapshot_file) {
            log::warn!("{} | 快照保存失败: {}", tag, e);
        }

        {
            let mut sorted: Vec<(String, String)> = prefixes.into_iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            *last_prefixes_lock.lock().await = Some(sorted);
        }

        format!(
            "{} | 路由总数 {}, 新增 {}, 删除 {}\n{} | 同步成功 {}, 同步失败 {}",
            tag, total, added_count, removed_count, tag, ok, fail,
        )
    }
}
