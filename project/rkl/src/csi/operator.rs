//! SlayerFS FUSE operator — manages the full mount lifecycle on a worker node.
//!
//! Responsibilities:
//! - **Stage**: create a data directory, initialise a SlayerFS VFS, and FUSE-mount
//!   it at the global staging path (privileged mount).
//! - **Publish**: bind-mount the staging path into a Pod container path.
//! - **Unpublish**: unmount the bind-mount.
//! - **Unstage**: unmount the FUSE filesystem.
//! - **Recover**: on startup, reload persisted state and validate mount points.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use dashmap::DashMap;
use nix::mount::{MntFlags, MsFlags};
use rfuse3::raw::MountHandle;
use tracing::{error, info, warn};

use common::{SlayerFsMetaBackend, SlayerFsVolumeConfig};
use libcsi::{CsiError, NodePublishVolumeRequest, NodeStageVolumeRequest, VolumeId, VolumeState};
use slayerfs::{
    ChunkLayout, LocalFsBackend, ObjectBlockStore, ObjectClient, create_meta_store_from_url,
};

use super::state::{LocalVolumeState, VolumeStateStore};

/// Maximum write size for FUSE mount options (4 MiB).
const FUSE_MAX_WRITE: u32 = 4 * 1024 * 1024;

/// Operator that manages SlayerFS FUSE mount and bind-mount lifecycles.
pub struct SlayerFsOperator {
    /// Root directory for per-volume SlayerFS data.
    data_root: PathBuf,
    /// Chunk layout configuration.
    layout: ChunkLayout,
    /// Active FUSE mount handles keyed by volume id.
    mount_handles: DashMap<VolumeId, MountHandle>,
    /// Persisted volume state.
    state_store: VolumeStateStore,
    /// Node identity string.
    node_id: String,
}

impl SlayerFsOperator {
    /// Create a new operator.
    ///
    /// * `data_root` — e.g. `/var/lib/rkl/slayerfs/`
    /// * `state_dir` — e.g. `/var/lib/rkl/volumes/.state/`
    /// * `layout`    — chunk layout for SlayerFS VFS instances
    /// * `node_id`   — unique node name
    pub fn new(
        data_root: impl Into<PathBuf>,
        state_dir: impl Into<PathBuf>,
        layout: ChunkLayout,
        node_id: String,
    ) -> Result<Self, CsiError> {
        let data_root = data_root.into();
        std::fs::create_dir_all(&data_root).map_err(|e| {
            CsiError::Internal(format!(
                "failed to create data root {}: {e}",
                data_root.display()
            ))
        })?;

        let state_store = VolumeStateStore::new(state_dir)
            .map_err(|e| CsiError::Internal(format!("failed to create state store: {e}")))?;

        Ok(Self {
            data_root,
            layout,
            mount_handles: DashMap::new(),
            state_store,
            node_id,
        })
    }

    /// Return the node identity string.
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Parse SlayerFsVolumeConfig from volume_context, returning defaults if absent or invalid.
    fn parse_volume_config(
        volume_context: &std::collections::HashMap<String, String>,
    ) -> SlayerFsVolumeConfig {
        match volume_context.get("slayerfs_config") {
            Some(json) => serde_json::from_str(json).unwrap_or_else(|e| {
                warn!("failed to parse slayerfs_config, using defaults: {e}");
                SlayerFsVolumeConfig::default()
            }),
            None => SlayerFsVolumeConfig::default(),
        }
    }

    // ----- Stage (FUSE mount) -----------------------------------------------

