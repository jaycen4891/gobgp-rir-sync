use std::process::Command;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use crate::config::Settings;

/// 并发执行 GoBGP 命令的返回结果
pub struct ConcurrencyResult {
    pub ok: u32,
    pub fail: u32,
}

/// 带团体字的路由条目
pub struct RouteEntry {
    pub prefix: String,
    pub community: String,
}

/// GoBGP命令执行器
pub struct CommandExecutor;

impl CommandExecutor {
    /// 并发添加路由（带团体字），返回 (成功数, 失败数)
    pub async fn add_routes(
        entries: &[RouteEntry],
        settings: &Settings,
        tag: &str,
        concurrency: usize,
    ) -> ConcurrencyResult {
        let total = entries.len();
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let mut handles: Vec<JoinHandle<bool>> = Vec::with_capacity(total);
        let gobgp_path = settings.gobgp_path.clone();
        let started = std::time::Instant::now();

        log::info!(
            "{} | 添加开始执行, 共 {} 条, 并发 {}",
            tag,
            total,
            concurrency
        );

        for entry in entries {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let p = entry.prefix.clone();
            let c = entry.community.clone();
            let path = gobgp_path.clone();
            let t = tag.to_string();

            handles.push(tokio::spawn(async move {
                let result = if c.is_empty() {
                    // 无团体字
                    let args = if p.contains(':') {
                        vec!["global", "rib", "add", "-a", "ipv6", &p]
                    } else {
                        vec!["global", "rib", "add", "-a", "ipv4", &p]
                    };
                    Self::execute_command(&path, &args)
                } else {
                    let args = if p.contains(':') {
                        vec!["global", "rib", "add", "-a", "ipv6", &p, "community", &c]
                    } else {
                        vec!["global", "rib", "add", "-a", "ipv4", &p, "community", &c]
                    };
                    Self::execute_command(&path, &args)
                };

                if result {
                    log::debug!("{} | 添加成功: {} ({})", t, p, c);
                } else {
                    log::warn!("{} | 添加失败: {} ({})", t, p, c);
                }
                drop(permit);
                result
            }));
        }

        let mut ok = 0u32;
        let mut fail = 0u32;

        for handle in handles.into_iter() {
            match handle.await {
                Ok(true) => ok += 1,
                Ok(false) => fail += 1,
                Err(e) => {
                    log::error!("{} | 任务异常: {}", tag, e);
                    fail += 1;
                }
            }
        }

        let elapsed = started.elapsed().as_secs_f64();
        let rate = if elapsed > 0.0 {
            total as f64 / elapsed
        } else {
            0.0
        };
        log::info!(
            "{} | 添加完成 — 成功 {}, 失败 {}, 总数 {}, 耗时 {:.1}s, 平均速率 {:.0}条/s",
            tag,
            ok,
            fail,
            total,
            elapsed,
            rate
        );

        ConcurrencyResult { ok, fail }
    }

    /// 并发删除路由，返回 (成功数, 失败数)
    pub async fn del_routes(
        prefixes: &[String],
        settings: &Settings,
        tag: &str,
        concurrency: usize,
    ) -> ConcurrencyResult {
        Self::execute_batch(prefixes, settings, tag, concurrency, true).await
    }

    /// 批量并发执行
    async fn execute_batch(
        prefixes: &[String],
        settings: &Settings,
        tag: &str,
        concurrency: usize,
        is_del: bool,
    ) -> ConcurrencyResult {
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let total = prefixes.len();
        let mut handles: Vec<JoinHandle<bool>> = Vec::with_capacity(total);
        let gobgp_path = settings.gobgp_path.clone();
        let started = std::time::Instant::now();

        let action = if is_del { "删除" } else { "添加" };
        log::info!(
            "{} | {}开始执行, 共 {} 条, 并发 {}",
            tag,
            action,
            total,
            concurrency
        );

        for prefix in prefixes {
            let permit = semaphore.clone().acquire_owned().await.unwrap();
            let p = prefix.clone();
            let path = gobgp_path.clone();
            let t = tag.to_string();
            let a = action.to_string();

            handles.push(tokio::spawn(async move {
                let result = if p.contains(':') {
                    // IPv6
                    Self::execute_command(
                        &path,
                        &[
                            "global",
                            "rib",
                            if is_del { "del" } else { "add" },
                            "-a",
                            "ipv6",
                            &p,
                        ],
                    )
                } else {
                    // IPv4
                    Self::execute_command(
                        &path,
                        &[
                            "global",
                            "rib",
                            if is_del { "del" } else { "add" },
                            "-a",
                            "ipv4",
                            &p,
                        ],
                    )
                };

                if result {
                    log::debug!("{} | {}成功: {}", t, a, p);
                } else {
                    log::warn!("{} | {}失败: {}", t, a, p);
                }
                drop(permit);
                result
            }));
        }

        let mut ok = 0u32;
        let mut fail = 0u32;

        for handle in handles.into_iter() {
            match handle.await {
                Ok(true) => ok += 1,
                Ok(false) => fail += 1,
                Err(e) => {
                    log::error!("{} | 任务异常: {}", tag, e);
                    fail += 1;
                }
            }
        }

        let elapsed = started.elapsed().as_secs_f64();
        let rate = if elapsed > 0.0 {
            total as f64 / elapsed
        } else {
            0.0
        };
        log::info!(
            "{} | {}完成 — 成功 {}, 失败 {}, 总数 {}, 耗时 {:.1}s, 平均速率 {:.0}条/s",
            tag,
            action,
            ok,
            fail,
            total,
            elapsed,
            rate
        );

        ConcurrencyResult { ok, fail }
    }

    /// 执行单条命令（同步，在 blocking 线程中运行）
    fn execute_command(gobgp_path: &str, args: &[&str]) -> bool {
        match Command::new(gobgp_path).args(args).output() {
            Ok(output) => {
                if output.status.success() {
                    true
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    log::error!("命令执行失败: {} {:?}", gobgp_path, args);
                    if !stderr.trim().is_empty() {
                        log::error!("错误详情: {}", stderr.trim());
                    }
                    false
                }
            }
            Err(e) => {
                log::error!("执行命令异常: {} {:?} - {}", gobgp_path, args, e);
                false
            }
        }
    }
}
