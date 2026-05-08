/// GoBGP v3 使用的 `apipb` 包
pub mod apipb {
    /// `google.protobuf.Any` 的最小定义，用于承载 NLRI 和 Path Attribute
    pub mod google {
        pub mod protobuf {
            #[derive(Clone, PartialEq, ::prost::Message)]
            pub struct Any {
                #[prost(string, tag = "1")]
                pub type_url: String,
                #[prost(bytes = "vec", tag = "2")]
                pub value: Vec<u8>,
            }
        }
    }

    /// GoBGP 表类型；本工具只写入 Global RIB
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum TableType {
        Global = 0,
        Local = 1,
        AdjIn = 2,
        AdjOut = 3,
        Vrf = 4,
    }

    /// BGP 地址族，决定路径属于 IPv4/IPv6 Unicast
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Family {
        #[prost(enumeration = "family::Afi", tag = "1")]
        pub afi: i32,
        #[prost(enumeration = "family::Safi", tag = "2")]
        pub safi: i32,
    }

    /// 地址族和 SAFI 枚举
    pub mod family {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum Afi {
            Unknown = 0,
            Ip = 1,
            Ip6 = 2,
        }

        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum Safi {
            Unknown = 0,
            Unicast = 1,
        }
    }

    /// 前缀 NLRI，编码 CIDR 中的地址和前缀长度
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct IpAddressPrefix {
        #[prost(uint32, tag = "1")]
        pub prefix_len: u32,
        #[prost(string, tag = "2")]
        pub prefix: String,
    }

    /// BGP Origin 属性；本工具使用 IGP(0)
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct OriginAttribute {
        #[prost(uint32, tag = "1")]
        pub origin: u32,
    }

    /// BGP Next Hop 属性
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct NextHopAttribute {
        #[prost(string, tag = "1")]
        pub next_hop: String,
    }

    /// 标准 32-bit community 属性
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct CommunitiesAttribute {
        #[prost(uint32, repeated, packed = "true", tag = "1")]
        pub communities: Vec<u32>,
    }

    /// GoBGP Path；删除时也通过同一结构匹配待删除路径
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Path {
        #[prost(message, optional, tag = "1")]
        pub nlri: Option<google::protobuf::Any>,
        #[prost(message, repeated, tag = "2")]
        pub pattrs: Vec<google::protobuf::Any>,
        #[prost(bool, tag = "5")]
        pub is_withdraw: bool,
        #[prost(bool, tag = "8")]
        pub no_implicit_withdraw: bool,
        #[prost(message, optional, tag = "9")]
        pub family: Option<Family>,
    }

    /// AddPath 请求
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct AddPathRequest {
        #[prost(enumeration = "TableType", tag = "1")]
        pub table_type: i32,
        #[prost(string, tag = "2")]
        pub vrf_id: String,
        #[prost(message, optional, tag = "3")]
        pub path: Option<Path>,
    }

    /// AddPath 响应；本工具不依赖 UUID，快照同步按 Path 删除
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct AddPathResponse {
        #[prost(bytes = "vec", tag = "1")]
        pub uuid: Vec<u8>,
    }

    /// DeletePath 请求；uuid 为空时按 path/family 删除
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct DeletePathRequest {
        #[prost(enumeration = "TableType", tag = "1")]
        pub table_type: i32,
        #[prost(string, tag = "2")]
        pub vrf_id: String,
        #[prost(message, optional, tag = "3")]
        pub family: Option<Family>,
        #[prost(message, optional, tag = "4")]
        pub path: Option<Path>,
        #[prost(bytes = "vec", tag = "5")]
        pub uuid: Vec<u8>,
    }

    /// DeletePath 空响应
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct DeletePathResponse {}

    /// API representation of table.LookupPrefix.
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct TableLookupPrefix {
        #[prost(string, tag = "1")]
        pub prefix: String,
        #[prost(enumeration = "table_lookup_prefix::Type", tag = "2")]
        pub r#type: i32,
    }

    pub mod table_lookup_prefix {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum Type {
            Exact = 0,
            Longer = 1,
            Shorter = 2,
        }
    }

