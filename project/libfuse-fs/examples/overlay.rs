// Copyright (C) 2024 rk8s authors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Example binary to mount overlay filesystem implemented by libfuse-fs.
// Used by xfstests for overlayfs validation.

use libfuse_fs::overlayfs::OverlayArgs;

#[derive(Debug, Default)]
struct Args {
    name: String,
    mountpoint: String,
    lowerdir: Vec<String>,
    upperdir: String,
    workdir: String,
    log_level: String,
}

fn help() {
    println!(
        "Usage:\n   overlay -o lowerdir=<lower1>:<lower2>:<more>,upperdir=<upper>,workdir=<work> <name> <mountpoint> [-l log_level]\n"
    );
}

fn parse_args() -> Result<Args, std::io::Error> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 5 {
        help();
        return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
    }

    let mut cmd_args = Args {
        name: "".to_string(),
        mountpoint: "".to_string(),
        ..Default::default()
    };

    let mut i = 0;
    loop {
        i += 1;
        if i >= args.len() {
            break;
        }
        if args[i].as_str() == "-h" {
            help();
            return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
        }
        if args[i].as_str() == "-o" {
            i += 1;
            let option = args[i].clone();
            option.split(",").for_each(|value| {
                let kv = value.split("=").collect::<Vec<&str>>();
                if kv.len() != 2 {
                    println!("Unknown option: {}", value);
                    return;
                }

                match kv[0] {
                    "lowerdir" => {
                        cmd_args.lowerdir = kv[1].split(":").map(|s| s.to_string()).collect();
                    }
                    "upperdir" => {
                        cmd_args.upperdir = kv[1].to_string();
                    }
                    "workdir" => {
                        cmd_args.workdir = kv[1].to_string();
                    }
                    _ => {
                        println!("Unknown option: {}", kv[0]);
                    }
                }
            });
            continue;
        }

        if args[i].as_str() == "-l" {
            i += 1;
            cmd_args.log_level = args[i].clone();
        }

        if cmd_args.name.is_empty() {
            cmd_args.name = args[i].clone();
            continue;
        } else if cmd_args.mountpoint.is_empty() {
            cmd_args.mountpoint = args[i].clone();
            continue;
        }
    }

    if cmd_args.lowerdir.is_empty() || cmd_args.upperdir.is_empty() || cmd_args.workdir.is_empty() {
        println!("lowerdir, upperdir and workdir must be specified");
        help();
        return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
    }
    Ok(cmd_args)
}

fn set_log(args: &Args) {
    let log_level = match args.log_level.as_str() {
        "error" | "warn" | "info" | "debug" | "trace" => args.log_level.as_str(),
        _ => "trace",
    };
    let filter_str = format!("libfuse_fs={}", log_level);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter_str));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let args = parse_args()?;

    set_log(&args);

    // This is commented out because some testcase(fsstress) may output huge logs, exhausting disk space.
    // Uncomment this when we need to debug.
    // let file = std::fs::OpenOptions::new()
    //     .create(true)
    //     .write(true)
    //     .truncate(true)
    //     .open("/tmp/overlayfs.log")?;
    // use std::os::unix::io::AsRawFd;
    // unsafe {
    //     libc::dup2(file.as_raw_fd(), libc::STDOUT_FILENO);
    //     libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO);
    // }
    let mut mount_handle = libfuse_fs::overlayfs::mount_fs(OverlayArgs {
        name: Some(args.name),
        mountpoint: args.mountpoint,
        lowerdir: args.lowerdir,
        upperdir: args.upperdir,
        mapping: None::<&str>,
        privileged: true,
        // SECURITY: allow_other permits all users to access this filesystem.
        // This is required for testing with xfstests which uses different UIDs.
        // In production, set to false unless you specifically need multi-user access
        // and have proper permission checks in place.
        allow_other: true,
    })
    .await;
    println!("Mounted");

    let handle = &mut mount_handle;

    use tokio::signal::unix::{SignalKind, signal};
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sighup = signal(SignalKind::hangup())?;

    tokio::select! {
        res = handle => res?,
        _ = sigint.recv() => mount_handle.unmount().await?,
        _ = sigterm.recv() => mount_handle.unmount().await?,
        _ = sighup.recv() => mount_handle.unmount().await?,
    }

    Ok(())
}