    /// Stage a volume: create a SlayerFS VFS and FUSE-mount it at
    /// `staging_target_path` using privileged mount.
    ///
    /// Idempotent — succeeds without error if already staged.
    pub async fn stage_volume(&self, req: NodeStageVolumeRequest) -> Result<(), CsiError> {
        let vol_id = &req.volume_id;

        // Idempotent: already tracked in memory?
        if self.mount_handles.contains_key(vol_id) {
            info!(volume_id = %vol_id, "volume already staged, idempotent success");
            return Ok(());
        }

        // Idempotent: mount survived from a previous process (e.g. after restart)?
        if is_mount_point(&req.staging_target_path) {
            info!(
                volume_id = %vol_id,
                path = %req.staging_target_path,
                "staging path is already a mount point (recovered), idempotent success"
            );
            return Ok(());
        }

        // 1. Prepare data directory for this volume's block store
        let vol_data_dir = self.data_root.join(vol_id.to_string());
        std::fs::create_dir_all(&vol_data_dir).map_err(|e| CsiError::MountFailed {
            path: vol_data_dir.display().to_string(),
            reason: format!("create data dir: {e}"),
        })?;

        // 2. Prepare staging mount point
        std::fs::create_dir_all(&req.staging_target_path).map_err(|e| CsiError::MountFailed {
            path: req.staging_target_path.clone(),
            reason: format!("create staging dir: {e}"),
        })?;

        // 3. Parse backend config from volume_context
        let cfg = Self::parse_volume_config(&req.volume_context);

        // 4. Build data backend
        let data_dir = match &cfg.data.localfs {
            Some(lfs) if !lfs.data_dir.is_empty() => std::path::PathBuf::from(&lfs.data_dir),
            _ => vol_data_dir.join("data"),
        };
        std::fs::create_dir_all(&data_dir).map_err(|e| CsiError::MountFailed {
            path: data_dir.display().to_string(),
            reason: format!("create data backend dir: {e}"),
        })?;
        let backend = LocalFsBackend::new(&data_dir);
        let client = ObjectClient::new(backend);
        let store = ObjectBlockStore::new(client);

        // 5. Build metadata store URL
        let meta_url = match cfg.meta.backend {
            SlayerFsMetaBackend::Sqlx => {
                if let Some(ref sqlx_cfg) = cfg.meta.sqlx {
                    if !sqlx_cfg.url.is_empty() {
                        sqlx_cfg.url.clone()
                    } else {
                        let meta_db_path = vol_data_dir.join("meta.db");
                        format!("sqlite://{}?mode=rwc", meta_db_path.display())
                    }
                } else {
                    let meta_db_path = vol_data_dir.join("meta.db");
                    format!("sqlite://{}?mode=rwc", meta_db_path.display())
                }
            }
            SlayerFsMetaBackend::Redis => cfg
                .meta
                .redis
                .as_ref()
                .map(|r| r.url.clone())
                .unwrap_or_else(|| {
                    warn!(
                        "redis meta backend selected but no url provided, falling back to sqlite"
                    );
                    let meta_db_path = vol_data_dir.join("meta.db");
                    format!("sqlite://{}?mode=rwc", meta_db_path.display())
                }),
            SlayerFsMetaBackend::Etcd => {
                warn!("etcd meta backend is not yet supported, falling back to sqlite");
                let meta_db_path = vol_data_dir.join("meta.db");
                format!("sqlite://{}?mode=rwc", meta_db_path.display())
            }
        };
        let meta_handle = match create_meta_store_from_url(&meta_url).await {
            Ok(h) => h,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&vol_data_dir);
                return Err(CsiError::BackendError(format!("create meta store: {e}")));
            }
        };

        // 6. Build layout (use config values if provided, else fall back to operator defaults)
        let layout = ChunkLayout {
            chunk_size: cfg.layout.chunk_size.unwrap_or(self.layout.chunk_size),
            block_size: cfg.layout.block_size.unwrap_or(self.layout.block_size),
        };

        let vfs = match slayerfs::VFS::new(layout, store, meta_handle.store().clone()).await {
            Ok(v) => v,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&vol_data_dir);
                return Err(CsiError::BackendError(format!("create VFS: {e}")));
            }
        };

        // 7. Privileged FUSE mount
        let mount_opts = Self::default_mount_options();
        let handle = match rfuse3::raw::Session::new(mount_opts)
            .mount(vfs, &req.staging_target_path)
            .await
        {
            Ok(h) => h,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&vol_data_dir);
                return Err(CsiError::MountFailed {
                    path: req.staging_target_path.clone(),
                    reason: format!("FUSE mount: {e}"),
                });
            }
        };

        // 8. Store handle and persist state
        self.mount_handles.insert(vol_id.clone(), handle);

        self.state_store
            .save(&LocalVolumeState {
                volume_id: vol_id.clone(),
                state: VolumeState::Staged,
                staging_target_path: Some(req.staging_target_path.clone()),
                target_path: None,
                volume_context: req.volume_context.clone(),
            })
            .map_err(|e| CsiError::Internal(format!("persist state: {e}")))?;

        info!(volume_id = %vol_id, path = %req.staging_target_path, "volume staged");
        Ok(())
    }

    // ----- Unstage (FUSE unmount) -------------------------------------------

    /// Unstage a volume: unmount the FUSE filesystem.
    ///
    /// Idempotent — succeeds without error if already unstaged.
    /// Cleanup (directory removal, state deletion) runs even if unmount fails.
    pub async fn unstage_volume(
        &self,
        volume_id: &VolumeId,
        staging_target_path: &str,
    ) -> Result<(), CsiError> {
        let unmount_err = if let Some((_, handle)) = self.mount_handles.remove(volume_id) {
            // Proper async unmount via rfuse3 API
            match handle.unmount().await {
                Ok(()) => {
                    info!(volume_id = %volume_id, "FUSE unmount completed");
                    None
                }
                Err(e) => {
                    let err = CsiError::UnmountFailed {
                        path: staging_target_path.to_owned(),
                        reason: format!("FUSE unmount: {e}"),
                    };
                    error!(volume_id = %volume_id, error = %e, "FUSE unmount failed, continuing cleanup");
                    Some(err)
                }
            }
        } else {
            // Idempotent: no handle tracked. Try a lazy umount in case the
            // mount survived from a previous process.
            let _ = nix::mount::umount2(Path::new(staging_target_path), MntFlags::MNT_DETACH);
            info!(volume_id = %volume_id, "volume not tracked, idempotent unstage");
            None
        };

        // Always clean up regardless of unmount result
        let _ = std::fs::remove_dir(staging_target_path);

        let vol_data_dir = self.data_root.join(volume_id.to_string());
        let _ = std::fs::remove_dir_all(&vol_data_dir);

        let _ = self.state_store.delete(volume_id);

        info!(volume_id = %volume_id, "volume unstaged");

        match unmount_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    // ----- Publish (bind-mount) ---------------------------------------------

    /// Publish a volume: bind-mount from staging path to target path.
    ///
    /// Idempotent — succeeds without error if already published at the same path.
    pub async fn publish_volume(&self, req: NodePublishVolumeRequest) -> Result<(), CsiError> {
        let vol_id = &req.volume_id;

        // Verify the volume is staged (in-memory handle or surviving mount)
        if !self.mount_handles.contains_key(vol_id) && !is_mount_point(&req.staging_target_path) {
            return Err(CsiError::Internal(format!(
                "volume {vol_id} is not staged, cannot publish"
            )));
        }

        // Idempotent: if target is already a mount point, succeed
        if is_mount_point(&req.target_path) {
            info!(volume_id = %vol_id, target = %req.target_path, "already published, idempotent");
            return Ok(());
        }

        // Create target directory
        std::fs::create_dir_all(&req.target_path).map_err(|e| CsiError::MountFailed {
            path: req.target_path.clone(),
            reason: format!("create target dir: {e}"),
        })?;

        // Bind mount: staging_target_path → target_path
        nix::mount::mount(
            Some(Path::new(&req.staging_target_path)),
            Path::new(&req.target_path),
            None::<&str>,
            MsFlags::MS_BIND,
            None::<&str>,
        )
        .map_err(|e| CsiError::MountFailed {
            path: req.target_path.clone(),
            reason: format!("bind mount: {e}"),
        })?;

        // If read-only, remount with MS_RDONLY
        if req.read_only {
            nix::mount::mount(
                Some(Path::new(&req.staging_target_path)),
                Path::new(&req.target_path),
                None::<&str>,
                MsFlags::MS_BIND | MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY,
                None::<&str>,
            )
            .map_err(|e| CsiError::MountFailed {
                path: req.target_path.clone(),
                reason: format!("read-only remount: {e}"),
            })?;
        }

        // Persist state — load existing to preserve volume_context from stage
        let updated_state = match self.state_store.load(vol_id) {
            Ok(mut existing) => {
                existing.state = VolumeState::Published;
                existing.target_path = Some(req.target_path.clone());
                existing
            }
            Err(_) => LocalVolumeState {
                volume_id: vol_id.clone(),
                state: VolumeState::Published,
                staging_target_path: Some(req.staging_target_path.clone()),
                target_path: Some(req.target_path.clone()),
                volume_context: Default::default(),
            },
        };
        self.state_store
            .save(&updated_state)
            .map_err(|e| CsiError::Internal(format!("persist state: {e}")))?;

        info!(
            volume_id = %vol_id,
            staging = %req.staging_target_path,
            target = %req.target_path,
            read_only = req.read_only,
            "volume published"
        );
        Ok(())
    }

    // ----- Unpublish (umount bind) ------------------------------------------

    /// Unpublish a volume: remove the bind-mount from the container path.
    ///
    /// Idempotent — succeeds without error if already unpublished.
    pub async fn unpublish_volume(
        &self,
        volume_id: &VolumeId,
        target_path: &str,
    ) -> Result<(), CsiError> {
        // Lazy detach unmount — won't block even if the mount point is busy
        match nix::mount::umount2(Path::new(target_path), MntFlags::MNT_DETACH) {
            Ok(()) => {}
            Err(nix::errno::Errno::EINVAL | nix::errno::Errno::ENOENT) => {
                // Not mounted or path gone — idempotent
                info!(volume_id = %volume_id, target = %target_path, "not mounted, idempotent unpublish");
            }
            Err(e) => {
                return Err(CsiError::UnmountFailed {
                    path: target_path.to_owned(),
                    reason: format!("umount2: {e}"),
                });
            }
        }

        // Clean up target directory (best effort)
        let _ = std::fs::remove_dir(target_path);

        // Update state back to Staged
        if let Ok(mut state) = self.state_store.load(volume_id) {
            state.state = VolumeState::Staged;
            state.target_path = None;
            let _ = self.state_store.save(&state);
        }

        info!(volume_id = %volume_id, target = %target_path, "volume unpublished");
        Ok(())
    }

    // ----- Recovery ---------------------------------------------------------

    /// Recover previously active volumes on startup.
    ///
    /// Loads all persisted states and validates that mount points are still
    /// present. Volumes whose mounts are gone are cleaned up.
    pub async fn recover(&self) {
        let states = match self.state_store.load_all() {
            Ok(s) => s,
            Err(e) => {
                error!("failed to load volume states for recovery: {e}");
                return;
            }
        };

        for state in states {
            match state.state {
                VolumeState::Published | VolumeState::Staged => {
                    if let Some(ref staging) = state.staging_target_path
                        && is_mount_point(staging)
                    {
                        // Mount survived from the previous process. We
                        // cannot reclaim the MountHandle, but the FUSE
                        // mount is still alive in the kernel. The fallback
                        // umount2 path in unstage_volume handles cleanup.
                        // stage_volume's /proc/mounts check ensures
                        // idempotent re-stage requests also succeed.
                        info!(
                            volume_id = %state.volume_id,
                            state = ?state.state,
                            "recovered active volume (mount alive)"
                        );
                        continue;
                    }
                    // Mount point gone — clean up stale state
                    warn!(
                        volume_id = %state.volume_id,
                        "mount point missing during recovery, cleaning up"
                    );
                    let _ = self.state_store.delete(&state.volume_id);
                }
                VolumeState::Created => {
                    // Nothing to recover for Created state
                    let _ = self.state_store.delete(&state.volume_id);
                }
            }
        }
    }

    // ----- Helpers ----------------------------------------------------------

    fn default_mount_options() -> rfuse3::MountOptions {
        let mut opts = rfuse3::MountOptions::default();
        opts.fs_name("slayerfs");
        opts.allow_other(true);
        opts.default_permissions(true);
        opts.max_write(NonZeroU32::new(FUSE_MAX_WRITE).unwrap());
        opts
    }
}

/// Best-effort check whether `path` is a mount point by reading `/proc/mounts`.
fn is_mount_point(path: &str) -> bool {
    let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else {
        return false;
    };
    let canonical = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_owned());
    mounts.lines().any(|line| {
        line.split_whitespace()
            .nth(1)
            .is_some_and(|mp| mp == canonical)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_mount_point_root() {
        // `/` is always a mount point
        assert!(is_mount_point("/"));
    }

    #[test]
    fn is_mount_point_nonexistent() {
        assert!(!is_mount_point("/nonexistent/path/xyz"));
    }

    #[test]
    fn default_mount_options_has_fs_name() {
        let opts = SlayerFsOperator::default_mount_options();
        // Just verify it doesn't panic
        drop(opts);
    }
}
