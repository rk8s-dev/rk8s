//! Mount helpers for starting/stopping FUSE
//!
//! Notes:
//! - Only supported on Unix-like systems. On Linux we support unprivileged mount via fusermount3.
//! - These helpers are thin wrappers over rfuse3 raw Session APIs.

use std::num::NonZeroU32;
use std::path::Path;

use rfuse3::MountOptions;

use crate::chuck::store::BlockStore;
use crate::meta::MetaLayer;
use crate::vfs::fs::VFS;

/// Build default mount options for SlayerFS.
#[allow(dead_code)]
fn default_mount_options() -> MountOptions {
    let mut mo = MountOptions::default();
    mo.fs_name("slayerfs");
    // Enable kernel-side permission checking (recommended for most filesystems)
    mo.default_permissions(true);
    // Allow other users to access the filesystem (required for multi-user scenarios and xfstests)
    // Note: Requires 'user_allow_other' in /etc/fuse.conf for non-root mounts
    mo.allow_other(true);
    // Default to 4 MiB for higher throughput while keeping memory usage reasonable.
    mo.max_write(NonZeroU32::new(4 * 1024 * 1024).unwrap());
    mo
}

/// Mount a VFS instance to the given empty directory using unprivileged mode when available.
#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub async fn mount_vfs_unprivileged<S, M>(
    fs: VFS<S, M>,
    mount_point: impl AsRef<Path>,
) -> std::io::Result<rfuse3::raw::MountHandle>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    let opts = default_mount_options();
    let session = rfuse3::raw::Session::new(opts);
    // Prefer unprivileged mount on Linux (requires fusermount3 in PATH)
    session.mount_with_unprivileged(fs, mount_point).await
}

/// Fallback stub for non-Linux targets.
#[cfg(not(target_os = "linux"))]
pub async fn mount_vfs_unprivileged<S, M>(
    _fs: VFS<S, M>,
    _mount_point: impl AsRef<Path>,
) -> std::io::Result<rfuse3::raw::MountHandle>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "FUSE mount is only supported on Linux in this build",
    ))
}
