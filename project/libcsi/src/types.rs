//! Core CSI types: volumes, capabilities, requests, and topology.
//!
//! These types form the data model shared by the CSI traits, transport layer,
//! and backend implementations.  They are all [`Serialize`]/[`Deserialize`] so
//! they can be transmitted over QUIC as JSON.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// Volume identity
// ---------------------------------------------------------------------------

/// Opaque, unique identifier for a volume.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct VolumeId(pub String);

impl fmt::Display for VolumeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for VolumeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for VolumeId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

// ---------------------------------------------------------------------------
// Access mode & capabilities
// ---------------------------------------------------------------------------

/// Describes how a volume may be accessed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccessMode {
    /// Single-node read-write.
    ReadWriteOnce,
    /// Multi-node read-only.
    ReadOnlyMany,
    /// Multi-node read-write.
    ReadWriteMany,
}

/// Describes the capabilities required from a volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeCapability {
    /// Requested access mode.
    pub access_mode: AccessMode,
    /// Additional mount flags (e.g. `"noatime"`).
    #[serde(default)]
    pub mount_flags: Vec<String>,
    /// Filesystem type – typically `"slayerfs"` for SlayerFS-backed volumes.
    #[serde(default = "default_fs_type")]
    pub fs_type: String,
}

fn default_fs_type() -> String {
    "slayerfs".to_owned()
}

impl Default for VolumeCapability {
    fn default() -> Self {
        Self {
            access_mode: AccessMode::ReadWriteOnce,
            mount_flags: Vec::new(),
            fs_type: default_fs_type(),
        }
    }
}

// ---------------------------------------------------------------------------
// Volume metadata
// ---------------------------------------------------------------------------

/// Full metadata for a provisioned volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    /// Unique volume identifier.
    pub volume_id: VolumeId,
    /// Provisioned capacity in bytes.
    pub capacity_bytes: u64,
    /// User-supplied parameters from the storage class / request.
    #[serde(default)]
    pub parameters: HashMap<String, String>,
    /// Opaque context passed from Controller to Node operations.
    #[serde(default)]
    pub volume_context: HashMap<String, String>,
    /// Topology constraints (e.g. node affinity).
    #[serde(default)]
    pub accessible_topology: Vec<Topology>,
}

/// Topology constraint expressed as key-value segments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    /// Topology segments, e.g. `{"node": "node-01"}`.
    #[serde(default)]
    pub segments: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Volume lifecycle state
// ---------------------------------------------------------------------------

/// Tracks the lifecycle state of a volume on a particular node.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VolumeState {
    /// Volume has been created but not yet staged.
    Created,
    /// Volume is FUSE-mounted at the global staging path.
    Staged,
    /// Volume is bind-mounted into one or more Pod containers.
    Published,
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/// Request to create a new volume.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateVolumeRequest {
    /// Human-readable name for the volume.
    pub name: String,
    /// Desired capacity in bytes.
    pub capacity_bytes: u64,
    /// Required capabilities.
    #[serde(default)]
    pub volume_capabilities: Vec<VolumeCapability>,
    /// Arbitrary parameters forwarded to the backend.
    #[serde(default)]
    pub parameters: HashMap<String, String>,
}

/// Request to stage (globally mount) a volume on a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStageVolumeRequest {
    /// Volume to stage.
    pub volume_id: VolumeId,
    /// Global staging mount point,
    /// e.g. `/var/lib/rkl/volumes/<vol-id>/globalmount`.
    pub staging_target_path: String,
    /// Requested capability.
    pub volume_capability: VolumeCapability,
    /// Opaque context carried from `CreateVolume`.
    #[serde(default)]
    pub volume_context: HashMap<String, String>,
}

/// Request to publish (bind-mount) a staged volume into a Pod container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePublishVolumeRequest {
    /// Volume to publish.
    pub volume_id: VolumeId,
    /// The global staging mount point (source of the bind mount).
    pub staging_target_path: String,
    /// Target path inside the container,
    /// e.g. `/var/lib/rkl/pods/<pod-uid>/volumes/<vol-name>`.
    pub target_path: String,
    /// Requested capability.
    pub volume_capability: VolumeCapability,
    /// Whether the bind mount should be read-only.
    #[serde(default)]
    pub read_only: bool,
}

// ---------------------------------------------------------------------------
// Plugin & node info
// ---------------------------------------------------------------------------

/// Information about the CSI plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Plugin name, e.g. `"rk8s.slayerfs.csi"`.
    pub name: String,
    /// Vendor-provided version string.
    pub vendor_version: String,
}

/// Capabilities advertised by the CSI plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginCapability {
    /// Plugin provides a Controller service.
    ControllerService,
    /// Plugin supports volume topology constraints.
    VolumeAccessibilityConstraints,
}

/// Information about the node on which the CSI Node service runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique node identifier.
    pub node_id: String,
    /// Maximum number of volumes the node can host.
    pub max_volumes: u64,
    /// Optional topology of this node.
    #[serde(default)]
    pub accessible_topology: Option<Topology>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_id_display() {
        let id = VolumeId("vol-abc".into());
        assert_eq!(id.to_string(), "vol-abc");
    }

    #[test]
    fn volume_serde_roundtrip() {
        let vol = Volume {
            volume_id: VolumeId("v1".into()),
            capacity_bytes: 1024 * 1024,
            parameters: HashMap::from([("key".into(), "val".into())]),
            volume_context: HashMap::new(),
            accessible_topology: vec![Topology {
                segments: HashMap::from([("node".into(), "node-01".into())]),
            }],
        };
        let json = serde_json::to_string(&vol).expect("serialize");
        let de: Volume = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(de.volume_id, vol.volume_id);
        assert_eq!(de.capacity_bytes, vol.capacity_bytes);
    }

    #[test]
    fn create_volume_request_default() {
        let req = CreateVolumeRequest::default();
        assert!(req.name.is_empty());
        assert_eq!(req.capacity_bytes, 0);
    }

    #[test]
    fn volume_capability_default() {
        let cap = VolumeCapability::default();
        assert_eq!(cap.access_mode, AccessMode::ReadWriteOnce);
        assert_eq!(cap.fs_type, "slayerfs");
    }
}
