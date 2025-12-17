// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Example binary to mount overlay filesystem implemented by libfuse-fs.

use clap::Parser;
use libfuse_fs::unionfs::{OverlayArgs, mount_fs};
use libfuse_fs::util::bind_mount::{BindMount, BindMountManager};
use tokio::signal;
use tracing::{debug, error, info};

#[derive(Parser, Debug)]
#[command(author, version, about = "UnionFS example for integration tests")]
struct Args {
    /// Mount point path
    #[arg(long)]
    mountpoint: String,
    /// Upper writable layer directory
    #[arg(long)]
    upperdir: String,
    /// Lower read-only layer directories (repeatable)
    #[arg(long)]
    lowerdir: Vec<String>,
    /// Use privileged mount instead of unprivileged (default false)
    #[arg(long, default_value_t = false)]
    privileged: bool,
    /// Options, currently contains uid/gid mapping info
    #[arg(long, short)]
    mapping: Option<String>,
    #[arg(long)]
    allow_other: bool,
    /// Bind mounts in format "source:target" (repeatable)
    #[arg(long = "bind")]
    bind_mounts: Vec<String>,
}

fn set_log() {
    let log_level = "trace";
    let filter_str = format!("libfuse_fs={}", log_level);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter_str));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    set_log();
    debug!("Starting overlay filesystem with args: {:?}", args);

    // Parse bind mounts
    let bind_specs: Result<Vec<BindMount>, _> = args
        .bind_mounts
        .iter()
        .map(|s| BindMount::parse(s))
        .collect();

    let bind_specs = match bind_specs {
        Ok(specs) => specs,
        Err(e) => {
            error!("Failed to parse bind mount specifications: {}", e);
            std::process::exit(1);
        }
    };

    // Create bind mount manager
    let bind_manager = BindMountManager::new(&args.mountpoint);

    let mut mount_handle = mount_fs(OverlayArgs {
        name: None::<String>,
        mountpoint: args.mountpoint.clone(),
        lowerdir: args.lowerdir,
        upperdir: args.upperdir,
        mapping: args.mapping,
        privileged: args.privileged,
        allow_other: args.allow_other,
    })
    .await;

    // Mount bind mounts after the overlay filesystem is mounted
    if !bind_specs.is_empty() {
        info!("Mounting {} bind mount(s)", bind_specs.len());
        if let Err(e) = bind_manager.mount_all(&bind_specs).await {
            error!("Failed to mount bind mounts: {}", e);
            // Unmount the overlay filesystem
            mount_handle.unmount().await.unwrap();
            std::process::exit(1);
        }
    }

    tokio::select! {
        res = &mut mount_handle => {
            if let Err(e) = res {
                error!("Overlay filesystem error: {:?}", e);
            }
            info!("Cleaning up...");
            // Unmount bind mounts first
            if let Err(e) = bind_manager.unmount_all().await {
                error!("Failed to unmount bind mounts: {}", e);
            }
        },
        _ = signal::ctrl_c() => {
            info!("Received SIGINT signal, cleaning up...");
            // Unmount bind mounts first
            if let Err(e) = bind_manager.unmount_all().await {
                error!("Failed to unmount bind mounts: {}", e);
            }
            info!("Bind mounts unmounted.");

            // Then unmount the overlay filesystem
            if let Err(e) = mount_handle.unmount().await {
                error!("Failed to unmount overlay filesystem: {}", e);
            }
            info!("Overlay filesystem unmounted.");
        }
        _ = async {
            use tokio::signal::unix::{signal, SignalKind};
            let mut term = signal(SignalKind::terminate()).expect("Failed to setup SIGTERM handler");
            term.recv().await
        } => {
            info!("Received SIGTERM signal, cleaning up...");
            // Unmount bind mounts first
            if let Err(e) = bind_manager.unmount_all().await {
                error!("Failed to unmount bind mounts: {}", e);
            }
            info!("Bind mounts unmounted.");

            // Then unmount the overlay filesystem
            if let Err(e) = mount_handle.unmount().await {
                error!("Failed to unmount overlay filesystem: {}", e);
            }
            info!("Overlay filesystem unmounted.");
        }
    }

    info!("Exiting process.");
    // Force exit to ensure all threads are killed
    std::process::exit(0);
}
