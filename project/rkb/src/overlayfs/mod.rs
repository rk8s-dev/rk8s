// pub mod mount_linux;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::registry::CONFIG;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MountConfig {
    pub lower_dir: Vec<PathBuf>,
    pub upper_dir: PathBuf,
    pub mountpoint: PathBuf,
    pub work_dir: PathBuf,
    /// All above directories are created inside this directory.
    ///
    /// When the struct is being dropped, remove this directory.
    pub overlay: PathBuf,
    upper_cnt: i32,
}

impl MountConfig {
    pub fn new(overlay: PathBuf) -> Self {
        MountConfig {
            lower_dir: Vec::new(),
            upper_dir: PathBuf::default(),
            mountpoint: PathBuf::default(),
            work_dir: PathBuf::default(),
            overlay,
            upper_cnt: 0,
        }
    }

    pub fn init(&mut self) -> Result<()> {
        fs::create_dir_all(&self.overlay).with_context(|| {
            format!(
                "Failed to create overlay directory {}",
                self.overlay.display()
            )
        })?;
        assert!(!self.lower_dir.is_empty());
        self.upper_dir = self.overlay.join(format!("diff{}", self.upper_cnt));
        self.mountpoint = self.overlay.join("merged");
        self.work_dir = self.overlay.join("work");
        Ok(())
    }

    /// Clean up the overlay directory for the next mount.
    pub fn prepare(&mut self) -> Result<()> {
        self.upper_cnt += 1;
        self.upper_dir = self.overlay.join(format!("diff{}", self.upper_cnt));

        if self.mountpoint.exists() {
            fs::remove_dir_all(&self.mountpoint).with_context(|| {
                format!("Failed to remove mountpoint {}", self.mountpoint.display())
            })?;
        }
        if self.work_dir.exists() {
            fs::remove_dir_all(&self.work_dir).with_context(|| {
                format!(
                    "Failed to remove work directory {}",
                    self.work_dir.display()
                )
            })?;
        }

        fs::create_dir_all(&self.upper_dir).with_context(|| {
            format!(
                "Failed to create upper directory {}",
                self.upper_dir.display()
            )
        })?;
        fs::create_dir_all(&self.mountpoint).with_context(|| {
            format!("Failed to create mountpoint {}", self.mountpoint.display())
        })?;
        fs::create_dir_all(&self.work_dir).with_context(|| {
            format!(
                "Failed to create work directory {}",
                self.work_dir.display()
            )
        })?;

        Ok(())
    }

    /// When a mount is finished, the upper directory is moved to the end of the lower directory.
    pub fn finish(&mut self) -> Result<()> {
        assert!(self.upper_dir.exists());
        self.lower_dir.push(self.upper_dir.clone());
        Ok(())
    }
}

impl Default for MountConfig {
    /// By default, the overlay field is set to `${build_dir}/overlay`.
    fn default() -> Self {
        MountConfig {
            lower_dir: Vec::new(),
            upper_dir: PathBuf::default(),
            mountpoint: PathBuf::default(),
            work_dir: PathBuf::default(),
            overlay: CONFIG.build_dir.join("overlay"),
            upper_cnt: 0,
        }
    }
}

pub struct OverlayGuard {
    overlay_dir: PathBuf,
}

impl OverlayGuard {
    pub fn new(overlay_dir: PathBuf) -> Self {
        OverlayGuard { overlay_dir }
    }
}

impl Drop for OverlayGuard {
    fn drop(&mut self) {
        if self.overlay_dir.exists()
            && let Err(e) = std::fs::remove_dir_all(&self.overlay_dir)
        {
            tracing::error!(
                "Failed to remove overlay directory {}: {e}",
                self.overlay_dir.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::MountConfig;

    #[test]
    fn test_mount_config() {
        let tmp_dir = tempdir().unwrap();
        let tmp_path = tmp_dir.path().to_path_buf();

        let mut mount_config = MountConfig::new(tmp_path);
        mount_config.init().unwrap();
        assert_eq!(mount_config.lower_dir.len(), 0);

        mount_config.prepare().unwrap();
        assert_eq!(mount_config.lower_dir.len(), 0);
        assert!(mount_config.upper_dir.exists());
        assert!(mount_config.mountpoint.exists());
        assert!(mount_config.work_dir.exists());

        mount_config.finish().unwrap();
        assert_eq!(mount_config.lower_dir.len(), 1);
        assert!(mount_config.lower_dir[0].exists());
        assert!(mount_config.upper_dir.exists());
        assert!(mount_config.mountpoint.exists());
        assert!(mount_config.work_dir.exists());
    }
}
