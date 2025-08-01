use super::mount_config::MountConfig;

use std::fs;
use std::path::{Path, PathBuf};

use nix::mount::{MsFlags, mount, umount};
use nix::sched::{CloneFlags, unshare};
use std::os::unix::fs::symlink;

use anyhow::{Context, Result, anyhow};

pub fn mount_linux(config: &MountConfig) -> Result<MountManager> {
    let lower_dir = &config.lower_dir;
    let upper_dir = &config.upper_dir;
    let mountpoint = &config.mountpoint;
    let work_dir = &config.work_dir;

    // assert all directories exist
    for dir in lower_dir.iter().chain([upper_dir, mountpoint, work_dir]) {
        match fs::metadata(dir) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    return Err(anyhow!("{} is not a directory", dir.display()));
                }
            }
            Err(_) => {
                return Err(anyhow!("{} does not exist", dir.display()));
            }
        }
    }

    let mut mount_manager = MountManager::default();

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
    )
    .context("Failed to mount root")?;

    // overlayfs options
    let lower_dirs = lower_dir
        .iter()
        .map(|dir| Path::new(dir).canonicalize().unwrap().display().to_string())
        .collect::<Vec<String>>()
        .join(":");
    let options = format!(
        "lowerdir={},upperdir={},workdir={}",
        lower_dirs,
        Path::new(upper_dir).canonicalize().unwrap().display(),
        Path::new(work_dir).canonicalize().unwrap().display()
    );

    // mount overlayfs
    mount::<str, Path, str, str>(
        Some("overlay"),        // source: overlayfs type
        Path::new(mountpoint),  // target: mountpoint
        Some("overlay"),        // fstype: type of filesystem
        MsFlags::empty(),       // flags: mount flags
        Some(options.as_str()), // data: overlayfs options
    )
    .context("Failed to mount overlayfs")?;
    mount_manager.add_mountpoint(mountpoint.clone());

    // mount /proc
    let proc_dir = mountpoint.join("proc");
    fs::create_dir_all(&proc_dir).unwrap();
    mount::<_, _, _, str>(
        Some("proc"),
        Path::new(&proc_dir),
        Some("proc"),
        MsFlags::empty(),
        None,
    )
    .context("Failed to mount /proc")?;
    mount_manager.add_mountpoint(proc_dir.clone());

    // mount /dev
    let dev_dir = mountpoint.join("dev");
    fs::create_dir_all(&dev_dir).unwrap();
    mount::<_, _, _, str>(
        Some("tmpfs"),
        Path::new(&dev_dir),
        Some("tmpfs"),
        MsFlags::empty(),
        None,
    )
    .context("Failed to mount /dev")?;
    mount_manager.add_mountpoint(dev_dir.clone());

    // mount /dev/pts
    let dev_pts = mountpoint.join("dev/pts");
    fs::create_dir_all(&dev_pts).unwrap();
    mount::<_, _, _, _>(
        Some("devpts"),
        Path::new(&dev_pts),
        Some("devpts"),
        MsFlags::empty(),
        Some("newinstance,ptmxmode=0666") /* create a new independent pts instance, set the ptmx device permissions to be readable and writable by all users */
    ).context("Failed to mount /dev/pts")?;
    mount_manager.add_mountpoint(dev_pts.clone());

    // mount /dev/shm
    let dev_shm = mountpoint.join("dev/shm");
    fs::create_dir_all(&dev_shm).unwrap();
    mount::<_, _, _, str>(
        Some("tmpfs"),
        Path::new(&dev_shm),
        Some("tmpfs"),
        MsFlags::empty(),
        None,
    )
    .context("Failed to mount /dev/shm")?;
    mount_manager.add_mountpoint(dev_shm.clone());

    // bind mount the device files
    for file in [
        "full", "zero", "null", "random", "urandom", "tty", "console",
    ] {
        let host_dev = format!("/dev/{file}");
        let container_dev = mountpoint.join(format!("dev/{file}"));
        fs::File::create(&container_dev).unwrap();
        mount::<_, _, str, str>(
            Some(Path::new(&host_dev)),
            Path::new(&container_dev),
            None,
            MsFlags::MS_BIND | MsFlags::MS_REC,
            None,
        )
        .with_context(|| format!("Failed to mount {host_dev}"))?;
        mount_manager.add_mountpoint(container_dev.clone());
    }

    let symlinks = [
        ("/proc/self/fd", mountpoint.join("dev/fd")),
        ("/proc/self/fd/0", mountpoint.join("dev/stdin")),
        ("/proc/self/fd/1", mountpoint.join("dev/stdout")),
        ("/proc/self/fd/2", mountpoint.join("dev/stderr")),
        ("/dev/pts/ptmx", mountpoint.join("dev/ptmx")),
    ];

    for (src, dest) in symlinks {
        symlink(src, dest).with_context(|| format!("Failed to create symlink {src}"))?;
    }

    Ok(mount_manager)
}

/// Manages the mountpoints created by the mount_linux function.
///
/// When the MountManager is dropped, it will unmount all mountpoints.
#[derive(Default)]
pub struct MountManager {
    mountpoints: Vec<PathBuf>,
}

impl MountManager {
    pub fn add_mountpoint(&mut self, mountpoint: PathBuf) {
        self.mountpoints.push(mountpoint);
    }

    pub fn umount_all(&mut self) -> Result<()> {
        let mut errors = Vec::new();
        for mountpoint in self.mountpoints.drain(..).rev() {
            match umount(&mountpoint) {
                Ok(_) => {
                    println!("Unmounted {}", mountpoint.display());
                }
                Err(e) => {
                    errors.push(anyhow!("Failed to unmount {}: {}", mountpoint.display(), e));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow!("Failed to unmount some mountpoints: {:?}", errors))
        }
    }
}

impl Drop for MountManager {
    fn drop(&mut self) {
        let _ = self.umount_all();
    }
}
