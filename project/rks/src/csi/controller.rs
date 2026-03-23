//! CsiController trait implementation for RKS.

use async_trait::async_trait;
use libcsi::{CreateVolumeRequest, CsiController, CsiError, Volume, VolumeCapability, VolumeId};
use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::csi::volume_store::VolumeStore;

/// Generate a unique volume ID with a `vol-` prefix.
pub fn generate_volume_id() -> VolumeId {
    VolumeId(format!("vol-{}", Uuid::new_v4()))
}

/// Build a `Volume` from a `CreateVolumeRequest` and assigned ID.
pub fn build_volume(volume_id: VolumeId, req: &CreateVolumeRequest) -> Volume {
    let mut volume_context = HashMap::new();
    volume_context.insert("name".to_owned(), req.name.clone());
    if let Some(cfg_json) = req.parameters.get("slayerfs_config") {
        volume_context.insert("slayerfs_config".to_owned(), cfg_json.clone());
    }

    Volume {
        volume_id,
        capacity_bytes: req.capacity_bytes,
        parameters: req.parameters.clone(),
        volume_context,
        accessible_topology: vec![],
    }
}

/// RKS-side CSI Controller implementation.
///
/// `create_volume` records metadata in xline;
/// physical volume creation (FUSE mount) is deferred to the Node side during `stage_volume`.
pub struct RksCsiController {
    store: Arc<VolumeStore>,
}

impl RksCsiController {
    pub fn new(store: Arc<VolumeStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl CsiController for RksCsiController {
    async fn create_volume(&self, req: CreateVolumeRequest) -> Result<Volume, CsiError> {
        if req.name.is_empty() {
            return Err(CsiError::InvalidArgument("volume name is empty".into()));
        }

        let volume_id = generate_volume_id();
        let volume = build_volume(volume_id, &req);

        self.store.put(&volume).await.map_err(CsiError::internal)?;

        info!(
            target: "rks::csi::controller",
            "created volume {} (name={})",
            volume.volume_id, req.name
        );

        Ok(volume)
    }

    async fn delete_volume(&self, volume_id: &VolumeId) -> Result<(), CsiError> {
        let existing = self
            .store
            .get(volume_id)
            .await
            .map_err(CsiError::internal)?;

        if existing.is_none() {
            // Idempotent: deleting a non-existent volume is not an error
            return Ok(());
        }

        self.store
            .delete(volume_id)
            .await
            .map_err(CsiError::internal)?;

        info!(
            target: "rks::csi::controller",
            "deleted volume {}",
            volume_id
        );

        Ok(())
    }

    async fn validate_volume_capabilities(
        &self,
        volume_id: &VolumeId,
        _capabilities: &[VolumeCapability],
    ) -> Result<bool, CsiError> {
        let existing = self
            .store
            .get(volume_id)
            .await
            .map_err(CsiError::internal)?;

        if existing.is_none() {
            return Err(CsiError::VolumeNotFound(volume_id.to_string()));
        }

        // SlayerFS supports all access modes
        Ok(true)
    }

    async fn list_volumes(&self) -> Result<Vec<Volume>, CsiError> {
        self.store.list().await.map_err(CsiError::internal)
    }

    async fn get_capacity(&self) -> Result<u64, CsiError> {
        // TODO: aggregate capacity from connected nodes via QUIC queries
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_volume_id_format() {
        let id = generate_volume_id();
        assert!(id.0.starts_with("vol-"));
        // UUID v4 is 36 chars, "vol-" prefix makes 40
        assert_eq!(id.0.len(), 40);
    }

    #[test]
    fn build_volume_from_request() {
        let req = CreateVolumeRequest {
            name: "test-vol".into(),
            capacity_bytes: 1024 * 1024,
            volume_capabilities: vec![VolumeCapability::default()],
            parameters: Default::default(),
        };
        let vol_id = generate_volume_id();
        let vol = build_volume(vol_id.clone(), &req);
        assert_eq!(vol.volume_id, vol_id);
        assert_eq!(vol.capacity_bytes, req.capacity_bytes);
        assert_eq!(vol.volume_context.get("name").unwrap(), "test-vol");
    }
}
