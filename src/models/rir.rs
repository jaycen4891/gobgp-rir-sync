use std::collections::HashMap;
use std::time::Duration;

use crate::config::{IpVersion, Settings};
use crate::models::country::CountryCodeMap;

/// RIR 数据获取器
///
/// 每次同步按配置决定需要下载哪些 RIR delegated 文件；任一目标源下载失败时，
/// 本轮同步会失败并保留旧快照，避免使用不完整数据更新 GoBGP
pub struct RIRDataFetcher {
    retry: u32,
    timeout: u64,
}

impl Default for RIRDataFetcher {
    fn default() -> Self {
        Self {
            retry: 3,
            timeout: 120,
        }
    }
}

impl RIRDataFetcher {
    /// 使用默认重试次数和超时时间创建下载器
    pub fn new() -> Self {
        Self::default()
    }

    /// 下载本轮需要处理的 RIR 数据
    pub async fn download_rir_data(
        &self,
        settings: &Settings,
    ) -> anyhow::Result<HashMap<String, String>> {
        // 获取需要下载的RIR列表
        let rir_list = settings.get_rir_list();
        let mut rir_data = HashMap::new();

        for rir_name in &rir_list {
            let url = match settings.rir_urls.get(rir_name) {
                Some(u) => u.clone(),
                None => {
                    log::warn!("未知RIR: {}", rir_name);
                    continue;
                }
            };

            log::info!("下载 {} 数据", rir_name);
            match self.download_with_retry(&url).await {
                Ok(data) => {
                    rir_data.insert(rir_name.clone(), data);
                }
                Err(e) => {
                    log::error!("下载 {} 失败: {}", rir_name, e);
                    return Err(e);
                }
            }
        }

        Ok(rir_data)
    }

    /// 带重试的 HTTP 下载
    ///
    /// delegated 文件可能较大，因此这里按 chunk 读取并周期性输出进度日志
    async fn download_with_retry(&self, url: &str) -> anyhow::Result<String> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(self.timeout))
            .no_gzip()
            .build()?;

        let mut last_error = None;

        for attempt in 1..=self.retry {
            log::info!("下载请求 (尝试 {}/{})...", attempt, self.retry);
            match client.get(url).send().await {
                Ok(mut resp) => {
                    if resp.status().is_success() {
                        log::info!("收到响应, 正在读取数据...");
                        let mut buf = String::new();
                        let mut total = 0u64;
                        loop {
                            match resp.chunk().await {
                                Ok(Some(chunk)) => {
                                    total += chunk.len() as u64;
                                    buf.push_str(&String::from_utf8_lossy(&chunk));
                                    if total.is_multiple_of(1024 * 1024) {
                                        log::info!("已读取 {} MB...", total / 1024 / 1024);
                                    }
                                }
                                Ok(None) => {
                                    log::info!("数据读取完成, 共 {} bytes", total);
                                    return Ok(buf);
                                }
                                Err(e) => {
                                    last_error = Some(anyhow::anyhow!("{}", e));
                                    log::warn!("读取响应体失败: {}", e);
                                    break;
                                }
                            }
                        }
                    } else {
                        last_error = Some(anyhow::anyhow!("HTTP {}: {}", resp.status(), url));
                    }
                }
                Err(e) => {
                    last_error = Some(anyhow::anyhow!("{}", e));
                    log::warn!("下载请求失败: {}", e);
                }
            }

            if attempt < self.retry {
                log::warn!("下载失败 (尝试 {}/{}), 3秒后重试...", attempt, self.retry);
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("下载失败")))
    }
}

/// RIR delegated 数据前缀提取器
pub struct PrefixExtractor;

impl PrefixExtractor {
    /// 从 IPv4 delegated 行的地址数量计算 CIDR 前缀长度
    fn count_to_prefixlen(count_str: &str) -> Option<u32> {
        let count: f64 = match count_str.parse() {
            Ok(v) => v,
            Err(e) => {
                log::warn!("计算prefixlen失败: {} - {}", count_str, e);
                return None;
            }
        };

        if count <= 0.0 {
            return None;
        }

        Some(32 - (count.log2().ceil() as u32))
    }

    /// 校验 CIDR 格式是否合法
    pub fn is_valid_cidr(prefix: &str) -> bool {
        let (ip_part, len_part) = match prefix.split_once('/') {
            Some(p) => p,
            None => return false,
        };

        // IP 部分不能包含 * 或其他非法字符
        if ip_part.contains('*') || ip_part.contains(' ') {
            return false;
        }

        // 长度部分必须是数字
        let prefix_len: u32 = match len_part.parse() {
            Ok(n) => n,
            Err(_) => return false,
        };

        // 判断是 IPv4 还是 IPv6
        if ip_part.contains(':') {
            prefix_len <= 128
        } else {
            let octets: Vec<&str> = ip_part.split('.').collect();
            if octets.len() != 4 {
                return false;
            }
            for octet in &octets {
                match octet.parse::<u32>() {
                    Ok(n) if n <= 255 => {}
                    _ => return false,
                }
            }
            prefix_len <= 32
        }
    }

