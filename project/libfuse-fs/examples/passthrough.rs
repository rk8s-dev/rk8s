// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Simple passthrough filesystem example for integration tests.

use clap::Parser;
use libfuse_fs::passthrough::{
    PassthroughArgs, new_passthroughfs_layer, newlogfs::LoggingFileSystem,
};
use rfuse3::{MountOptions, raw::Session};
use std::ffi::OsString;
use tokio::signal;
use tracing::debug;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Passthrough FS example for integration tests"
)]
struct Args {
    /// Path to mount point
    #[arg(long)]
    mountpoint: String,
    /// Source directory to expose
    #[arg(long)]
    rootdir: String,
    /// Use privileged mount instead of unprivileged (default false)
    #[arg(long, default_value_t = false)]
    privileged: bool,
    /// Options, currently contains uid/gid mapping info
    #[arg(long, short)]
    options: Option<String>,
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
    debug!("Starting passthrough filesystem with args: {:?}", args);

    let fs = new_passthroughfs_layer(PassthroughArgs {
        root_dir: args.rootdir,
        mapping: args.options,
    })
    .await
    .expect("Failed to init passthrough fs");

    let fs = LoggingFileSystem::new(fs);
    let mount_path = OsString::from(&args.mountpoint);
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };

    let mut mount_options = MountOptions::default();
    mount_options
        .force_readdir_plus(true)
        .uid(uid)
        .gid(gid)
        .allow_other(args.allow_other);

    let mut mount_handle = if !args.privileged {
        debug!("Mounting passthrough (unprivileged)");
        Session::new(mount_options)
            .mount_with_unprivileged(fs, mount_path)
            .await
            .expect("Unprivileged mount failed")
    } else {
        debug!("Mounting passthrough (privileged)");
        Session::new(mount_options)
            .mount(fs, mount_path)
            .await
            .expect("Privileged mount failed")
    };

    let handle = &mut mount_handle;
    tokio::select! {
        res = handle => res.unwrap(),
        _ = signal::ctrl_c() => {
            mount_handle.unmount().await.unwrap();
        }
    }
}
