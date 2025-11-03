// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Example binary to mount overlay filesystem implemented by libfuse-fs.
// Used by integration tests (fio & IOR) for overlayfs validation.

use clap::Parser;
use libfuse_fs::overlayfs::{OverlayArgs, mount_fs};
use tokio::signal;
use tracing::debug;

#[derive(Parser, Debug)]
#[command(author, version, about = "OverlayFS example for integration tests")]
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
    #[arg(long, default_value_t = true)]
    privileged: bool,
    /// Options, currently contains uid/gid mapping info
    #[arg(long, short)]
    mapping: Option<String>,
    #[arg(long)]
    allow_other: bool,
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

    let mut mount_handle = mount_fs(OverlayArgs {
        name: None::<String>,
        mountpoint: args.mountpoint,
        lowerdir: args.lowerdir,
        upperdir: args.upperdir,
        mapping: args.mapping,
        privileged: args.privileged,
        allow_other: args.allow_other,
    })
    .await;

    let handle = &mut mount_handle;
    tokio::select! {
        res = handle => res.unwrap(),
        _ = signal::ctrl_c() => {
            mount_handle.unmount().await.unwrap();
        }
    }
}
