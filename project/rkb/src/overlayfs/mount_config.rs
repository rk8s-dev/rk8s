use std::{fs, path::PathBuf};

use super::copy_dir;
use anyhow::{Context, Result};

#[derive(Debug, Clone)]
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
        let lower_dir = self.overlay.join("lower");
        fs::create_dir_all(&lower_dir)
            .with_context(|| format!("Failed to create lower directory {}", lower_dir.display()))?;
        self.upper_dir = self.overlay.join(format!("diff{}", self.upper_cnt));
        self.mountpoint = self.overlay.join("merged");
        self.work_dir = self.overlay.join("work");
        Ok(())
    }

    /// Clean up the overlay directory for the next mount.
    pub fn prepare(&mut self) -> Result<()> {
        if fs::metadata(&self.upper_dir).is_ok() {
            fs::remove_dir_all(&self.upper_dir).with_context(|| {
                format!(
                    "Failed to remove upper directory {}",
                    self.upper_dir.display()
                )
            })?;
        }
        self.upper_cnt += 1;
        self.upper_dir = self.overlay.join(format!("diff{}", self.upper_cnt));

        if fs::metadata(&self.mountpoint).is_ok() {
            fs::remove_dir_all(&self.mountpoint).with_context(|| {
                format!("Failed to remove mountpoint {}", self.mountpoint.display())
            })?;
        }
        if fs::metadata(&self.work_dir).is_ok() {
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
        let lower_dir = self.overlay.join("lower");
        copy_dir(&self.upper_dir, &lower_dir)?;
        self.lower_dir
            .push(self.overlay.join(format!("lower/diff{}", self.upper_cnt)));
        Ok(())
    }
}

impl Default for MountConfig {
    /// By default, the overlay field is set to `./overlay`.
    fn default() -> Self {
        MountConfig {
            lower_dir: Vec::new(),
            upper_dir: PathBuf::default(),
            mountpoint: PathBuf::default(),
            work_dir: PathBuf::default(),
            overlay: PathBuf::from("overlay"),
            upper_cnt: 0,
        }
    }
}

impl Drop for MountConfig {
    fn drop(&mut self) {
        if fs::metadata(&self.overlay).is_ok() {
            fs::remove_dir_all(&self.overlay)
                .with_context(|| {
                    format!(
                        "Failed to remove overlay directory {}",
                        self.overlay.display()
                    )
                })
                .ok();
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
        assert_eq!(mount_config.upper_dir.exists(), true);
        assert_eq!(mount_config.mountpoint.exists(), true);
        assert_eq!(mount_config.work_dir.exists(), true);

        mount_config.finish().unwrap();
        assert_eq!(mount_config.lower_dir.len(), 1);
        assert_eq!(mount_config.lower_dir[0].exists(), true);
        assert_eq!(mount_config.upper_dir.exists(), true);
        assert_eq!(mount_config.mountpoint.exists(), true);
        assert_eq!(mount_config.work_dir.exists(), true);
    }
}
