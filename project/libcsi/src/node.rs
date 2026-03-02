//! CSI Node service trait.
//!
//! The Node service runs on each worker node and handles the local filesystem
//! operations required to make a volume available to Pod containers:
//!
//! 1. **Stage** — FUSE-mount the SlayerFS volume at a global path.
//! 2. **Publish** — bind-mount the global path into the Pod's container.
//! 3. **Unpublish** — remove the bind-mount.
//! 4. **Unstage** — unmount the FUSE mount.

use async_trait::async_trait;

use crate::error::CsiError;
use crate::types::{NodeInfo, NodePublishVolumeRequest, NodeStageVolumeRequest, VolumeId};

/// Node service — local mount / unmount operations.
#[async_trait]
pub trait CsiNode: Send + Sync {
    /// Stage a volume: FUSE-mount SlayerFS at the global staging path.
    ///
    /// This is idempotent — calling it again for an already-staged volume
    /// should succeed without error.
    async fn stage_volume(&self, req: NodeStageVolumeRequest) -> Result<(), CsiError>;

    /// Unstage a volume: unmount the FUSE filesystem from the staging path.
    ///
    /// This is idempotent — calling it on an already-unstaged volume should
    /// succeed without error.
    async fn unstage_volume(
        &self,
        volume_id: &VolumeId,
        staging_target_path: &str,
    ) -> Result<(), CsiError>;

    /// Publish a volume: bind-mount the staged global path into the container.
    ///
    /// This is idempotent — calling it again for the same `target_path` should
    /// succeed without error.
    async fn publish_volume(&self, req: NodePublishVolumeRequest) -> Result<(), CsiError>;

    /// Unpublish a volume: unmount the bind-mount from the container path.
    ///
    /// This is idempotent.
    async fn unpublish_volume(
        &self,
        volume_id: &VolumeId,
        target_path: &str,
    ) -> Result<(), CsiError>;

    /// Return information about the node on which this service is running.
    async fn get_info(&self) -> Result<NodeInfo, CsiError>;
}
