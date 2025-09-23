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
        anyhow::bail!("Avoiding running rkb as root/sudo.\nIt will prompt a password when needed.")
    }
    Ok(())
}

pub fn check_internal_sudo() -> bool {
    std::env::var("RKB_INTERNAL_SUDO").is_ok()
}

///
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
