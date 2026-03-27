//! Volume lifecycle orchestration for pod create/delete flows.

use crate::csi::controller::RksCsiController;
use crate::node::NodeRegistry;
use common::RksMessage;
use dashmap::DashMap;
use libcsi::{
    AccessMode, CreateVolumeRequest, CsiController, CsiError, CsiMessage, NodePublishVolumeRequest,
    NodeStageVolumeRequest, Volume, VolumeId,
};
use log::info;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;

/// Default timeout for waiting on a CSI response from a worker node.
const CSI_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Orchestrates volume lifecycle in coordination with RKL nodes.
///
/// Called during pod scheduling: creates volumes, sends stage/publish
/// requests to the target node. Called during pod deletion: sends
/// unpublish/unstage requests, then deletes volume metadata.
pub struct VolumeOrchestrator {
    controller: Arc<RksCsiController>,
    node_registry: Arc<NodeRegistry>,
    pending: DashMap<Uuid, oneshot::Sender<CsiMessage>>,
}

impl VolumeOrchestrator {
    pub fn new(controller: Arc<RksCsiController>, node_registry: Arc<NodeRegistry>) -> Self {
        Self {
            controller,
            node_registry,
            pending: DashMap::new(),
        }
    }

    /// Route an incoming CSI response from a worker node to the waiting
    /// caller.  Called by the QUIC dispatch layer when it receives a
    /// `RksMessage::CsiResponse`.
    pub fn route_response(&self, id: Uuid, message: CsiMessage) {
        if let Some((_, sender)) = self.pending.remove(&id) {
            log::info!(
                target: "rks::csi::orchestrator",
                "routing CSI response for request {id}"
            );
            let _ = sender.send(message);
        } else {
            log::warn!(
                target: "rks::csi::orchestrator",
                "received CSI response for unknown request {id}"
            );
        }
    }

    /// Build the global staging mount path for a volume.
    fn staging_path(vol_id: &VolumeId) -> String {
        format!("/var/lib/rkl/volumes/{}/globalmount", vol_id)
    }

    /// Provision and mount a volume on the target node.
    ///
    /// Flow: create_volume -> send StageVolume -> send PublishVolume
    ///
    /// If stage or publish fails, the volume metadata is rolled back.
    pub async fn provision_and_mount(
        &self,
        req: CreateVolumeRequest,
        node_id: &str,
        target_path: &str,
    ) -> Result<Volume, CsiError> {
        // Resolve the capability from the request (first entry or default).
        let capability = req.volume_capabilities.first().cloned().unwrap_or_default();
        let read_only = capability.access_mode == AccessMode::ReadOnlyMany;

        // 1. Create volume (metadata in xline)
        let volume = self.controller.create_volume(req).await?;
        let vol_id = &volume.volume_id;

        let staging_target_path = Self::staging_path(vol_id);

        // 2. Send StageVolume to target node and wait for response
        let stage_req = NodeStageVolumeRequest {
            volume_id: vol_id.clone(),
            staging_target_path: staging_target_path.clone(),
            volume_capability: capability.clone(),
            volume_context: volume.volume_context.clone(),
        };
        if let Err(e) = self
            .send_csi_request_and_wait(node_id, CsiMessage::StageVolume(stage_req))
            .await
        {
            // Rollback: delete volume metadata
            if let Err(del_err) = self.controller.delete_volume(vol_id).await {
                log::error!(
                    target: "rks::csi::orchestrator",
                    "rollback: failed to delete volume {} after stage failure: {del_err}",
                    vol_id
                );
            }
            return Err(e);
        }

        info!(
            target: "rks::csi::orchestrator",
            "staged volume {} on node {}",
            vol_id, node_id
        );

        // 3. Send PublishVolume to target node and wait for response
        let publish_req = NodePublishVolumeRequest {
            volume_id: vol_id.clone(),
            staging_target_path: staging_target_path.clone(),
            target_path: target_path.to_owned(),
            volume_capability: capability,
            read_only,
        };
        if let Err(e) = self
            .send_csi_request_and_wait(node_id, CsiMessage::PublishVolume(publish_req))
            .await
        {
            // Rollback: unstage then delete volume metadata
            if let Err(unstage_err) = self
                .send_csi_request_and_wait(
                    node_id,
                    CsiMessage::UnstageVolume {
                        volume_id: vol_id.clone(),
                        staging_target_path,
                    },
                )
                .await
            {
                log::warn!(
                    target: "rks::csi::orchestrator",
                    "rollback: failed to unstage volume {} after publish failure: {unstage_err}",
                    vol_id
                );
            }
            if let Err(del_err) = self.controller.delete_volume(vol_id).await {
                log::error!(
                    target: "rks::csi::orchestrator",
                    "rollback: failed to delete volume {} after publish failure: {del_err}",
                    vol_id
                );
            }
            return Err(e);
        }

        info!(
            target: "rks::csi::orchestrator",
            "published volume {} on node {} at {}",
            vol_id, node_id, target_path
        );

        Ok(volume)
    }

