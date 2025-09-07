use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose};
use clap::Parser;
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use nix::{
    mount::{MsFlags, mount},
    sched::{CloneFlags, unshare},
};
use std::fs;

#[derive(Parser, Debug)]
pub struct MountArgs {
    #[arg(long)]
    config_base64: String,
}

pub fn main(args: MountArgs) -> Result<()> {
    let config_json = general_purpose::STANDARD
        .decode(args.config_base64)
        .context("Failed to decode base64 config")?;
    let mount_config: crate::overlayfs::MountConfig =
        serde_json::from_slice(&config_json).context("Failed to parse mount config from json")?;

    let parent_tx = IpcSender::connect(
        std::env::var("PARENT_SERVER_NAME").context("PARENT_SERVER_NAME not set")?,
    )
    .context("Failed to connect to parent IPC server")?;
    let child_tx: IpcSender<String> = IpcSender::connect(
        std::env::var("CHILD_SERVER_NAME").context("CHILD_SERVER_NAME not set")?,
    )
    .context("Failed to connect to child IPC server")?;
    let (tx, rx): (IpcSender<String>, IpcReceiver<String>) =
        ipc_channel::ipc::channel().context("Failed to create IPC channel")?;
    parent_tx
        .send(tx)
        .context("Failed to send IPC sender to parent")?;

    // create new mount namespace
    unshare(CloneFlags::CLONE_NEWNS).context("Failed to call unshare")?;
    // The process that calls unshare will detach from the namespaces it originally shared with its parent process.
    // This step ensures that modifications to mount points within the new namespace will not propagate back to the parent namespace (the host system),
    // guaranteeing the isolation of mount operations.
    let root = "/";
    mount::<str, _, str, str>(
        None,
        root,
        None,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None,
    )?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let lowerdir: Vec<String> = mount_config
            .lower_dir
            .iter()
            .map(|d| d.to_str().unwrap().to_string())
            .collect();
        let upperdir = mount_config.upper_dir.to_str().unwrap().to_string();
        let mountpoint = mount_config.mountpoint.to_str().unwrap().to_string();

        for dir_str in lowerdir.iter().chain([&upperdir, &mountpoint]) {
            if !fs::metadata(dir_str).map(|m| m.is_dir()).unwrap_or(false) {
                bail!(
                    "Cannot mount: {} does not exist or is not a directory",
                    dir_str
                );
            }
        }

        let mut mount_handle =
            libfuse_fs::overlayfs::mount_fs(mountpoint, upperdir, lowerdir, true).await;

        child_tx
            .send("ready".to_string())
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
    })?;

    Ok(())
}
