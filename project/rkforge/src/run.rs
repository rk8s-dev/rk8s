use crate::overlayfs::{bind_mount, do_exec, prepare_network, switch_namespace};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose};
use clap::Parser;
use std::{ffi::CString, path::Path};

#[derive(Debug, Parser, Clone)]
pub struct ExecInternalArgs {
    #[arg(long)]
    pub mountpoint: String,
    #[arg(long)]
    pub envp_base64: String,
    /// In order to support commands with arguments(e.g. `-c`),
    /// we use base64 to encode the entire command list as a single argument.
    #[arg(long)]
    pub commands_base64: String,
}

pub fn exec_internal(args: ExecInternalArgs) -> Result<()> {
    let mount_pid = std::env::var("MOUNT_PID")?.parse::<u32>()?;
    switch_namespace(mount_pid)?;

    let mountpoint = Path::new(&args.mountpoint);
    prepare_network(mountpoint)?;
    bind_mount(mountpoint)?;

    let commands_json = general_purpose::STANDARD
        .decode(&args.commands_base64)
        .context("Failed to decode commands from base64")?;
    let commands: Vec<String> = serde_json::from_slice(&commands_json)
        .context("Failed to deserialize commands from json")?;
    let commands = commands.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    if commands.is_empty() {
        bail!("At least one command is required to run");
    }

    let envp_json = general_purpose::STANDARD
        .decode(&args.envp_base64)
        .context("Failed to decode envp from base64")?;
    let envp: Vec<String> =
        serde_json::from_slice(&envp_json).context("Failed to deserialize envp from json")?;
    let envp = envp
        .iter()
        .map(|s| CString::new(s.as_bytes()).context("Environment variable contains null byte"))
        .collect::<Result<Vec<_>>>()?;

    do_exec(mountpoint, &commands, &envp)?;
    unreachable!();
}
