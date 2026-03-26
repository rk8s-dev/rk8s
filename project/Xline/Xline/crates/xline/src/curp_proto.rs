// Hand-maintained curp Protocol service definitions for the xline gRPC boundary layer.
//
// This module was extracted from tonic_build codegen output for curp-command.proto.
// It is now maintained manually to avoid depending on tonic_build codegen.
// The Protocol trait and ProtocolServer are used ONLY at the xline gRPC boundary
// (AuthWrapper) to expose curp RPCs on the client-facing tonic endpoint.
//
// Message types are imported from curp::rpc (prost-generated, no tonic dependency).

#[allow(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    unused_qualifications,
    unreachable_pub,
    variant_size_differences,
    missing_copy_implementations,
    missing_docs,
    trivial_casts,
    unused_results
)]
pub(crate) mod commandpb {
    pub mod protocol_server {
        #![allow(
            unused_variables,
            dead_code,
            missing_docs,
            clippy::wildcard_imports,
            clippy::let_unit_value
        )]
        use tonic::codegen::*;
        /// Generated trait containing gRPC methods that should be implemented for
        /// use with ProtocolServer.
        #[async_trait]
        pub trait Protocol: std::marker::Send + std::marker::Sync + 'static {
            /// Server streaming response type for the ProposeStream method.
            type ProposeStreamStream: tonic::codegen::tokio_stream::Stream<
                    Item = std::result::Result<::curp::rpc::OpResponse, tonic::Status>,
                > + std::marker::Send
                + 'static;
            /// Unary
            async fn propose_stream(
                &self,
                request: tonic::Request<::curp::rpc::ProposeRequest>,
            ) -> std::result::Result<tonic::Response<Self::ProposeStreamStream>, tonic::Status>;
            async fn record(
                &self,
                request: tonic::Request<::curp::rpc::RecordRequest>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::RecordResponse>, tonic::Status>;
            async fn read_index(
                &self,
                request: tonic::Request<::curp::rpc::ReadIndexRequest>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::ReadIndexResponse>, tonic::Status>;
            async fn propose_conf_change(
                &self,
                request: tonic::Request<::curp::rpc::ProposeConfChangeRequest>,
            ) -> std::result::Result<
                tonic::Response<::curp::rpc::ProposeConfChangeResponse>,
                tonic::Status,
            >;
            async fn publish(
                &self,
                request: tonic::Request<::curp::rpc::PublishRequest>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::PublishResponse>, tonic::Status>;
            async fn shutdown(
                &self,
                request: tonic::Request<::curp::rpc::ShutdownRequest>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::ShutdownResponse>, tonic::Status>;
            async fn fetch_cluster(
                &self,
                request: tonic::Request<::curp::rpc::FetchClusterRequest>,
            ) -> std::result::Result<
                tonic::Response<::curp::rpc::FetchClusterResponse>,
                tonic::Status,
            >;
            async fn fetch_read_state(
                &self,
                request: tonic::Request<::curp::rpc::FetchReadStateRequest>,
            ) -> std::result::Result<
                tonic::Response<::curp::rpc::FetchReadStateResponse>,
                tonic::Status,
            >;
            async fn move_leader(
                &self,
                request: tonic::Request<::curp::rpc::MoveLeaderRequest>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::MoveLeaderResponse>, tonic::Status>;
            /// Stream
            async fn lease_keep_alive(
                &self,
                request: tonic::Request<tonic::Streaming<::curp::rpc::LeaseKeepAliveMsg>>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::LeaseKeepAliveMsg>, tonic::Status>;
        }
    }
}
