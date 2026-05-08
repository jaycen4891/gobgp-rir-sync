use std::collections::{HashMap, HashSet};
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
    /// 默认每日同步时间
    fn default_sync_time() -> NaiveTime {
        NaiveTime::from_num_seconds_from_midnight_opt(2 * 60 * 60, 0).unwrap_or(NaiveTime::MIN)
    }

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
                Self::default_sync_time()
            }
        };

        log::info!("路由同步调度器启动");

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

        match snapshot_mtime_date {
            None => {
                log::info!("{} | 无快照文件，下载 RIR 数据", tag);
                return self
                    .sync_with_rir(protocol, &snapshot_file, last_prefixes_lock)
                    .await;
            }
            Some(snapshot_date) if snapshot_date < today => {
                log::info!("{} | 快照不是今天的，下载最新 RIR 数据", tag);
                return self
                    .sync_with_rir(protocol, &snapshot_file, last_prefixes_lock)
                    .await;
            }
            Some(_) => {}
        }

        // 快照是今天的 → 按快照恢复一次
        let snapshot_prefixes = RouteManager::load_snapshot(&snapshot_file);

        if snapshot_prefixes.is_empty() {
            return format!("{} | 快照文件为空，跳过", tag);
        }

        // 今日快照只在内存为空时写入，避免覆盖本进程已完成的最新同步结果
        Self::store_cached_prefixes(last_prefixes_lock, &snapshot_prefixes, true).await;

        let snapshot_count = snapshot_prefixes.len();
        log::info!(
            "{} | 对账快照 {} 条路由与 GoBGP Global RIB",
            tag,
            snapshot_count
        );

        let existing_prefixes = match self
            .route_manager
            .list_global_prefixes(protocol, &tag)
            .await
        {
            Ok(prefixes) => prefixes,
            Err(e) => {
                return format!("{} | 查询 GoBGP Global RIB 失败: {}", tag, e);
            }
        };

        let missing_prefixes =
            Self::missing_snapshot_prefixes(&snapshot_prefixes, &existing_prefixes);
        let missing_count = missing_prefixes.len();

        if missing_prefixes.is_empty() {
            return format!(
                "{} | 快照路由 {} 条，GoBGP 已存在 {} 条，缺失 0 条",
                tag,
                snapshot_count,
                existing_prefixes.len(),
            );
        }

        log::info!(
            "{} | 快照路由 {} 条，GoBGP 已存在 {} 条，追加缺失 {} 条",
            tag,
            snapshot_count,
            existing_prefixes.len(),
            missing_count
        );

        let (ok, fail) = self
            .route_manager
            .batch_sync(&missing_prefixes, &HashMap::new(), &tag)
            .await;

        format!(
            "{} | 快照路由 {} 条，缺失追加 {} 条\n{} | 同步成功 {}, 同步失败 {}",
            tag, snapshot_count, missing_count, tag, ok, fail,
        )
    }

    /// 从快照中筛出 GoBGP Global RIB 当前不存在的前缀。
    fn missing_snapshot_prefixes(
        snapshot_prefixes: &HashMap<String, String>,
        existing_prefixes: &HashSet<String>,
    ) -> HashMap<String, String> {
        snapshot_prefixes
            .iter()
            .filter(|(prefix, _)| !existing_prefixes.contains(*prefix))
            .map(|(prefix, community)| (prefix.clone(), community.clone()))
            .collect()
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

    /// 将前缀集合按前缀排序后写入内存缓存，保证后续差异比较顺序稳定
    async fn store_cached_prefixes(
        lock: &Arc<Mutex<Option<PrefixEntry>>>,
        entries: &HashMap<String, String>,
        only_if_empty: bool,
    ) {
        let mut guard = lock.lock().await;
        if only_if_empty && guard.is_some() {
            return;
        }

        let mut sorted: PrefixEntry = entries
            .iter()
            .map(|(prefix, community)| (prefix.clone(), community.clone()))
            .collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        *guard = Some(sorted);
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
        let changed_prefixes: Vec<String> = prefixes
            .iter()
            .filter(|(prefix, community)| {
                last_set
                    .get(*prefix)
                    .is_some_and(|last_community| last_community != *community)
            })
            .map(|(prefix, _)| prefix.clone())
            .collect();
        let changed_count = changed_prefixes.len();

        if prefixes == last_set {
            let _ = self.route_manager.save_snapshot(&prefixes, snapshot_file);
            return format!("{} | 路由总数 {}, 新增 0, 删除 0, 无变化", tag, total);
        }

        // 构建 to_add: 本次新增的前缀，或团体字变化后需要重新注入的前缀
        let to_add: HashMap<String, String> = prefixes
            .iter()
            .filter(|(prefix, community)| {
                !last_set.contains_key(*prefix)
                    || last_set
                        .get(*prefix)
                        .is_some_and(|last_community| last_community != *community)
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // 构建 to_del: 上次有但本次没有，或团体字变化后需要先删除再添加的前缀
        let mut to_del: HashMap<String, String> = last_set
            .iter()
            .filter(|(prefix, _)| !prefixes.contains_key(*prefix))
            .map(|(prefix, community)| (prefix.clone(), community.clone()))
            .collect();
        to_del.extend(changed_prefixes.into_iter().filter_map(|prefix| {
            last_set
                .get(&prefix)
                .map(|community| (prefix, community.clone()))
        }));

        log::info!(
            "{} | 新增 {}, 删除 {}, 更新 {}",
            tag,
            added_count,
            removed_count,
            changed_count
        );

        let (ok, fail) = self.route_manager.batch_sync(&to_add, &to_del, &tag).await;

        // 只有 GoBGP 增删全部成功后才推进快照，避免快照与实际 RIB 长期不一致
        if fail > 0 {
            return format!(
                "{} | 路由总数 {}, 新增 {}, 删除 {}, 更新 {}\n{} | 同步成功 {}, 同步失败 {}，保留旧快照等待下次重试",
                tag, total, added_count, removed_count, changed_count, tag, ok, fail,
            );
        }

        if let Err(e) = self.route_manager.save_snapshot(&prefixes, snapshot_file) {
            log::warn!("{} | 快照保存失败: {}", tag, e);
        }

        Self::store_cached_prefixes(last_prefixes_lock, &prefixes, false).await;

        format!(
            "{} | 路由总数 {}, 新增 {}, 删除 {}, 更新 {}\n{} | 同步成功 {}, 同步失败 {}",
            tag, total, added_count, removed_count, changed_count, tag, ok, fail,
        )
    }
}
