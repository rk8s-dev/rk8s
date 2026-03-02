pub mod libfuse;
pub mod linux;

use crate::config::image::CONFIG;
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

pub static DNS_CONFIG: &str = "/etc/resolv.conf";
pub static BIND_MOUNTS: [&str; 3] = ["/dev", "/proc", "/sys"];

#[derive(Parser, Debug)]
pub struct MountArgs {
    #[arg(long)]
    config_base64: String,
    #[arg(long)]
    libfuse: bool,
    /// Container runtime daemon mode: do not unshare mount namespace, control lifecycle via IPC messages (SIGTERM as fallback)
    #[arg(long)]
    daemon: bool,
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

    if args.daemon {
        daemon_mount(&cfg, child_tx, rx, args.libfuse)
    } else if args.libfuse {
        libfuse::do_mount(&cfg, child_tx, rx)
    } else {
        linux::do_mount(&cfg, child_tx, rx)
    }
}

/// Daemon mode mount: do not unshare mount namespace, control lifecycle via IPC messages.
/// The mount point is visible in the host namespace, ready for youki to use directly.
///
/// Lifecycle management strategy:
/// - Normal exit: Parent process sends "exit" message via IPC, daemon unmounts gracefully upon receipt
/// - Abnormal exit: Parent process crashes causing IPC disconnect, daemon detects it, attempts to unmount and exits
/// - Fallback: Parent process can terminate daemon via SIGTERM signal (caught by tokio signal handler
///   to ensure unmount before exit, rather than direct _exit causing resource leaks)
fn daemon_mount(
    cfg: &MountConfig,
    tx: IpcSender<String>,
    rx: IpcReceiver<String>,
    use_libfuse: bool,
) -> Result<()> {
    check_mountpoint(cfg)?;

    // setsid detaches from terminal, becoming the leader of a new session
    nix::unistd::setsid().ok(); // ignore error (might already be session leader)

    if use_libfuse {
        daemon_mount_libfuse(cfg, tx, rx)
    } else {
        daemon_mount_native(cfg, tx, rx)
    }
}

/// Mount overlayfs using libfuse in daemon mode.
///
/// Listens to three events simultaneously via tokio::select!:
/// 1. FUSE mount handle completion/error
/// 2. IPC receives explicit "exit" message (normal shutdown via `RootfsMount::stop()`)
/// 3. SIGTERM signal (fallback shutdown path, used by `delete_container` after `RootfsMount::load()`)
///
/// IMPORTANT: IPC disconnect (parent process exiting) does NOT trigger unmount.
/// The parent process (`rkforge run`) exits after starting the container, which drops
/// the `IpcSender` and disconnects IPC. The daemon must keep serving the FUSE mount
/// so the container can continue running. Cleanup happens via SIGTERM during container delete.
fn daemon_mount_libfuse(
    cfg: &MountConfig,
    tx: IpcSender<String>,
    rx: IpcReceiver<String>,
) -> Result<()> {
    use libfuse_fs::overlayfs::OverlayArgs;
    use std::sync::Arc;

    let mut lowerdir = cfg.lower_dir.clone();
    lowerdir.reverse();

    crate::rt::block_on(async {
        let mut mount_handle = libfuse_fs::overlayfs::mount_fs(OverlayArgs {
            lowerdir: &lowerdir,
            upperdir: &cfg.upper_dir,
            mountpoint: &cfg.mountpoint,
            privileged: CONFIG.is_root,
            mapping: None::<&str>,
            name: None::<String>,
            allow_other: true,
        })
        .await;

        tx.send("ready".to_string())
            .context("Failed to send ready message to parent")?;

        let handle = &mut mount_handle;

        // Register SIGTERM signal listener (tokio async signal handling, safe and composable)
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .context("Failed to register SIGTERM handler")?;

        // Decouple IPC "exit" message from IPC disconnect using Notify.
        // Only an explicit "exit" message triggers unmount; IPC disconnect (parent process
        // exiting normally) is expected and the daemon continues serving.
        let exit_notify = Arc::new(tokio::sync::Notify::new());
        let notify_clone = exit_notify.clone();
        tokio::task::spawn_blocking(move || match rx.recv() {
            Ok(line) if line.trim() == "exit" => {
                notify_clone.notify_one();
            }
            Ok(line) => {
                tracing::error!("Unknown IPC message from parent: {line}");
            }
            Err(e) => {
                // IPC disconnected: parent process exited normally after starting the container.
                // Do NOT trigger unmount — the container is still running and needs the FUSE mount.
                // The daemon will be cleaned up via SIGTERM during container delete.
                tracing::info!(
                    "IPC disconnected: {e}, parent exited. Continuing to serve overlay mount."
                );
            }
        });

        tokio::select! {
            res = handle => {
                res?;
                Ok(())
            },
            _ = exit_notify.notified() => {
                tracing::info!("Received 'exit' via IPC, unmounting libfuse overlay");
                mount_handle.unmount().await?;
                Ok(())
            },
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM, unmounting libfuse overlay");
                mount_handle.unmount().await?;
                Ok(())
            }
        }
    })?
}