    /// Unmount and deprovision a volume from the target node.
    ///
    /// Flow: send UnpublishVolume -> send UnstageVolume -> delete_volume
    ///
    /// Best-effort: each step is attempted regardless of prior failures.
    /// Returns the first error encountered, if any.
    pub async fn unmount_and_deprovision(
        &self,
        volume_id: &VolumeId,
        node_id: &str,
        target_path: &str,
    ) -> Result<(), CsiError> {
        let staging_target_path = Self::staging_path(volume_id);
        let mut first_error: Option<CsiError> = None;

        // 1. Unpublish (best-effort)
        if let Err(e) = self
            .send_csi_request_and_wait(
                node_id,
                CsiMessage::UnpublishVolume {
                    volume_id: volume_id.clone(),
                    target_path: target_path.to_owned(),
                },
            )
            .await
        {
            log::error!(
                target: "rks::csi::orchestrator",
                "failed to unpublish volume {} from node {}: {e}",
                volume_id, node_id
            );
            first_error.get_or_insert(e);
        } else {
            info!(
                target: "rks::csi::orchestrator",
                "unpublished volume {} from node {}",
                volume_id, node_id
            );
        }

        // 2. Unstage (best-effort)
        if let Err(e) = self
            .send_csi_request_and_wait(
                node_id,
                CsiMessage::UnstageVolume {
                    volume_id: volume_id.clone(),
                    staging_target_path,
                },
            )
            .await
        {
            log::error!(
                target: "rks::csi::orchestrator",
                "failed to unstage volume {} from node {}: {e}",
                volume_id, node_id
            );
            first_error.get_or_insert(e);
        } else {
            info!(
                target: "rks::csi::orchestrator",
                "unstaged volume {} from node {}",
                volume_id, node_id
            );
        }

        // 3. Delete volume metadata (best-effort)
        if let Err(e) = self.controller.delete_volume(volume_id).await {
            log::error!(
                target: "rks::csi::orchestrator",
                "failed to delete volume {}: {e}",
                volume_id
            );
            first_error.get_or_insert(e);
        } else {
            info!(
                target: "rks::csi::orchestrator",
                "deleted volume {}",
                volume_id
            );
        }

        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Send a CSI request to a specific node and wait for the response.
    ///
    /// Registers a oneshot channel keyed by a unique request id, sends the
    /// request through the worker session, then awaits the response with a
    /// timeout.  The response is decoded: `CsiMessage::Ok` maps to `Ok(())`,
    /// `CsiMessage::Error(e)` maps to `Err(e)`, anything else is an internal
    /// error.
    ///
    /// NOTE: The pending map entry is cleaned up either by the response
    /// handler (in `dispatch.rs`) on success, or by this method on timeout /
    /// channel drop.
    async fn send_csi_request_and_wait(
        &self,
        node_id: &str,
        request: CsiMessage,
    ) -> Result<(), CsiError> {
        let request_id = Uuid::new_v4();

        let (tx, rx) = oneshot::channel();
        self.pending.insert(request_id, tx);

        // Look up the worker session and send the request
        let session = self.node_registry.get(node_id).await.ok_or_else(|| {
            self.pending.remove(&request_id);
            CsiError::Internal(format!("node {} not found in registry", node_id))
        })?;

        if let Err(e) = session
            .tx
            .send(RksMessage::CsiRequest {
                id: request_id,
                message: request,
            })
            .await
        {
            self.pending.remove(&request_id);
            return Err(CsiError::transport(format!(
                "failed to send to node {}: {}",
                node_id, e
            )));
        }

        // Wait for the response with timeout
        let response = tokio::time::timeout(CSI_REQUEST_TIMEOUT, rx)
            .await
            .map_err(|_| {
                self.pending.remove(&request_id);
                CsiError::Internal(format!(
                    "CSI request to node {} timed out after {}s",
                    node_id,
                    CSI_REQUEST_TIMEOUT.as_secs()
                ))
            })?
            .map_err(|_| {
                self.pending.remove(&request_id);
                CsiError::Internal(format!("CSI response channel dropped for node {}", node_id))
            })?;

        // Interpret the response
        match response {
            CsiMessage::Ok => Ok(()),
            CsiMessage::Error(e) => Err(e),
            other => Err(CsiError::Internal(format!(
                "unexpected CSI response: {}",
                other
            ))),
        }
    }
}
