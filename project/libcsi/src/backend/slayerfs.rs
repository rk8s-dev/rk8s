//! SlayerFS storage backend for CSI.
//!
//! [`SlayerFsBackend`] implements [`CsiIdentity`], [`CsiController`], and
//! [`CsiNode`] using SlayerFS's `LocalClient` as the underlying storage
//! engine.  Volumes are stored as sub-directories under a configurable
//! `object_root`, and made available to containers via FUSE staging and
//! bind-mount publishing.
//!
//! # On-disk layout
//!
//! ```text
//! <object_root>/
//!   <volume-id>/            # SlayerFS object store for each volume
//!   <volume-id>.meta.json   # Persisted CSI metadata (used for recovery)
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tracing::{debug, info, instrument, warn};

use slayerfs::{ChunkLayout, ClientBackend, LocalClient};

use crate::controller::CsiController;
use crate::error::CsiError;
use crate::identity::CsiIdentity;
use crate::node::CsiNode;
use crate::types::*;

/// Key stored in [`Volume::parameters`] to persist the caller-supplied volume
/// name across process restarts, enabling `create_volume` idempotency after
/// recovery.
const PARAM_CSI_NAME: &str = "_csi_name";

/// Concrete CSI backend backed by SlayerFS.
///
/// # Thread safety
///
/// All mutable state is behind concurrent maps ([`DashMap`]), allowing
/// multiple Tokio tasks to operate on different volumes concurrently.
pub struct SlayerFsBackend {
    /// Root directory for all SlayerFS volume object stores.
    object_root: PathBuf,
    /// Chunk layout configuration forwarded to each `LocalClient`.
    layout: ChunkLayout,
    /// Live SlayerFS clients, keyed by volume ID.
    volumes: DashMap<VolumeId, Arc<dyn ClientBackend>>,
    /// Volume metadata, keyed by volume ID.
    volume_meta: DashMap<VolumeId, Volume>,
    /// Maps the caller-supplied volume name to the assigned [`VolumeId`].
    /// Used to enforce idempotency in `create_volume`: repeated calls with
    /// the same name return the existing volume instead of creating a new one.
    volume_names: DashMap<String, VolumeId>,
    /// Node identifier (hostname or user-supplied string).
    node_id: String,
}

impl SlayerFsBackend {
    /// Create a new backend.
    ///
    /// Call [`Self::recover`] afterwards to restore state from a previous
    /// process run.
    ///
    /// * `object_root` — base directory on the local filesystem
    /// * `layout` — SlayerFS chunk layout
    /// * `node_id` — unique identifier for this node
    pub fn new(object_root: impl Into<PathBuf>, layout: ChunkLayout, node_id: String) -> Self {
        Self {
            object_root: object_root.into(),
            layout,
            volumes: DashMap::new(),
            volume_meta: DashMap::new(),
            volume_names: DashMap::new(),
            node_id,
        }
    }

    /// Resolve the on-disk object store directory for a given volume.
    fn volume_root(&self, volume_id: &VolumeId) -> PathBuf {
        self.object_root.join(&volume_id.0)
    }

    /// Resolve the path to the persisted metadata sidecar for a volume.
    fn meta_path(&self, volume_id: &VolumeId) -> PathBuf {
        self.object_root.join(format!("{}.meta.json", volume_id.0))
    }

