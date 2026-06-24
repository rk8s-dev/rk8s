use std::{sync::Arc, time::Duration};

use dquic::prelude::QuicClient;
use futures::Stream;
use prost::Message;
pub use xlinerpc::Streaming;
use xlinerpc::{H3Channel, MethodId, status::Status};

#[derive(Clone)]
pub(crate) struct Channel {
    inner: H3Channel,
    token: Option<Arc<str>>,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}

impl Channel {
    #[inline]
    pub(crate) fn new(
        client: Arc<QuicClient>,
        addrs: Vec<String>,
        token: Option<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            inner: H3Channel::new(client, addrs, timeout),
            token: token.map(String::into_boxed_str).map(Arc::from),
        }
    }

    #[inline]
    pub(crate) fn with_token(&self, token: Option<String>) -> Self {
        Self {
            inner: self.inner.clone(),
            token: token.map(String::into_boxed_str).map(Arc::from),
        }
    }

    #[inline]
    fn metadata(&self) -> Vec<(String, String)> {
        self.token
            .as_ref()
            .map(|token| vec![(String::from("authorization"), token.to_string())])
            .unwrap_or_default()
    }

    /// Convert MethodId to hex string for HTTP header
    fn method_id_to_hex(method: MethodId) -> String {
        format!("{:04X}", method.as_u16())
    }

    /// Map MethodId to the corresponding service URL path
    fn method_to_path(method: MethodId) -> &'static str {
        match method {
            // Xline Auth -> /etcdserverpb.Auth
            MethodId::XlineAuthenticate => "/etcdserverpb.Auth/Authenticate",

            // Xline Lease -> /etcdserverpb.Lease
            MethodId::XlineLeaseRevoke => "/etcdserverpb.Lease/LeaseRevoke",
            MethodId::XlineLeaseKeepAlive => "/etcdserverpb.Lease/LeaseKeepAlive",
            MethodId::XlineLeaseTtl => "/etcdserverpb.Lease/LeaseTimeToLive",

            // Xline Watch -> /etcdserverpb.Watch
            MethodId::XlineWatch => "/etcdserverpb.Watch/Watch",

            // Xline Maintenance -> /etcdserverpb.Maintenance
            MethodId::XlineSnapshot => "/etcdserverpb.Maintenance/Snapshot",
            MethodId::XlineAlarm => "/etcdserverpb.Maintenance/Alarm",
            MethodId::XlineMaintStatus => "/etcdserverpb.Maintenance/Status",

            // Xline Cluster -> /etcdserverpb.Cluster
            MethodId::XlineMemberAdd => "/etcdserverpb.Cluster/MemberAdd",
            MethodId::XlineMemberRemove => "/etcdserverpb.Cluster/MemberRemove",
            MethodId::XlineMemberPromote => "/etcdserverpb.Cluster/MemberPromote",
            MethodId::XlineMemberUpdate => "/etcdserverpb.Cluster/MemberUpdate",
            MethodId::XlineMemberList => "/etcdserverpb.Cluster/MemberList",

            // Xline KV -> /etcdserverpb.KV
            MethodId::XlineCompact => "/etcdserverpb.KV/Compact",

            // All curp protocol methods
            MethodId::FetchCluster => "/commandpb.Protocol/FetchCluster",
            MethodId::FetchReadState => "/commandpb.Protocol/FetchReadState",
            MethodId::Record => "/commandpb.Protocol/Record",
            MethodId::ReadIndex => "/commandpb.Protocol/ReadIndex",
            MethodId::Shutdown => "/commandpb.Protocol/Shutdown",
            MethodId::ProposeConfChange => "/commandpb.Protocol/ProposeConfChange",
            MethodId::Publish => "/commandpb.Protocol/Publish",
            MethodId::MoveLeader => "/commandpb.Protocol/MoveLeader",
            MethodId::ProposeStream => "/commandpb.Protocol/ProposeStream",
            MethodId::LeaseKeepAlive => "/commandpb.Protocol/LeaseKeepAlive",

            // Inner protocol methods (currently not used by xline client) - fall back to service path
            _ => "/commandpb.Protocol/Propose",
        }
    }

    pub(crate) async fn unary<Req, Resp>(&self, method: MethodId, req: Req) -> Result<Resp, Status>
    where
        Req: Message,
        Resp: Message + Default,
    {
        let path = Self::method_to_path(method);
        let method_hex = Self::method_id_to_hex(method);
        self.inner
            .unary(path, Some(method_hex.as_str()), &self.metadata(), req)
            .await
    }

    pub(crate) async fn server_streaming<Req, Resp>(
        &self,
        method: MethodId,
        req: Req,
    ) -> Result<Streaming<Resp>, Status>
    where
        Req: Message,
        Resp: Message + Default + Send + Unpin + 'static,
    {
        let path = Self::method_to_path(method);
        let method_hex = Self::method_id_to_hex(method);
        self.inner
            .server_streaming(path, Some(method_hex.as_str()), &self.metadata(), req)
            .await
    }

    pub(crate) async fn client_streaming<Req, Resp, St>(
        &self,
        method: MethodId,
        input: St,
    ) -> Result<Streaming<Resp>, Status>
    where
        Req: Message + Send + 'static,
        Resp: Message + Default + Send + Unpin + 'static,
        St: Stream<Item = Req> + Send + 'static,
    {
        let path = Self::method_to_path(method);
        let method_hex = Self::method_id_to_hex(method);
        self.inner
            .client_streaming(path, Some(method_hex.as_str()), &self.metadata(), input)
            .await
    }
}
