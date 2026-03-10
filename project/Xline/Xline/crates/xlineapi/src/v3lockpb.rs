//! Lock protocol buffer types (manually implemented)

use prost::Message;
use serde::{Deserialize, Serialize};

// ============================================================================
// Lock Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct LockRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub name: ::prost::alloc::vec::Vec<u8>,
    #[prost(int64, tag = "2")]
    pub lease: i64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct LockResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<super::etcdserverpb::ResponseHeader>,
    #[prost(bytes = "vec", tag = "2")]
    pub key: ::prost::alloc::vec::Vec<u8>,
}

// ============================================================================
// Unlock Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct UnlockRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct UnlockResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<super::etcdserverpb::ResponseHeader>,
}

// ============================================================================
// gRPC Service Traits
// ============================================================================

pub mod lock_server {
    use super::*;
    use tonic::{Request, Response, Status};
    use tonic::server::NamedService;

    #[async_trait::async_trait]
    pub trait Lock: Send + Sync + 'static {
        async fn lock(&self, request: Request<LockRequest>) -> Result<Response<LockResponse>, Status>;
        async fn unlock(&self, request: Request<UnlockRequest>) -> Result<Response<UnlockResponse>, Status>;
    }

    #[derive(Debug, Clone)]
    pub struct LockServer<S> {
        inner: std::sync::Arc<S>,
    }

    impl<S> LockServer<S>
    where
        S: Lock + Send + Sync + 'static,
    {
        pub fn new(service: S) -> Self {
            Self {
                inner: std::sync::Arc::new(service),
            }
        }
    }

    impl<S> NamedService for LockServer<S>
    where
        S: Lock + Send + Sync + 'static,
    {
        const NAME: &'static str = "v3lockpb.Lock";
    }
}

pub mod lock_client {
    use super::*;
    use tonic::transport::Channel;
    use tonic::client::GrpcService;

    pub struct LockClient<T> {
        inner: tonic::client::Grpc<T>,
    }

    impl<T> LockClient<T>
    where
        T: GrpcService<tonic::body::BoxBody>,
        T::Error: Into<tonic::codegen::StdError>,
        T::ResponseBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
        <T::ResponseBody as http_body::Body>::Error: Into<tonic::codegen::StdError> + Send,
    {
        pub fn new(channel: T) -> Self {
            Self {
                inner: tonic::client::Grpc::new(channel),
            }
        }
    }
}