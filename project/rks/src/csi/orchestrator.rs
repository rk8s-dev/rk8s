//! Volume lifecycle orchestration for pod create/delete flows.

use crate::csi::controller::RksCsiController;
use crate::node::NodeRegistry;
use common::RksMessage;
use libcsi::{
    CsiController, CsiError, CsiMessage, CreateVolumeRequest, NodePublishVolumeRequest,
    NodeStageVolumeRequest, Volume, VolumeCapability, VolumeId,
};
use log::info;
use std::sync::Arc;

/// Orchestrates volume lifecycle in coordination with RKL nodes.
///
/// Called during pod scheduling: creates volumes, sends stage/publish
/// requests to the target node. Called during pod deletion: sends
/// unpublish/unstage requests, then deletes volume metadata.
pub struct VolumeOrchestrator {
    controller: Arc<RksCsiController>,
    node_registry: Arc<NodeRegistry>,
}

impl VolumeOrchestrator {
    pub fn new(controller: Arc<RksCsiController>, node_registry: Arc<NodeRegistry>) -> Self {
        Self {
            controller,
            node_registry,
        }
    }

    /// Provision and mount a volume on the target node.
    ///
    /// Flow: create_volume -> send StageVolume -> send PublishVolume
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

        // 2. Send StageVolume to target node
        let stage_req = NodeStageVolumeRequest {
            volume_id: vol_id.clone(),
            staging_target_path: staging_target_path.clone(),
            volume_capability: VolumeCapability::default(),
            volume_context: volume.volume_context.clone(),
        };
        self.send_csi_request(node_id, CsiMessage::StageVolume(stage_req))
            .await?;

        info!(
            target: "rks::csi::orchestrator",
            "staged volume {} on node {}",
            vol_id, node_id
        );

        // 3. Send PublishVolume to target node
        let publish_req = NodePublishVolumeRequest {
            volume_id: vol_id.clone(),
            staging_target_path,
            target_path: target_path.to_owned(),
            volume_capability: VolumeCapability::default(),
            read_only: false,
        };
        self.send_csi_request(node_id, CsiMessage::PublishVolume(publish_req))
            .await?;

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
        self.send_csi_request(
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
        self.send_csi_request(
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

    /// Send a CSI request to a specific node via its WorkerSession channel.
    async fn send_csi_request(
        &self,
        node_id: &str,
        request: CsiMessage,
    ) -> Result<(), CsiError> {
        let session = self
            .node_registry
            .get(node_id)
            .await
            .ok_or_else(|| {
                CsiError::Internal(format!("node {} not found in registry", node_id))
            })?;

        session
            .tx
            .send(RksMessage::CsiRequest(request))
            .await
            .map_err(|e| CsiError::transport(format!("failed to send to node {}: {}", node_id, e)))?;

        Ok(())
    }
}
