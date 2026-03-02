use std::fmt::Debug;
#[cfg(feature = "quic")]
use std::sync::Arc;

use tonic::transport::Channel;
use xlineapi::{
    AlarmAction, AlarmRequest, AlarmResponse, AlarmType, SnapshotRequest, StatusRequest,
    StatusResponse,
};

use crate::{
    error::Result,
    transport::{RpcTransport, new_rpc_transport},
    types::stream::SnapshotStream,
};

/// Client for Maintenance operations.
#[derive(Clone, Debug)]
pub struct MaintenanceClient {
    /// The maintenance RPC client, only communicate with one server at a time.
    inner: xlineapi::MaintenanceClient<RpcTransport>,
    /// Optional QUIC transport for direct RPCs (snapshot, alarm, status).
    #[cfg(feature = "quic")]
    quic: Option<Arc<crate::transport::QuicXlineTransport>>,
}

impl MaintenanceClient {
    /// Creates a new `MaintenanceClient`.
    #[inline]
    #[must_use]
    pub fn new(channel: Channel, token: Option<String>) -> Self {
        Self {
            inner: xlineapi::MaintenanceClient::new(new_rpc_transport(channel, token.as_deref())),
            #[cfg(feature = "quic")]
            quic: None,
        }
    }

    /// Attach a `QuicXlineTransport` so that direct RPCs use QUIC.
    ///
    /// Covers `snapshot()`, `alarm()`, and `status()`.
    #[cfg(feature = "quic")]
    #[inline]
    #[must_use]
    pub(crate) fn with_quic(mut self, quic: Arc<crate::transport::QuicXlineTransport>) -> Self {
        self.quic = Some(quic);
        self
    }

    /// Gets a snapshot over a stream.
    ///
    /// # Errors
    ///
    /// This function will return an error if the inner RPC client encountered a propose failure
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xline_client::{Client, ClientOptions};
    /// use anyhow::Result;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     // the name and address of all curp members
    ///     let curp_members = ["10.0.0.1:2379", "10.0.0.2:2379", "10.0.0.3:2379"];
    ///
    ///     let mut client = Client::connect(curp_members, ClientOptions::default())
    ///         .await?
    ///         .maintenance_client();
    ///
    ///     // snapshot
    ///     let mut msg = client.snapshot().await?;
    ///     let mut snapshot = vec![];
    ///     loop {
    ///         if let Some(resp) = msg.message().await? {
    ///             snapshot.extend_from_slice(&resp.blob);
    ///             if resp.remaining_bytes == 0 {
    ///                 break;
    ///             }
    ///         }
    ///     }
    ///     println!("snapshot size: {}", snapshot.len());
    ///
    ///     Ok(())
    /// }
    /// ```
    #[inline]
    pub async fn snapshot(&mut self) -> Result<SnapshotStream> {
        #[cfg(feature = "quic")]
        if let Some(ref quic) = self.quic {
            return quic.snapshot().await;
        }
        Ok(SnapshotStream::from(
            self.inner.snapshot(SnapshotRequest {}).await?.into_inner(),
        ))
    }

    /// Sends a alarm request
    ///
    /// # Errors
    ///
    /// This function will return an error if the inner RPC client encountered a propose failure
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xline_client::{Client, ClientOptions};
    /// use xlineapi::{AlarmAction, AlarmRequest, AlarmType};
    /// use anyhow::Result;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     // the name and address of all curp members
    ///     let curp_members = ["10.0.0.1:2379", "10.0.0.2:2379", "10.0.0.3:2379"];
    ///
    ///     let mut client = Client::connect(curp_members, ClientOptions::default())
    ///         .await?
    ///         .maintenance_client();
    ///
    ///     client.alarm(AlarmAction::Get, 0, AlarmType::None).await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    #[inline]
    pub async fn alarm(
        &mut self,
        action: AlarmAction,
        member_id: u64,
        alarm_type: AlarmType,
    ) -> Result<AlarmResponse> {
        let req = AlarmRequest {
            action: action.into(),
            member_id,
            alarm: alarm_type.into(),
        };
        #[cfg(feature = "quic")]
        if let Some(ref quic) = self.quic {
            return quic.alarm(req).await;
        }
        Ok(self.inner.alarm(req).await?.into_inner())
    }

    /// Sends a status request
    ///
    /// # Errors
    ///
    /// This function will return an error if the inner RPC client encountered a propose failure
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xline_client::{Client, ClientOptions};
    /// use anyhow::Result;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     // the name and address of all curp members
    ///     let curp_members = ["10.0.0.1:2379", "10.0.0.2:2379", "10.0.0.3:2379"];
    ///
    ///     let mut client = Client::connect(curp_members, ClientOptions::default())
    ///         .await?
    ///         .maintenance_client();
    ///
    ///     client.status().await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    #[inline]
    pub async fn status(&mut self) -> Result<StatusResponse> {
        #[cfg(feature = "quic")]
        if let Some(ref quic) = self.quic {
            return quic.maint_status().await;
        }
        Ok(self
            .inner
            .status(StatusRequest::default())
            .await?
            .into_inner())
    }
}