    /// ListPath 请求，用于查询 Global RIB 中的现有路由。
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ListPathRequest {
        #[prost(enumeration = "TableType", tag = "1")]
        pub table_type: i32,
        #[prost(string, tag = "2")]
        pub name: String,
        #[prost(message, optional, tag = "3")]
        pub family: Option<Family>,
        #[prost(message, repeated, tag = "4")]
        pub prefixes: Vec<TableLookupPrefix>,
        #[prost(enumeration = "list_path_request::SortType", tag = "5")]
        pub sort_type: i32,
        #[prost(bool, tag = "6")]
        pub enable_filtered: bool,
        #[prost(bool, tag = "7")]
        pub enable_nlri_binary: bool,
        #[prost(bool, tag = "8")]
        pub enable_attribute_binary: bool,
        #[prost(bool, tag = "9")]
        pub enable_only_binary: bool,
    }

    pub mod list_path_request {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum SortType {
            None = 0,
            Prefix = 1,
        }
    }

    /// ListPath 响应中的目的前缀。
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Destination {
        #[prost(string, tag = "1")]
        pub prefix: String,
        #[prost(message, repeated, tag = "2")]
        pub paths: Vec<Path>,
    }

    /// ListPath 流式响应。
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ListPathResponse {
        #[prost(message, optional, tag = "1")]
        pub destination: Option<Destination>,
    }

    /// `apipb.GobgpApi` 的最小 tonic 客户端
    pub mod gobgp_api_client {
        use tonic::codegen::{http, Body, Bytes, StdError};

        /// 可克隆的 GoBGP API 客户端，便于并发任务复用同一 Channel
        #[derive(Debug, Clone)]
        pub struct GobgpApiClient<T> {
            inner: tonic::client::Grpc<T>,
        }

        impl GobgpApiClient<tonic::transport::Channel> {
            pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
            where
                D: TryInto<tonic::transport::Endpoint>,
                D::Error: Into<StdError>,
            {
                let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
                Ok(Self::new(conn))
            }
        }

        impl<T> GobgpApiClient<T>
        where
            T: tonic::client::GrpcService<tonic::body::Body>,
            T::Error: Into<StdError>,
            T::ResponseBody: Body<Data = Bytes> + Send + 'static,
            <T::ResponseBody as Body>::Error: Into<StdError> + Send,
        {
            pub fn new(inner: T) -> Self {
                let inner = tonic::client::Grpc::new(inner);
                Self { inner }
            }

            pub async fn add_path(
                &mut self,
                request: impl tonic::IntoRequest<super::AddPathRequest>,
            ) -> Result<tonic::Response<super::AddPathResponse>, tonic::Status> {
                self.inner.ready().await.map_err(|e| {
                    tonic::Status::unknown(format!("GoBGP API 服务未就绪: {}", e.into()))
                })?;
                let path = http::uri::PathAndQuery::from_static("/apipb.GobgpApi/AddPath");
                let codec = tonic_prost::ProstCodec::default();
                self.inner.unary(request.into_request(), path, codec).await
            }

            pub async fn delete_path(
                &mut self,
                request: impl tonic::IntoRequest<super::DeletePathRequest>,
            ) -> Result<tonic::Response<super::DeletePathResponse>, tonic::Status> {
                self.inner.ready().await.map_err(|e| {
                    tonic::Status::unknown(format!("GoBGP API 服务未就绪: {}", e.into()))
                })?;
                let path = http::uri::PathAndQuery::from_static("/apipb.GobgpApi/DeletePath");
                let codec = tonic_prost::ProstCodec::default();
                self.inner.unary(request.into_request(), path, codec).await
            }

            pub async fn list_path(
                &mut self,
                request: impl tonic::IntoRequest<super::ListPathRequest>,
            ) -> Result<
                tonic::Response<tonic::codec::Streaming<super::ListPathResponse>>,
                tonic::Status,
            > {
                self.inner.ready().await.map_err(|e| {
                    tonic::Status::unknown(format!("GoBGP API 服务未就绪: {}", e.into()))
                })?;
                let path = http::uri::PathAndQuery::from_static("/apipb.GobgpApi/ListPath");
                let codec = tonic_prost::ProstCodec::default();
                self.inner
                    .server_streaming(request.into_request(), path, codec)
                    .await
            }
        }
    }
}
