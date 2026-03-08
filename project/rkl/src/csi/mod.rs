//! CSI Node implementation for RKL worker nodes.
//!
//! This module provides:
//! - **[`CsiNodeService`]** — the [`CsiNode`](libcsi::CsiNode) trait impl
//! - **[`RklCsiIdentity`]** — the [`CsiIdentity`](libcsi::CsiIdentity) trait impl
//! - **[`SlayerFsOperator`]** — manages FUSE mount / bind-mount lifecycle
//! - **[`VolumeStateStore`]** — persists volume state for crash recovery
//! - **[`handle_csi_request`]** — top-level dispatcher from [`CsiMessage`](libcsi::CsiMessage)

pub mod identity;
pub mod node_service;
pub mod operator;
pub mod state;

pub use identity::RklCsiIdentity;
pub use node_service::CsiNodeService;
pub use operator::SlayerFsOperator;
#[allow(unused_imports)]
pub use state::VolumeStateStore;

use libcsi::{CsiIdentity, CsiMessage, CsiNode};
use tracing::warn;

/// Dispatch an incoming [`CsiMessage`] to the appropriate service method
/// and return the response message.
pub async fn handle_csi_request(
    node_service: &CsiNodeService,
    identity: &RklCsiIdentity,
    msg: CsiMessage,
) -> CsiMessage {
    match msg {
        // ----- Node operations ------
        CsiMessage::StageVolume(req) => match node_service.stage_volume(req).await {
            Ok(()) => CsiMessage::Ok,
            Err(e) => CsiMessage::Error(e),
        },
        CsiMessage::UnstageVolume {
            volume_id,
            staging_target_path,
        } => match node_service
            .unstage_volume(&volume_id, &staging_target_path)
            .await
        {
            Ok(()) => CsiMessage::Ok,
            Err(e) => CsiMessage::Error(e),
        },
        CsiMessage::PublishVolume(req) => match node_service.publish_volume(req).await {
            Ok(()) => CsiMessage::Ok,
            Err(e) => CsiMessage::Error(e),
        },
        CsiMessage::UnpublishVolume {
            volume_id,
            target_path,
        } => match node_service
            .unpublish_volume(&volume_id, &target_path)
            .await
        {
            Ok(()) => CsiMessage::Ok,
            Err(e) => CsiMessage::Error(e),
        },
        CsiMessage::GetNodeInfo => match node_service.get_info().await {
            Ok(info) => CsiMessage::NodeInfoResponse(info),
            Err(e) => CsiMessage::Error(e),
        },

        // ----- Identity operations ------
        CsiMessage::Probe => match identity.probe().await {
            Ok(ready) => CsiMessage::ProbeResult(ready),
            Err(e) => CsiMessage::Error(e),
        },
        CsiMessage::GetPluginInfo => match identity.get_plugin_info().await {
            Ok(info) => CsiMessage::PluginInfoResponse(info),
            Err(e) => CsiMessage::Error(e),
        },
        CsiMessage::GetPluginCapabilities => match identity.get_plugin_capabilities().await {
            Ok(caps) => CsiMessage::PluginCapabilitiesResponse(caps),
            Err(e) => CsiMessage::Error(e),
        },

        // ----- Unsupported on Node ------
        other => {
            warn!("[csi] unexpected CSI message on node side: {other}");
            CsiMessage::Error(libcsi::CsiError::InvalidArgument(format!(
                "unsupported operation on node: {other}"
            )))
        }
    }
}
