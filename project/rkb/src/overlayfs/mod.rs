pub mod libfuse;
pub mod linux;

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose};
use clap::Parser;
use ipc_channel::ipc::{IpcReceiver, IpcSender};
use nix::unistd::{chdir, chown, chroot, execve, execvpe, getgid, getuid, setuid};
use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::fs;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};

use crate::config::image::CONFIG;

pub static DNS_CONFIG: &str = "/etc/resolv.conf";
pub static BIND_MOUNTS: [&str; 3] = ["/dev", "/proc", "/sys"];

#[derive(Parser, Debug)]
pub struct MountArgs {
    #[arg(long)]
    config_base64: String,
    #[arg(long)]
    libfuse: bool,
}

pub fn do_mount(args: MountArgs) -> Result<()> {
    let config_json = general_purpose::STANDARD
        .decode(args.config_base64)
        .context("Failed to decode base64 config")?;
    let cfg: MountConfig =
        serde_json::from_slice(&config_json).context("Failed to parse mount config from json")?;

    // Use IPC to notify parent process when the mount is ready.
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

    if args.libfuse {
        libfuse::do_mount(&cfg, child_tx, rx)
    } else {
        linux::do_mount(&cfg, child_tx, rx)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MountConfig {
    pub lower_dir: Vec<PathBuf>,
    pub upper_dir: PathBuf,
    pub mountpoint: PathBuf,
    pub work_dir: PathBuf,
    /// All above directories are created inside this directory.
    ///
    /// When the struct is being dropped, remove this directory.
    pub overlay: PathBuf,
    upper_cnt: i32,
    pub libfuse: bool,
}

impl MountConfig {
    pub fn new(overlay: PathBuf, libfuse: bool) -> Self {
        MountConfig {
            lower_dir: Vec::new(),
            upper_dir: PathBuf::default(),
            mountpoint: PathBuf::default(),
            work_dir: PathBuf::default(),
            overlay,
            upper_cnt: 0,
            libfuse,
        }
    }

    pub fn init(&mut self) -> Result<()> {
        if self.overlay.exists() {
            fs::remove_dir_all(&self.overlay)?;
        }
        fs::create_dir_all(&self.overlay)?;
        assert!(!self.lower_dir.is_empty());
        self.upper_dir = self.overlay.join(format!("diff{}", self.upper_cnt));
        self.mountpoint = self.overlay.join("merged");
        self.work_dir = self.overlay.join("work");
        Ok(())
    }

    /// Clean up the overlay directory for the next mount.
    pub fn prepare(&mut self) -> Result<()> {
        self.upper_cnt += 1;
        self.upper_dir = self.overlay.join(format!("diff{}", self.upper_cnt));

        if self.upper_dir.exists() {
            fs::remove_dir_all(&self.upper_dir)?;
        }
        if self.work_dir.exists() {
            fs::remove_dir_all(&self.work_dir)?;
        }
        if self.mountpoint.exists() {
            fs::remove_dir_all(&self.mountpoint)?;
        }
        fs::create_dir_all(&self.upper_dir)?;
        fs::create_dir_all(&self.work_dir)?;
        fs::create_dir_all(&self.mountpoint)?;

        Ok(())
    }

    /// When a mount is finished, the upper directory is moved to the end of the lower directory.
    pub fn finish(&mut self) -> Result<()> {
        assert!(self.upper_dir.exists());
        self.lower_dir.push(self.upper_dir.clone());
        Ok(())
    }
}

impl Default for MountConfig {
    /// By default, the overlay field is set to `${build_dir}/overlay`.
    fn default() -> Self {
        MountConfig {
            lower_dir: Vec::new(),
            upper_dir: PathBuf::default(),
            mountpoint: PathBuf::default(),
            work_dir: PathBuf::default(),
            overlay: CONFIG.build_dir.join("overlay"),
            upper_cnt: 0,
            libfuse: false,
        }
    }
}

/// A guard that removes the overlay directory when dropped.
pub struct OverlayGuard {
    overlay_dir: PathBuf,
}

impl OverlayGuard {
    pub fn new(overlay_dir: PathBuf) -> Self {
        OverlayGuard { overlay_dir }
    }
}

impl Drop for OverlayGuard {
    fn drop(&mut self) {
        if self.overlay_dir.exists()
            && let Err(e) = std::fs::remove_dir_all(&self.overlay_dir)
        {
            tracing::error!(
                "Failed to remove overlay directory {}: {e}",
                self.overlay_dir.display()
            );
        }
    }
}

/// Check if all directories in the mount config exist and are directories.
fn check_mountpoint(cfg: &MountConfig) -> Result<()> {
    for dir in cfg
        .lower_dir
        .iter()
        .chain([&cfg.upper_dir, &cfg.mountpoint])
    {
        if !fs::metadata(dir).map(|m| m.is_dir()).unwrap_or(false) {
            bail!(
                "Cannot mount: {} does not exist or is not a directory",
                dir.display()
            );
        }
    }
    Ok(())
}

/// Detach the current process from the parent mount namespace and create a new mount namespace.
fn detach() -> Result<()> {
    // create new mount namespace
    nix::sched::unshare(nix::sched::CloneFlags::CLONE_NEWNS).context("Failed to call unshare")?;

    // The process that calls unshare will detach from the namespaces it originally shared with its parent process.
    // This step ensures that modifications to mount points within the new namespace will not propagate back to the parent namespace (the host system),
    // guaranteeing the isolation of mount operations.
    use nix::mount::MsFlags;
    let root = "/";
    nix::mount::mount::<str, _, str, str>(
        None,
        root,
        None,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None,
    )
    .context("Failed to mount root")?;
    Ok(())
}

pub fn switch_namespace(mount_pid: u32) -> Result<()> {
    let ns_path = format!("/proc/{mount_pid}/ns/mnt");
    let ns_fd = fs::File::open(&ns_path)
        .with_context(|| format!("Failed to open namespace file: {ns_path}"))?;
    nix::sched::setns(ns_fd.as_fd(), nix::sched::CloneFlags::CLONE_NEWNS)
        .with_context(|| format!("Failed to setns to {ns_path}"))?;
    Ok(())
}

fn do_chroot<P: AsRef<Path>>(mountpoint: P) -> Result<()> {
    let mountpoint = mountpoint.as_ref();
    tracing::trace!("Chrooting to {}", mountpoint.display());
    chroot(mountpoint).with_context(|| format!("Failed to chroot to {}", mountpoint.display()))?;
    chdir("/").context("Failed to chdir")?;
    setuid(getuid()).context("Failed to set uid")?;
    Ok(())
}

fn exec_in_shell(envp: &[CString]) -> Result<()> {
    let file = CString::new("/bin/sh").unwrap();
    let argv = vec![&file];
    execve(&file, &argv, envp).context("Failed to execve /bin/sh")?;
    Ok(())
}

fn execute_command(command_args: &[&str], envp: &[CString]) -> Result<()> {
    let file = CString::new(command_args[0]).unwrap();
    let argv: Vec<CString> = command_args
        .iter()
        .map(|s| CString::new(*s).unwrap())
        .collect();
    execvpe(&file, &argv, envp)
        .with_context(|| format!("Failed to execute command: {command_args:?}"))?;
    unreachable!();
}

pub fn do_exec<P: AsRef<Path>>(
    mountpoint: P,
    command_args: &[&str],
    envp: &[CString],
) -> Result<()> {
    let mountpoint = mountpoint.as_ref();
    do_chroot(mountpoint)?;
    execute_command(command_args, envp)?;
    unreachable!();
}

pub fn bind_mount<P: AsRef<Path>>(mountpoint: P) -> Result<()> {
    let mountpoint = mountpoint.as_ref();
    for b in BIND_MOUNTS {
        let dir = mountpoint.join(b.strip_prefix('/').unwrap());
        nix::mount::mount::<_, _, str, str>(
            Some(b),
            &dir,
            None,
            nix::mount::MsFlags::MS_BIND | nix::mount::MsFlags::MS_REC,
            None,
        )
        .with_context(|| format!("Failed to bind mount {}", dir.display()))?;
    }
    Ok(())
}

pub fn prepare_network<P: AsRef<Path>>(mountpoint: P) -> Result<()> {
    let mountpoint = mountpoint.as_ref();
    // Create a temp `resolv.conf` file
    let host_resolv_conf = CONFIG.build_dir.join("resolv.conf");
    if host_resolv_conf.exists() {
        fs::remove_file(&host_resolv_conf)?;
    }
    fs::write(&host_resolv_conf, b"nameserver 8.8.8.8\n")?;
    let uid = getuid();
    let gid = getgid();
    chown(&host_resolv_conf, Some(uid), Some(gid))
        .with_context(|| format!("Failed to set ownership on {}", host_resolv_conf.display()))?;

    let target_resolv_conf = mountpoint.join(DNS_CONFIG.strip_prefix('/').unwrap());
    nix::mount::mount::<_, _, str, str>(
        Some(&host_resolv_conf),
        &target_resolv_conf,
        None,
        nix::mount::MsFlags::MS_BIND,
        None,
    )
    .with_context(|| {
        format!(
            "Failed to bind mount {} to {}",
            host_resolv_conf.display(),
            target_resolv_conf.display()
        )
    })?;

    Ok(())
}

fn cleanup_network<P: AsRef<Path>>(mountpoint: P) -> Result<()> {
    let mountpoint = mountpoint.as_ref();
    let target_resolv_conf = mountpoint.join(DNS_CONFIG.strip_prefix('/').unwrap());
    if target_resolv_conf.exists() {
        nix::mount::umount2(&target_resolv_conf, nix::mount::MntFlags::MNT_DETACH)
            .with_context(|| format!("Failed to unmount {}", target_resolv_conf.display()))?;
    }

    let host_resolv_conf = CONFIG.build_dir.join("resolv.conf");
    if host_resolv_conf.exists() {
        fs::remove_file(&host_resolv_conf)?;
    }
    Ok(())
}

#[derive(Debug, Parser)]
pub struct CleanupArgs {
    #[arg(long)]
    mountpoint: String,
}

fn do_cleanup<P: AsRef<Path>>(mount_pid: u32, mountpoint: P) -> Result<()> {
    switch_namespace(mount_pid)?;
    let mountpoint = mountpoint.as_ref();
    cleanup_network(mountpoint)?;
    for b in BIND_MOUNTS.iter().rev() {
        let dir = mountpoint.join(b.strip_prefix('/').unwrap());
        if dir.exists() {
            nix::mount::umount2(&dir, nix::mount::MntFlags::MNT_DETACH)
                .with_context(|| format!("Failed to unmount {}", dir.display()))?;
        }
    }
    Ok(())
}

pub fn cleanup(args: CleanupArgs) -> Result<()> {
    let mount_pid = std::env::var("MOUNT_PID")?.parse::<u32>()?;
    do_cleanup(mount_pid, args.mountpoint)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::MountConfig;

    #[test]
    fn test_mount_config() {
        let tmp_dir = tempdir().unwrap();
        let tmp_path = tmp_dir.path().to_path_buf();

        let mut cfg = MountConfig::new(tmp_path, false);
        cfg.init().unwrap();
        assert_eq!(cfg.lower_dir.len(), 0);

        cfg.prepare().unwrap();
        assert_eq!(cfg.lower_dir.len(), 0);
        assert!(cfg.upper_dir.exists());
        assert!(cfg.mountpoint.exists());
        assert!(cfg.work_dir.exists());

        cfg.finish().unwrap();
        assert_eq!(cfg.lower_dir.len(), 1);
        assert!(cfg.lower_dir[0].exists());
        assert!(cfg.upper_dir.exists());
        assert!(cfg.mountpoint.exists());
        assert!(cfg.work_dir.exists());
    }
}
