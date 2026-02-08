// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bind mount utilities for container volume management

use std::io::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

/// Represents a single bind mount
#[derive(Debug, Clone)]
pub struct BindMount {
    /// Source path on host
    pub source: PathBuf,
    /// Target path relative to mount point
    pub target: PathBuf,
}

impl BindMount {
    /// Parse a bind mount specification like "proc:/proc" or "/host/path:/container/path"
    pub fn parse(spec: &str) -> Result<Self> {
        let parts: Vec<&str> = spec.split(':').collect();
        if parts.len() != 2 {
            return Err(Error::other(format!(
                "Invalid bind mount spec: '{}'. Expected format: 'source:target'",
                spec
            )));
        }

        let source = PathBuf::from(parts[0]);
        let target = PathBuf::from(parts[1]);

        // Convert relative source paths to absolute from root
        let source = if source.is_relative() {
            PathBuf::from("/").join(source)
        } else {
            source
        };

        Ok(BindMount { source, target })
    }
}

/// Manages multiple bind mounts with automatic cleanup
pub struct BindMountManager {
    mounts: Arc<Mutex<Vec<MountPoint>>>,
    mountpoint: PathBuf,
}

#[derive(Debug)]
struct MountPoint {
    target: PathBuf,
    mounted: bool,
}

impl BindMountManager {
    /// Create a new bind mount manager
    pub fn new<P: AsRef<Path>>(mountpoint: P) -> Self {
        Self {
            mounts: Arc::new(Mutex::new(Vec::new())),
            mountpoint: mountpoint.as_ref().to_path_buf(),
        }
    }

    /// Mount all bind mounts
    pub async fn mount_all(&self, bind_specs: &[BindMount]) -> Result<()> {
        let mut mounts = self.mounts.lock().await;

        for bind in bind_specs {
            let target_path = self
                .mountpoint
                .join(bind.target.strip_prefix("/").unwrap_or(&bind.target));

            // Check if source is a file or directory
            let source_metadata = std::fs::metadata(&bind.source)?;

            if !target_path.exists() {
                if source_metadata.is_file() {
                    // For file bind mounts, create parent directory and an empty file
                    if let Some(parent) = target_path.parent() {
                        std::fs::create_dir_all(parent)?;
                        debug!("Created parent directory: {:?}", parent);
                    }
                    std::fs::File::create(&target_path)?;
                    debug!("Created target file: {:?}", target_path);
                } else {
                    // For directory bind mounts, create the directory
                    std::fs::create_dir_all(&target_path)?;
                    debug!("Created target directory: {:?}", target_path);
                }
            }

            // Perform the bind mount
            self.do_mount(&bind.source, &target_path)?;

            mounts.push(MountPoint {
                target: target_path.clone(),
                mounted: true,
            });

            info!("Bind mounted {:?} -> {:?}", bind.source, target_path);
        }

        Ok(())
    }

