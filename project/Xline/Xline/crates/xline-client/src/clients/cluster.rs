#[cfg(feature = "quic")]
use std::sync::Arc;

use tonic::transport::Channel;

use crate::{
    error::Result,
    transport::{RpcTransport, new_rpc_transport},
};
use xlineapi::{
    MemberAddResponse, MemberListResponse, MemberPromoteResponse, MemberRemoveResponse,
    MemberUpdateResponse,
};

/// Client for Cluster operations.
#[derive(Clone, Debug)]
pub struct ClusterClient {
    /// Inner cluster RPC client.
    inner: xlineapi::ClusterClient<RpcTransport>,
    /// Optional QUIC transport for direct RPCs (member add/remove/promote/update/list).
    #[cfg(feature = "quic")]
    quic: Option<Arc<crate::transport::QuicXlineTransport>>,
}

impl ClusterClient {
    /// Create a new `ClusterClient`.
    #[inline]
    #[must_use]
    pub fn new(channel: Channel, token: Option<String>) -> Self {
        Self {
            inner: xlineapi::ClusterClient::new(new_rpc_transport(channel, token.as_deref())),
            #[cfg(feature = "quic")]
            quic: None,
        }
    }

    /// Attach a `QuicXlineTransport` so that direct RPCs use QUIC.
    ///
    /// Covers `member_add()`, `member_remove()`, `member_promote()`,
    /// `member_update()`, and `member_list()`.
    #[cfg(feature = "quic")]
    #[inline]
    #[must_use]
    pub(crate) fn with_quic(mut self, quic: Arc<crate::transport::QuicXlineTransport>) -> Self {
        self.quic = Some(quic);
        self
    }

    /// Add a new member to the cluster.
    ///
    /// # Errors
    ///
    /// Returns an error if the request could not be sent or if the response is invalid.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xline_client::{Client, ClientOptions};
    /// use anyhow::Result;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let curp_members = ["10.0.0.1:2379", "10.0.0.2:2379", "10.0.0.3:2379"];
    ///
    ///     let mut client = Client::connect(curp_members, ClientOptions::default())
    ///         .await?
    ///         .cluster_client();
    ///
    ///     let resp = client.member_add(["127.0.0.1:2380"], true).await?;
    ///
    ///     println!(
    ///         "members: {:?}, added: {:?}",
    ///         resp.members, resp.member
    ///     );
    ///
    ///     Ok(())
    /// }
    /// ```
    #[inline]
    pub async fn member_add<I: Into<String>, P: Into<Vec<I>>>(
        &mut self,
        peer_urls: P,
        is_learner: bool,
    ) -> Result<MemberAddResponse> {
        let req = xlineapi::MemberAddRequest {
            peer_ur_ls: peer_urls.into().into_iter().map(Into::into).collect(),
            is_learner,
        };
        #[cfg(feature = "quic")]
        if let Some(ref quic) = self.quic {
            return quic.member_add(req).await;
        }
        Ok(self.inner.member_add(req).await?.into_inner())
    }

    /// Remove an existing member from the cluster.
    ///
    /// # Errors
    ///
    /// Returns an error if the request could not be sent or if the response is invalid.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xline_client::{Client, ClientOptions};
    /// use anyhow::Result;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let curp_members = ["10.0.0.1:2379", "10.0.0.2:2379", "10.0.0.3:2379"];
    ///
    ///     let mut client = Client::connect(curp_members, ClientOptions::default())
    ///         .await?
    ///         .cluster_client();
    ///     let resp = client.member_remove(1).await?;
    ///
    ///     println!("members: {:?}", resp.members);
    ///
    ///     Ok(())
    ///  }
    ///
    #[inline]
    pub async fn member_remove(&mut self, id: u64) -> Result<MemberRemoveResponse> {
        #[cfg(feature = "quic")]
        if let Some(ref quic) = self.quic {
            return quic.member_remove(id).await;
        }
        Ok(self
            .inner
            .member_remove(xlineapi::MemberRemoveRequest { id })
            .await?
            .into_inner())
    }

    /// Promote an existing member to be the leader of the cluster.
    ///
    /// # Errors
    ///
    /// Returns an error if the request could not be sent or if the response is invalid.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xline_client::{Client, ClientOptions};
    /// use anyhow::Result;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let curp_members = ["10.0.0.1:2379", "10.0.0.2:2379", "10.0.0.3:2379"];
    ///
    ///     let mut client = Client::connect(curp_members, ClientOptions::default())
    ///         .await?
    ///         .cluster_client();
    ///     let resp = client.member_promote(1).await?;
    ///
    ///     println!("members: {:?}", resp.members);
    ///
    ///     Ok(())
    /// }
    ///
    #[inline]
    pub async fn member_promote(&mut self, id: u64) -> Result<MemberPromoteResponse> {
        #[cfg(feature = "quic")]
        if let Some(ref quic) = self.quic {
            return quic.member_promote(id).await;
        }
        Ok(self
            .inner
            .member_promote(xlineapi::MemberPromoteRequest { id })
            .await?
            .into_inner())
    }

    /// Update an existing member in the cluster.
    ///
    /// # Errors
    ///
    /// Returns an error if the request could not be sent or if the response is invalid.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xline_client::{Client, ClientOptions};
    /// use anyhow::Result;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let curp_members = ["10.0.0.1:2379", "10.0.0.2:2379", "10.0.0.3:2379"];
    ///
    ///     let mut client = Client::connect(curp_members, ClientOptions::default())
    ///         .await?
    ///         .cluster_client();
    ///     let resp = client.member_update(1, ["127.0.0.1:2379"]).await?;
    ///
    ///     println!("members: {:?}", resp.members);
    ///
    ///     Ok(())
    ///  }
    ///
    #[inline]
    pub async fn member_update<I: Into<String>, P: Into<Vec<I>>>(
        &mut self,
        id: u64,
        peer_urls: P,
    ) -> Result<MemberUpdateResponse> {
        let req = xlineapi::MemberUpdateRequest {
            id,
            peer_ur_ls: peer_urls.into().into_iter().map(Into::into).collect(),
        };
        #[cfg(feature = "quic")]
        if let Some(ref quic) = self.quic {
            return quic.member_update(req).await;
        }
        Ok(self.inner.member_update(req).await?.into_inner())
    }

    /// List all members in the cluster.
    ///
    /// # Errors
    ///
    /// Returns an error if the request could not be sent or if the response is invalid.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xline_client::{Client, ClientOptions};
    /// use anyhow::Result;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let curp_members = ["10.0.0.1:2379", "10.0.0.2:2379", "10.0.0.3:2379"];
    ///
    ///     let mut client = Client::connect(curp_members, ClientOptions::default())
    ///         .await?
    ///         .cluster_client();
    ///     let resp = client.member_list(false).await?;
    ///
    ///     println!("members: {:?}", resp.members);
    ///
    ///     Ok(())
    /// }
    #[inline]
    pub async fn member_list(&mut self, linearizable: bool) -> Result<MemberListResponse> {
        #[cfg(feature = "quic")]
        if let Some(ref quic) = self.quic {
            return quic.member_list(linearizable).await;
        }
        Ok(self
            .inner
            .member_list(xlineapi::MemberListRequest { linearizable })
            .await?
            .into_inner())
    }
}
