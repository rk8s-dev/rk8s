//! Command protocol buffer types for CURP

pub mod types;

pub use types::*;

// Re-export tonic server/client types
pub use self::servers::{protocol_client, protocol_server};

mod servers {
    use tonic::transport::Channel;
    use tonic::server::NamedService;
    use tonic::codec::{CompressionEncoding, Decoder, Encoder};
    use tonic::body::BoxBody;
    use http::{Request, Response};
    use std::task::{Context, Poll};
    use tower_service::Service;
    use futures::future::BoxFuture;

    // Protocol client
    pub mod protocol_client {
        use super::*;
        
        pub struct ProtocolClient<T> {
            inner: T,
        }

        impl<T> ProtocolClient<T> {
            pub fn new(inner: T) -> Self {
                Self { inner }
            }
        }
    }

    // Protocol server
    pub mod protocol_server {
        use super::*;
        use crate::commandpb::types::*;

        pub trait Protocol: Send + Sync + 'static {
            fn propose(
                &self,
                request: tonic::Request<ProposeRequest>,
            ) -> Result<tonic::Response<ProposeResponse>, tonic::Status>;

            fn record(
                &self,
                request: tonic::Request<RecordRequest>,
            ) -> Result<tonic::Response<RecordResponse>, tonic::Status>;

            fn read_index(
                &self,
                request: tonic::Request<ReadIndexRequest>,
            ) -> Result<tonic::Response<ReadIndexResponse>, tonic::Status>;

            fn shutdown(
                &self,
                request: tonic::Request<ShutdownRequest>,
            ) -> Result<tonic::Response<ShutdownResponse>, tonic::Status>;

            fn propose_conf_change(
                &self,
                request: tonic::Request<ProposeConfChangeRequest>,
            ) -> Result<tonic::Response<ProposeConfChangeResponse>, tonic::Status>;

            fn publish(
                &self,
                request: tonic::Request<PublishRequest>,
            ) -> Result<tonic::Response<PublishResponse>, tonic::Status>;

            fn fetch_cluster(
                &self,
                request: tonic::Request<FetchClusterRequest>,
            ) -> Result<tonic::Response<FetchClusterResponse>, tonic::Status>;

            fn fetch_read_state(
                &self,
                request: tonic::Request<FetchReadStateRequest>,
            ) -> Result<tonic::Response<FetchReadStateResponse>, tonic::Status>;

            fn move_leader(
                &self,
                request: tonic::Request<MoveLeaderRequest>,
            ) -> Result<tonic::Response<MoveLeaderResponse>, tonic::Status>;
        }

        #[derive(Debug, Clone)]
        pub struct ProtocolServer<S> {
            inner: std::sync::Arc<S>,
        }

        impl<S> ProtocolServer<S>
        where
            S: Protocol + Send + Sync + 'static,
        {
            pub fn new(service: S) -> Self {
                Self {
                    inner: std::sync::Arc::new(service),
                }
            }
        }

        impl<S> NamedService for ProtocolServer<S>
        where
            S: Protocol + Send + Sync + 'static,
        {
            const NAME: &'static str = "curp.Protocol";
        }
    }
}