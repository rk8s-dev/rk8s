use crate::overlayfs::switch_namespace;
use anyhow::{Result, bail};
use clap::Parser;
use std::{path::Path, process::Command};

#[derive(Debug, Parser, Clone)]
pub struct CopyArgs {
    #[arg(long)]
    pub src: Vec<String>,
    #[arg(long)]
    pub dest: String,
}

pub fn copy(args: CopyArgs) -> Result<()> {
    let mount_pid = std::env::var("MOUNT_PID")?.parse::<u32>()?;
    switch_namespace(mount_pid)?;

    let dest_path = Path::new(&args.dest);
    for s in args.src {
        let src_path = Path::new(&s);
        let status = Command::new("cp")
            .arg("-r")
            .arg(src_path)
            .arg(dest_path)
            .status()?;
        if !status.success() {
            bail!(
                "Failed to copy {} to {}",
                src_path.display(),
                dest_path.display()
            );
        }
    }
    Ok(())
}
