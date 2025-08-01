use nix::unistd::{chdir, chroot, execve, execvpe, getuid, setuid};

use std::{ffi::CString, path::Path};

use anyhow::{Context, Result};

pub fn chroot_linux(mountpoint: &Path) -> Result<()> {
    chroot(mountpoint).with_context(|| format!("Failed to chroot to {}", mountpoint.display()))?;
    chdir("/").with_context(|| "Failed to chdir")?;
    setuid(getuid())?;
    Ok(())
}

pub fn sh_shell(envp: &Vec<CString>) -> Result<()> {
    let file = CString::new("/bin/sh").unwrap();
    let argv = vec![&file];
    execve(&file, &argv, envp.as_slice()).context("Failed to execve /bin/sh")?;
    Ok(())
}

pub fn execute_command(command: &Vec<&str>, envp: &Vec<CString>) -> Result<()> {
    let file = CString::new(command[0]).unwrap();
    let argv = command
        .iter()
        .map(|arg| CString::new(*arg).unwrap())
        .collect::<Vec<CString>>();
    execvpe(&file, &argv, envp.as_slice())
        .with_context(|| format!("Failed to execute command: {command:?}"))?;
    Ok(())
}
