use crate::config::registry::DNS_CONFIG;
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose};
use clap::Parser;
use nix::unistd::{chdir, chroot, execve, execvpe, getuid, setuid};
use serde::{Deserialize, Serialize};
use std::{ffi::CString, os::fd::AsFd, path::Path, process::Command};

#[derive(Debug, Parser)]
pub struct ExecArgs {
    #[arg(long)]
    mountpoint: String,
    #[arg(long)]
    task_base64: String,
}

#[derive(Debug, Parser)]
pub struct CleanupArgs {
    #[arg(long)]
    mountpoint: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Task {
    Run {
        command: Vec<String>,
        envp: Vec<String>,
    },
    Copy {
        src: Vec<String>,
        dest: String,
    },
}

#[allow(unused)]
pub fn exec(args: ExecArgs) -> Result<()> {
    let mount_pid = std::env::var("MOUNT_PID")?.parse::<u32>()?;
    switch_namespace(mount_pid)?;

    let mountpoint = Path::new(&args.mountpoint);
    let task = general_purpose::STANDARD.decode(args.task_base64)?;
    let task: Task = serde_json::from_slice(&task)?;
    match task {
        Task::Run { command, envp } => {
            // prepare_network(mountpoint).context("Failed to prepare network")?;
            // for b in config::BIND_MOUNTS {
            //     let dir = mountpoint.join(b.strip_prefix('/').unwrap());
            //     nix::mount::mount::<_, _, str, str>(
            //         Some(b),
            //         &dir,
            //         None,
            //         nix::mount::MsFlags::MS_BIND | nix::mount::MsFlags::MS_REC,
            //         None,
            //     )
            //     .with_context(|| format!("Failed to bind mount {}", dir.display()))?;
            // }
            let command = command.iter().map(|s| s.as_str()).collect::<Vec<_>>();
            let envp = envp
                .iter()
                .map(|s| CString::new(s.as_bytes()).unwrap())
                .collect::<Vec<_>>();
            do_exec(mountpoint, &command, &envp)?;
            unreachable!();
        }
        Task::Copy { src, dest } => {
            let dest_path = Path::new(&dest);
            for s in src {
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
    }
}

#[allow(unused)]
fn chroot_linux(mountpoint: &Path) -> Result<()> {
    tracing::trace!("Chrooting to {}", mountpoint.display());
    chroot(mountpoint).with_context(|| format!("Failed to chroot to {}", mountpoint.display()))?;
    chdir("/").context("Failed to chdir")?;
    setuid(getuid())?;
    Ok(())
}

#[allow(unused)]
fn sh_shell(envp: &Vec<CString>) -> Result<()> {
    let file = CString::new("/bin/sh").unwrap();
    let argv = vec![&file];
    execve(&file, &argv, envp.as_slice()).context("Failed to execve /bin/sh")?;
    Ok(())
}

#[allow(unused)]
fn execute_command(command: &[&str], envp: &[CString]) -> Result<()> {
    let file = CString::new(command[0]).unwrap();
    let argv: Vec<CString> = command.iter().map(|s| CString::new(*s).unwrap()).collect();
    execvpe(&file, &argv, envp)
        .with_context(|| format!("Failed to execute command: {command:?}"))?;
    unreachable!();
}

#[allow(unused)]
fn do_exec(mountpoint: &Path, command: &[&str], envp: &[CString]) -> Result<()> {
    assert!(mountpoint.exists());
    chroot_linux(mountpoint).context("Failed to chroot")?;
    execute_command(command, envp).context("Failed to execute command")?;
    unreachable!();
}

#[allow(unused)]
fn switch_namespace(mount_pid: u32) -> Result<()> {
    let ns_path = format!("/proc/{mount_pid}/ns/mnt");
    let ns_fd = std::fs::File::open(&ns_path)
        .with_context(|| format!("Failed to open namespace file: {ns_path}"))?;
    nix::sched::setns(ns_fd.as_fd(), nix::sched::CloneFlags::CLONE_NEWNS)
        .with_context(|| format!("Failed to setns to {ns_path}"))?;
    Ok(())
}

#[allow(unused)]
fn prepare_network(mountpoint: &Path) -> Result<()> {
    let host_resolv_conf = Path::new(DNS_CONFIG);
    let target_resolv_conf = mountpoint.join(DNS_CONFIG.strip_prefix('/').unwrap());

    assert!(host_resolv_conf.exists());
    if !target_resolv_conf.exists() {
        std::fs::File::create(&target_resolv_conf)
            .with_context(|| format!("Failed to create {}", target_resolv_conf.display()))?;
    }

    nix::mount::mount::<_, _, str, str>(
        Some(host_resolv_conf),
        &target_resolv_conf,
        None,
        nix::mount::MsFlags::MS_BIND,
        None,
    )
    .with_context(|| format!("Failed to bind mount {}", target_resolv_conf.display()))?;

    Ok(())
}

#[allow(unused)]
fn cleanup_network(mountpoint: &Path) -> Result<()> {
    let target_resolv_conf = mountpoint.join(DNS_CONFIG.strip_prefix('/').unwrap());
    if target_resolv_conf.exists() {
        nix::mount::umount2(&target_resolv_conf, nix::mount::MntFlags::MNT_DETACH)
            .with_context(|| format!("Failed to unmount {}", target_resolv_conf.display()))?;
    }
    Ok(())
}

#[allow(unused)]
pub fn cleanup(_: CleanupArgs) -> Result<()> {
    let mount_pid = std::env::var("MOUNT_PID")?.parse::<u32>()?;
    switch_namespace(mount_pid)?;

    // let mountpoint = Path::new(&args.mountpoint);
    // cleanup_network(mountpoint).context("Failed to cleanup network")?;
    // for b in config::BIND_MOUNTS.iter().rev() {
    //     let dir = mountpoint.join(b.strip_prefix('/').unwrap());
    //     if dir.exists() {
    //         nix::mount::umount2(&dir, nix::mount::MntFlags::MNT_DETACH)
    //             .with_context(|| format!("Failed to unmount {}", dir.display()))?;
    //     }
    // }
    Ok(())
}
