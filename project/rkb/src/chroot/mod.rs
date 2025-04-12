use std::{ffi::CString, path::Path, process::Command};

use anyhow::{Context, Result, anyhow};
use chroot_linux::{chroot_linux, execute_command};
use nix::{
    sys::wait::{WaitStatus, waitpid},
    unistd::{ForkResult, fork},
};

use crate::overlayfs::{mount_config::MountConfig, mount_linux::mount_linux};

pub mod chroot_linux;

/// TODO: add lib_fuse support
pub fn exec_run_in_subprocess(
    mount_config: &MountConfig,
    commands: &Vec<&str>,
    envp: &Vec<CString>,
) -> Result<()> {
    fn do_child_work(
        mount_config: &MountConfig,
        commands: &Vec<&str>,
        envp: &Vec<CString>,
    ) -> Result<()> {
        let _mount_manager =
            mount_linux(mount_config).with_context(|| "Failed to mount overlayfs")?;

        chroot_linux(&mount_config.mountpoint).with_context(|| "Failed to chroot")?;

        execute_command(commands, envp).with_context(|| "Failed to execute command")?;

        Ok(())
    }

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => match waitpid(child, None)? {
            WaitStatus::Exited(_, status) => {
                if status != 0 {
                    return Err(anyhow!("Sub process exited with: {}", status));
                }
            }
            status => {
                return Err(anyhow!(
                    "Sub process exited with abnormal status: {:?}",
                    status
                ));
            }
        },
        Ok(ForkResult::Child) => match do_child_work(mount_config, commands, envp) {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("Sub process error: {}", e);
                std::process::exit(1);
            }
        },
        Err(e) => return Err(anyhow!("fork failed: {}", e)),
    }

    Ok(())
}

/// TODO: add lib_fuse support
pub fn exec_copy_in_subprocess<P: AsRef<Path>, Q: AsRef<Path>>(
    mount_config: &MountConfig,
    src: &Vec<P>,
    dest: Q,
) -> Result<()> {
    fn do_child_work<P: AsRef<Path>, Q: AsRef<Path>>(
        mount_config: &MountConfig,
        src: &Vec<P>,
        dest: Q,
    ) -> Result<()> {
        let _mount_manager =
            mount_linux(mount_config).with_context(|| "Failed to mount overlayfs")?;

        for s in src {
            // not working
            // fs::copy(&s, &dest)
            //     .with_context(|| format!("Failed to copy file {} to {}", s.as_ref().display(), dest.as_ref().display()))?;

            let status = Command::new("cp")
                .args([
                    s.as_ref().to_str().unwrap(),
                    dest.as_ref().to_str().unwrap(),
                ])
                .status()?;
            if !status.success() {
                return Err(anyhow::anyhow!(
                    "Failed to copy file from {} to {}",
                    s.as_ref().display(),
                    dest.as_ref().display()
                ));
            }
        }

        Ok(())
    }

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => match waitpid(child, None)? {
            WaitStatus::Exited(_, status) => {
                if status != 0 {
                    return Err(anyhow!("Sub process exited with: {}", status));
                }
            }
            status => {
                return Err(anyhow!(
                    "Sub process exited with abnormal status: {:?}",
                    status
                ));
            }
        },
        Ok(ForkResult::Child) => match do_child_work(mount_config, src, dest) {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("Sub process error: {}", e);
                std::process::exit(1);
            }
        },
        Err(e) => return Err(anyhow!("fork failed: {}", e)),
    }

    Ok(())
}
