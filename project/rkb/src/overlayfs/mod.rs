use std::{path::Path, process::Command};

use anyhow::Result;

pub mod mount_config;
pub mod mount_libfuse;
pub mod mount_linux;

pub fn copy_dir<P: AsRef<Path>, Q: AsRef<Path>>(source: P, dest: Q) -> Result<()> {
    let status = Command::new("cp")
        .args(&[
            "-a",
            source.as_ref().to_str().unwrap(),
            dest.as_ref().to_str().unwrap(),
        ])
        .status()?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "Failed to copy directory from {} to {}",
            source.as_ref().display(),
            dest.as_ref().display()
        ));
    }
    Ok(())
}