    /// 提取 IPv4 前缀，返回 `前缀 -> 团体字` 映射
    ///
    /// RIR delegated IPv4 行中的第五列是地址数量，需要转换成 CIDR 前缀长度
    fn extract_ipv4_prefixes(
        text: &str,
        target_country: Option<&str>,
        exclude: bool,
        country_map: &CountryCodeMap,
        community_prefix: &str,
    ) -> HashMap<String, String> {
        let mut prefixes = HashMap::new();

        for line in text.lines() {
            if line.starts_with('#') || !line.contains("|ipv4|") {
                continue;
            }

            let parts: Vec<&str> = line.trim().split('|').collect();
            if parts.len() < 5 {
                continue;
            }

            let cc = parts[1];
            let base_ip = parts[3];
            let count_str = parts[4];

            // 跳过 base_ip 为 * 的汇总行
            if base_ip.contains('*') {
                continue;
            }

            // 根据国家代码过滤
            if let Some(target) = target_country {
                if exclude && cc == target {
                    continue;
                }
                if !exclude && cc != target {
                    continue;
                }
            }

            if let Some(prefix_len) = Self::count_to_prefixlen(count_str) {
                let cidr = format!("{}/{}", base_ip, prefix_len);
                if Self::is_valid_cidr(&cidr) {
                    // 获取团体字：如果国家代码查不到，用空字符串
                    let community = country_map
                        .community(cc, community_prefix)
                        .unwrap_or_default();
                    prefixes.insert(cidr, community);
                } else {
                    log::warn!("无效CIDR, 跳过: {}", cidr);
                }
            }
        }

        prefixes
    }

    /// 提取 IPv6 前缀，返回 `前缀 -> 团体字` 映射
    ///
    /// RIR delegated IPv6 行中的第五列已经是前缀长度，可直接拼成 CIDR
    fn extract_ipv6_prefixes(
        text: &str,
        target_country: Option<&str>,
        exclude: bool,
        country_map: &CountryCodeMap,
        community_prefix: &str,
    ) -> HashMap<String, String> {
        let mut prefixes = HashMap::new();

        for line in text.lines() {
            if line.starts_with('#') || !line.contains("|ipv6|") {
                continue;
            }

            let parts: Vec<&str> = line.trim().split('|').collect();
            if parts.len() < 5 {
                continue;
            }

            let cc = parts[1];
            let base_ip = parts[3];
            let prefix_len = parts[4];

            // 跳过 base_ip 为 * 的汇总行
            if base_ip.contains('*') {
                continue;
            }

            // 根据国家代码过滤
            if let Some(target) = target_country {
                if exclude && cc == target {
                    continue;
                }
                if !exclude && cc != target {
                    continue;
                }
            }

            let cidr = format!("{}/{}", base_ip, prefix_len);
            if Self::is_valid_cidr(&cidr) {
                let community = country_map
                    .community(cc, community_prefix)
                    .unwrap_or_default();
                prefixes.insert(cidr, community);
            } else {
                log::warn!("无效CIDR, 跳过: {}", cidr);
            }
        }

        prefixes
    }

    /// 根据国家模式获取前缀
    ///
    /// `ALL` 不做国家过滤，`NONECN` 排除 CN，其它值只保留对应国家/地区
    /// 返回值分别是 IPv4 和 IPv6 的 `前缀 -> 团体字` 映射
    pub fn get_prefixes_by_country_mode(
        settings: &Settings,
        rir_data: &HashMap<String, String>,
        ip_version: Option<&IpVersion>,
    ) -> (HashMap<String, String>, HashMap<String, String>) {
        let country_map = CountryCodeMap::default();
        let community_prefix = &settings.community_prefix;
        let mut ipv4_prefixes = HashMap::new();
        let mut ipv6_prefixes = HashMap::new();

        let process_ipv4 = ip_version.map(|v| v.should_process_ipv4()).unwrap_or(true);
        let process_ipv6 = ip_version.map(|v| v.should_process_ipv6()).unwrap_or(true);

        let filter_cn = settings.should_filter_cn();
        let target_country = if filter_cn {
            "CN"
        } else {
            &settings.country_code
        };

        let country_mode = &settings.country_code;

        for data in rir_data.values() {
            if process_ipv4 {
                ipv4_prefixes.extend(Self::extract_ipv4_prefixes(
                    data,
                    if country_mode == "ALL" {
                        None
                    } else {
                        Some(target_country)
                    },
                    filter_cn,
                    &country_map,
                    community_prefix,
                ));
            }

            if process_ipv6 {
                ipv6_prefixes.extend(Self::extract_ipv6_prefixes(
                    data,
                    if country_mode == "ALL" {
                        None
                    } else {
                        Some(target_country)
                    },
                    filter_cn,
                    &country_map,
                    community_prefix,
                ));
            }
        }

        (ipv4_prefixes, ipv6_prefixes)
    }
}
