use anyhow::{Context, Result, bail};
use libfuse_fs::overlayfs::OverlayArgs;

use crate::{
    config::image::CONFIG,
    overlayfs::{MountConfig, check_mountpoint, detach},
    rt::block_on,
};

pub(super) fn do_mount(
    cfg: &MountConfig,
    tx: ipc_channel::ipc::IpcSender<String>,
    rx: ipc_channel::ipc::IpcReceiver<String>,
) -> Result<()> {
    check_mountpoint(cfg)?;
    detach()?;

    block_on(async {
        let mut mount_handle = libfuse_fs::overlayfs::mount_fs(OverlayArgs {
            lowerdir: &cfg.lower_dir,
            upperdir: &cfg.upper_dir,
            mountpoint: &cfg.mountpoint,
            privileged: CONFIG.is_root,
            mapping: None::<&str>,
            name: None::<String>,
            allow_other: true,
        })
        .await;

        // send ready message to parent process
        tx.send("ready".to_string())
            .context("Failed to send ready message to parent")?;

        let handle = &mut mount_handle;

        tracing::trace!("Entering select loop");
        tokio::select! {
            res = handle => {
                res?;
                Ok(())
            },
            res = tokio::task::spawn_blocking(move || rx.recv()) => {
                let recv_result = res.context("Spawn blocking task failed")?;
                match recv_result {
                    Ok(line) => {
                        if line.trim() == "exit" {
                            mount_handle.unmount().await?;
                            Ok(())
                        } else {
                            bail!("Unknown message received: {line}");
                        }
                    },
                    Err(e) => bail!("Failed to receive IPC message: {e}"),
                }
            }
        }
    })?
}
