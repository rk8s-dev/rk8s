//! CSI protocol messages transmitted over QUIC.
//!
//! [`CsiMessage`] is the top-level envelope for all request and response
//! variants exchanged between the CSI client (RKS side) and the CSI server
//! (Node side) via QUIC bi-directional streams.

use serde::{Deserialize, Serialize};

use crate::error::CsiError;
use crate::types::*;

/// Top-level message envelope for CSI over QUIC.
///
/// Each QUIC bi-stream carries exactly one request followed by one response.
/// The client sends a *request* variant and the server replies with the
/// corresponding *response* variant (or [`CsiMessage::Error`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CsiMessage {
    // ----- Requests --------------------------------------------------------
    /// Create a new volume (Controller).
    CreateVolume(CreateVolumeRequest),
    /// Delete a volume (Controller).
    DeleteVolume(VolumeId),
    /// List all known volumes (Controller).
    ListVolumes,
    /// Query remaining capacity (Controller).
    GetCapacity,
    /// Validate volume capabilities (Controller).
    ValidateVolumeCapabilities {
        volume_id: VolumeId,
        capabilities: Vec<VolumeCapability>,
    },

    /// Stage (FUSE-mount) a volume at a global path (Node).
    StageVolume(NodeStageVolumeRequest),
    /// Unstage a previously staged volume (Node).
    UnstageVolume {
        volume_id: VolumeId,
        staging_target_path: String,
    },
    /// Publish (bind-mount) a staged volume into a Pod (Node).
    PublishVolume(NodePublishVolumeRequest),
    /// Unpublish a previously published volume (Node).
    UnpublishVolume {
        volume_id: VolumeId,
        target_path: String,
    },

    /// Health probe (Identity).
    Probe,
    /// Query plugin info (Identity).
    GetPluginInfo,
    /// Query plugin capabilities (Identity).
    GetPluginCapabilities,
    /// Query node info (Node).
    GetNodeInfo,

    // ----- Responses -------------------------------------------------------
    /// A volume was successfully created.
    VolumeCreated(Volume),
    /// A list of volumes.
    VolumeList(Vec<Volume>),
    /// Available capacity in bytes.
    Capacity(u64),
    /// Whether the requested capabilities are valid.
    CapabilitiesValid(bool),
    /// Plugin information.
    PluginInfoResponse(PluginInfo),
    /// Plugin capabilities.
    PluginCapabilitiesResponse(Vec<PluginCapability>),
    /// Node information.
    NodeInfoResponse(NodeInfo),

    /// Generic success acknowledgement (no payload).
    Ok,
    /// Probe result.
    ProbeResult(bool),
    /// An error occurred.
    Error(CsiError),
}

impl std::fmt::Display for CsiMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateVolume(req) => write!(f, "CreateVolume(name={})", req.name),
            Self::DeleteVolume(id) => write!(f, "DeleteVolume({})", id),
            Self::ListVolumes => f.write_str("ListVolumes"),
            Self::GetCapacity => f.write_str("GetCapacity"),
            Self::ValidateVolumeCapabilities { volume_id, .. } => {
                write!(f, "ValidateVolumeCapabilities({})", volume_id)
            }
            Self::StageVolume(req) => write!(f, "StageVolume({})", req.volume_id),
            Self::UnstageVolume { volume_id, .. } => write!(f, "UnstageVolume({})", volume_id),
            Self::PublishVolume(req) => write!(f, "PublishVolume({})", req.volume_id),
            Self::UnpublishVolume { volume_id, .. } => {
                write!(f, "UnpublishVolume({})", volume_id)
            }
            Self::Probe => f.write_str("Probe"),
            Self::GetPluginInfo => f.write_str("GetPluginInfo"),
            Self::GetPluginCapabilities => f.write_str("GetPluginCapabilities"),
            Self::GetNodeInfo => f.write_str("GetNodeInfo"),
            Self::VolumeCreated(v) => write!(f, "VolumeCreated({})", v.volume_id),
            Self::VolumeList(vs) => write!(f, "VolumeList(count={})", vs.len()),
            Self::Capacity(c) => write!(f, "Capacity({})", c),
            Self::CapabilitiesValid(v) => write!(f, "CapabilitiesValid({})", v),
            Self::PluginInfoResponse(info) => {
                write!(f, "PluginInfo(name={})", info.name)
            }
            Self::PluginCapabilitiesResponse(caps) => {
                write!(f, "PluginCapabilities(count={})", caps.len())
            }
            Self::NodeInfoResponse(info) => write!(f, "NodeInfo({})", info.node_id),
            Self::Ok => f.write_str("Ok"),
            Self::ProbeResult(ok) => write!(f, "ProbeResult({})", ok),
            Self::Error(e) => write!(f, "Error({})", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_serde_roundtrip() {
        let msg = CsiMessage::CreateVolume(CreateVolumeRequest {
            name: "test".into(),
            capacity_bytes: 1024,
            volume_capabilities: vec![VolumeCapability::default()],
            parameters: Default::default(),
        });
        let json = serde_json::to_string(&msg).expect("serialize");
        let de: CsiMessage = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(de, CsiMessage::CreateVolume(_)));
    }

    #[test]
    fn error_message_roundtrip() {
        let msg = CsiMessage::Error(CsiError::VolumeNotFound("vol-1".into()));
        let json = serde_json::to_string(&msg).expect("serialize");
        let de: CsiMessage = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(de, CsiMessage::Error(CsiError::VolumeNotFound(_))));
    }

    #[test]
    fn display_formatting() {
        let msg = CsiMessage::Ok;
        assert_eq!(msg.to_string(), "Ok");

        let msg = CsiMessage::Probe;
        assert_eq!(msg.to_string(), "Probe");
    }
}
