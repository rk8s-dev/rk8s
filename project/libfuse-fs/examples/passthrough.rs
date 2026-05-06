// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Simple passthrough filesystem example for integration tests.

use clap::Parser;
use libfuse_fs::passthrough::{PassthroughArgs, new_passthroughfs_layer};
#[cfg(target_os = "macos")]
use libfuse_fs::passthrough::{PassthroughFs, config::Config};
use libfuse_fs::util::bind_mount::{BindMount, BindMountManager};
use rfuse3::raw::logfs::LoggingFileSystem;
use rfuse3::{MountOptions, raw::Session};
use std::ffi::OsString;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::time::Duration;
use tokio::signal;
use tracing::{debug, error, info};

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
    /// Bind mounts in format "source:target" (repeatable)
    #[arg(long = "bind")]
    bind_mounts: Vec<String>,
    /// macOS only: force the eager-fd path (`macos_lazy_inode_fd=false`)
    /// for A/B benchmarking. Default is lazy. No effect on Linux.
    #[arg(long, default_value_t = false)]
    macos_eager: bool,
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

    #[cfg(target_os = "macos")]
    let fs = if args.macos_eager {
        // Build the eager-mode config explicitly: lazy fd path off, TTLs
        // forced to zero (matches the fallback shape inside
        // `new_passthroughfs_layer`). Used only for `--ab` benchmark
        // comparisons against the lazy path.
        debug!("macOS eager-fd mode (A/B benchmark)");
        let mut cfg = Config {
            root_dir: PathBuf::from(&args.rootdir),
            xattr: true,
            do_import: true,
            macos_lazy_inode_fd: false,
            ..Default::default()
        };
        cfg.entry_timeout = Duration::ZERO;
        cfg.attr_timeout = Duration::ZERO;
        cfg.dir_entry_timeout = Some(Duration::ZERO);
        cfg.dir_attr_timeout = Some(Duration::ZERO);
        if let Some(m) = args.options.as_deref() {
            cfg.mapping = m.parse().unwrap_or_else(|e| {
                error!("invalid mapping {m}: {e}; using empty mapping");
                Default::default()
            });
        }
        let fs = PassthroughFs::<()>::new(cfg).expect("Failed to init passthrough fs (eager)");
        fs.import().await.expect("Failed to import root inode");
        fs
    } else {
        new_passthroughfs_layer(PassthroughArgs {
            root_dir: args.rootdir,
            mapping: args.options,
        })
        .await
        .expect("Failed to init passthrough fs")
    };

    #[cfg(not(target_os = "macos"))]
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
    #[cfg(target_os = "linux")]
    mount_options.force_readdir_plus(true);

    mount_options
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

    // Mount bind mounts after the passthrough filesystem is mounted
    if !bind_specs.is_empty() {
        info!("Mounting {} bind mount(s)", bind_specs.len());
        if let Err(e) = bind_manager.mount_all(&bind_specs).await {
            error!("Failed to mount bind mounts: {}", e);
            // Unmount the passthrough filesystem
            mount_handle.unmount().await.unwrap();
            std::process::exit(1);
        }
    }

    tokio::select! {
        res = &mut mount_handle => {
            if let Err(e) = res {
                error!("Passthrough filesystem error: {:?}", e);
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
            // Then unmount the passthrough filesystem
            if let Err(e) = mount_handle.unmount().await {
                error!("Failed to unmount passthrough filesystem: {}", e);
            }
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
            // Then unmount the passthrough filesystem
            if let Err(e) = mount_handle.unmount().await {
                error!("Failed to unmount passthrough filesystem: {}", e);
            }
        }
    }
}
