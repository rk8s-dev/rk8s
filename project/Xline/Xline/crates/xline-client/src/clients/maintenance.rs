use std::fmt::Debug;

use xlineapi::{
    AlarmAction, AlarmRequest, AlarmResponse, AlarmType, SnapshotRequest, SnapshotResponse,
    StatusRequest, StatusResponse,
};

use crate::{
    error::Result,
    transport::{Channel, MethodId, Streaming},
};

/// Client for Maintenance operations.
#[derive(Clone, Debug)]
pub struct MaintenanceClient {
    /// The maintenance transport
    inner: Channel,
}

impl MaintenanceClient {
    /// Creates a new maintenance client
    #[inline]
    #[must_use]
    pub(crate) fn new(channel: Channel) -> Self {
        Self { inner: channel }
    }

    /// Gets a snapshot over a stream
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
    pub async fn snapshot(&mut self) -> Result<Streaming<SnapshotResponse>> {
        self.inner
            .server_streaming(MethodId::XlineSnapshot, SnapshotRequest {})
            .await
            .map_err(Into::into)
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
        self.inner
            .unary(
                MethodId::XlineAlarm,
                AlarmRequest {
                    action: action.into(),
                    member_id,
                    alarm: alarm_type.into(),
                },
            )
            .await
            .map_err(Into::into)
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
        self.inner
            .unary(MethodId::XlineMaintStatus, StatusRequest::default())
            .await
            .map_err(Into::into)
    }
}