    /// Scan `object_root` for persisted volume metadata and rebuild the
    /// in-memory state maps.
    ///
    /// This is a best-effort operation: volumes whose directories or metadata
    /// files are missing are skipped with a warning rather than treated as
    /// hard errors.  Call this once after construction to restore state across
    /// process restarts.
    pub async fn recover(&self) -> Result<(), CsiError> {
        let mut dir = match tokio::fs::read_dir(&self.object_root).await {
            Ok(d) => d,
            // Nothing to recover if the root does not exist yet.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(CsiError::BackendError(format!(
                    "read_dir {}: {e}",
                    self.object_root.display()
                )));
            }
        };

        while let Some(entry) = dir.next_entry().await.map_err(CsiError::backend)? {
            let path = entry.path();

            // Only process `.meta.json` sidecar files.
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !file_name.ends_with(".meta.json") {
                continue;
            }

            let json = match tokio::fs::read_to_string(&path).await {
                Ok(s) => s,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "failed to read volume metadata, skipping");
                    continue;
                }
            };

            let volume: Volume = match serde_json::from_str(&json) {
                Ok(v) => v,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "failed to parse volume metadata, skipping");
                    continue;
                }
            };

            let vol_root = self.volume_root(&volume.volume_id);
            if !vol_root.exists() {
                warn!(volume_id = %volume.volume_id, "volume directory missing, skipping recovery");
                continue;
            }

            match LocalClient::new_local(&vol_root, self.layout).await {
                Ok(client) => {
                    if let Some(name) = volume.parameters.get(PARAM_CSI_NAME) {
                        self.volume_names
                            .insert(name.clone(), volume.volume_id.clone());
                    }
                    self.volumes
                        .insert(volume.volume_id.clone(), Arc::new(client));
                    self.volume_meta.insert(volume.volume_id.clone(), volume);
                }
                Err(e) => {
                    warn!(volume_id = %volume.volume_id, error = %e,
                        "failed to re-open SlayerFS client, skipping");
                }
            }
        }

        info!(
            object_root = %self.object_root.display(),
            count = self.volume_meta.len(),
            "recovery complete",
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Mount point detection
// ---------------------------------------------------------------------------

/// Return `true` if `path` is currently listed as a mount point in
/// `/proc/self/mounts`.
///
/// Used by [`publish_volume`] and [`unpublish_volume`] to implement
/// idempotency without relying solely on directory-existence checks.
///
/// Note: `/proc/self/mounts` uses octal escapes (`\040` for space, etc.).
/// CSI target paths must not contain whitespace, so direct string comparison
/// is safe here.
async fn is_mountpoint(path: &str) -> bool {
    let contents = match tokio::fs::read_to_string("/proc/self/mounts").await {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Format: <device> <mountpoint> <fstype> <options> <dump> <pass>
    contents
        .lines()
        .any(|line| line.split_whitespace().nth(1) == Some(path))
}

// ---------------------------------------------------------------------------
// CsiIdentity
// ---------------------------------------------------------------------------

#[async_trait]
impl CsiIdentity for SlayerFsBackend {
    async fn get_plugin_info(&self) -> Result<PluginInfo, CsiError> {
        Ok(PluginInfo {
            name: "rk8s.slayerfs.csi".to_owned(),
            vendor_version: env!("CARGO_PKG_VERSION").to_owned(),
        })
    }

    async fn probe(&self) -> Result<bool, CsiError> {
        // The backend is healthy when its object root exists and is a directory.
        let exists = tokio::fs::metadata(&self.object_root)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false);
        Ok(exists)
    }

    async fn get_plugin_capabilities(&self) -> Result<Vec<PluginCapability>, CsiError> {
        Ok(vec![PluginCapability::ControllerService])
    }
}

// ---------------------------------------------------------------------------
// CsiController
// ---------------------------------------------------------------------------

#[async_trait]
impl CsiController for SlayerFsBackend {
    #[instrument(skip(self), fields(name = %req.name))]
    async fn create_volume(&self, req: CreateVolumeRequest) -> Result<Volume, CsiError> {
        // Idempotency: if a volume with this name was already provisioned,
        // return it rather than allocating a new one.
        let existing_id = self.volume_names.get(&req.name).map(|r| r.clone());
        if let Some(id) = existing_id {
            if let Some(vol) = self.volume_meta.get(&id).map(|r| r.clone()) {
                debug!(name = %req.name, %id, "returning existing volume for idempotent create");
                return Ok(vol);
            }
            // Stale entry: name is recorded but metadata is gone.
            // Clean up so the code below can proceed with a fresh allocation.
            self.volume_names.remove(&req.name);
        }

        let vol_id = VolumeId(format!("slayerfs-{}", uuid::Uuid::new_v4()));
        let vol_root = self.volume_root(&vol_id);

        tokio::fs::create_dir_all(&vol_root).await.map_err(|e| {
            CsiError::BackendError(format!("create dir {}: {e}", vol_root.display()))
        })?;

        let client = LocalClient::new_local(&vol_root, self.layout)
            .await
            .map_err(CsiError::backend)?;

        client.mkdir_p("/").await.map_err(CsiError::backend)?;

        // Embed the caller-supplied name in parameters so it survives restarts
        // and can be used to rebuild `volume_names` during recovery.
        let mut parameters = req.parameters;
        parameters.insert(PARAM_CSI_NAME.to_owned(), req.name.clone());

        let volume = Volume {
            volume_id: vol_id.clone(),
            capacity_bytes: req.capacity_bytes,
            parameters,
            volume_context: HashMap::from([(
                "object_root".to_owned(),
                vol_root.to_string_lossy().into_owned(),
            )]),
            accessible_topology: vec![Topology {
                segments: HashMap::from([("node".to_owned(), self.node_id.clone())]),
            }],
        };

        // Persist metadata to disk *before* updating in-memory state.
        // If the write fails the caller can safely retry: no volume ID has
        // been committed yet.
        let meta_json = serde_json::to_string_pretty(&volume).map_err(CsiError::backend)?;
        tokio::fs::write(self.meta_path(&vol_id), meta_json)
            .await
            .map_err(|e| CsiError::BackendError(format!("write meta {vol_id}: {e}")))?;

        self.volumes.insert(vol_id.clone(), Arc::new(client));
        self.volume_meta.insert(vol_id.clone(), volume.clone());
        self.volume_names.insert(req.name, vol_id.clone());

        info!(%vol_id, "volume created");
        Ok(volume)
    }

    #[instrument(skip(self))]
    async fn delete_volume(&self, volume_id: &VolumeId) -> Result<(), CsiError> {
        // Delete on-disk data *first* so that if removal fails the in-memory
        // state is still intact and the caller can safely retry.
        let vol_root = self.volume_root(volume_id);
        if vol_root.exists() {
            tokio::fs::remove_dir_all(&vol_root).await.map_err(|e| {
                CsiError::BackendError(format!("remove dir {}: {e}", vol_root.display()))
            })?;
        }

        let meta_path = self.meta_path(volume_id);
        if meta_path.exists() {
            tokio::fs::remove_file(&meta_path).await.map_err(|e| {
                CsiError::BackendError(format!("remove meta {}: {e}", meta_path.display()))
            })?;
        }

        // Update in-memory state only after the disk is clean.
        self.volumes.remove(volume_id);
        if let Some((_, vol)) = self.volume_meta.remove(volume_id)
            && let Some(name) = vol.parameters.get(PARAM_CSI_NAME)
        {
            self.volume_names.remove(name);
        }

        info!(%volume_id, "volume deleted");
        Ok(())
    }

    async fn validate_volume_capabilities(
        &self,
        volume_id: &VolumeId,
        _capabilities: &[VolumeCapability],
    ) -> Result<bool, CsiError> {
        if !self.volume_meta.contains_key(volume_id) {
            return Err(CsiError::VolumeNotFound(volume_id.to_string()));
        }
        // SlayerFS supports all access modes.
        Ok(true)
    }

    async fn list_volumes(&self) -> Result<Vec<Volume>, CsiError> {
        let vols = self
            .volume_meta
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        Ok(vols)
    }

    async fn get_capacity(&self) -> Result<u64, CsiError> {
        let stat = nix::sys::statvfs::statvfs(
            self.object_root
                .to_str()
                .ok_or_else(|| CsiError::Internal("non-UTF8 object root path".into()))?,
        )
        .map_err(|e| CsiError::Internal(format!("statvfs: {e}")))?;
        Ok(stat.fragment_size() * stat.blocks_available())
    }
}

// ---------------------------------------------------------------------------
// CsiNode
// ---------------------------------------------------------------------------

#[async_trait]
impl CsiNode for SlayerFsBackend {
    #[instrument(skip(self), fields(volume_id = %req.volume_id))]
    async fn stage_volume(&self, req: NodeStageVolumeRequest) -> Result<(), CsiError> {
        let staging = Path::new(&req.staging_target_path);

        // Idempotent: treat an existing staging directory as evidence that a
        // prior stage succeeded.  This invariant is maintained by
        // `unstage_volume`, which always removes the directory on unmount so
        // that a leftover directory unambiguously means the volume is staged.
        //
        // TODO: Once FUSE (rfuse3) integration lands, replace this
        // directory-existence check with `is_mountpoint()` to correctly handle
        // the case where the directory was created but the mount was not
        // completed.
        if staging.exists() {
            debug!(path = %req.staging_target_path, "staging path already exists, assuming idempotent retry");
            return Ok(());
        }

        tokio::fs::create_dir_all(staging)
            .await
            .map_err(|e| CsiError::MountFailed {
                path: req.staging_target_path.clone(),
                reason: e.to_string(),
            })?;

        // NOTE: The actual FUSE mount of SlayerFS at the staging path will be
        // done here via rfuse3 once that integration is implemented (P2):
        //
        //   let vfs = build_slayerfs_vfs(&req.volume_context)?;
        //   tokio::spawn(rfuse3::mount(vfs, staging, mount_options));

        info!(path = %req.staging_target_path, "volume staged (FUSE mount placeholder)");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn unstage_volume(
        &self,
        volume_id: &VolumeId,
        staging_target_path: &str,
    ) -> Result<(), CsiError> {
        let staging = Path::new(staging_target_path);
        if !staging.exists() {
            debug!(%volume_id, "staging path gone, nothing to unstage");
            return Ok(());
        }

        // Unmount the FUSE filesystem.
        let status = tokio::process::Command::new("fusermount3")
            .args(["-u", staging_target_path])
            .status()
            .await
            .map_err(|e| CsiError::UnmountFailed {
                path: staging_target_path.to_owned(),
                reason: e.to_string(),
            })?;

        if !status.success() {
            // A non-zero exit typically means the path was already unmounted.
            warn!(
                %volume_id,
                path = staging_target_path,
                code = ?status.code(),
                "fusermount3 returned non-zero (may already be unmounted)",
            );
        }

        // Remove the staging directory so that a subsequent `stage_volume`
        // call correctly detects that no mount is active.  If this directory
        // were left behind, `stage_volume`'s existence check would
        // incorrectly skip the mount entirely.
        tokio::fs::remove_dir(staging)
            .await
            .map_err(|e| CsiError::UnmountFailed {
                path: staging_target_path.to_owned(),
                reason: format!("remove staging dir: {e}"),
            })?;

        info!(%volume_id, path = staging_target_path, "volume unstaged");
        Ok(())
    }

    #[instrument(skip(self), fields(volume_id = %req.volume_id))]
    async fn publish_volume(&self, req: NodePublishVolumeRequest) -> Result<(), CsiError> {
        // Idempotent: skip the bind-mount if the target is already a mount
        // point.  Without this check a second call would fail with EBUSY.
        if is_mountpoint(&req.target_path).await {
            debug!(target_path = %req.target_path, "target already mounted, assuming idempotent retry");
            return Ok(());
        }

        tokio::fs::create_dir_all(Path::new(&req.target_path))
            .await
            .map_err(|e| CsiError::MountFailed {
                path: req.target_path.clone(),
                reason: e.to_string(),
            })?;

        let mut flags = nix::mount::MsFlags::MS_BIND;
        if req.read_only {
            flags |= nix::mount::MsFlags::MS_RDONLY;
        }

        nix::mount::mount(
            Some(req.staging_target_path.as_str()),
            req.target_path.as_str(),
            None::<&str>,
            flags,
            None::<&str>,
        )
        .map_err(|e| CsiError::MountFailed {
            path: req.target_path.clone(),
            reason: e.to_string(),
        })?;

        // Some kernels ignore MS_RDONLY on the initial bind-mount call; a
        // separate remount is required to actually enforce read-only access.
        if req.read_only {
            nix::mount::mount(
                None::<&str>,
                req.target_path.as_str(),
                None::<&str>,
                nix::mount::MsFlags::MS_BIND
                    | nix::mount::MsFlags::MS_REMOUNT
                    | nix::mount::MsFlags::MS_RDONLY,
                None::<&str>,
            )
            .map_err(|e| CsiError::MountFailed {
                path: req.target_path.clone(),
                reason: format!("remount read-only: {e}"),
            })?;
        }

        info!(
            target_path = %req.target_path,
            read_only = req.read_only,
            "volume published (bind-mount)",
        );
        Ok(())
    }

    #[instrument(skip(self))]
    async fn unpublish_volume(
        &self,
        volume_id: &VolumeId,
        target_path: &str,
    ) -> Result<(), CsiError> {
        // Idempotent: skip umount if the target is not currently a mount point.
        // Using a mountpoint check rather than directory existence avoids
        // spurious EINVAL errors on repeated calls.
        if !is_mountpoint(target_path).await {
            debug!(%volume_id, "target not mounted, nothing to unpublish");
            return Ok(());
        }

        nix::mount::umount(target_path).map_err(|e| CsiError::UnmountFailed {
            path: target_path.to_owned(),
            reason: e.to_string(),
        })?;

        info!(%volume_id, %target_path, "volume unpublished");
        Ok(())
    }

    async fn get_info(&self) -> Result<NodeInfo, CsiError> {
        Ok(NodeInfo {
            node_id: self.node_id.clone(),
            max_volumes: 256,
            accessible_topology: Some(Topology {
                segments: HashMap::from([("node".to_owned(), self.node_id.clone())]),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_backend(dir: &Path) -> SlayerFsBackend {
        SlayerFsBackend::new(dir, ChunkLayout::default(), "test-node".to_owned())
    }

    #[tokio::test]
    async fn create_and_delete_volume() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = make_backend(tmp.path());

        let vol = backend
            .create_volume(CreateVolumeRequest {
                name: "test-vol".into(),
                capacity_bytes: 64 * 1024 * 1024,
                ..Default::default()
            })
            .await
            .unwrap();

        assert!(vol.volume_id.0.starts_with("slayerfs-"));
        assert!(backend.volume_meta.contains_key(&vol.volume_id));
        assert!(backend.volumes.contains_key(&vol.volume_id));
        assert!(backend.volume_names.contains_key("test-vol"));

        // The metadata sidecar file must be written to disk.
        assert!(backend.meta_path(&vol.volume_id).exists());

        // List should include the volume.
        let list = backend.list_volumes().await.unwrap();
        assert_eq!(list.len(), 1);

        // Delete.
        backend.delete_volume(&vol.volume_id).await.unwrap();
        assert!(!backend.volume_meta.contains_key(&vol.volume_id));
        assert!(!backend.volume_names.contains_key("test-vol"));
        assert!(!backend.meta_path(&vol.volume_id).exists());

        let list = backend.list_volumes().await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn create_volume_idempotent_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = make_backend(tmp.path());

        let vol1 = backend
            .create_volume(CreateVolumeRequest {
                name: "my-vol".into(),
                capacity_bytes: 1024,
                ..Default::default()
            })
            .await
            .unwrap();

        // Second call with the same name must return the same volume.
        let vol2 = backend
            .create_volume(CreateVolumeRequest {
                name: "my-vol".into(),
                capacity_bytes: 1024,
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(vol1.volume_id, vol2.volume_id);
        assert_eq!(backend.list_volumes().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn recover_restores_state() {
        let tmp = tempfile::tempdir().unwrap();

        // Create a volume in the first backend instance.
        let vol_id = {
            let backend = make_backend(tmp.path());
            let vol = backend
                .create_volume(CreateVolumeRequest {
                    name: "persistent-vol".into(),
                    capacity_bytes: 1024,
                    ..Default::default()
                })
                .await
                .unwrap();
            vol.volume_id
        };

        // A fresh backend instance must recover the volume via recover().
        let backend2 = make_backend(tmp.path());
        assert!(!backend2.volume_meta.contains_key(&vol_id));

        backend2.recover().await.unwrap();

        assert!(backend2.volume_meta.contains_key(&vol_id));
        assert!(backend2.volume_names.contains_key("persistent-vol"));
        assert_eq!(backend2.list_volumes().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn probe_healthy_root() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = make_backend(tmp.path());
        assert!(backend.probe().await.unwrap());
    }

    #[tokio::test]
    async fn probe_missing_root() {
        let backend = make_backend(Path::new("/nonexistent/path/for/test"));
        assert!(!backend.probe().await.unwrap());
    }

    #[tokio::test]
    async fn plugin_info() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = make_backend(tmp.path());
        let info = backend.get_plugin_info().await.unwrap();
        assert_eq!(info.name, "rk8s.slayerfs.csi");
    }

    #[tokio::test]
    async fn validate_missing_volume() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = make_backend(tmp.path());
        let result = backend
            .validate_volume_capabilities(&VolumeId("nope".into()), &[])
            .await;
        assert!(matches!(result, Err(CsiError::VolumeNotFound(_))));
    }

    #[tokio::test]
    async fn get_node_info() {
        let tmp = tempfile::tempdir().unwrap();
        let backend = make_backend(tmp.path());
        let info = backend.get_info().await.unwrap();
        assert_eq!(info.node_id, "test-node");
    }
}