    /// Perform the actual bind mount using mount(2) syscall
    #[cfg(target_os = "linux")]
    fn do_mount(&self, source: &Path, target: &Path) -> Result<()> {
        use std::ffi::CString;

        let source_cstr = CString::new(
            source
                .to_str()
                .ok_or_else(|| Error::other(format!("Invalid source path: {:?}", source)))?,
        )
        .map_err(|e| Error::other(format!("CString error: {}", e)))?;

        let target_cstr = CString::new(
            target
                .to_str()
                .ok_or_else(|| Error::other(format!("Invalid target path: {:?}", target)))?,
        )
        .map_err(|e| Error::other(format!("CString error: {}", e)))?;

        let fstype = CString::new("none").unwrap();

        let ret = unsafe {
            libc::mount(
                source_cstr.as_ptr(),
                target_cstr.as_ptr(),
                fstype.as_ptr(),
                libc::MS_BIND | libc::MS_REC,
                std::ptr::null(),
            )
        };

        if ret != 0 {
            let err = Error::last_os_error();
            error!("Failed to bind mount {:?} to {:?}: {}", source, target, err);
            return Err(err);
        }

        // Prevent mount propagation issues by making the mount point a slave.
        // This ensures that unmounting the target doesn't propagate back to the host/source
        // if they are part of a shared subtree (which is common on modern Linux).
        let ret = unsafe {
            libc::mount(
                std::ptr::null(),
                target_cstr.as_ptr(),
                std::ptr::null(),
                libc::MS_SLAVE | libc::MS_REC,
                std::ptr::null(),
            )
        };

        if ret != 0 {
            let err = Error::last_os_error();
            error!("Failed to set mount propagation for {:?}: {}", target, err);
            // Attempt cleanup
            unsafe { libc::umount2(target_cstr.as_ptr(), libc::MNT_DETACH) };
            return Err(err);
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn do_mount(&self, _source: &Path, _target: &Path) -> Result<()> {
        // Bind mounts are not supported on non-Linux platforms yet
        Err(Error::other("Bind mounts are not supported on macOS"))
    }

    /// Unmount all bind mounts
    pub async fn unmount_all(&self) -> Result<()> {
        let mut mounts = self.mounts.lock().await;
        let mut errors = Vec::new();

        // Unmount in reverse order
        while let Some(mut mount) = mounts.pop() {
            if mount.mounted {
                if let Err(e) = self.do_unmount(&mount.target) {
                    error!("Failed to unmount {:?}: {}", mount.target, e);
                    errors.push(e);
                } else {
                    mount.mounted = false;
                    info!("Unmounted {:?}", mount.target);
                }
            }
        }

        if !errors.is_empty() {
            return Err(Error::other(format!(
                "Failed to unmount {} bind mounts",
                errors.len()
            )));
        }

        Ok(())
    }

    /// Perform the actual unmount using umount(2) syscall
    #[cfg(target_os = "linux")]
    fn do_unmount(&self, target: &Path) -> Result<()> {
        use std::ffi::CString;

        let target_cstr = CString::new(
            target
                .to_str()
                .ok_or_else(|| Error::other(format!("Invalid target path: {:?}", target)))?,
        )
        .map_err(|e| Error::other(format!("CString error: {}", e)))?;

        let ret = unsafe { libc::umount2(target_cstr.as_ptr(), libc::MNT_DETACH) };

        if ret != 0 {
            let err = Error::last_os_error();
            // EINVAL or ENOENT might mean it's already unmounted
            if err.raw_os_error() != Some(libc::EINVAL) && err.raw_os_error() != Some(libc::ENOENT)
            {
                return Err(err);
            }
        }

        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn do_unmount(&self, _target: &Path) -> Result<()> {
        Ok(())
    }
}

impl Drop for BindMountManager {
    fn drop(&mut self) {
        // Attempt to clean up on drop (synchronously)
        let mounts = self.mounts.try_lock();
        if let Ok(mut mounts) = mounts {
            while let Some(mount) = mounts.pop() {
                if mount.mounted {
                    let _ = self.do_unmount(&mount.target);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bind_mount() {
        let bind = BindMount::parse("proc:/proc").unwrap();
        assert_eq!(bind.source, PathBuf::from("/proc"));
        assert_eq!(bind.target, PathBuf::from("/proc"));

        let bind = BindMount::parse("/host/path:/container/path").unwrap();
        assert_eq!(bind.source, PathBuf::from("/host/path"));
        assert_eq!(bind.target, PathBuf::from("/container/path"));

        let bind = BindMount::parse("sys:/sys").unwrap();
        assert_eq!(bind.source, PathBuf::from("/sys"));
        assert_eq!(bind.target, PathBuf::from("/sys"));
    }

    #[test]
    fn test_invalid_bind_mount() {
        assert!(BindMount::parse("invalid").is_err());
        assert!(BindMount::parse("too:many:colons").is_err());
    }

    #[tokio::test]
    #[cfg(target_os = "macos")]
    async fn test_bind_mount_macos_fail() {
        // Since mount_all calls do_mount, it should fail
        // However, mount_all creates directories first. We should mock or use temp dirs.
        // Or we can just call do_mount via internal method if it was public? It is private.
        // We can call mount_all with dummy paths.
        // But mount_all attempts to create dirs.
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        let target_dir = temp.path().join("target_dir");
        let manager = BindMountManager::new(&target_dir);
        let bind = BindMount {
            source: source.clone(),
            target: std::path::PathBuf::from("mnt"),
        };

        let result = manager.mount_all(&[bind]).await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Bind mounts are not supported on macOS"
        );
    }
}
