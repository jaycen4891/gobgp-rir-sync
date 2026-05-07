use prost::Message;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tonic::transport::Channel;

use crate::config::Settings;
use crate::gobgp::apipb;
use apipb::gobgp_api_client::GobgpApiClient;

/// 并发执行 GoBGP API 的返回结果
pub struct ConcurrencyResult {
    pub ok: u32,
    pub fail: u32,
}

/// 带团体字的路由条目
pub struct RouteEntry {
    pub prefix: String,
    pub community: String,
}

/// GoBGP API 执行器
pub struct CommandExecutor;

impl CommandExecutor {
    /// 并发添加路由
    ///
    /// 每条路由都携带快照中计算出的团体字；下一跳会按团体字中的国家/地区数字码
    /// 匹配覆盖表，未命中时使用默认下一跳
    pub async fn add_routes(
        entries: &[RouteEntry],
        settings: &Settings,
        tag: &str,
        concurrency: usize,
    ) -> ConcurrencyResult {
        let total = entries.len();
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let mut handles: Vec<JoinHandle<bool>> = Vec::with_capacity(total);
        let client = match Self::connect(settings, tag).await {
            Some(client) => client,
            None => {
                return ConcurrencyResult {
                    ok: 0,
                    fail: total as u32,
                }
            }
        };
        let started = std::time::Instant::now();

        log::info!(
            "{} | 添加开始执行, 共 {} 条, 并发 {}",
            tag,
            total,
            concurrency
        );

        let mut acquire_fail = 0u32;
        for entry in entries {
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(e) => {
                    log::error!("{} | 添加任务获取并发许可失败: {}", tag, e);
                    acquire_fail += 1;
                    continue;
                }
            };
            let p = entry.prefix.clone();
            let c = entry.community.clone();
            let next_hop = settings.next_hop_for_community(&c, p.contains(':'));
            let mut route_client = client.clone();
            let t = tag.to_string();

            handles.push(tokio::spawn(async move {
                let result = Self::add_route(&mut route_client, &p, &c, &next_hop).await;

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
        let mut fail = acquire_fail;

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

    /// 并发删除路由
    ///
    /// 删除时不携带团体字，避免 GoBGP 因属性匹配过严导致路径无法移除；
    /// 历史团体字只用于选择当时注入路由使用的下一跳
    pub async fn del_routes(
        entries: &[RouteEntry],
        settings: &Settings,
        tag: &str,
        concurrency: usize,
    ) -> ConcurrencyResult {
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let total = entries.len();
        let mut handles: Vec<JoinHandle<bool>> = Vec::with_capacity(total);
        let client = match Self::connect(settings, tag).await {
            Some(client) => client,
            None => {
                return ConcurrencyResult {
                    ok: 0,
                    fail: total as u32,
                }
            }
        };
        let started = std::time::Instant::now();

        log::info!(
            "{} | 删除开始执行, 共 {} 条, 并发 {}",
            tag,
            total,
            concurrency
        );

        let mut acquire_fail = 0u32;
        for entry in entries {
            let permit = match semaphore.clone().acquire_owned().await {
                Ok(permit) => permit,
                Err(e) => {
                    log::error!("{} | 删除任务获取并发许可失败: {}", tag, e);
                    acquire_fail += 1;
                    continue;
                }
            };
            let p = entry.prefix.clone();
            let c = entry.community.clone();
            let next_hop = settings.next_hop_for_community(&c, p.contains(':'));
            let mut route_client = client.clone();
            let t = tag.to_string();

            handles.push(tokio::spawn(async move {
                let result = Self::delete_route(&mut route_client, &p, &next_hop).await;

                if result {
                    log::debug!("{} | 删除成功: {} ({})", t, p, c);
                } else {
                    log::warn!("{} | 删除失败: {} ({})", t, p, c);
                }
                drop(permit);
                result
            }));
        }

        let mut ok = 0u32;
        let mut fail = acquire_fail;

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
            "{} | 删除完成 — 成功 {}, 失败 {}, 总数 {}, 耗时 {:.1}s, 平均速率 {:.0}条/s",
            tag,
            ok,
            fail,
            total,
            elapsed,
            rate
        );

        ConcurrencyResult { ok, fail }
    }

    /// 建立 GoBGP gRPC 连接；连接失败时由调用方把整批任务记为失败
    async fn connect(settings: &Settings, tag: &str) -> Option<GobgpApiClient<Channel>> {
        match GobgpApiClient::connect(settings.gobgp_api_addr()).await {
            Ok(client) => Some(client),
            Err(e) => {
                log::error!("{} | 连接 GoBGP API 失败: {}", tag, e);
                None
            }
        }
    }

    /// 添加单条路由到 Global RIB
    async fn add_route(
        client: &mut GobgpApiClient<Channel>,
        prefix: &str,
        community: &str,
        next_hop: &str,
    ) -> bool {
        let path = match Self::build_path(prefix, community, next_hop) {
            Ok(path) => path,
            Err(e) => {
                log::error!("构造添加路由请求失败: {} - {}", prefix, e);
                return false;
            }
        };

        let request = apipb::AddPathRequest {
            table_type: apipb::TableType::Global as i32,
            vrf_id: String::new(),
            path: Some(path),
        };

        match client.add_path(request).await {
            Ok(_) => true,
            Err(e) => {
                log::error!("GoBGP API 添加路由失败: {} - {}", prefix, e);
                false
            }
        }
    }

    /// 从 Global RIB 删除单条路由
    ///
    /// `uuid` 留空，依靠 prefix/family/next-hop 删除；删除 Path 不携带团体字
    async fn delete_route(
        client: &mut GobgpApiClient<Channel>,
        prefix: &str,
        next_hop: &str,
    ) -> bool {
        let path = match Self::build_path(prefix, "", next_hop) {
            Ok(path) => path,
            Err(e) => {
                log::error!("构造删除路由请求失败: {} - {}", prefix, e);
                return false;
            }
        };

        let request = apipb::DeletePathRequest {
            table_type: apipb::TableType::Global as i32,
            vrf_id: String::new(),
            family: path.family.clone(),
            path: Some(path),
            uuid: Vec::new(),
        };

        match client.delete_path(request).await {
            Ok(_) => true,
            Err(e) => {
                log::error!("GoBGP API 删除路由失败: {} - {}", prefix, e);
                false
            }
        }
    }

    /// 构造 GoBGP v3 API 需要的 Path
    fn build_path(prefix: &str, community: &str, next_hop: &str) -> anyhow::Result<apipb::Path> {
        let (ip, prefix_len) = Self::parse_cidr(prefix)?;
        let is_ipv6 = matches!(ip, IpAddr::V6(_));
        let afi = if is_ipv6 {
            apipb::family::Afi::Ip6
        } else {
            apipb::family::Afi::Ip
        };

        let mut pattrs = vec![Self::encode_any(
            "apipb.OriginAttribute",
            &apipb::OriginAttribute { origin: 0 },
        )?];

        let family = apipb::Family {
            afi: afi as i32,
            safi: apipb::family::Safi::Unicast as i32,
        };
        let nlri = Self::encode_any(
            "apipb.IPAddressPrefix",
            &apipb::IpAddressPrefix {
                prefix_len,
                prefix: ip.to_string(),
            },
        )?;

        pattrs.push(Self::encode_any(
            "apipb.NextHopAttribute",
            &apipb::NextHopAttribute {
                next_hop: next_hop.to_string(),
            },
        )?);

        if !community.trim().is_empty() {
            pattrs.push(Self::encode_any(
                "apipb.CommunitiesAttribute",
                &apipb::CommunitiesAttribute {
                    communities: vec![Self::community_to_u32(community)?],
                },
            )?);
        }

        Ok(apipb::Path {
            nlri: Some(nlri),
            pattrs,
            is_withdraw: false,
            no_implicit_withdraw: false,
            family: Some(family),
        })
    }

    /// 将 prost 消息编码为 `google.protobuf.Any`
    fn encode_any<M: Message>(
        type_name: &str,
        message: &M,
    ) -> anyhow::Result<apipb::google::protobuf::Any> {
        let mut value = Vec::new();
        message.encode(&mut value)?;
        Ok(apipb::google::protobuf::Any {
            type_url: format!("type.googleapis.com/{}", type_name),
            value,
        })
    }

    /// 将 `ASN:VALUE` 格式的标准 community 转为 32-bit 整数
    fn community_to_u32(community: &str) -> anyhow::Result<u32> {
        let (asn, value) = community
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("community 必须是 <asn>:<value> 格式"))?;
        let asn: u32 = asn.parse()?;
        let value: u32 = value.parse()?;

        if asn > u16::MAX as u32 || value > u16::MAX as u32 {
            anyhow::bail!("标准 community 每段必须在 0..=65535 范围内");
        }

        Ok((asn << 16) | value)
    }

    /// 解析并校验 CIDR 前缀
    fn parse_cidr(prefix: &str) -> anyhow::Result<(IpAddr, u32)> {
        let (ip, prefix_len) = prefix
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("路由前缀必须是 CIDR 格式"))?;
        let ip: IpAddr = ip.parse()?;
        let prefix_len: u32 = prefix_len.parse()?;

        let max_len = match ip {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };

        if prefix_len > max_len {
            anyhow::bail!("前缀长度 {} 超出最大值 {}", prefix_len, max_len);
        }

        Ok((ip, prefix_len))
    }
}
