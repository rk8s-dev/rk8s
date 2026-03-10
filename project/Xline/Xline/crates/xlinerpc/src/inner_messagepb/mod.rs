//! Inner message protocol buffer types for CURP

pub mod types;

pub use types::*;

// Re-export tonic server types
pub use self::servers::inner_protocol_server;

mod servers {
    use tonic::server::NamedService;

    pub mod inner_protocol_server {
        use super::*;
        use crate::inner_messagepb::types::*;

        pub trait InnerProtocol: Send + Sync + 'static {
            fn append_entries(
                &self,
                request: tonic::Request<AppendEntriesRequest>,
            ) -> Result<tonic::Response<AppendEntriesResponse>, tonic::Status>;

            fn vote(
                &self,
                request: tonic::Request<VoteRequest>,
            ) -> Result<tonic::Response<VoteResponse>, tonic::Status>;

            fn install_snapshot(
                &self,
                request: tonic::Request<tonic::Streaming<InstallSnapshotRequest>>,
            ) -> Result<tonic::Response<InstallSnapshotResponse>, tonic::Status>;

            fn trigger_shutdown(
                &self,
                request: tonic::Request<TriggerShutdownRequest>,
            ) -> Result<tonic::Response<TriggerShutdownResponse>, tonic::Status>;

            fn try_become_leader_now(
                &self,
                request: tonic::Request<TryBecomeLeaderNowRequest>,
            ) -> Result<tonic::Response<TryBecomeLeaderNowResponse>, tonic::Status>;
        }

        #[derive(Debug, Clone)]
        pub struct InnerProtocolServer<S> {
            inner: std::sync::Arc<S>,
        }

        impl<S> InnerProtocolServer<S>
        where
            S: InnerProtocol + Send + Sync + 'static,
        {
            pub fn new(service: S) -> Self {
                Self {
                    inner: std::sync::Arc::new(service),
                }
            }
        }

        impl<S> NamedService for InnerProtocolServer<S>
        where
            S: InnerProtocol + Send + Sync + 'static,
        {
            const NAME: &'static str = "curp.InnerProtocol";
        }
    }
}