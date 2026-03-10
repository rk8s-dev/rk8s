//! CSI Controller service trait.
//!
//! The Controller service manages the centralized volume lifecycle: creation,
//! deletion, capability validation, listing, and capacity queries.  It is
//! typically invoked by the RKS control plane during scheduling decisions.

use async_trait::async_trait;

use crate::error::CsiError;
use crate::types::{CreateVolumeRequest, Volume, VolumeCapability, VolumeId};

/// Controller service — centralized volume management.
///
/// Operations in this trait run on the control plane (RKS) and coordinate
/// with the storage backend to provision / deprovision volumes.
#[async_trait]
pub trait CsiController: Send + Sync {
    /// Provision a new volume.
    ///
    /// The returned [`Volume`] contains the assigned `volume_id` and
    /// `volume_context` that must be forwarded to subsequent Node operations.
    async fn create_volume(&self, req: CreateVolumeRequest) -> Result<Volume, CsiError>;

    /// Delete a previously provisioned volume.
    async fn delete_volume(&self, volume_id: &VolumeId) -> Result<(), CsiError>;

    /// Check whether the given capabilities are compatible with the volume.
    async fn validate_volume_capabilities(
        &self,
        volume_id: &VolumeId,
        capabilities: &[VolumeCapability],
    ) -> Result<bool, CsiError>;

    /// List all volumes known to this controller.
    async fn list_volumes(&self) -> Result<Vec<Volume>, CsiError>;

    /// Return the total available capacity in bytes.
    async fn get_capacity(&self) -> Result<u64, CsiError>;
}
