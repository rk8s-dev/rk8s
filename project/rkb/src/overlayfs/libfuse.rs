use anyhow::{Context, Result, bail};

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
        // TODO: avoid to_string_lossy and to_string
        // TODO: change the signature of mount_fs to accept AsRef<Path>
        let mountpoint = cfg.mountpoint.to_string_lossy().to_string();
        let upperdir = cfg.upper_dir.to_string_lossy().to_string();
        let lowerdir = cfg
            .lower_dir
            .iter()
            .map(|d| d.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        let mut mount_handle =
            libfuse_fs::overlayfs::mount_fs(mountpoint, upperdir, lowerdir, CONFIG.is_root).await;

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
