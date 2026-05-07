use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::config::Settings;
use crate::models::rir::PrefixExtractor;
use crate::utils::command::{CommandExecutor, RouteEntry};

/// 路由管理器
pub struct RouteManager {
    settings: Settings,
}

impl RouteManager {
    /// 使用运行时配置创建路由管理器
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }

    /// 批量同步路由：先删除旧路由，再添加新路由（带团体字）
    /// 参数使用 `HashMap<前缀, 团体字>`；删除时团体字只用于选择当时注入的下一跳
    pub async fn batch_sync(
        &self,
        to_add: &HashMap<String, String>,
        to_del: &HashMap<String, String>,
        tag: &str,
    ) -> (u32, u32) {
        let mut ok = 0u32;
        let mut fail = 0u32;

        // 删除旧路由
        if !to_del.is_empty() {
            let entries: Vec<RouteEntry> = to_del
                .iter()
                .map(|(prefix, community)| RouteEntry {
                    prefix: prefix.clone(),
                    community: community.clone(),
                })
                .collect();
            let result = CommandExecutor::del_routes(
                &entries,
                &self.settings,
                tag,
                self.settings.concurrency,
            )
            .await;
            ok += result.ok;
            fail += result.fail;
        }

        // 添加新路由（带团体字）
        if !to_add.is_empty() {
            let entries: Vec<RouteEntry> = to_add
                .iter()
                .map(|(prefix, community)| RouteEntry {
                    prefix: prefix.clone(),
                    community: community.clone(),
                })
                .collect();
            let result = CommandExecutor::add_routes(
                &entries,
                &self.settings,
                tag,
                self.settings.concurrency,
            )
            .await;
            ok += result.ok;
            fail += result.fail;
        }

        (ok, fail)
    }

    /// 从文件加载快照，返回 (前缀→团体字) 映射
    /// 格式: 每行 "前缀 团体字" 或 "前缀"（兼容旧格式）
    pub fn load_snapshot(snapshot_file: &str) -> HashMap<String, String> {
        let path = Path::new(snapshot_file);
        if !path.exists() {
            return HashMap::new();
        }

        match fs::read_to_string(path) {
            Ok(content) => content
                .lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty())
                .filter_map(|line| {
                    // 支持两种格式：
                    // "前缀 团体字" 或 "前缀"（旧格式兼容）
                    let parts: Vec<&str> = line.splitn(2, ' ').collect();
                    let prefix = parts[0].trim().to_string();
                    if !PrefixExtractor::is_valid_cidr(&prefix) {
                        return None;
                    }
                    let community = parts.get(1).unwrap_or(&"").trim().to_string();
                    Some((prefix, community))
                })
                .collect(),
            Err(e) => {
                log::error!("加载快照失败: {}", e);
                HashMap::new()
            }
        }
    }

    /// 保存快照到文件
    /// 格式: 每行 "前缀 团体字"
    pub fn save_snapshot(
        &self,
        entries: &HashMap<String, String>,
        snapshot_file: &str,
    ) -> anyhow::Result<()> {
        // 确保父目录存在
        if let Some(parent) = Path::new(snapshot_file).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let mut sorted: Vec<(&String, &String)> = entries.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        let content = sorted
            .into_iter()
            .map(|(prefix, community)| {
                if community.is_empty() {
                    prefix.clone()
                } else {
                    format!("{} {}", prefix, community)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(snapshot_file, content)?;
        log::debug!("快照已保存: {}", snapshot_file);
        Ok(())
    }
}
