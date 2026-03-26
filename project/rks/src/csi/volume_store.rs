//! Xline-backed volume metadata storage.

use crate::api::xlinestore::XlineStore;
use libcsi::{Volume, VolumeId};
use std::sync::Arc;

/// Xline-backed volume metadata store.
///
/// Volumes are stored as JSON under `/registry/volumes/{volume_id}`.
pub struct VolumeStore {
    xline_store: Arc<XlineStore>,
}

impl VolumeStore {
    pub fn new(xline_store: Arc<XlineStore>) -> Self {
        Self { xline_store }
    }

    /// Build the xline key for a given volume ID.
    pub fn key(volume_id: &VolumeId) -> String {
        format!("/registry/volumes/{}", volume_id)
    }

    /// Persist a volume's metadata.
    pub async fn put(&self, volume: &Volume) -> anyhow::Result<()> {
        let key = Self::key(&volume.volume_id);
        let json = serde_json::to_string(volume)?;
        self.xline_store.put_raw(&key, &json).await
    }

    /// Retrieve a volume by ID.
    pub async fn get(&self, volume_id: &VolumeId) -> anyhow::Result<Option<Volume>> {
        let key = Self::key(volume_id);
        let value = self.xline_store.get_raw(&key).await?;
        match value {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Delete a volume's metadata.
    pub async fn delete(&self, volume_id: &VolumeId) -> anyhow::Result<()> {
        let key = Self::key(volume_id);
        self.xline_store.delete_raw(&key).await
    }

    /// List all volumes.
    pub async fn list(&self) -> anyhow::Result<Vec<Volume>> {
        let entries = self.xline_store.list_raw("/registry/volumes/").await?;
        let mut volumes = Vec::with_capacity(entries.len());
        for (_key, json) in entries {
            volumes.push(serde_json::from_str(&json)?);
        }
        Ok(volumes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_volume(id: &str) -> Volume {
        Volume {
            volume_id: VolumeId(id.to_owned()),
            capacity_bytes: 1024 * 1024 * 100,
            parameters: HashMap::new(),
            volume_context: HashMap::new(),
            accessible_topology: vec![],
        }
    }

    #[test]
    fn key_for_volume_id() {
        let id = VolumeId("vol-abc-123".into());
        assert_eq!(VolumeStore::key(&id), "/registry/volumes/vol-abc-123");
    }

    #[test]
    fn volume_json_roundtrip() {
        let vol = sample_volume("vol-1");
        let json = serde_json::to_string(&vol).unwrap();
        let de: Volume = serde_json::from_str(&json).unwrap();
        assert_eq!(de.volume_id, vol.volume_id);
        assert_eq!(de.capacity_bytes, vol.capacity_bytes);
    }
}
