use crate::{
    error::Result,
    transport::{Channel, MethodId},
};
use xlineapi::{
    MemberAddResponse, MemberListResponse, MemberPromoteResponse, MemberRemoveResponse,
    MemberUpdateResponse,
};

/// Client for Cluster operations.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ClusterClient {
    /// Inner transport
    inner: Channel,
}

impl ClusterClient {
    /// Create a new cluster client
    #[inline]
    #[must_use]
    pub(crate) fn new(channel: Channel) -> Self {
        Self { inner: channel }
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
        self.inner
            .unary(
                MethodId::XlineMemberAdd,
                xlineapi::MemberAddRequest {
                    peer_ur_ls: peer_urls.into().into_iter().map(Into::into).collect(),
                    is_learner,
                },
            )
            .await
            .map_err(Into::into)
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
        self.inner
            .unary(
                MethodId::XlineMemberRemove,
                xlineapi::MemberRemoveRequest { id },
            )
            .await
            .map_err(Into::into)
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
        self.inner
            .unary(
                MethodId::XlineMemberPromote,
                xlineapi::MemberPromoteRequest { id },
            )
            .await
            .map_err(Into::into)
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
        self.inner
            .unary(
                MethodId::XlineMemberUpdate,
                xlineapi::MemberUpdateRequest {
                    id,
                    peer_ur_ls: peer_urls.into().into_iter().map(Into::into).collect(),
                },
            )
            .await
            .map_err(Into::into)
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
        self.inner
            .unary(
                MethodId::XlineMemberList,
                xlineapi::MemberListRequest { linearizable },
            )
            .await
            .map_err(Into::into)
    }
}
