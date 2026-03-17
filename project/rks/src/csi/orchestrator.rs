//! Volume lifecycle orchestration for pod create/delete flows.

use crate::csi::controller::RksCsiController;
use crate::node::NodeRegistry;
use common::RksMessage;
use dashmap::DashMap;
use libcsi::{
    CreateVolumeRequest, CsiController, CsiError, CsiMessage, NodePublishVolumeRequest,
    NodeStageVolumeRequest, Volume, VolumeCapability, VolumeId,
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
    pending: Arc<DashMap<Uuid, oneshot::Sender<CsiMessage>>>,
}

impl VolumeOrchestrator {
    pub fn new(
        controller: Arc<RksCsiController>,
        node_registry: Arc<NodeRegistry>,
        pending: Arc<DashMap<Uuid, oneshot::Sender<CsiMessage>>>,
    ) -> Self {
        Self {
            controller,
            node_registry,
            pending,
        }
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
        // 1. Create volume (metadata in xline)
        let volume = self.controller.create_volume(req).await?;
        let vol_id = &volume.volume_id;

        let staging_target_path = format!("/var/lib/rkl/volumes/{}/globalmount", vol_id);

        // 2. Send StageVolume to target node and wait for response
        let stage_req = NodeStageVolumeRequest {
            volume_id: vol_id.clone(),
            staging_target_path: staging_target_path.clone(),
            volume_capability: VolumeCapability::default(),
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
            volume_capability: VolumeCapability::default(),
            read_only: false,
        };
        if let Err(e) = self
            .send_csi_request_and_wait(node_id, CsiMessage::PublishVolume(publish_req))
            .await
        {
            // Rollback: unstage then delete volume metadata
            let _ = self
                .send_csi_request_and_wait(
                    node_id,
                    CsiMessage::UnstageVolume {
                        volume_id: vol_id.clone(),
                        staging_target_path,
                    },
                )
                .await;
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
    pub async fn unmount_and_deprovision(
        &self,
        volume_id: &VolumeId,
        node_id: &str,
        target_path: &str,
    ) -> Result<(), CsiError> {
        let staging_target_path = format!("/var/lib/rkl/volumes/{}/globalmount", volume_id);

        // 1. Unpublish
        self.send_csi_request_and_wait(
            node_id,
            CsiMessage::UnpublishVolume {
                volume_id: volume_id.clone(),
                target_path: target_path.to_owned(),
            },
        )
        .await?;

        info!(
            target: "rks::csi::orchestrator",
            "unpublished volume {} from node {}",
            volume_id, node_id
        );

        // 2. Unstage
        self.send_csi_request_and_wait(
            node_id,
            CsiMessage::UnstageVolume {
                volume_id: volume_id.clone(),
                staging_target_path,
            },
        )
        .await?;

        info!(
            target: "rks::csi::orchestrator",
            "unstaged volume {} from node {}",
            volume_id, node_id
        );

        // 3. Delete volume metadata
        self.controller.delete_volume(volume_id).await?;

        info!(
            target: "rks::csi::orchestrator",
            "deleted volume {}",
            volume_id
        );

        Ok(())
    }

    /// Send a CSI request to a specific node and wait for the response.
    ///
    /// Registers a oneshot channel keyed by a unique request id, sends the
    /// request through the worker session, then awaits the response with a
    /// timeout.  The response is decoded: `CsiMessage::Ok` maps to `Ok(())`,
    /// `CsiMessage::Error(e)` maps to `Err(e)`, anything else is an internal
    /// error.
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
