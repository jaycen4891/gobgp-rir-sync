//! 手动验证 GoBGP API 添加/删除路由的测试工具。
//!
//! 默认会向 `10.64.129.53:50051` 添加 `1.1.1.1/32`，下一跳为
//! `198.19.0.254`，团体字为 `3166:156`。删除时只携带前缀和下一跳，
//! 不携带团体字，便于验证生产删除逻辑是否可以正常移除路由。

#[path = "../src/gobgp.rs"]
pub mod gobgp;

use clap::{Parser, ValueEnum};
use gobgp::apipb;
use prost::Message;

/// 手动测试动作。
#[derive(Debug, Clone, ValueEnum)]
enum Action {
    Add,
    Del,
    Both,
}

/// GoBGP API 路由添加/删除测试参数。
#[derive(Parser, Debug)]
#[command(name = "gobgp-route-test", about = "手动验证 GoBGP API 路由增删")]
struct Args {
    /// GoBGP API 地址，例如 http://10.64.129.53:50051
    #[arg(long, default_value = "http://10.64.129.53:50051")]
    api: String,

    /// 测试动作: add, del, both
    #[arg(long, value_enum, default_value_t = Action::Both)]
    action: Action,

    /// 测试前缀
    #[arg(long, default_value = "1.1.1.1/32")]
    prefix: String,

    /// 测试下一跳
    #[arg(long, default_value = "198.19.0.254")]
    next_hop: String,

    /// 添加路由时携带的团体字
    #[arg(long, default_value = "3166:156")]
    community: String,
}

/// 将 prost 消息编码为 GoBGP v3 API 使用的 Any。
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

/// 将 `ASN:VALUE` 标准团体字转换为 32-bit community。
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

/// 解析 CIDR，返回 IP 地址字符串、前缀长度和地址族。
fn parse_prefix(prefix: &str) -> anyhow::Result<(String, u32, apipb::family::Afi)> {
    let (ip, prefix_len) = prefix
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("测试前缀必须是 CIDR 格式"))?;
    let ip_addr: std::net::IpAddr = ip.parse()?;
    let prefix_len: u32 = prefix_len.parse()?;
    let max_len = if ip_addr.is_ipv6() { 128 } else { 32 };

    if prefix_len > max_len {
        anyhow::bail!("前缀长度 {} 超出最大值 {}", prefix_len, max_len);
    }

    let afi = if ip_addr.is_ipv6() {
        apipb::family::Afi::Ip6
    } else {
        apipb::family::Afi::Ip
    };

    Ok((ip_addr.to_string(), prefix_len, afi))
}

/// 构造添加路由使用的 Path，包含 Origin、NextHop 和 Communities。
fn build_add_path(args: &Args) -> anyhow::Result<apipb::Path> {
    build_path(args, Some(args.community.as_str()))
}

/// 构造删除路由使用的 Path，不携带 Communities。
fn build_del_path(args: &Args) -> anyhow::Result<apipb::Path> {
    build_path(args, None)
}

/// 根据是否传入团体字构造 GoBGP Path。
fn build_path(args: &Args, community: Option<&str>) -> anyhow::Result<apipb::Path> {
    let (ip, prefix_len, afi) = parse_prefix(&args.prefix)?;
    let family = apipb::Family {
        afi: afi as i32,
        safi: apipb::family::Safi::Unicast as i32,
    };
    let nlri = encode_any(
        "apipb.IPAddressPrefix",
        &apipb::IpAddressPrefix {
            prefix_len,
            prefix: ip,
        },
    )?;
    let mut pattrs = vec![
        encode_any(
            "apipb.OriginAttribute",
            &apipb::OriginAttribute { origin: 0 },
        )?,
        encode_any(
            "apipb.NextHopAttribute",
            &apipb::NextHopAttribute {
                next_hop: args.next_hop.clone(),
            },
        )?,
    ];

    if let Some(community) = community {
        pattrs.push(encode_any(
            "apipb.CommunitiesAttribute",
            &apipb::CommunitiesAttribute {
                communities: vec![community_to_u32(community)?],
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

/// 调用 AddPath 添加测试路由。
async fn add_route(
    client: &mut apipb::gobgp_api_client::GobgpApiClient<tonic::transport::Channel>,
    args: &Args,
) -> anyhow::Result<()> {
    client
        .add_path(apipb::AddPathRequest {
            table_type: apipb::TableType::Global as i32,
            vrf_id: String::new(),
            path: Some(build_add_path(args)?),
        })
        .await?;
    println!(
        "add ok: prefix={}, next-hop={}, community={}",
        args.prefix, args.next_hop, args.community
    );
    Ok(())
}

/// 调用 DeletePath 删除测试路由；删除 Path 不携带团体字。
async fn del_route(
    client: &mut apipb::gobgp_api_client::GobgpApiClient<tonic::transport::Channel>,
    args: &Args,
) -> anyhow::Result<()> {
    let path = build_del_path(args)?;
    client
        .delete_path(apipb::DeletePathRequest {
            table_type: apipb::TableType::Global as i32,
            vrf_id: String::new(),
            family: path.family.clone(),
            path: Some(path),
            uuid: Vec::new(),
        })
        .await?;
    println!("del ok: prefix={}, next-hop={}", args.prefix, args.next_hop);
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut client = apipb::gobgp_api_client::GobgpApiClient::connect(args.api.clone()).await?;

    match args.action {
        Action::Add => add_route(&mut client, &args).await?,
        Action::Del => del_route(&mut client, &args).await?,
        Action::Both => {
            add_route(&mut client, &args).await?;
            del_route(&mut client, &args).await?;
        }
    }

    Ok(())
}
