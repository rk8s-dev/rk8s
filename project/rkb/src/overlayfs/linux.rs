use crate::overlayfs::{MountConfig, check_mountpoint, detach};
use anyhow::{Context, Result, bail};
use nix::mount::{MntFlags, MsFlags, mount, umount2};
use std::os::unix::ffi::OsStrExt;

pub(super) fn do_mount(
    cfg: &MountConfig,
    tx: ipc_channel::ipc::IpcSender<String>,
    rx: ipc_channel::ipc::IpcReceiver<String>,
) -> Result<()> {
    let lower_dir = &cfg.lower_dir;
    let upper_dir = &cfg.upper_dir;
    let work_dir = &cfg.work_dir;
    let mountpoint = &cfg.mountpoint;

    check_mountpoint(cfg)?;
    detach()?;

    // Safely construct overlayfs options using byte operations
    let lower_dirs_bytes: Vec<&[u8]> = lower_dir.iter().map(|p| p.as_os_str().as_bytes()).collect();
    let lower_dir_option = lower_dirs_bytes.join(&b':');
    let mut options = Vec::new();
    options.extend_from_slice(b"lowerdir=");
    options.extend_from_slice(&lower_dir_option);
    options.extend_from_slice(b",upperdir=");
    options.extend_from_slice(upper_dir.as_os_str().as_bytes());
    options.extend_from_slice(b",workdir=");
    options.extend_from_slice(work_dir.as_os_str().as_bytes());

    mount(
        Some("overlay".as_bytes()), // source: overlayfs type
        mountpoint,                 // target: mountpoint
        Some("overlay".as_bytes()), // fstype: type of filesystem
        MsFlags::empty(),           // flags: mount flags
        Some(&options[..]),         // data: overlayfs options as a byte slice
    )
    .context("Failed to mount overlayfs")?;

    tx.send("ready".to_string())
        .context("Failed to send ready message to parent")?;

    let line = rx.recv().context("Failed to receive IPC message")?;
    if line.trim() == "exit" {
        umount2(mountpoint, MntFlags::MNT_DETACH).context("Failed to unmount overlayfs")?;
    } else {
        bail!("Unknown message received: {line}");
    }

    Ok(())
}