/// Mount Linux native overlayfs in daemon mode.
///
/// Native overlay is a kernel mount, the mount point persists even if the daemon process exits,
/// but it should still be gracefully unmounted to avoid leftovers. Listens to both
/// IPC explicit "exit" message and SIGTERM signal via tokio::select!.
///
/// IMPORTANT: IPC disconnect (parent process exiting) does NOT trigger unmount.
/// See `daemon_mount_libfuse` for rationale.
fn daemon_mount_native(
    cfg: &MountConfig,
    tx: IpcSender<String>,
    rx: IpcReceiver<String>,
) -> Result<()> {
    use nix::mount::{MntFlags, MsFlags, mount, umount2};
    use std::os::unix::ffi::OsStrExt;
    use std::sync::Arc;

    let lower_dirs_bytes: Vec<&[u8]> = cfg
        .lower_dir
        .iter()
        .rev()
        .map(|p| p.as_os_str().as_bytes())
        .collect();
    let lower_dir_option = lower_dirs_bytes.join(&b':');
    let mut options = Vec::new();
    options.extend_from_slice(b"lowerdir=");
    options.extend_from_slice(&lower_dir_option);
    options.extend_from_slice(b",upperdir=");
    options.extend_from_slice(cfg.upper_dir.as_os_str().as_bytes());
    options.extend_from_slice(b",workdir=");
    options.extend_from_slice(cfg.work_dir.as_os_str().as_bytes());

    mount(
        Some("overlay".as_bytes()),
        &cfg.mountpoint,
        Some("overlay".as_bytes()),
        MsFlags::empty(),
        Some(&options[..]),
    )
    .context("Failed to mount native overlayfs")?;

    tx.send("ready".to_string())
        .context("Failed to send ready message to parent")?;

    let mountpoint = cfg.mountpoint.clone();

    // Decouple IPC "exit" message from IPC disconnect using Notify.
    // Only an explicit "exit" message triggers unmount; IPC disconnect is expected.
    let exit_notify = Arc::new(tokio::sync::Notify::new());
    let notify_clone = exit_notify.clone();
    // Spawn IPC listener in a background thread (must be outside the async block
    // so that `rx` is moved into the thread, not into the async context)
    std::thread::spawn(move || match rx.recv() {
        Ok(line) if line.trim() == "exit" => {
            notify_clone.notify_one();
        }
        Ok(line) => {
            tracing::error!("Unknown IPC message from parent: {line}");
        }
        Err(e) => {
            tracing::info!(
                "IPC disconnected: {e}, parent exited. Continuing to serve overlay mount."
            );
        }
    });

    // Use tokio runtime to wait for exit_notify or SIGTERM signal
    crate::rt::block_on(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .context("Failed to register SIGTERM handler")?;

        tokio::select! {
            _ = exit_notify.notified() => {
                tracing::info!("Received 'exit' via IPC, unmounting native overlayfs");
                umount2(&mountpoint, MntFlags::MNT_DETACH)
                    .context("Failed to unmount native overlayfs")?;
            },
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM, unmounting native overlayfs");
                let _ = umount2(&mountpoint, MntFlags::MNT_DETACH);
            }
        }
        Ok(())
    })?
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
    pub(crate) upper_cnt: i32,
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

/// User specification containing uid and optional gid.
#[derive(Debug, Clone)]
pub struct UserSpec {
    pub uid: u32,
    pub gid: Option<u32>,
}

