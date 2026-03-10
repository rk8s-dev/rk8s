//! Persistent overlayfs mount manager for container rootfs.
//!
//! Lifecycle: `start()` on container create → `stop()` on container delete.
//! The mount process runs as an independent daemon, persisting state via a metadata file.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use base64::{Engine, engine::general_purpose};
use ipc_channel::ipc::{IpcOneShotServer, IpcSender};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::overlayfs::MountConfig;

/// Rootfs overlay mount metadata persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootfsMountConfig {
    pub lower_dirs: Vec<PathBuf>,
    pub upper_dir: PathBuf,
    pub work_dir: PathBuf,
    pub mountpoint: PathBuf,
    pub mount_pid: u32,
    pub use_libfuse: bool,
}

/// Metadata file name
const ROOTFS_MOUNT_METADATA: &str = "rootfs_mount.json";

/// Persistent overlayfs mount manager for container rootfs
pub struct RootfsMount {
    config: RootfsMountConfig,
    /// IPC sender used to notify the mount process to exit (only available when created via start())
    tx: Option<IpcSender<String>>,
}

impl RootfsMount {
    /// Start the background overlay daemon mount process.
    ///
    /// Creates the overlay directory structure under `bundle_path` and starts the background daemon process:
    /// - `lower_dirs`: Read-only layers arranged according to overlayfs semantics (newest layer first)
    /// - `upper_dir`, `work_dir`, `mountpoint`: Paths for overlay layers
    /// - `bundle_path`: Bundle root directory, used to store the metadata file
    /// - `use_libfuse`: true to use libfuse backend, false to use Linux native overlayfs
    pub fn start(
        lower_dirs: &[PathBuf],
        upper_dir: &Path,
        work_dir: &Path,
        mountpoint: &Path,
        bundle_path: &Path,
        use_libfuse: bool,
    ) -> Result<Self> {
        // Construct MountConfig for the child process
        let mount_cfg = MountConfig {
            lower_dir: lower_dirs.to_vec(),
            upper_dir: upper_dir.to_path_buf(),
            mountpoint: mountpoint.to_path_buf(),
            work_dir: work_dir.to_path_buf(),
            overlay: bundle_path.to_path_buf(),
            upper_cnt: 0,
            libfuse: use_libfuse,
        };

        let cfg_json =
            serde_json::to_string(&mount_cfg).context("Failed to serialize mount config")?;
        let cfg_base64 = general_purpose::STANDARD.encode(cfg_json);

        // Create IPC server pair
        let (parent_server, parent_server_name) =
            IpcOneShotServer::new().context("Failed to create parent IPC server")?;
        let (child_server, child_server_name) =
            IpcOneShotServer::<String>::new().context("Failed to create child IPC server")?;

        // Build child process command
        let mut cmd =
            Command::new(std::env::current_exe().context("Failed to get current exe path")?);
        cmd.arg("mount")
            .arg("--config-base64")
            .arg(&cfg_base64)
            .arg("--daemon")
            .env("PARENT_SERVER_NAME", &parent_server_name)
            .env("CHILD_SERVER_NAME", &child_server_name);

        if use_libfuse {
            cmd.arg("--libfuse");
        }

        debug!("Spawning overlay daemon: {:?}", cmd);

        let mut child = cmd
            .spawn()
            .context("Failed to spawn overlay daemon process")?;
        let mount_pid = child.id();

        let ipc_timeout = Duration::from_secs(30);

        // Receive IPC sender sent by child process from parent_server (with timeout).
        // IpcOneShotServer::accept() blocks indefinitely, so run it in a thread
        // and use mpsc::channel with recv_timeout to bound the wait.
        let (ptx, prx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = ptx.send(parent_server.accept());
        });
        let tx: IpcSender<String> = match prx.recv_timeout(ipc_timeout) {
            Ok(Ok((_, tx))) => tx,
            Ok(Err(e)) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!("Failed to accept IPC connection from daemon: {e}"));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!(
                    "Timed out waiting for IPC connection from overlay daemon (pid={mount_pid})"
                ));
            }
        };

        // Wait for child process to send "ready" (with timeout)
        let (ctx, crx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = ctx.send(child_server.accept());
        });
        let msg = match crx.recv_timeout(ipc_timeout) {
            Ok(Ok((_, msg))) => msg,
            Ok(Err(e)) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!("Failed to receive ready signal from daemon: {e}"));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!(
                    "Timed out waiting for ready signal from overlay daemon (pid={mount_pid})"
                ));
            }
        };

        // Check if child exited prematurely during IPC handshake
        if let Some(status) = child.try_wait().context("Failed to check daemon status")? {
            anyhow::bail!("Overlay daemon exited prematurely with status: {status}");
        }

        if msg != "ready" {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Unexpected message from daemon: {msg}");
        }

        debug!(
            "Overlay daemon started, pid={mount_pid}, mountpoint={}",
            mountpoint.display()
        );

        let config = RootfsMountConfig {
            lower_dirs: lower_dirs.to_vec(),
            upper_dir: upper_dir.to_path_buf(),
            work_dir: work_dir.to_path_buf(),
            mountpoint: mountpoint.to_path_buf(),
            mount_pid,
            use_libfuse,
        };

        // Persist metadata
        let metadata_path = bundle_path.join(ROOTFS_MOUNT_METADATA);
        let metadata_json = serde_json::to_string_pretty(&config)
            .context("Failed to serialize rootfs mount config")?;
        std::fs::write(&metadata_path, metadata_json)
            .with_context(|| format!("Failed to write metadata to {}", metadata_path.display()))?;

        Ok(Self {
            config,
            tx: Some(tx),
        })
    }

    /// Restore `RootfsMount` instance from persisted file (used to regain control during delete).
    ///
    /// Returns `Ok(None)` if the metadata file does not exist or the process is dead.
    pub fn load(bundle_path: &Path) -> Result<Option<Self>> {
        let metadata_path = bundle_path.join(ROOTFS_MOUNT_METADATA);
        if !metadata_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&metadata_path)
            .with_context(|| format!("Failed to read {}", metadata_path.display()))?;
        let config: RootfsMountConfig = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", metadata_path.display()))?;

        // Check if process is alive
        let proc_path = format!("/proc/{}", config.mount_pid);
        if !Path::new(&proc_path).exists() {
            debug!(
                "Overlay daemon pid={} no longer exists, cleaning up metadata",
                config.mount_pid
            );
            let _ = std::fs::remove_file(&metadata_path);
            return Ok(None);
        }

        Ok(Some(Self { config, tx: None }))
    }

    /// Return mountpoint path
    pub fn mountpoint(&self) -> &Path {
        &self.config.mountpoint
    }

    /// Notify daemon process to unmount and exit, cleaning up resources.
    pub fn stop(self) -> Result<()> {
        let pid = Pid::from_raw(self.config.mount_pid as i32);
        let stop_start = Instant::now();

        // Prefer sending "exit" message via IPC (available when stop() is called from
        // the same process that called start(), e.g. during error rollback)
        if let Some(tx) = &self.tx {
            match tx.send("exit".to_string()) {
                Ok(()) => {
                    debug!(
                        "Sent 'exit' to overlay daemon pid={}",
                        self.config.mount_pid
                    );
                    let result = self.wait_for_exit(pid);
                    info!("Overlay daemon stop (IPC) took {:?}", stop_start.elapsed());
                    return result;
                }
                Err(e) => {
                    warn!(
                        "IPC send failed for pid={}: {e}, falling back to direct unmount",
                        self.config.mount_pid
                    );
                }
            }
        }

        // IPC unavailable (loaded from disk during delete): unmount directly from this process.
        //
        // Why not SIGTERM first?
        // The SIGTERM → tokio async select → mount_handle.unmount() → rfuse3 inner_unmount
        // chain is slow (requires FUSE task completion + kernel umount). Directly unmounting
        // the mountpoint from the kernel side causes the FUSE session to end, making the
        // daemon's `handle` future return immediately, so the process exits naturally.
        let result = self.stop_direct_unmount(pid);
        info!(
            "Overlay daemon stop (direct unmount) took {:?}",
            stop_start.elapsed()
        );
        result
    }

    /// Stop daemon by directly unmounting the FUSE mountpoint from the caller process,
    /// then waiting for the daemon to exit. This is much faster than SIGTERM because
    /// it doesn't depend on the daemon's async unmount chain.
    fn stop_direct_unmount(&self, pid: Pid) -> Result<()> {
        let mountpoint = &self.config.mountpoint;

        // Step 1: Directly unmount the FUSE/overlay mountpoint from kernel side.
        // For libfuse, this terminates the FUSE session, causing the daemon's mount handle
        // future to return, which naturally exits the daemon process.
        // For native overlay, umount removes the mount and the daemon detects this.
        let unmount_ok = if self.config.use_libfuse {
            self.do_fusermount_unmount(mountpoint)
        } else {
            self.do_kernel_unmount(mountpoint)
        };

        if unmount_ok {
            debug!(
                "Successfully unmounted {} from caller process",
                mountpoint.display()
            );
        } else {
            warn!(
                "Direct unmount of {} failed, falling back to SIGTERM",
                mountpoint.display()
            );
        }

        // Step 2: Wait briefly for the daemon to exit naturally after unmount.
        // If unmount succeeded, the daemon should exit within ~200ms (just cleanup).
        let quick_timeout = if unmount_ok {
            Duration::from_secs(2)
        } else {
            Duration::from_millis(0) // Skip waiting, go straight to SIGTERM
        };

        if quick_timeout > Duration::ZERO {
            let exited = self.poll_proc_exit(pid, quick_timeout);
            if exited {
                debug!(
                    "Overlay daemon pid={} exited after direct unmount",
                    self.config.mount_pid
                );
                self.cleanup_metadata()?;
                return Ok(());
            }
            debug!(
                "Overlay daemon pid={} still alive after direct unmount, sending SIGTERM",
                self.config.mount_pid
            );
        }

        // Step 3: SIGTERM as fallback
        self.stop_via_signal(pid)
    }

    /// Unmount using fusermount3 -u (or fusermount -u as fallback)
    fn do_fusermount_unmount(&self, mountpoint: &Path) -> bool {
        // Try fusermount3 first (newer systems), then fusermount
        for cmd_name in &["fusermount3", "fusermount"] {
            match Command::new(cmd_name).arg("-u").arg(mountpoint).output() {
                Ok(output) if output.status.success() => return true,
                Ok(output) => {
                    debug!(
                        "{cmd_name} -u {} failed: {}",
                        mountpoint.display(),
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => {
                    debug!("{cmd_name} -u {} error: {e}", mountpoint.display());
                }
            }
        }

        // Last resort: raw umount syscall
        self.do_kernel_unmount(mountpoint)
    }

    /// Unmount using the umount2 syscall with MNT_DETACH
    fn do_kernel_unmount(&self, mountpoint: &Path) -> bool {
        match nix::mount::umount2(mountpoint, nix::mount::MntFlags::MNT_DETACH) {
            Ok(()) => true,
            Err(e) => {
                debug!("umount2({}) failed: {e}", mountpoint.display());
                false
            }
        }
    }

    /// Poll /proc/<pid> existence until the process exits or timeout.
    /// Returns true if the process exited.
    fn poll_proc_exit(&self, pid: Pid, timeout: Duration) -> bool {
        let start = Instant::now();
        let proc_path = format!("/proc/{}", pid.as_raw());
        loop {
            if !Path::new(&proc_path).exists() {
                return true;
            }
            if start.elapsed() > timeout {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Stop daemon via SIGTERM signal
    fn stop_via_signal(&self, pid: Pid) -> Result<()> {
        if let Err(e) = signal::kill(pid, Signal::SIGTERM) {
            if e == nix::errno::Errno::ESRCH {
                debug!(
                    "Overlay daemon pid={} already exited",
                    self.config.mount_pid
                );
                self.cleanup_metadata()?;
                return Ok(());
            }
            return Err(anyhow!(
                "Failed to send SIGTERM to pid={}: {e}",
                self.config.mount_pid
            ));
        }

        debug!(
            "Sent SIGTERM to overlay daemon pid={}",
            self.config.mount_pid
        );
        self.wait_for_exit(pid)
    }

    /// Wait for daemon process to exit, SIGKILL on timeout
    fn wait_for_exit(&self, pid: Pid) -> Result<()> {
        let start = Instant::now();
        let timeout = Duration::from_secs(3);

        loop {
            match nix::sys::wait::waitpid(pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG)) {
                Ok(nix::sys::wait::WaitStatus::StillAlive) => {
                    if start.elapsed() > timeout {
                        warn!(
                            "Overlay daemon pid={} did not exit in {:?}, sending SIGKILL",
                            self.config.mount_pid, timeout
                        );
                        let _ = signal::kill(pid, Signal::SIGKILL);
                        let _ = nix::sys::wait::waitpid(pid, None);
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Ok(_) => {
                    debug!(
                        "Overlay daemon pid={} exited in {:?}",
                        self.config.mount_pid,
                        start.elapsed()
                    );
                    break;
                }
                Err(nix::errno::Errno::ECHILD) => {
                    // Not a child process (common in the delete path), poll /proc instead
                    if self.poll_proc_exit(pid, timeout.saturating_sub(start.elapsed())) {
                        debug!(
                            "Overlay daemon pid={} exited (not our child) in {:?}",
                            self.config.mount_pid,
                            start.elapsed()
                        );
                        break;
                    }
                    warn!(
                        "Overlay daemon pid={} still alive after {:?}, sending SIGKILL",
                        self.config.mount_pid, timeout
                    );
                    let _ = signal::kill(pid, Signal::SIGKILL);
                    std::thread::sleep(Duration::from_millis(100));
                    break;
                }
                Err(e) => {
                    warn!("waitpid for pid={} failed: {e}", self.config.mount_pid);
                    break;
                }
            }
        }

        // In libfuse mode, mountpoint might still exist after process dies, try fusermount cleanup
        if self.config.use_libfuse {
            self.try_fusermount_cleanup();
        }

        self.cleanup_metadata()
    }

    /// Attempt to unmount leftover FUSE mountpoint using fusermount
    fn try_fusermount_cleanup(&self) {
        let mountpoint = &self.config.mountpoint;
        // Check if mountpoint is still mounted
        if let Ok(output) = Command::new("mountpoint")
            .arg("-q")
            .arg(mountpoint)
            .output()
            && output.status.success()
        {
            debug!(
                "Mountpoint {} still mounted, running fusermount -u",
                mountpoint.display()
            );
            if let Err(e) = Command::new("fusermount")
                .arg("-u")
                .arg(mountpoint)
                .status()
            {
                error!("fusermount -u {} failed: {e}", mountpoint.display());
            }
        }
    }

    /// Clean up metadata file
    fn cleanup_metadata(&self) -> Result<()> {
        // Locate metadata from mountpoint's parent directory (bundle_path)
        if let Some(bundle_path) = self.config.mountpoint.parent() {
            let metadata_path = bundle_path.join(ROOTFS_MOUNT_METADATA);
            if metadata_path.exists() {
                std::fs::remove_file(&metadata_path)
                    .with_context(|| format!("Failed to remove {}", metadata_path.display()))?;
            }
        }
        Ok(())
    }
}

/// Check whether the overlay daemon for a given bundle is still alive.
///
/// Returns a human-readable status string. Useful for debugging.
/// Usage: `RootfsMount::check_daemon_status(bundle_path)`
pub fn check_daemon_status(bundle_path: &Path) -> String {
    let metadata_path = bundle_path.join(ROOTFS_MOUNT_METADATA);
    if !metadata_path.exists() {
        return format!("No rootfs_mount.json found in {}", bundle_path.display());
    }

    let content = match std::fs::read_to_string(&metadata_path) {
        Ok(c) => c,
        Err(e) => return format!("Failed to read metadata: {e}"),
    };
    let config: RootfsMountConfig = match serde_json::from_str(&content) {
        Ok(c) => c,
        Err(e) => return format!("Failed to parse metadata: {e}"),
    };

    let pid = config.mount_pid;
    let proc_path = format!("/proc/{pid}");
    let proc_exists = Path::new(&proc_path).exists();

    let mountpoint = &config.mountpoint;
    let mount_active = Command::new("mountpoint")
        .arg("-q")
        .arg(mountpoint)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let mut status = format!(
        "Overlay daemon status for {}:\n  PID: {}\n  Process alive: {}\n  Mountpoint: {}\n  Mount active: {}\n  Backend: {}",
        bundle_path.display(),
        pid,
        proc_exists,
        mountpoint.display(),
        mount_active,
        if config.use_libfuse {
            "libfuse"
        } else {
            "native overlay"
        }
    );

    // Try to read process command line for extra diagnostics
    if proc_exists && let Ok(cmdline) = std::fs::read_to_string(format!("/proc/{pid}/cmdline")) {
        let cmdline = cmdline.replace('\0', " ");
        status.push_str(&format!("\n  Cmdline: {}", cmdline.trim()));
    }

    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rootfs_mount_config_serde() {
        let config = RootfsMountConfig {
            lower_dirs: vec![PathBuf::from("/layer1"), PathBuf::from("/layer2")],
            upper_dir: PathBuf::from("/upper"),
            work_dir: PathBuf::from("/work"),
            mountpoint: PathBuf::from("/merged"),
            mount_pid: 12345,
            use_libfuse: true,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: RootfsMountConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.mount_pid, 12345);
        assert_eq!(deserialized.lower_dirs.len(), 2);
        assert!(deserialized.use_libfuse);
    }

    #[test]
    fn test_load_nonexistent() {
        let result = RootfsMount::load(Path::new("/tmp/nonexistent-bundle-path-xyz"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
