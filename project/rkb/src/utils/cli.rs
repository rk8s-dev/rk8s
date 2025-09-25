use anyhow::Context;
use reqwest::RequestBuilder;
use serde::de::DeserializeOwned;
use std::ffi::OsStr;
use std::process::{Command, exit};

#[async_trait::async_trait]
pub trait RequestBuilderExt {
    async fn send_and_json<U>(self) -> anyhow::Result<U>
    where
        U: DeserializeOwned;
}

#[async_trait::async_trait]
impl RequestBuilderExt for RequestBuilder {
    async fn send_and_json<U>(self) -> anyhow::Result<U>
    where
        U: DeserializeOwned,
    {
        self.send()
            .await?
            .json::<U>()
            .await
            .with_context(|| "Failed to deserialize response")
    }
}

pub fn assert_not_sudo() -> anyhow::Result<()> {
    if nix::unistd::getuid().is_root() {
        anyhow::bail!(
            "Please avoid running rkb as root/sudo.\nIt will prompt a password when needed."
        )
    }
    Ok(())
}

pub fn check_internal_sudo() -> bool {
    std::env::var("RKB_INTERNAL_SUDO").is_ok()
}

// This is a little tricky. What makes our lives harder is the authentication config file.
// The file will be saved in current user's config directory. But if we run this with sudo,
// the config file will be saved in `/root/.config`, which is logical but not what we want.
// Under the current situation, we must use sudo because of `libfuse`, but at the
// same time, we also need to avoid messy config files. A feasible solution is to either prohibit
// users from using sudo directly, or only permit users to run it via our specific sudo implementation.
// When a non-root user runs this program, it will reinvoke itself with sudo and set a special
// environment variable `RKB_INTERNAL_SUDO`. This variable, along with other extra envs, provides the necessary
// information for the new process to proceed.
pub fn sudo_guard(extra_env: Vec<(impl AsRef<OsStr>, impl AsRef<OsStr>)>) -> anyhow::Result<()> {
    if nix::unistd::getuid().is_root() && !check_internal_sudo() {
        return Err(anyhow::anyhow!(
            "Avoiding running rkb as root/sudo.\nIt will prompt a password when needed."
        ));
    }

    if !nix::unistd::getuid().is_root() {
        let exe_path = std::env::current_exe()?;
        let args = std::env::args().skip(1).collect::<Vec<_>>();
        let mut command = Command::new("sudo");
        command
            .env("RKB_INTERNAL_SUDO", "tRuE")
            .envs(extra_env)
            .arg("-E")
            .arg(exe_path)
            .args(args);

        let status = command
            .status()
            .with_context(|| "Failed to execute command")?;
        exit(status.code().unwrap_or(1));
    }
    Ok(())
}