pub fn do_exec<P: AsRef<Path>>(
    mountpoint: P,
    command_args: &[&str],
    envp: &[CString],
    working_dir: &str,
    user: Option<&str>,
) -> Result<()> {
    let mountpoint = mountpoint.as_ref();
    do_chroot(mountpoint)?;

    // Change to the specified working directory
    // Create the directory if it doesn't exist (Docker behavior)
    if let Err(e) = chdir(working_dir) {
        // Check if the error is because directory doesn't exist
        if e == nix::errno::Errno::ENOENT {
            // Create the directory recursively
            fs::create_dir_all(working_dir)
                .with_context(|| format!("Failed to create working directory {}", working_dir))?;
            // Try chdir again
            chdir(working_dir)
                .with_context(|| format!("Failed to chdir to {} after creating it", working_dir))?;
        } else {
            return Err(e).with_context(|| format!("Failed to chdir to {}", working_dir));
        }
    }

    // Resolve and switch user if specified.
    // IMPORTANT: User resolution happens AFTER chroot so that username/group lookups
    // read the container's /etc/passwd and /etc/group instead of the host's.
    if let Some(user_str) = user {
        let spec = resolve_user_spec(user_str)?;

        // Determine gid: use explicit gid if provided, otherwise look up user's primary gid
        let gid = match spec.gid {
            Some(gid) => gid,
            None => {
                // Look up the user's primary group from /etc/passwd (inside chroot)
                uzers::get_user_by_uid(spec.uid)
                    .map(|u| u.primary_group_id())
                    .unwrap_or(spec.uid)
            }
        };

        // Set gid first (must be done before setuid)
        nix::unistd::setgid(nix::unistd::Gid::from_raw(gid))
            .with_context(|| format!("Failed to setgid to {}", gid))?;

        // Clear supplementary groups to prevent retaining host/root group privileges
        nix::unistd::setgroups(&[]).context("Failed to clear supplementary groups")?;

        // Set uid
        nix::unistd::setuid(nix::unistd::Uid::from_raw(spec.uid))
            .with_context(|| format!("Failed to setuid to {}", spec.uid))?;
    }

    execute_command(command_args, envp)?;
    unreachable!();
}

/// Resolve a user specification string into uid and optional gid.
/// This should be called AFTER chroot so it reads the container's /etc/passwd and /etc/group.
/// Supported formats: "user", "uid", "user:group", "uid:gid", "uid:group", "user:gid"
fn resolve_user_spec(user_str: &str) -> Result<UserSpec> {
    let parts: Vec<&str> = user_str.split(':').collect();

    let uid = resolve_uid(parts[0])?;
    let gid = if parts.len() > 1 {
        Some(resolve_gid(parts[1])?)
    } else {
        None
    };

    Ok(UserSpec { uid, gid })
}

/// Parse a string as either a numeric uid or resolve a username from /etc/passwd.
fn resolve_uid(s: &str) -> Result<u32> {
    // Try parsing as numeric uid first
    if let Ok(uid) = s.parse::<u32>() {
        return Ok(uid);
    }

    // Look up as username (reads container's /etc/passwd after chroot)
    uzers::get_user_by_name(s)
        .map(|u| u.uid())
        .ok_or_else(|| anyhow::anyhow!("Unknown user: {}", s))
}

/// Parse a string as either a numeric gid or resolve a group name from /etc/group.
fn resolve_gid(s: &str) -> Result<u32> {
    // Try parsing as numeric gid first
    if let Ok(gid) = s.parse::<u32>() {
        return Ok(gid);
    }

    // Look up as group name (reads container's /etc/group after chroot)
    uzers::get_group_by_name(s)
        .map(|g| g.gid())
        .ok_or_else(|| anyhow::anyhow!("Unknown group: {}", s))
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
        let overlay_tmp = tempdir().unwrap();
        let lower_tmp = tempdir().unwrap();
        let tmp_path = overlay_tmp.path().to_path_buf();

        let mut cfg = MountConfig::new(tmp_path, false);
        cfg.lower_dir.push(lower_tmp.path().to_path_buf());
        cfg.init().unwrap();
        assert_eq!(cfg.lower_dir.len(), 1);

        cfg.prepare().unwrap();
        assert_eq!(cfg.lower_dir.len(), 1);
        assert!(cfg.upper_dir.exists());
        assert!(cfg.mountpoint.exists());
        assert!(cfg.work_dir.exists());

        cfg.finish().unwrap();
        assert_eq!(cfg.lower_dir.len(), 2);
        assert!(cfg.lower_dir[0].exists());
        assert!(cfg.lower_dir[1].exists());
        assert!(cfg.upper_dir.exists());
        assert!(cfg.mountpoint.exists());
        assert!(cfg.work_dir.exists());
    }
}
